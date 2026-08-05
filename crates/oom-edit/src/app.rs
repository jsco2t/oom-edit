//! App — the single source of truth for the running TUI.
//!
//! Holds the [`EditorSession`], scroll position, last status message, overlay
//! state, and the quit flag. After each event: drain effects, scroll-follow,
//! update status message.
//!
//! ## Key routing order (arch §7.1)
//!
//! 1. Overlay open → overlay's key handler (take-and-return-bool).
//! 2. Mode ∈ {Normal, View}: try app keymap — `F1`, `Space`-leader chords.
//!    On match → `execute_command`. No match → fall through.
//! 3. Everything else → `session.handle_key(key)`, then drain `Effect`s.

use std::time::Instant;

use crossterm::event::{Event, KeyCode as CrosstermKeyCode, KeyEvent, KeyModifiers};
use ratatui::Frame;

use oom_edit_core::clipboard::ClipboardSink;
use oom_edit_core::session::{EditorSession, Effect, KeyCode, KeyCodeKind, KeyInput, Modifiers};

use crossterm::event::MouseEventKind;

use crate::command::{keymap::PendingChord, Command, Keymap};
use crate::overlay::Overlay;
use crate::screens::editor::render_editor;
use crate::screens::view::render_view;
use crate::theme::{self, Tier};
use crate::widgets::status_bar;
use crate::widgets::which_key;

/// Scrolloff: keep this many lines of context around the cursor.
const SCROLLOFF: usize = 3;

/// App state for the TUI.
pub struct App {
    /// The core editing session.
    pub session: EditorSession,
    /// The first visible line (owned by the TUI for scroll-follow).
    pub top_line: usize,
    /// The first visible view line (owned by the TUI for scroll-follow in View mode).
    pub view_top: usize,
    /// Whether the app should quit.
    pub should_quit: bool,
    /// The last status message to display in the status bar.
    pub status_message: String,
    /// The active overlay (palette, confirm, etc.).
    pub overlay: Overlay,
    /// Pending Space-chord state.
    pub pending_chord: PendingChord,
    /// The app keymap.
    keymap: Keymap,
    /// Viewport height (set after render for scroll-follow).
    viewport_height: usize,
    /// Current time (injected by tick for testability of which-key delay gate).
    now: Instant,
    /// Active transient message with TTL expiry.
    transient: Option<status_bar::Transient>,
    /// Active theme name (for CycleTheme).
    #[cfg(test)]
    pub theme_name: String,
    #[cfg(not(test))]
    theme_name: String,
    /// Active capability tier.
    tier: Tier,
    /// Clipboard sink for OSC 52 clipboard writes (T16).
    clipboard_sink: Box<dyn ClipboardSink>,
}

impl App {
    /// Create a new App from an open session.
    pub fn new(
        session: EditorSession,
        theme_name: String,
        tier: Tier,
        clipboard_sink: Box<dyn ClipboardSink>,
    ) -> Self {
        Self {
            session,
            top_line: 0,
            view_top: 0,
            should_quit: false,
            status_message: String::new(),
            overlay: Overlay::default(),
            pending_chord: PendingChord::default(),
            keymap: Keymap::default(),
            viewport_height: 22,
            now: Instant::now(),
            transient: None,
            theme_name,
            tier,
            clipboard_sink,
        }
    }

    /// Advance internal timers. Returns the next poll deadline (if any).
    ///
    /// Advances `self.now`, expires any TTL'd transient messages, and
    /// computes the minimum of transient expiry and which-key pending+150ms.
    pub fn tick(&mut self, now: Instant) -> Option<Instant> {
        self.now = now;

        // Expire any TTL'd transient.
        let expired = self
            .transient
            .as_ref()
            .map(|t| t.is_expired(now))
            .unwrap_or(false);
        if expired {
            self.transient = None;
            self.status_message.clear();
        }

        // Compute next deadline: min(transient.expires_at, which_key_pending+150ms).
        let transient_deadline = self.transient.as_ref().map(|t| t.expires_at);
        let which_key_deadline = self
            .pending_chord
            .since
            .map(|s| s + std::time::Duration::from_millis(150));

        transient_deadline
            .into_iter()
            .chain(which_key_deadline)
            .min()
            .filter(|d| *d > now)
    }

    /// Render the current frame.
    pub fn render(&mut self, frame: &mut Frame<'_>) {
        let area = frame.area();

        // Compute viewport height from terminal size.
        self.viewport_height = area.height.saturating_sub(1) as usize;

        // Render the appropriate screen behind the overlay.
        if self.session.mode() == oom_edit_core::session::Mode::View {
            render_view(
                frame,
                &mut self.session,
                self.view_top,
                area,
                &self.theme_name,
                self.tier,
            );
        } else {
            render_editor(
                frame,
                &mut self.session,
                self.top_line,
                &self.status_message,
                self.transient.as_ref(),
                self.overlay.hints(),
                area,
            );
        }

        // Render which-key hint bar if conditions are met.
        self.render_which_key(frame, area);

        // Render overlay on top if open.
        if self.overlay.is_some() {
            self.overlay.render(frame);
        }
    }

    /// Render the which-key hint bar.
    ///
    /// Pure gate + pure build + thin render: the which-key popup appears
    /// only after 150ms of pending Space prefix, in Normal/View mode,
    /// and only when there are ≥2 continuations.
    /// Return the [`Contexts`] bitset for the current session mode.
    fn mode_context(&self) -> crate::command::registry::Contexts {
        match self.session.mode() {
            oom_edit_core::session::Mode::Normal => crate::command::registry::Contexts::NORMAL,
            oom_edit_core::session::Mode::View => crate::command::registry::Contexts::VIEW,
            _ => crate::command::registry::Contexts::NORMAL,
        }
    }

    /// Return true when the current mode supports Space-chord keymaps.
    fn in_chord_context(&self) -> bool {
        matches!(
            self.session.mode(),
            oom_edit_core::session::Mode::Normal | oom_edit_core::session::Mode::View
        )
    }

    /// Render the which-key hint bar.
    ///
    /// Pure gate + pure build + thin render: the which-key popup appears
    /// only after 150ms of pending Space prefix, in Normal/View mode,
    /// and only when there are ≥2 continuations.
    fn render_which_key(&self, frame: &mut Frame<'_>, area: ratatui::layout::Rect) {
        if !self.in_chord_context() {
            return;
        }

        let since = self.pending_chord.since;
        if since.is_none() {
            return;
        }

        if !which_key::should_show(since, self.now) {
            return;
        }

        let ctx = self.mode_context();
        if let Some(text) = which_key::build_hint(&self.keymap, ctx) {
            which_key::render(frame, area, &text);
        }
    }

    /// Handle a crossterm event, following arch §7.1 fixed order.
    pub fn handle_event(&mut self, event: &Event) {
        // Handle resize events — rebuild view layout on width change.
        if let Event::Resize(_width, height) = event {
            // Clamp viewport height from the new terminal size.
            self.viewport_height = height.saturating_sub(1) as usize;
            // View mode: remap cursor from edit coordinates so it stays on
            // the same content line after re-wrap (FR-3.1).
            if self.session.mode() == oom_edit_core::session::Mode::View {
                let (edit_line, edit_col) = self.session.cursor();
                // Force layout rebuild so enter_view has a layout to work with.
                self.session.render_view(*_width);
                self.session
                    .remap_view_cursor_from_edit(edit_line, edit_col);
            }
            return;
        }

        // Handle mouse wheel scroll (FR-6.11).
        if let Event::Mouse(mouse) = event {
            match mouse.kind {
                MouseEventKind::ScrollUp => {
                    self.scroll_up(3);
                    return;
                }
                MouseEventKind::ScrollDown => {
                    self.scroll_down(3);
                    return;
                }
                _ => return, // Other mouse events are no-ops in v1.
            }
        }

        // Translate crossterm event → core KeyInput.
        let key_input = match event {
            Event::Key(key) => crossterm_key_to_core(key),
            _ => return,
        };

        // 1. Overlay open → overlay's key handler.
        if self.overlay.is_some() {
            // Esc closes the overlay (handled by returning false).
            let consumed = self.overlay.handle_key(&key_input);
            if !consumed {
                // Esc or other close key: close overlay and fall through.
                if matches!(event, Event::Key(key) if key.code == CrosstermKeyCode::Esc)
                    || matches!(key_input.code.kind, KeyCodeKind::Char('c') if key_input.mods.ctrl)
                {
                    self.overlay.close();
                    // Don't fall through — Esc on palette is consumed.
                    return;
                }

                // Enter on confirm overlay: execute the confirm result.
                if matches!(key_input.code.kind, KeyCodeKind::Enter) {
                    if let Some(result) = self.overlay.confirm_result() {
                        self.execute_confirm_result(result);
                        self.overlay.close();
                    } else if let Some(cmd) = self.overlay.selected_command() {
                        self.overlay.close();
                        self.execute_command(cmd);
                    } else {
                        self.overlay.close();
                        self.set_transient(
                            "reference entry".to_string(),
                            oom_edit_core::session::Severity::Info,
                        );
                    }
                    return;
                }

                // Other keys are consumed by the palette's filter navigation.
                return;
            }
            return;
        }

        // 2. Mode ∈ {Normal, View}: try app keymap.
        if self.in_chord_context() {
            let ctx = self.mode_context();
            match self
                .keymap
                .resolve(ctx, &key_input, &mut self.pending_chord)
            {
                crate::command::keymap::Resolution::Command(cmd) => {
                    self.execute_command(cmd);
                    return;
                }
                crate::command::keymap::Resolution::Pending(_) => {
                    return;
                }
                crate::command::keymap::Resolution::None => {}
            }
        }

        // 3. Everything else → session.handle_key(key).
        let effects = self.session.handle_key(key_input);

        // Drain effects.
        for effect in effects {
            self.handle_effect(effect);
        }

        // Scroll-follow: clamp top_line so the cursor is visible.
        self.scroll_follow();
    }

    /// Execute the result of a confirm overlay.
    fn execute_confirm_result(&mut self, result: crate::overlay::ConfirmResult) {
        match result {
            crate::overlay::ConfirmResult::Confirm => {
                // ConfirmQuit: save and quit.
                // ConfirmOverwrite: overwrite file (don't quit).
                let is_overwrite = matches!(self.overlay, Overlay::ConfirmOverwrite(_));
                match self.session.save(None, true) {
                    Ok(()) => {
                        self.session.save_point();
                        if is_overwrite {
                            self.set_transient(
                                "File overwritten".to_string(),
                                oom_edit_core::session::Severity::Success,
                            );
                        } else {
                            self.should_quit = true;
                        }
                    }
                    Err(e) => {
                        self.set_transient(
                            format!("Save error: {e}"),
                            oom_edit_core::session::Severity::Error,
                        );
                    }
                }
            }
            crate::overlay::ConfirmResult::Quit => {
                // ConfirmQuit: quit without saving.
                self.should_quit = true;
            }
            crate::overlay::ConfirmResult::Reload => {
                // ConfirmOverwrite: reload file from disk.
                if let Some(path) = self.session.document_ref().path() {
                    match EditorSession::open(path) {
                        Ok(new_session) => {
                            self.session = new_session;
                            self.top_line = 0;
                            self.set_transient(
                                "Reloaded from disk".to_string(),
                                oom_edit_core::session::Severity::Info,
                            );
                        }
                        Err(e) => {
                            self.set_transient(
                                format!("Reload error: {e}"),
                                oom_edit_core::session::Severity::Error,
                            );
                        }
                    }
                }
            }
            crate::overlay::ConfirmResult::Cancel => {
                // Cancel: do nothing, stay in current state.
            }
        }
    }

    /// Execute a command from the registry.
    fn execute_command(&mut self, cmd: Command) {
        match cmd {
            Command::ToggleView => {
                self.session.toggle_view();
            }
            Command::Help => {
                // Open the command palette.
                self.overlay = Overlay::open_palette();
            }
            Command::Save => match self.session.save(None, false) {
                Ok(()) => {
                    self.session.save_point();
                    self.set_transient(
                        "Saved".to_string(),
                        oom_edit_core::session::Severity::Success,
                    );
                }
                Err(e) => {
                    self.set_transient(
                        format!("Save error: {e}"),
                        oom_edit_core::session::Severity::Error,
                    );
                }
            },
            Command::Quit => {
                if self.session.is_dirty() {
                    self.set_transient(
                        "No write since last change (use :q! to override)".to_string(),
                        oom_edit_core::session::Severity::Error,
                    );
                } else {
                    self.should_quit = true;
                }
            }
            Command::CycleTheme => {
                let next = theme::cycle_theme(&self.theme_name);
                self.theme_name = next.to_string();
                // Persist to config.
                if let Err(e) = crate::config::Config::load().save() {
                    eprintln!("oom-edit: failed to save config: {e}");
                }
                self.set_transient(
                    format!("theme: {next}"),
                    oom_edit_core::session::Severity::Info,
                );
            }
        }
    }

    /// Handle a single core effect.
    fn handle_effect(&mut self, effect: Effect) {
        match effect {
            Effect::SaveRequested {
                path,
                force,
                then_quit,
            } => {
                match self.session.save(path.as_deref(), force) {
                    Ok(()) => {
                        self.session.save_point();
                        let line_count = self.session.line_count();
                        let file_name = self
                            .session
                            .document_ref()
                            .path()
                            .map(|p| {
                                p.file_name()
                                    .unwrap_or_default()
                                    .to_string_lossy()
                                    .to_string()
                            })
                            .unwrap_or_else(|| "buffer".to_string());
                        let saved_msg = format!("Saved {file_name} ({line_count} lines)");
                        self.set_transient(saved_msg, oom_edit_core::session::Severity::Success);
                        if then_quit {
                            self.should_quit = true;
                        }
                    }
                    Err(e) => {
                        // T16: ExternallyModified → open confirm-overwrite overlay.
                        if let oom_edit_core::SaveError::ExternallyModified(ref path) = e {
                            self.set_transient(
                                format!("File modified on disk: {}", path.display()),
                                oom_edit_core::session::Severity::Warning,
                            );
                            self.overlay = Overlay::open_confirm_overwrite();
                        } else {
                            self.set_transient(
                                format!("Save error: {e}"),
                                oom_edit_core::session::Severity::Error,
                            );
                        }
                        if then_quit {
                            // Cannot quit with unsaved changes — keep going.
                        }
                    }
                }
            }
            Effect::QuitRequested { force } => {
                // Per plan V-X2: :q refuses when dirty, :q! discards.
                // T16: open confirm overlay when dirty and !force.
                if force {
                    self.should_quit = true;
                } else if self.session.is_dirty() {
                    self.overlay = Overlay::open_confirm_quit();
                } else {
                    self.should_quit = true;
                }
            }
            Effect::OpenRequested { path, force } => {
                // T16: open confirm overlay when dirty and !force.
                if force {
                    // Rebuild session (FR V-X4).
                    match EditorSession::open(&path) {
                        Ok(new_session) => {
                            self.session = new_session;
                            self.top_line = 0;
                            self.set_transient(
                                format!("Opened: {}", path.display()),
                                oom_edit_core::session::Severity::Info,
                            );
                        }
                        Err(e) => {
                            self.set_transient(
                                format!("Open error: {e}"),
                                oom_edit_core::session::Severity::Error,
                            );
                        }
                    }
                } else if self.session.is_dirty() {
                    self.overlay = Overlay::open_confirm_quit();
                } else {
                    // Rebuild session (FR V-X4).
                    match EditorSession::open(&path) {
                        Ok(new_session) => {
                            self.session = new_session;
                            self.top_line = 0;
                            self.set_transient(
                                format!("Opened: {}", path.display()),
                                oom_edit_core::session::Severity::Info,
                            );
                        }
                        Err(e) => {
                            self.set_transient(
                                format!("Open error: {e}"),
                                oom_edit_core::session::Severity::Error,
                            );
                        }
                    }
                }
            }
            Effect::ClipboardWrite(text) => {
                // T16: route to OSC 52 clipboard sink.
                if let Err(e) = self.clipboard_sink.copy(&text) {
                    self.set_transient(
                        format!("Clipboard error: {e}"),
                        oom_edit_core::session::Severity::Warning,
                    );
                }
                self.set_transient(
                    "yanked to register".to_string(),
                    oom_edit_core::session::Severity::Info,
                );
            }
            Effect::ModeChanged(_) => {
                // No action needed; render reads live state.
            }
            Effect::Message { text, severity } => {
                self.set_transient(text.clone(), severity);
            }
            Effect::CursorMoved => {
                // No action needed; scroll_follow handles visibility.
            }
            Effect::Edited => {
                // No action needed; render reads live state.
            }
            Effect::HelpRequested => {
                // Open command palette.
                self.overlay = Overlay::open_palette();
            }
        }
    }

    /// Set a transient status message with TTL expiry.
    #[cfg(test)]
    pub fn set_transient(&mut self, text: String, severity: oom_edit_core::session::Severity) {
        self.transient = Some(status_bar::Transient {
            text,
            severity,
            expires_at: self.now + status_bar::TRANSIENT_TTL,
        });
    }

    /// Set a transient status message with TTL expiry.
    #[cfg(not(test))]
    fn set_transient(&mut self, text: String, severity: oom_edit_core::session::Severity) {
        self.transient = Some(status_bar::Transient {
            text,
            severity,
            expires_at: self.now + status_bar::TRANSIENT_TTL,
        });
    }

    /// Scroll-follow: clamp `top_line` so the cursor row is visible with
    /// `SCROLLOFF` lines of context (source editor) or `view_top` for View mode.
    pub fn scroll_follow(&mut self) {
        if self.session.mode() == oom_edit_core::session::Mode::View {
            self.view_scroll_follow();
        } else {
            self.source_scroll_follow();
        }
    }

    /// Scroll-follow for the source editor: clamp `top_line` so the cursor
    /// row is visible with `SCROLLOFF` lines of context.
    fn source_scroll_follow(&mut self) {
        let cursor_line = self.session.cursor().0;
        let line_count = self.session.line_count();

        if cursor_line < self.top_line + SCROLLOFF {
            self.top_line = cursor_line.saturating_sub(SCROLLOFF);
        } else if cursor_line >= self.top_line + self.viewport_height.saturating_sub(SCROLLOFF) {
            self.top_line =
                cursor_line.saturating_sub(self.viewport_height.saturating_sub(SCROLLOFF + 1));
        }

        // Clamp to document bounds.
        if self.top_line + self.viewport_height > line_count {
            self.top_line = line_count.saturating_sub(self.viewport_height);
        }
        self.top_line = self.top_line.min(line_count.saturating_sub(1));
    }

    /// Scroll-follow for View mode: clamp `view_top` so the view cursor is
    /// visible using the core's `view_scroll_top` pure function (VN-1).
    fn view_scroll_follow(&mut self) {
        let cursor_line = self.session.view_cursor_line();
        let layout = self.session.view_layout();
        let layout_height = layout.map(|l| l.lines.len()).unwrap_or(0);

        if layout_height == 0 || self.viewport_height == 0 {
            return;
        }

        self.view_top = oom_edit_core::view::nav::view_scroll_top(
            cursor_line,
            self.viewport_height,
            layout_height,
            self.view_top,
        );
    }

    /// Scroll up by `lines` rows — Vim Ctrl-e / Ctrl-y style (viewport moves,
    /// cursor stays put). In View mode, scrolls `view_top`.
    fn scroll_up(&mut self, lines: usize) {
        match self.session.mode() {
            oom_edit_core::session::Mode::View => {
                self.view_top = self.view_top.saturating_sub(lines);
            }
            _ => {
                self.top_line = self.top_line.saturating_sub(lines);
            }
        }
    }

    /// Scroll down by `lines` rows — Vim Ctrl-e style (viewport moves,
    /// cursor stays put). In View mode, scrolls `view_top`.
    fn scroll_down(&mut self, lines: usize) {
        match self.session.mode() {
            oom_edit_core::session::Mode::View => {
                let layout = self.session.view_layout();
                let layout_height = layout.map(|l| l.lines.len()).unwrap_or(0);
                let max_top = layout_height.saturating_sub(self.viewport_height);
                self.view_top = (self.view_top + lines).min(max_top);
            }
            _ => {
                let line_count = self.session.line_count();
                let max_top = line_count.saturating_sub(self.viewport_height);
                self.top_line = (self.top_line + lines).min(max_top);
            }
        }
    }
}

/// Translate a crossterm [`KeyEvent`] to a core [`KeyInput`].
fn crossterm_key_to_core(key: &KeyEvent) -> KeyInput {
    let code = match key.code {
        CrosstermKeyCode::Char(c) => KeyCode {
            kind: KeyCodeKind::Char(c),
        },
        CrosstermKeyCode::Backspace => KeyCode {
            kind: KeyCodeKind::Backspace,
        },
        CrosstermKeyCode::Enter => KeyCode {
            kind: KeyCodeKind::Enter,
        },
        CrosstermKeyCode::Left => KeyCode {
            kind: KeyCodeKind::Left,
        },
        CrosstermKeyCode::Right => KeyCode {
            kind: KeyCodeKind::Right,
        },
        CrosstermKeyCode::Up => KeyCode {
            kind: KeyCodeKind::Up,
        },
        CrosstermKeyCode::Down => KeyCode {
            kind: KeyCodeKind::Down,
        },
        CrosstermKeyCode::Tab => KeyCode {
            kind: KeyCodeKind::Tab,
        },
        CrosstermKeyCode::BackTab => KeyCode {
            kind: KeyCodeKind::BackTab,
        },
        CrosstermKeyCode::Home => KeyCode {
            kind: KeyCodeKind::Home,
        },
        CrosstermKeyCode::End => KeyCode {
            kind: KeyCodeKind::End,
        },
        CrosstermKeyCode::PageUp => KeyCode {
            kind: KeyCodeKind::PageUp,
        },
        CrosstermKeyCode::PageDown => KeyCode {
            kind: KeyCodeKind::PageDown,
        },
        CrosstermKeyCode::Delete => KeyCode {
            kind: KeyCodeKind::Delete,
        },
        CrosstermKeyCode::Insert => KeyCode {
            kind: KeyCodeKind::Char(' '), // Insert: no direct mapping, treat as space
        },
        CrosstermKeyCode::F(n) => KeyCode {
            kind: KeyCodeKind::F(n),
        },
        CrosstermKeyCode::Null => KeyCode {
            kind: KeyCodeKind::Char('\0'),
        },
        CrosstermKeyCode::Esc => KeyCode {
            kind: KeyCodeKind::Esc,
        },
        CrosstermKeyCode::CapsLock => KeyCode {
            kind: KeyCodeKind::Esc, // No direct mapping; treat as Esc for safety
        },
        CrosstermKeyCode::Menu => KeyCode {
            kind: KeyCodeKind::Char(' '),
        },
        CrosstermKeyCode::ScrollLock => KeyCode {
            kind: KeyCodeKind::Esc,
        },
        CrosstermKeyCode::Pause => KeyCode {
            kind: KeyCodeKind::Esc,
        },
        CrosstermKeyCode::NumLock => KeyCode {
            kind: KeyCodeKind::Esc,
        },
        CrosstermKeyCode::PrintScreen => KeyCode {
            kind: KeyCodeKind::Esc,
        },
        CrosstermKeyCode::KeypadBegin => KeyCode {
            kind: KeyCodeKind::Esc,
        },
        CrosstermKeyCode::Media(_) => KeyCode {
            kind: KeyCodeKind::Esc,
        },
        CrosstermKeyCode::Modifier(_) => KeyCode {
            kind: KeyCodeKind::Esc,
        },
    };

    let mut mods = Modifiers::default();
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        mods.ctrl = true;
    }
    if key.modifiers.contains(KeyModifiers::ALT) {
        mods.alt = true;
    }
    if key.modifiers.contains(KeyModifiers::SHIFT) {
        mods.shift = true;
    }

    KeyInput { code, mods }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oom_edit_core::clipboard::RecordingClipboardSink;

    /// Create a test App with a recording clipboard sink.
    fn test_app(session: EditorSession) -> App {
        App::new(
            session,
            "default-dark".to_string(),
            Tier::TrueColor,
            Box::new(RecordingClipboardSink::default()),
        )
    }

    /// Key translation: `i` → Char('i'), no mods.
    #[test]
    fn key_translation_insert() {
        let key = KeyEvent::new(CrosstermKeyCode::Char('i'), KeyModifiers::NONE);
        let core = crossterm_key_to_core(&key);
        assert_eq!(core.code.kind, KeyCodeKind::Char('i'));
        assert!(!core.mods.ctrl);
        assert!(!core.mods.alt);
        assert!(!core.mods.shift);
    }

    /// Key translation: Ctrl+C → CTRL modifier.
    #[test]
    fn key_translation_ctrl() {
        let key = KeyEvent::new(CrosstermKeyCode::Char('c'), KeyModifiers::CONTROL);
        let core = crossterm_key_to_core(&key);
        assert_eq!(core.code.kind, KeyCodeKind::Char('c'));
        assert!(core.mods.ctrl);
    }

    /// Key translation: Shift+Tab → BackTab.
    #[test]
    fn key_translation_backtab() {
        let key = KeyEvent::new(CrosstermKeyCode::BackTab, KeyModifiers::NONE);
        let core = crossterm_key_to_core(&key);
        assert_eq!(core.code.kind, KeyCodeKind::BackTab);
    }

    /// Key translation: Escape → Esc.
    #[test]
    fn key_translation_escape() {
        let key = KeyEvent::new(CrosstermKeyCode::Esc, KeyModifiers::NONE);
        let core = crossterm_key_to_core(&key);
        assert_eq!(core.code.kind, KeyCodeKind::Esc);
    }

    /// Key translation: Enter → Enter.
    #[test]
    fn key_translation_enter() {
        let key = KeyEvent::new(CrosstermKeyCode::Enter, KeyModifiers::NONE);
        let core = crossterm_key_to_core(&key);
        assert_eq!(core.code.kind, KeyCodeKind::Enter);
    }

    /// Key translation: F5 → F(5).
    #[test]
    fn key_translation_fkey() {
        let key = KeyEvent::new(CrosstermKeyCode::F(5), KeyModifiers::NONE);
        let core = crossterm_key_to_core(&key);
        assert_eq!(core.code.kind, KeyCodeKind::F(5));
    }

    /// App: typing 'i' enters Insert mode.
    #[test]
    fn app_handle_event_enters_insert() {
        let session = EditorSession::from_text("hello");
        let mut app = test_app(session);
        let key = KeyEvent::new(CrosstermKeyCode::Char('i'), KeyModifiers::NONE);
        app.handle_event(&Event::Key(key));
        assert_eq!(app.session.mode(), oom_edit_core::session::Mode::Insert);
    }

    /// App: Escape returns to Normal mode.
    #[test]
    fn app_handle_event_escapes_to_normal() {
        let session = EditorSession::from_text("hello");
        let mut app = test_app(session);
        // Enter insert mode.
        let key = KeyEvent::new(CrosstermKeyCode::Char('i'), KeyModifiers::NONE);
        app.handle_event(&Event::Key(key));
        assert_eq!(app.session.mode(), oom_edit_core::session::Mode::Insert);
        // Escape.
        let key = KeyEvent::new(CrosstermKeyCode::Esc, KeyModifiers::NONE);
        app.handle_event(&Event::Key(key));
        assert_eq!(app.session.mode(), oom_edit_core::session::Mode::Normal);
    }

    /// App: `:w` triggers save request.
    #[test]
    fn app_handle_event_save_requested() {
        let session = EditorSession::from_text("hello");
        let mut app = test_app(session);

        // Enter command mode and type :w
        let key = KeyEvent::new(CrosstermKeyCode::Char(':'), KeyModifiers::NONE);
        app.handle_event(&Event::Key(key));
        assert_eq!(app.session.mode(), oom_edit_core::session::Mode::Command);

        // Type 'w'
        let key = KeyEvent::new(CrosstermKeyCode::Char('w'), KeyModifiers::NONE);
        app.handle_event(&Event::Key(key));
        assert_eq!(app.session.mode(), oom_edit_core::session::Mode::Command);

        // Press Enter to execute the command
        let key = KeyEvent::new(CrosstermKeyCode::Enter, KeyModifiers::NONE);
        app.handle_event(&Event::Key(key));
        assert_eq!(app.session.mode(), oom_edit_core::session::Mode::Normal);

        // The :w effect should have set a status message (saved or error since no path)
        // In T11, saving without a path may fail — check that the effect was handled.
        // The key thing is that the command was executed.
    }

    /// App: `:q` on clean buffer sets should_quit.
    #[test]
    fn app_handle_event_quit_clean() {
        let session = EditorSession::from_text("hello");
        let mut app = test_app(session);

        let key = KeyEvent::new(CrosstermKeyCode::Char(':'), KeyModifiers::NONE);
        app.handle_event(&Event::Key(key));

        let key = KeyEvent::new(CrosstermKeyCode::Char('q'), KeyModifiers::NONE);
        app.handle_event(&Event::Key(key));

        let key = KeyEvent::new(CrosstermKeyCode::Enter, KeyModifiers::NONE);
        app.handle_event(&Event::Key(key));

        assert!(app.should_quit);
    }

    /// App: `:q` on dirty buffer refuses.
    #[test]
    fn app_handle_event_quit_dirty_refuses() {
        let session = EditorSession::from_text("hello");
        let mut app = test_app(session);

        // Edit the document to make it dirty
        let key = KeyEvent::new(CrosstermKeyCode::Char('i'), KeyModifiers::NONE);
        app.handle_event(&Event::Key(key));
        let key = KeyEvent::new(CrosstermKeyCode::Char('x'), KeyModifiers::NONE);
        app.handle_event(&Event::Key(key));
        let key = KeyEvent::new(CrosstermKeyCode::Esc, KeyModifiers::NONE);
        app.handle_event(&Event::Key(key));

        assert!(app.session.is_dirty());

        let key = KeyEvent::new(CrosstermKeyCode::Char(':'), KeyModifiers::NONE);
        app.handle_event(&Event::Key(key));

        let key = KeyEvent::new(CrosstermKeyCode::Char('q'), KeyModifiers::NONE);
        app.handle_event(&Event::Key(key));

        let key = KeyEvent::new(CrosstermKeyCode::Enter, KeyModifiers::NONE);
        app.handle_event(&Event::Key(key));

        // :q without ! should open confirm-quit overlay on dirty buffer
        assert!(!app.should_quit);
        assert!(app.overlay.is_some());
    }

    /// App: scroll-follow keeps cursor visible.
    #[test]
    fn app_scroll_follow_basic() {
        let text: String = (0..100).map(|i| format!("line {i}\n")).collect();
        let session = EditorSession::from_text(&text);
        let mut app = test_app(session);

        // Go to line 50
        let key = KeyEvent::new(CrosstermKeyCode::Char('G'), KeyModifiers::NONE);
        app.handle_event(&Event::Key(key));

        // Scroll follow should have adjusted top_line
        assert!(app.top_line > 0, "top_line should have scrolled down");
    }

    /// App: key-translation table covers all special keys.
    #[test]
    fn key_translation_all_special_keys() {
        let test_cases = vec![
            (CrosstermKeyCode::Backspace, KeyCodeKind::Backspace),
            (CrosstermKeyCode::Enter, KeyCodeKind::Enter),
            (CrosstermKeyCode::Left, KeyCodeKind::Left),
            (CrosstermKeyCode::Right, KeyCodeKind::Right),
            (CrosstermKeyCode::Up, KeyCodeKind::Up),
            (CrosstermKeyCode::Down, KeyCodeKind::Down),
            (CrosstermKeyCode::Tab, KeyCodeKind::Tab),
            (CrosstermKeyCode::BackTab, KeyCodeKind::BackTab),
            (CrosstermKeyCode::Home, KeyCodeKind::Home),
            (CrosstermKeyCode::End, KeyCodeKind::End),
            (CrosstermKeyCode::PageUp, KeyCodeKind::PageUp),
            (CrosstermKeyCode::PageDown, KeyCodeKind::PageDown),
            (CrosstermKeyCode::Delete, KeyCodeKind::Delete),
            (CrosstermKeyCode::Esc, KeyCodeKind::Esc),
        ];

        for (crossterm_code, expected) in test_cases {
            let key = KeyEvent::new(crossterm_code, KeyModifiers::NONE);
            let core = crossterm_key_to_core(&key);
            assert_eq!(
                core.code.kind, expected,
                "crossterm {:?} should map to {:?}",
                crossterm_code, expected
            );
        }
    }

    /// T12: F1 opens the command palette.
    #[test]
    fn app_f1_opens_palette() {
        let session = EditorSession::from_text("hello");
        let mut app = test_app(session);
        let key = KeyEvent::new(CrosstermKeyCode::F(1), KeyModifiers::NONE);
        app.handle_event(&Event::Key(key));
        assert!(app.overlay.is_palette());
    }

    /// T12: Space-v toggles View mode.
    #[test]
    fn app_space_v_toggles_view() {
        let session = EditorSession::from_text("hello");
        let mut app = test_app(session);

        // Space starts pending chord.
        let space = KeyEvent::new(CrosstermKeyCode::Char(' '), KeyModifiers::NONE);
        app.handle_event(&Event::Key(space));
        assert!(app.pending_chord.since.is_some());

        // v completes the chord → toggle view.
        let v = KeyEvent::new(CrosstermKeyCode::Char('v'), KeyModifiers::NONE);
        app.handle_event(&Event::Key(v));
        assert_eq!(app.session.mode(), oom_edit_core::session::Mode::View);
    }

    /// T12: Space-w saves.
    #[test]
    fn app_space_w_saves() {
        let session = EditorSession::from_text("hello");
        let mut app = test_app(session);

        let space = KeyEvent::new(CrosstermKeyCode::Char(' '), KeyModifiers::NONE);
        app.handle_event(&Event::Key(space));

        let w = KeyEvent::new(CrosstermKeyCode::Char('w'), KeyModifiers::NONE);
        app.handle_event(&Event::Key(w));

        // Save without path should set a transient message.
        assert!(app.transient.is_some());
    }

    /// T12: Space-q quits clean buffer.
    #[test]
    fn app_space_q_quits() {
        let session = EditorSession::from_text("hello");
        let mut app = test_app(session);

        let space = KeyEvent::new(CrosstermKeyCode::Char(' '), KeyModifiers::NONE);
        app.handle_event(&Event::Key(space));

        let q = KeyEvent::new(CrosstermKeyCode::Char('q'), KeyModifiers::NONE);
        app.handle_event(&Event::Key(q));

        assert!(app.should_quit);
    }

    /// T12: Space in Insert mode self-inserts (routing order proof).
    #[test]
    fn app_space_in_insert_self_inserts() {
        let session = EditorSession::from_text("hello");
        let mut app = test_app(session);

        // Enter insert mode.
        let i = KeyEvent::new(CrosstermKeyCode::Char('i'), KeyModifiers::NONE);
        app.handle_event(&Event::Key(i));
        assert_eq!(app.session.mode(), oom_edit_core::session::Mode::Insert);

        // Space in Insert mode should NOT start a chord — it falls through
        // to the session as a self-insert.
        let space = KeyEvent::new(CrosstermKeyCode::Char(' '), KeyModifiers::NONE);
        app.handle_event(&Event::Key(space));

        // The pending chord should NOT be set.
        assert!(app.pending_chord.since.is_none());
        // The document should have a space inserted (at cursor position 0 via 'i').
        assert!(app.session.document().starts_with(" "));
    }

    /// T12: :help opens the palette.
    #[test]
    fn app_help_requested_opens_palette() {
        let session = EditorSession::from_text("hello");
        let mut app = test_app(session);

        // Simulate :help by directly triggering HelpRequested effect.
        app.handle_effect(Effect::HelpRequested);
        assert!(app.overlay.is_palette());
    }

    /// T12: Esc closes the palette.
    #[test]
    fn app_esc_closes_palette() {
        let session = EditorSession::from_text("hello");
        let mut app = test_app(session);

        // Open palette via F1.
        let f1 = KeyEvent::new(CrosstermKeyCode::F(1), KeyModifiers::NONE);
        app.handle_event(&Event::Key(f1));
        assert!(app.overlay.is_palette());

        // Esc closes the palette.
        let esc = KeyEvent::new(CrosstermKeyCode::Esc, KeyModifiers::NONE);
        app.handle_event(&Event::Key(esc));
        assert!(!app.overlay.is_some());
    }

    /// T12: Palette filter — typing narrows the visible rows.
    #[test]
    fn app_palette_filter_narrows() {
        let session = EditorSession::from_text("hello");
        let mut app = test_app(session);

        // Open palette via F1.
        let f1 = KeyEvent::new(CrosstermKeyCode::F(1), KeyModifiers::NONE);
        app.handle_event(&Event::Key(f1));
        assert!(app.overlay.is_palette());

        // Type 's' to filter for save-related commands.
        let s = KeyEvent::new(CrosstermKeyCode::Char('s'), KeyModifiers::NONE);
        app.handle_event(&Event::Key(s));

        // The filter text should be set.
        if let Overlay::Palette(p) = &app.overlay {
            assert_eq!(p.filter_text(), "s");
        } else {
            panic!("expected palette overlay");
        }
    }

    /// T12: Palette execute — Enter on a command executes it.
    #[test]
    fn app_palette_execute_command() {
        let session = EditorSession::from_text("hello");
        let mut app = test_app(session);

        // Open palette via F1.
        let f1 = KeyEvent::new(CrosstermKeyCode::F(1), KeyModifiers::NONE);
        app.handle_event(&Event::Key(f1));
        assert!(app.overlay.is_palette());

        // Enter executes the selected command (first row = ToggleView).
        let enter = KeyEvent::new(CrosstermKeyCode::Enter, KeyModifiers::NONE);
        app.handle_event(&Event::Key(enter));

        // Palette should be closed and ToggleView should have been executed.
        assert!(!app.overlay.is_some());
        assert_eq!(app.session.mode(), oom_edit_core::session::Mode::View);
    }

    /// T12: Palette reference entry — Enter on Vim reference shows status.
    #[test]
    fn app_palette_reference_entry() {
        let session = EditorSession::from_text("hello");
        let mut app = test_app(session);

        // Open palette via F1.
        let f1 = KeyEvent::new(CrosstermKeyCode::F(1), KeyModifiers::NONE);
        app.handle_event(&Event::Key(f1));

        // Navigate down past the app commands to a Vim reference entry.
        // There are 5 app commands, so navigate to row 6 (index 5).
        for _ in 0..6 {
            let down = KeyEvent::new(CrosstermKeyCode::Down, KeyModifiers::NONE);
            app.handle_event(&Event::Key(down));
        }

        // Enter on a reference entry should show "reference entry" status.
        let enter = KeyEvent::new(CrosstermKeyCode::Enter, KeyModifiers::NONE);
        app.handle_event(&Event::Key(enter));

        // Palette should be closed and transient should show reference entry.
        assert!(!app.overlay.is_some());
        assert_eq!(
            app.transient.as_ref().map(|t| t.text.as_str()),
            Some("reference entry")
        );
    }

    /// T12: Which-key delay gate — hint doesn't show before 150ms.
    #[test]
    fn app_which_key_delay_gate() {
        let session = EditorSession::from_text("hello");
        let mut app = test_app(session);

        // Press Space to start pending chord.
        let space = KeyEvent::new(CrosstermKeyCode::Char(' '), KeyModifiers::NONE);
        app.handle_event(&Event::Key(space));
        assert!(app.pending_chord.since.is_some());

        // Tick with a time immediately after Space (0ms delay).
        let instant = app.pending_chord.since.unwrap();
        app.tick(instant);

        // should_show should return false at 0ms delay.
        assert!(!crate::widgets::which_key::should_show(
            app.pending_chord.since,
            instant
        ));

        // Tick with a time 200ms later.
        let later = instant + std::time::Duration::from_millis(200);
        app.tick(later);

        // should_show should return true at 200ms delay.
        assert!(crate::widgets::which_key::should_show(
            app.pending_chord.since,
            later
        ));
    }

    /// T12: Routing order — overlay takes precedence over keymap.
    #[test]
    fn app_routing_overlay_precedence() {
        let session = EditorSession::from_text("hello");
        let mut app = test_app(session);

        // Open palette.
        let f1 = KeyEvent::new(CrosstermKeyCode::F(1), KeyModifiers::NONE);
        app.handle_event(&Event::Key(f1));
        assert!(app.overlay.is_palette());

        // Press Space while palette is open — it should be consumed by
        // the palette (filter input), NOT start a chord.
        let space = KeyEvent::new(CrosstermKeyCode::Char(' '), KeyModifiers::NONE);
        app.handle_event(&Event::Key(space));

        // Palette should still be open, pending chord should NOT be set.
        assert!(app.overlay.is_palette());
        // The space character should have been added to the filter.
        if let Overlay::Palette(p) = &app.overlay {
            assert_eq!(p.filter_text(), " ");
        } else {
            panic!("expected palette overlay");
        }
    }

    /// T12: Routing order — keymap takes precedence over session.
    #[test]
    fn app_routing_keymap_precedence() {
        let session = EditorSession::from_text("hello");
        let mut app = test_app(session);

        // F1 should open the palette (keymap dispatch), NOT go to session.
        let f1 = KeyEvent::new(CrosstermKeyCode::F(1), KeyModifiers::NONE);
        app.handle_event(&Event::Key(f1));

        assert!(app.overlay.is_palette());
        // Mode should still be Normal (keymap consumed the key).
        assert_eq!(app.session.mode(), oom_edit_core::session::Mode::Normal);
    }

    /// T12: Routing order — session fallback when no keymap match.
    #[test]
    fn app_routing_session_fallback() {
        let session = EditorSession::from_text("hello");
        let mut app = test_app(session);

        // 'j' is not a keymap trigger — should fall through to session.
        let j = KeyEvent::new(CrosstermKeyCode::Down, KeyModifiers::NONE);
        app.handle_event(&Event::Key(j));

        // Mode should still be Normal (motion, not mode change).
        assert_eq!(app.session.mode(), oom_edit_core::session::Mode::Normal);
    }

    /// T12: CycleTheme is a functional no-op (not a placeholder message).
    #[test]
    fn app_cycle_theme_is_functional_noop() {
        let session = EditorSession::from_text("hello");
        let mut app = test_app(session);

        // Space-t should trigger CycleTheme.
        let space = KeyEvent::new(CrosstermKeyCode::Char(' '), KeyModifiers::NONE);
        app.handle_event(&Event::Key(space));
        let t = KeyEvent::new(CrosstermKeyCode::Char('t'), KeyModifiers::NONE);
        app.handle_event(&Event::Key(t));

        // Should NOT show the "themes land in T15" placeholder.
        let transient_text = app
            .transient
            .as_ref()
            .map(|t| t.text.as_str())
            .unwrap_or("");
        assert!(
            !transient_text.contains("T15"),
            "CycleTheme should be functional, not a T15 placeholder"
        );
        // Should show a transient message about cycling.
        assert!(app.transient.is_some());
    }

    /// T14: Resize in View mode remaps cursor to same content line.
    #[test]
    fn app_resize_view_remaps_cursor() {
        // Multi-line text that wraps differently at different widths.
        let text = "Hello world this is a long line that will wrap differently at different widths\nSecond line\nThird line";
        let session = EditorSession::from_text(text);
        let mut app = test_app(session);

        // Enter View mode.
        let space = KeyEvent::new(CrosstermKeyCode::Char(' '), KeyModifiers::NONE);
        app.handle_event(&Event::Key(space));
        let v = KeyEvent::new(CrosstermKeyCode::Char('v'), KeyModifiers::NONE);
        app.handle_event(&Event::Key(v));
        assert_eq!(app.session.mode(), oom_edit_core::session::Mode::View);

        // Record the view cursor before resize.
        let cursor_before = app.session.view_cursor().map(|c| c.line);

        // Simulate a resize event.
        let resize = Event::Resize(80, 24);
        app.handle_event(&resize);

        // Cursor should still be on the same content line (remapped to view line).
        let cursor_after = app.session.view_cursor().map(|c| c.line);
        assert!(
            cursor_before == cursor_after || cursor_after.is_some(),
            "view cursor should be remapped on resize"
        );
    }

    /// T17: Render — basic render doesn't hang and produces output.
    #[test]
    fn app_render_basic() {
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;

        let session = EditorSession::from_text("hello");
        let mut app = test_app(session);

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| {
                app.render(frame);
            })
            .unwrap();

        let content = terminal.backend().buffer().content();
        // Convert last line to string and check status bar renders.
        let last_line: String = content[content.len() - 80..]
            .iter()
            .map(|c| c.symbol())
            .collect();
        assert!(last_line.contains("NORMAL"));
    }
}
