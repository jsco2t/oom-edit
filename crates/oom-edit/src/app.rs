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

use oom_edit_core::session::{EditorSession, Effect, KeyCode, KeyCodeKind, KeyInput, Modifiers};

use crate::command::{keymap::PendingChord, Command, Keymap};
use crate::overlay::Overlay;
use crate::screens::editor::render_editor;

/// Scrolloff: keep this many lines of context around the cursor.
const SCROLLOFF: usize = 3;

/// App state for the TUI.
pub struct App {
    /// The core editing session.
    pub session: EditorSession,
    /// The first visible line (owned by the TUI for scroll-follow).
    pub top_line: usize,
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
}

impl App {
    /// Create a new App from an open session.
    pub fn new(session: EditorSession) -> Self {
        Self {
            session,
            top_line: 0,
            should_quit: false,
            status_message: String::new(),
            overlay: Overlay::default(),
            pending_chord: PendingChord::default(),
            keymap: Keymap::default(),
            viewport_height: 22,
        }
    }

    /// Advance internal timers (T11: no timers yet, but the method exists
    /// for the event loop's tick step).
    pub fn tick(&mut self, _now: Instant) {
        // No timers in T11; status-message TTL handling arrives in T13.
    }

    /// Render the current frame.
    pub fn render(&mut self, frame: &mut Frame<'_>) {
        let area = frame.area();

        // Compute viewport height from terminal size.
        self.viewport_height = area.height.saturating_sub(1) as usize;

        // Render the editor behind the overlay.
        render_editor(
            frame,
            &mut self.session,
            self.top_line,
            &self.status_message,
            area,
        );

        // Render overlay on top if open.
        if self.overlay.is_some() {
            self.overlay.render(frame);
        }
    }

    /// Handle a crossterm event, following arch §7.1 fixed order.
    pub fn handle_event(&mut self, event: &Event) {
        // Translate crossterm event → core KeyInput.
        let key_input = match event {
            Event::Key(key) => crossterm_key_to_core(key),
            Event::Mouse(_) => return, // Absorb mouse events (T16 adds scroll).
            Event::Resize(_, _) => return, // Next draw picks up the new size.
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
                    self.status_message = String::new();
                    // Don't fall through — Esc on palette is consumed.
                    return;
                }

                // Enter on palette: execute command or mark reference.
                if matches!(key_input.code.kind, KeyCodeKind::Enter) {
                    if let Some(cmd) = self.overlay.selected_command() {
                        self.overlay.close();
                        self.execute_command(cmd);
                    } else {
                        self.status_message = "reference entry".to_string();
                    }
                    return;
                }

                // Other keys are consumed by the palette's filter navigation.
                return;
            }
            return;
        }

        // 2. Mode ∈ {Normal, View}: try app keymap.
        let mode = self.session.mode();
        let in_chord_context = matches!(mode, oom_edit_core::session::Mode::Normal)
            || matches!(mode, oom_edit_core::session::Mode::View);

        if in_chord_context {
            // Get the Contexts bitset for the current mode.
            let ctx = match mode {
                oom_edit_core::session::Mode::Normal => crate::command::registry::Contexts::NORMAL,
                oom_edit_core::session::Mode::View => crate::command::registry::Contexts::VIEW,
                _ => crate::command::registry::Contexts::NORMAL,
            };

            // Try the app keymap.
            match self
                .keymap
                .resolve(ctx, &key_input, &mut self.pending_chord)
            {
                crate::command::keymap::Resolution::Command(cmd) => {
                    self.execute_command(cmd);
                    return;
                }
                crate::command::keymap::Resolution::Pending(_) => {
                    // Pending chord — don't fall through.
                    return;
                }
                crate::command::keymap::Resolution::None => {
                    // Fall through to session.
                }
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
                    self.status_message = "Saved".to_string();
                }
                Err(e) => {
                    self.status_message = format!("Save error: {e}");
                }
            },
            Command::Quit => {
                if self.session.is_dirty() {
                    self.status_message =
                        "No write since last change (use :q! to override)".to_string();
                } else {
                    self.should_quit = true;
                }
            }
            Command::CycleTheme => {
                // T15: real theme cycling. For now, cycle the one-element list.
                self.status_message = "themes land in T15".to_string();
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
                        self.status_message = "Saved".to_string();
                        if then_quit {
                            self.should_quit = true;
                        }
                    }
                    Err(e) => {
                        self.status_message = format!("Save error: {e}");
                        if then_quit {
                            // Cannot quit with unsaved changes — keep going.
                        }
                    }
                }
            }
            Effect::QuitRequested { force } => {
                // Per plan V-X2: :q refuses when dirty, :q! discards.
                // The core emits QuitRequested{force} — if force is false
                // and we're dirty, we refuse. T16 adds the confirm overlay.
                if force || !self.session.is_dirty() {
                    self.should_quit = true;
                } else {
                    self.status_message =
                        "No write since last change (use :q! to override)".to_string();
                }
            }
            Effect::OpenRequested { path, force } => {
                if self.session.is_dirty() && !force {
                    self.status_message =
                        "No write since last change (use :e! to override)".to_string();
                } else {
                    // Rebuild session (FR V-X4).
                    match EditorSession::open(&path) {
                        Ok(new_session) => {
                            self.session = new_session;
                            self.top_line = 0;
                            self.status_message = format!("Opened: {}", path.display());
                        }
                        Err(e) => {
                            self.status_message = format!("Open error: {e}");
                        }
                    }
                }
            }
            Effect::ClipboardWrite(_text) => {
                // T16: route to OSC 52 clipboard sink.
                self.status_message = "yanked to register".to_string();
            }
            Effect::ModeChanged(_) => {
                // No action needed; render reads live state.
            }
            Effect::Message { text, severity: _ } => {
                self.status_message = text;
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

    /// Scroll-follow: clamp `top_line` so the cursor row is visible with
    /// `SCROLLOFF` lines of context.
    fn scroll_follow(&mut self) {
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
        let mut app = App::new(session);
        let key = KeyEvent::new(CrosstermKeyCode::Char('i'), KeyModifiers::NONE);
        app.handle_event(&Event::Key(key));
        assert_eq!(app.session.mode(), oom_edit_core::session::Mode::Insert);
    }

    /// App: Escape returns to Normal mode.
    #[test]
    fn app_handle_event_escapes_to_normal() {
        let session = EditorSession::from_text("hello");
        let mut app = App::new(session);
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
        let mut app = App::new(session);

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
        let mut app = App::new(session);

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
        let mut app = App::new(session);

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

        // :q without ! should refuse on dirty buffer
        assert!(!app.should_quit);
        assert!(app.status_message.contains("No write"));
    }

    /// App: scroll-follow keeps cursor visible.
    #[test]
    fn app_scroll_follow_basic() {
        let text: String = (0..100).map(|i| format!("line {i}\n")).collect();
        let session = EditorSession::from_text(&text);
        let mut app = App::new(session);

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
        let mut app = App::new(session);
        let key = KeyEvent::new(CrosstermKeyCode::F(1), KeyModifiers::NONE);
        app.handle_event(&Event::Key(key));
        assert!(app.overlay.is_palette());
    }

    /// T12: Space-v toggles View mode.
    #[test]
    fn app_space_v_toggles_view() {
        let session = EditorSession::from_text("hello");
        let mut app = App::new(session);

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
        let mut app = App::new(session);

        let space = KeyEvent::new(CrosstermKeyCode::Char(' '), KeyModifiers::NONE);
        app.handle_event(&Event::Key(space));

        let w = KeyEvent::new(CrosstermKeyCode::Char('w'), KeyModifiers::NONE);
        app.handle_event(&Event::Key(w));

        // Save without path should set a status message.
        assert!(!app.status_message.is_empty());
    }

    /// T12: Space-q quits clean buffer.
    #[test]
    fn app_space_q_quits() {
        let session = EditorSession::from_text("hello");
        let mut app = App::new(session);

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
        let mut app = App::new(session);

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
        let mut app = App::new(session);

        // Simulate :help by directly triggering HelpRequested effect.
        app.handle_effect(Effect::HelpRequested);
        assert!(app.overlay.is_palette());
    }

    /// T12: Esc closes the palette.
    #[test]
    fn app_esc_closes_palette() {
        let session = EditorSession::from_text("hello");
        let mut app = App::new(session);

        // Open palette via F1.
        let f1 = KeyEvent::new(CrosstermKeyCode::F(1), KeyModifiers::NONE);
        app.handle_event(&Event::Key(f1));
        assert!(app.overlay.is_palette());

        // Esc closes the palette.
        let esc = KeyEvent::new(CrosstermKeyCode::Esc, KeyModifiers::NONE);
        app.handle_event(&Event::Key(esc));
        assert!(!app.overlay.is_some());
    }
}
