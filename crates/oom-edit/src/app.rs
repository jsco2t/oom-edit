//! App — the single source of truth for the running TUI.
//!
//! Holds the tab stack, scroll positions, last status message, overlay
//! state, and the quit flag. After each event: drain effects, scroll-follow,
//! update status message.
//!
//! ## Key routing order (arch §7.1)
//!
//! 1. Overlay open → overlay's key handler (take-and-return-bool).
//! 2. Mode ∈ {Normal, View}: try app keymap — `F1`, `Space`-leader chords, `g`-chords.
//!    On match → `execute_command`. No match → fall through.
//! 3. Everything else → active session's `handle_key(key)`, then drain `Effect`s.

use std::time::Instant;

use crossterm::event::{Event, KeyCode as CrosstermKeyCode, KeyEvent, KeyModifiers};
use ratatui::Frame;

use oom_edit_core::clipboard::ClipboardSink;
use oom_edit_core::session::{EditorSession, Effect, KeyCode, KeyCodeKind, KeyInput, Modifiers};

use crossterm::event::MouseEventKind;

use crate::command::{keymap::PendingChord, Command, Keymap};
use crate::overlay::Overlay;
use crate::screens::editor::{render_editor, render_status_row};
use crate::screens::view::render_view;
use crate::theme::{self, Theme, Tier};
use crate::widgets::status_bar;
use crate::widgets::which_key;

/// Scrolloff: keep this many lines of context around the cursor.
const SCROLLOFF: usize = 3;

fn body_height(total_height: u16, has_multiple_tabs: bool) -> u16 {
    let tab_bar_height = u16::from(has_multiple_tabs);
    total_height.saturating_sub(tab_bar_height + 1)
}

/// A single tab entry: an [`EditorSession`] with per-tab UI state.
pub(crate) struct TabEntry {
    /// The core editing session for this tab.
    session: EditorSession,
    /// The first visible line (owned by the TUI for scroll-follow).
    top_line: usize,
    /// The first visible view line (owned by the TUI for scroll-follow in View mode).
    view_top: usize,
}

impl TabEntry {
    fn new(session: EditorSession) -> Self {
        Self {
            session,
            top_line: 0,
            view_top: 0,
        }
    }

    /// Get a mutable reference to the session.
    pub fn session_mut(&mut self) -> &mut EditorSession {
        &mut self.session
    }
}

/// App state for the TUI.
pub struct App {
    /// The tab stack. Each tab is an independent [`EditorSession`] with its own
    /// scroll position and UI state.
    tabs: Vec<TabEntry>,
    /// Index of the currently active tab.
    active_tab: usize,
    /// Whether the app should quit.
    pub should_quit: bool,
    /// The last status message to display in the status bar.
    status_message: String,
    /// The active overlay (palette, confirm, etc.).
    overlay: Overlay,
    /// Pending Space-chord state.
    pub pending_chord: PendingChord,
    /// Pending `g`-chord state (gt/gT for tab navigation).
    pending_g: bool,
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
    /// Create a new App from an open session (starts with one tab).
    pub fn new(
        session: EditorSession,
        theme_name: String,
        tier: Tier,
        clipboard_sink: Box<dyn ClipboardSink>,
    ) -> Self {
        Self {
            tabs: vec![TabEntry::new(session)],
            active_tab: 0,
            should_quit: false,
            status_message: String::new(),
            overlay: Overlay::default(),
            pending_chord: PendingChord::default(),
            pending_g: false,
            keymap: Keymap::default(),
            viewport_height: 22,
            now: Instant::now(),
            transient: None,
            theme_name,
            tier,
            clipboard_sink,
        }
    }

    /// Get a reference to the active tab entry.
    fn active(&self) -> Option<&TabEntry> {
        self.tabs.get(self.active_tab)
    }

    /// Get a mutable reference to the active tab entry.
    pub fn active_mut(&mut self) -> Option<&mut TabEntry> {
        self.tabs.get_mut(self.active_tab)
    }

    /// Set the overlay (test-only).
    #[cfg(test)]
    pub fn set_overlay(&mut self, overlay: Overlay) {
        self.overlay = overlay;
    }

    /// Get a reference to the active session.
    fn session(&self) -> Option<&EditorSession> {
        self.active().map(|t| &t.session)
    }

    /// Get a mutable reference to the active session.
    #[expect(dead_code)]
    fn session_mut(&mut self) -> Option<&mut EditorSession> {
        self.active_mut().map(|t| &mut t.session)
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

    /// Number of open tabs.
    pub fn tab_count(&self) -> usize {
        self.tabs.len()
    }

    /// Whether there is more than one tab open.
    pub fn has_multiple_tabs(&self) -> bool {
        self.tabs.len() > 1
    }

    /// Check if any tab is dirty.
    pub fn any_tab_dirty(&self) -> bool {
        self.tabs.iter().any(|t| t.session.is_dirty())
    }

    /// Get the active session's document reference.
    #[expect(dead_code)]
    pub fn document_ref(&self) -> Option<&oom_edit_core::document::Document> {
        self.session().map(|s| s.document_ref())
    }

    /// Render the current frame.
    pub fn render(&mut self, frame: &mut Frame<'_>) {
        let area = frame.area();
        let active_theme = theme::get_theme(&self.theme_name);

        // Compute viewport height from terminal size.
        // When >1 tab: tab bar (1) + body + status (1).
        // When 1 tab: body + status (1).
        let tab_bar_height = if self.has_multiple_tabs() { 1 } else { 0 };
        let status_height: u16 = 1;
        let body_height = body_height(area.height, self.has_multiple_tabs());

        self.viewport_height = body_height as usize;

        let mut draw_y = area.y;

        // Render tab bar if >1 tab.
        if tab_bar_height > 0 {
            let tab_area = ratatui::layout::Rect {
                x: area.x,
                y: draw_y,
                width: area.width,
                height: tab_bar_height,
            };
            render_tab_bar(
                frame,
                &self.tabs,
                self.active_tab,
                tab_area,
                active_theme,
                self.tier,
            );
            draw_y += tab_bar_height;
        }

        // Compute body area (after tab bar).
        let body_area = ratatui::layout::Rect {
            x: area.x,
            y: draw_y,
            width: area.width,
            height: body_height,
        };

        // Compute status area.
        let status_area = ratatui::layout::Rect {
            x: area.x,
            y: draw_y + body_height,
            width: area.width,
            height: status_height,
        };

        // Render the appropriate screen behind the overlay.
        if let Some(ref mut entry) = self.tabs.get_mut(self.active_tab) {
            if entry.session.mode() == oom_edit_core::session::Mode::View {
                render_view(
                    frame,
                    &mut entry.session,
                    entry.view_top,
                    body_area,
                    active_theme,
                    self.tier,
                );
            } else {
                render_editor(
                    frame,
                    &mut entry.session,
                    entry.top_line,
                    body_area,
                    active_theme,
                    self.tier,
                );
            }
        }

        // Render status row.
        if let Some(entry) = self.active() {
            render_status_row(
                frame,
                &entry.session,
                self.transient.as_ref(),
                self.overlay.hints(),
                status_area,
                active_theme,
                self.tier,
            );
        }

        // Render which-key hint bar if conditions are met.
        self.render_which_key(frame, area);

        // Render overlay on top if open.
        if self.overlay.is_some() {
            self.overlay.render(frame, active_theme, self.tier);
        }
    }

    /// Render the which-key hint bar.
    ///
    /// Pure gate + pure build + thin render: the which-key popup appears
    /// only after 150ms of pending Space prefix, in Normal/View mode,
    /// and only when there are ≥2 continuations.
    /// Return the [`Contexts`] bitset for the current session mode.
    fn mode_context(&self) -> crate::command::registry::Contexts {
        match self.session().map(|s| s.mode()) {
            Some(oom_edit_core::session::Mode::Normal) => {
                crate::command::registry::Contexts::NORMAL
            }
            Some(oom_edit_core::session::Mode::View) => crate::command::registry::Contexts::VIEW,
            _ => crate::command::registry::Contexts::NORMAL,
        }
    }

    /// Return true when the current mode supports Space-chord and g-chord keymaps.
    fn in_chord_context(&self) -> bool {
        self.session()
            .map(|s| {
                matches!(
                    s.mode(),
                    oom_edit_core::session::Mode::Normal | oom_edit_core::session::Mode::View
                )
            })
            .unwrap_or(false)
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

    /// Get the active tab's top_line.
    fn top_line(&self) -> usize {
        self.active().map(|t| t.top_line).unwrap_or(0)
    }

    /// Set the active tab's top_line.
    fn set_top_line(&mut self, val: usize) {
        if let Some(entry) = self.tabs.get_mut(self.active_tab) {
            entry.top_line = val;
        }
    }

    /// Get the active tab's view_top.
    #[expect(dead_code)]
    fn view_top(&self) -> usize {
        self.active().map(|t| t.view_top).unwrap_or(0)
    }

    /// Set the active tab's view_top.
    fn set_view_top(&mut self, val: usize) {
        if let Some(entry) = self.tabs.get_mut(self.active_tab) {
            entry.view_top = val;
        }
    }

    /// Handle a crossterm event, following arch §7.1 fixed order.
    pub fn handle_event(&mut self, event: &Event) {
        // Handle resize events — rebuild view layout on width change.
        if let Event::Resize(_width, height) = event {
            // Clamp viewport height using the same chrome rows as render().
            self.viewport_height = body_height(*height, self.has_multiple_tabs()) as usize;
            // View mode: remap cursor from edit coordinates so it stays on
            // the same content line after re-wrap (FR-3.1).
            if self.session().map(|s| s.mode()) == Some(oom_edit_core::session::Mode::View) {
                if let Some(ref mut entry) = self.tabs.get_mut(self.active_tab) {
                    let text = entry.session.document();
                    let (edit_line, edit_col) =
                        match (entry.session.view_cursor(), entry.session.view_layout()) {
                            (Some(cursor), Some(layout)) => {
                                oom_edit_core::view::nav::leave_view(&cursor, layout, &text)
                            }
                            _ => entry.session.cursor(),
                        };
                    // Force layout rebuild so enter_view has a layout to work with.
                    entry.session.render_view(*_width);
                    entry
                        .session
                        .remap_view_cursor_from_edit(edit_line, edit_col);
                }
                self.scroll_follow();
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

            // Handle 'g' prefix for tab navigation (gt/gT).
            if self.pending_g {
                self.pending_g = false;
                match key_input.code.kind {
                    KeyCodeKind::Char('t')
                        if !key_input.mods.ctrl && !key_input.mods.alt && !key_input.mods.shift =>
                    {
                        if self.tab_count() > 1 {
                            self.execute_command(Command::NextTab);
                        }
                        return;
                    }
                    KeyCodeKind::Char('T')
                        if !key_input.mods.ctrl && !key_input.mods.alt && !key_input.mods.shift =>
                    {
                        if self.tab_count() > 1 {
                            self.execute_command(Command::PrevTab);
                        }
                        return;
                    }
                    _ => {
                        // Not a g-chord we handle — fall through to engine.
                    }
                }
            }

            // Handle Space+digit for direct tab jump (Space 1..9).
            // If pending_chord.since is set, we're in a Space-pending state.
            // Check if the next key is a digit — if so, jump to that tab.
            // Otherwise, let the keymap handle it as a regular Space-chord.
            if self.pending_chord.since.is_some() {
                use oom_edit_core::session::KeyCodeKind;
                if let KeyCodeKind::Char(digit) = key_input.code.kind {
                    if !key_input.mods.ctrl && !key_input.mods.alt && !key_input.mods.shift {
                        let tab_num = digit.to_digit(9).unwrap_or(0) as usize;
                        if tab_num >= 1 && tab_num <= self.tab_count() {
                            self.pending_chord.reset();
                            self.jump_to_tab(tab_num - 1);
                            return;
                        }
                    }
                }
            }

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

            // Check if this key starts a 'g' chord (only in Normal/View).
            if let KeyCodeKind::Char('g') = key_input.code.kind {
                if !key_input.mods.ctrl && !key_input.mods.alt && !key_input.mods.shift {
                    self.pending_g = true;
                    return;
                }
            }
        }

        // 3. Everything else → session.handle_key(key).
        let effects = if let Some(ref mut entry) = self.tabs.get_mut(self.active_tab) {
            entry.session.handle_key(key_input)
        } else {
            Vec::new()
        };

        // Drain effects.
        for effect in effects {
            self.handle_effect(effect);
        }

        // Scroll-follow: clamp top_line so the cursor is visible.
        self.scroll_follow();
    }

    /// Open a new tab with the given file path.
    fn open_tab(&mut self, path: &std::path::Path) {
        match EditorSession::open(path) {
            Ok(session) => {
                let idx = self.tabs.len();
                self.tabs.push(TabEntry::new(session));
                self.active_tab = idx;
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

    /// Close the active tab (or a specific tab by index).
    fn close_tab(&mut self, index: Option<usize>, force: bool) {
        let idx = index.unwrap_or(self.active_tab);
        if idx >= self.tabs.len() {
            return;
        }

        // If force, just close. Otherwise check dirty.
        if !force {
            let is_dirty = self.tabs[idx].session.is_dirty();
            if is_dirty {
                self.overlay = Overlay::open_confirm_quit();
                // Store the tab close intent in the overlay state.
                // We'll handle it when the confirm result comes back.
                return;
            }
        }

        self.do_close_tab(idx);
    }

    /// Actually close a tab at the given index (no dirty check).
    fn do_close_tab(&mut self, idx: usize) {
        self.tabs.remove(idx);
        // Adjust active_tab if needed.
        if self.active_tab >= self.tabs.len() {
            self.active_tab = self.tabs.len().saturating_sub(1);
        }
        // If no tabs left, quit.
        if self.tabs.is_empty() {
            self.should_quit = true;
        }
    }

    /// Switch to tab by 1-based index.
    fn jump_to_tab(&mut self, index: usize) {
        // Convert 1-based to 0-based.
        let idx = index.saturating_sub(1);
        if idx < self.tabs.len() {
            self.active_tab = idx;
        }
    }

    /// Next tab (wrap).
    fn next_tab(&mut self) {
        if self.tabs.len() <= 1 {
            return;
        }
        self.active_tab = (self.active_tab + 1) % self.tabs.len();
    }

    /// Previous tab (wrap).
    fn prev_tab(&mut self) {
        if self.tabs.len() <= 1 {
            return;
        }
        self.active_tab = (self.active_tab + self.tabs.len() - 1) % self.tabs.len();
    }

    /// Quit all tabs.
    fn quit_all(&mut self, force: bool) {
        if !force && self.any_tab_dirty() {
            // Show dirty tabs message.
            let dirty_count = self.tabs.iter().filter(|t| t.session.is_dirty()).count();
            self.set_transient(
                format!("{dirty_count} unsaved tab(s) — use :qa! to discard"),
                oom_edit_core::session::Severity::Error,
            );
            return;
        }
        self.should_quit = true;
    }

    /// Execute the result of a confirm overlay.
    fn execute_confirm_result(&mut self, result: crate::overlay::ConfirmResult) {
        match result {
            crate::overlay::ConfirmResult::Confirm => {
                // ConfirmQuit: save and quit.
                // ConfirmOverwrite: overwrite file (don't quit).
                let is_overwrite = matches!(self.overlay, Overlay::ConfirmOverwrite(_));
                if let Some(ref mut entry) = self.tabs.get_mut(self.active_tab) {
                    match entry.session.save(None, true) {
                        Ok(()) => {
                            entry.session.save_point();
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
            }
            crate::overlay::ConfirmResult::Quit => {
                // ConfirmQuit: quit without saving.
                self.should_quit = true;
            }
            crate::overlay::ConfirmResult::Reload => {
                // ConfirmOverwrite: reload file from disk.
                if let Some(entry) = self.active() {
                    if let Some(path) = entry.session.document_ref().path() {
                        match EditorSession::open(path) {
                            Ok(new_session) => {
                                if let Some(ref mut entry) = self.tabs.get_mut(self.active_tab) {
                                    entry.session = new_session;
                                    entry.top_line = 0;
                                    self.set_transient(
                                        "Reloaded from disk".to_string(),
                                        oom_edit_core::session::Severity::Info,
                                    );
                                }
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
                if let Some(ref mut entry) = self.tabs.get_mut(self.active_tab) {
                    entry.session.toggle_view();
                }
            }
            Command::Help => {
                // Open the command palette.
                self.overlay = Overlay::open_palette();
            }
            Command::Save => {
                if let Some(ref mut entry) = self.tabs.get_mut(self.active_tab) {
                    match entry.session.save(None, false) {
                        Ok(()) => {
                            entry.session.save_point();
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
                    }
                }
            }
            Command::Quit => {
                // :q / Space q closes the active tab.
                // Last tab closing = app quit (with dirty check).
                let is_dirty = self.session().map(|s| s.is_dirty()).unwrap_or(false);
                if is_dirty {
                    self.set_transient(
                        "No write since last change (use :q! to override)".to_string(),
                        oom_edit_core::session::Severity::Error,
                    );
                } else if self.tab_count() == 1 {
                    // Last tab, clean → quit.
                    self.should_quit = true;
                } else {
                    // Close this tab, switch to another.
                    self.do_close_tab(self.active_tab);
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
            Command::NextTab => self.next_tab(),
            Command::PrevTab => self.prev_tab(),
            Command::JumpToTab => {
                // JumpToTab without count is a no-op; use {count}gt instead.
                // The count is handled in handle_event via the g-chord system.
            }
            Command::TabNew => {
                // :tabnew without path opens a new buffer.
                // For now, open a new buffer with empty text.
                if let Some(entry) = self.active() {
                    let path = entry.session.document_ref().path().map(|p| p.to_path_buf());
                    // Open in the same directory as the current file, if any.
                    let default_path = path
                        .as_deref()
                        .unwrap_or(std::path::Path::new("untitled.md"));
                    self.open_tab(default_path);
                }
            }
            Command::TabClose => {
                self.close_tab(None, false);
            }
            Command::QuitAll => {
                self.quit_all(false);
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
                if let Some(ref mut entry) = self.tabs.get_mut(self.active_tab) {
                    match entry.session.save(path.as_deref(), force) {
                        Ok(()) => {
                            entry.session.save_point();
                            let line_count = entry.session.line_count();
                            let file_name = entry
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
                            self.set_transient(
                                saved_msg,
                                oom_edit_core::session::Severity::Success,
                            );
                            if then_quit {
                                // Close this tab (quit after save).
                                if self.tab_count() == 1 {
                                    self.should_quit = true;
                                } else {
                                    self.do_close_tab(self.active_tab);
                                }
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
            }
            Effect::QuitRequested { force } => {
                // Per plan V-X2: :q refuses when dirty, :q! discards.
                // Closing active tab; last tab = quit.
                if force {
                    if self.tab_count() == 1 {
                        self.should_quit = true;
                    } else {
                        self.do_close_tab(self.active_tab);
                    }
                } else if self.session().map(|s| s.is_dirty()).unwrap_or(false) {
                    self.overlay = Overlay::open_confirm_quit();
                } else if self.tab_count() == 1 {
                    self.should_quit = true;
                } else {
                    self.do_close_tab(self.active_tab);
                }
            }
            Effect::OpenRequested { path, force } => {
                // T16: open confirm overlay when dirty and !force.
                // For tabs: opening a file replaces the current tab, or opens in new tab.
                if force {
                    match EditorSession::open(&path) {
                        Ok(new_session) => {
                            if let Some(ref mut entry) = self.tabs.get_mut(self.active_tab) {
                                entry.session = new_session;
                                entry.top_line = 0;
                            }
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
                } else if self.session().map(|s| s.is_dirty()).unwrap_or(false) {
                    self.overlay = Overlay::open_confirm_quit();
                } else {
                    match EditorSession::open(&path) {
                        Ok(new_session) => {
                            if let Some(ref mut entry) = self.tabs.get_mut(self.active_tab) {
                                entry.session = new_session;
                                entry.top_line = 0;
                            }
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
            Effect::TabNewRequested { path } => {
                self.open_tab(&path);
            }
            Effect::TabCloseRequested { index, force } => {
                let idx = index.unwrap_or(self.active_tab);
                if !force && idx < self.tabs.len() && self.tabs[idx].session.is_dirty() {
                    self.overlay = Overlay::open_confirm_quit();
                    return;
                }
                if idx < self.tabs.len() {
                    self.do_close_tab(idx);
                }
            }
            Effect::TabNext => self.next_tab(),
            Effect::TabPrev => self.prev_tab(),
            Effect::TabJump { index } => self.jump_to_tab(index),
            Effect::QuitAllRequested { force } => {
                self.quit_all(force);
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
        // Determine the mode first, then apply scroll-follow.
        let is_view = self
            .active()
            .map(|e| e.session.mode() == oom_edit_core::session::Mode::View)
            .unwrap_or(false);

        if is_view {
            // View mode scroll-follow.
            if let Some(entry) = self.active() {
                let cursor_line = entry.session.view_cursor_line();
                let layout = entry.session.view_layout();
                let layout_height = layout.map(|l| l.lines.len()).unwrap_or(0);
                let view_top = entry.view_top;

                if layout_height == 0 || self.viewport_height == 0 {
                    return;
                }

                let new_top = oom_edit_core::view::nav::view_scroll_top(
                    cursor_line,
                    self.viewport_height,
                    layout_height,
                    view_top,
                );
                self.set_view_top(new_top);
            }
        } else {
            // Source editor scroll-follow.
            if let Some(entry) = self.active() {
                let cursor_line = entry.session.cursor().0;
                let line_count = entry.session.line_count();
                let top_line = entry.top_line;

                if cursor_line < top_line + SCROLLOFF {
                    self.set_top_line(cursor_line.saturating_sub(SCROLLOFF));
                } else if cursor_line >= top_line + self.viewport_height.saturating_sub(SCROLLOFF) {
                    self.set_top_line(
                        cursor_line
                            .saturating_sub(self.viewport_height.saturating_sub(SCROLLOFF + 1)),
                    );
                }

                // Clamp to document bounds.
                let new_top = self.top_line();
                if new_top + self.viewport_height > line_count {
                    self.set_top_line(line_count.saturating_sub(self.viewport_height));
                }
                self.set_top_line(self.top_line().min(line_count.saturating_sub(1)));
            }
        }
    }

    /// Scroll up by `lines` rows — Vim Ctrl-e / Ctrl-y style (viewport moves,
    /// cursor stays put). In View mode, scrolls `view_top`.
    fn scroll_up(&mut self, lines: usize) {
        if let Some(entry) = self.active() {
            match entry.session.mode() {
                oom_edit_core::session::Mode::View => {
                    self.set_view_top(entry.view_top.saturating_sub(lines));
                }
                _ => {
                    self.set_top_line(entry.top_line.saturating_sub(lines));
                }
            }
        }
    }

    /// Scroll down by `lines` rows — Vim Ctrl-e style (viewport moves,
    /// cursor stays put). In View mode, scrolls `view_top`.
    fn scroll_down(&mut self, lines: usize) {
        if let Some(entry) = self.active() {
            match entry.session.mode() {
                oom_edit_core::session::Mode::View => {
                    let layout = entry.session.view_layout();
                    let layout_height = layout.map(|l| l.lines.len()).unwrap_or(0);
                    let max_top = layout_height.saturating_sub(self.viewport_height);
                    self.set_view_top((entry.view_top + lines).min(max_top));
                }
                _ => {
                    let line_count = entry.session.line_count();
                    let max_top = line_count.saturating_sub(self.viewport_height);
                    self.set_top_line((entry.top_line + lines).min(max_top));
                }
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

/// Render the tab bar.
fn render_tab_bar(
    frame: &mut Frame<'_>,
    tabs: &[TabEntry],
    active_index: usize,
    area: ratatui::layout::Rect,
    theme: &Theme,
    tier: Tier,
) {
    use crate::widgets::tab_bar;

    let tab_entries: Vec<tab_bar::TabEntry> = tabs
        .iter()
        .map(|t| tab_bar::TabEntry {
            path: t
                .session
                .document_ref()
                .path()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|| "untitled.md".to_string()),
            dirty: t.session.is_dirty(),
        })
        .collect();

    let bar = tab_bar::TabBar {
        tabs: tab_entries,
        active_index,
        width: area.width,
    };

    let text = bar.build(theme, tier);
    tab_bar::render(frame, area, &text);
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
        assert_eq!(
            app.session().unwrap().mode(),
            oom_edit_core::session::Mode::Insert
        );
    }

    /// App: Escape returns to Normal mode.
    #[test]
    fn app_handle_event_escapes_to_normal() {
        let session = EditorSession::from_text("hello");
        let mut app = test_app(session);
        // Enter insert mode.
        let key = KeyEvent::new(CrosstermKeyCode::Char('i'), KeyModifiers::NONE);
        app.handle_event(&Event::Key(key));
        assert_eq!(
            app.session().unwrap().mode(),
            oom_edit_core::session::Mode::Insert
        );
        // Escape.
        let key = KeyEvent::new(CrosstermKeyCode::Esc, KeyModifiers::NONE);
        app.handle_event(&Event::Key(key));
        assert_eq!(
            app.session().unwrap().mode(),
            oom_edit_core::session::Mode::Normal
        );
    }

    /// App: `:w` triggers save request.
    #[test]
    fn app_handle_event_save_requested() {
        let session = EditorSession::from_text("hello");
        let mut app = test_app(session);

        // Enter command mode and type :w
        let key = KeyEvent::new(CrosstermKeyCode::Char(':'), KeyModifiers::NONE);
        app.handle_event(&Event::Key(key));
        assert_eq!(
            app.session().unwrap().mode(),
            oom_edit_core::session::Mode::Command
        );

        // Type 'w'
        let key = KeyEvent::new(CrosstermKeyCode::Char('w'), KeyModifiers::NONE);
        app.handle_event(&Event::Key(key));
        assert_eq!(
            app.session().unwrap().mode(),
            oom_edit_core::session::Mode::Command
        );

        // Press Enter to execute the command
        let key = KeyEvent::new(CrosstermKeyCode::Enter, KeyModifiers::NONE);
        app.handle_event(&Event::Key(key));
        assert_eq!(
            app.session().unwrap().mode(),
            oom_edit_core::session::Mode::Normal
        );

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

        assert!(app.session().unwrap().is_dirty());

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
        assert!(app.top_line() > 0, "top_line should have scrolled down");
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
        assert_eq!(
            app.session().unwrap().mode(),
            oom_edit_core::session::Mode::View
        );
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
        assert_eq!(
            app.session().unwrap().mode(),
            oom_edit_core::session::Mode::Insert
        );

        // Space in Insert mode should NOT start a chord — it falls through
        // to the session as a self-insert.
        let space = KeyEvent::new(CrosstermKeyCode::Char(' '), KeyModifiers::NONE);
        app.handle_event(&Event::Key(space));

        // The pending chord should NOT be set.
        assert!(app.pending_chord.since.is_none());
        // The document should have a space inserted (at cursor position 0 via 'i').
        assert!(app.session().unwrap().document().starts_with(" "));
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
        assert_eq!(
            app.session().unwrap().mode(),
            oom_edit_core::session::Mode::View
        );
    }

    /// T12: Palette reference entry — Enter on Vim reference shows status.
    #[test]
    fn app_palette_reference_entry() {
        let session = EditorSession::from_text("hello");
        let mut app = test_app(session);

        // Open palette via F1.
        let f1 = KeyEvent::new(CrosstermKeyCode::F(1), KeyModifiers::NONE);
        app.handle_event(&Event::Key(f1));
        assert!(app.overlay.is_palette(), "palette should be open");

        // Navigate down past the app commands to a Vim reference entry.
        // There are 11 app commands (5 original + 6 tab commands), so navigate to row 12 (index 11).
        for _ in 0..11 {
            let down = KeyEvent::new(CrosstermKeyCode::Down, KeyModifiers::NONE);
            app.handle_event(&Event::Key(down));
        }

        // Enter on a reference entry should show "reference entry" status.
        let enter = KeyEvent::new(CrosstermKeyCode::Enter, KeyModifiers::NONE);
        app.handle_event(&Event::Key(enter));

        // Palette should be closed and transient should show reference entry.
        assert!(!app.overlay.is_some(), "palette should be closed");
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
        assert_eq!(
            app.session().unwrap().mode(),
            oom_edit_core::session::Mode::Normal
        );
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
        assert_eq!(
            app.session().unwrap().mode(),
            oom_edit_core::session::Mode::Normal
        );
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
        let text = "# Intro\n\nThis opening paragraph is deliberately long enough to wrap at forty columns but not at eighty columns.\n\n## Target heading\n\nThis trailing paragraph is also deliberately long enough to make the narrow layout visibly different.\n";
        let session = EditorSession::from_text(text);
        let mut app = test_app(session);

        // Enter View mode.
        let space = KeyEvent::new(CrosstermKeyCode::Char(' '), KeyModifiers::NONE);
        app.handle_event(&Event::Key(space));
        let v = KeyEvent::new(CrosstermKeyCode::Char('v'), KeyModifiers::NONE);
        app.handle_event(&Event::Key(v));
        assert_eq!(
            app.session().unwrap().mode(),
            oom_edit_core::session::Mode::View
        );

        // Navigate to the target heading in View mode.
        let down = KeyEvent::new(CrosstermKeyCode::Down, KeyModifiers::NONE);
        let content_line = |app: &App| {
            let session = app.session().unwrap();
            let cursor = session.view_cursor_line();
            let source_start = session.view_layout().unwrap().lines[cursor].source.start;
            text[..source_start]
                .bytes()
                .filter(|byte| *byte == b'\n')
                .count()
        };
        while content_line(&app) < 4 {
            app.handle_event(&Event::Key(down));
        }
        assert_eq!(content_line(&app), 4);
        let wide_line_count = app.session().unwrap().view_layout().unwrap().lines.len();

        // Simulate a narrow resize that changes wrapping.
        let resize = Event::Resize(40, 6);
        app.handle_event(&resize);

        let narrow_line_count = app.session().unwrap().view_layout().unwrap().lines.len();
        assert!(
            narrow_line_count > wide_line_count,
            "narrow resize should reflow the View layout"
        );
        assert_eq!(
            content_line(&app),
            4,
            "view cursor should remain on the target heading's logical source line"
        );
        let cursor = app.session().unwrap().view_cursor_line();
        let view_top = app.active().unwrap().view_top;
        assert!(
            (view_top..view_top + app.viewport_height).contains(&cursor),
            "remapped View cursor should remain visible after resize"
        );
    }

    #[test]
    fn app_resize_with_tabs_keeps_view_cursor_visible() {
        let text = "# Intro\n\nThis opening paragraph is deliberately long enough to wrap at forty columns but not at eighty columns.\n\n## Target heading\n\nThis trailing paragraph is also deliberately long enough to make the narrow layout visibly different.\n";
        let mut app = test_app(EditorSession::from_text(text));

        let space = KeyEvent::new(CrosstermKeyCode::Char(' '), KeyModifiers::NONE);
        app.handle_event(&Event::Key(space));
        let v = KeyEvent::new(CrosstermKeyCode::Char('v'), KeyModifiers::NONE);
        app.handle_event(&Event::Key(v));

        let wide_line_count = app.session().unwrap().view_layout().unwrap().lines.len();
        let down = KeyEvent::new(CrosstermKeyCode::Down, KeyModifiers::NONE);
        for _ in 0..wide_line_count {
            app.handle_event(&Event::Key(down));
        }
        assert_eq!(
            app.session().unwrap().view_cursor_line(),
            wide_line_count - 1
        );

        app.tabs
            .push(TabEntry::new(EditorSession::from_text("second tab")));
        app.handle_event(&Event::Resize(40, 6));

        assert_eq!(app.viewport_height, 4);
        let cursor = app.session().unwrap().view_cursor_line();
        let view_top = app.active().unwrap().view_top;
        assert!(
            (view_top..view_top + app.viewport_height).contains(&cursor),
            "remapped View cursor should remain visible below the tab bar"
        );
    }

    #[test]
    fn app_render_propagates_light_theme_to_editor_status_and_tabs() {
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;

        let mut app = App::new(
            EditorSession::from_text("first\nplain"),
            "default-light".to_string(),
            Tier::TrueColor,
            Box::new(RecordingClipboardSink::default()),
        );
        app.tabs
            .push(TabEntry::new(EditorSession::from_text("second")));
        let mut terminal = Terminal::new(TestBackend::new(80, 10)).unwrap();

        terminal.draw(|frame| app.render(frame)).unwrap();

        let buffer = terminal.backend().buffer();
        let theme = &theme::DEFAULT_LIGHT;
        let text_style = theme.style(Tier::TrueColor, oom_edit_core::SemanticStyle::Text);
        let cursor_style = theme.ui_style(Tier::TrueColor, theme::UiSlot::CursorLine);
        let cursor_cell = buffer.cell((4, 1)).expect("cursor-line document cell");
        assert_eq!(
            cursor_cell.fg,
            text_style.fg.expect("light text foreground")
        );
        assert_eq!(
            cursor_cell.bg,
            cursor_style.bg.expect("light cursor-line background")
        );
        assert_eq!(
            buffer.cell((4, 2)).expect("plain document cell").fg,
            text_style.fg.expect("light text foreground")
        );

        assert_eq!(
            buffer.cell((0, 0)).expect("active tab cell").fg,
            theme
                .ui_style(Tier::TrueColor, theme::UiSlot::TabActive)
                .fg
                .expect("active tab foreground")
        );
        let separator_x = (0..80)
            .find(|x| {
                buffer
                    .cell((*x, 0))
                    .is_some_and(|cell| cell.symbol() == "│")
            })
            .expect("tab separator");
        assert_eq!(
            buffer
                .cell((separator_x, 0))
                .expect("tab separator cell")
                .fg,
            theme
                .ui_style(Tier::TrueColor, theme::UiSlot::TabSeparator)
                .fg
                .expect("tab separator foreground")
        );
        assert_eq!(
            buffer
                .cell((separator_x + 1, 0))
                .expect("inactive tab cell")
                .fg,
            theme
                .ui_style(Tier::TrueColor, theme::UiSlot::TabInactive)
                .fg
                .expect("inactive tab foreground")
        );

        let status_x = (0..80)
            .find(|x| {
                buffer
                    .cell((*x, 9))
                    .is_some_and(|cell| cell.symbol() == "N")
            })
            .expect("normal mode badge");
        assert_eq!(
            buffer.cell((status_x, 9)).expect("mode badge cell").fg,
            theme
                .ui_style(Tier::TrueColor, theme::UiSlot::BadgeNormal)
                .fg
                .expect("mode badge foreground")
        );
    }

    #[test]
    fn app_render_propagates_monochrome_tier_to_palette() {
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;

        let mut app = App::new(
            EditorSession::from_text("text"),
            "default-dark".to_string(),
            Tier::Monochrome,
            Box::new(RecordingClipboardSink::default()),
        );
        app.overlay = Overlay::open_palette();
        for ch in "beginning".chars() {
            app.handle_event(&Event::Key(KeyEvent::new(
                CrosstermKeyCode::Char(ch),
                KeyModifiers::NONE,
            )));
        }
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();

        terminal.draw(|frame| app.render(frame)).unwrap();

        let muted =
            theme::DEFAULT_DARK.style(Tier::Monochrome, oom_edit_core::SemanticStyle::Muted);
        let buffer = terminal.backend().buffer();
        let muted_cell = (8..16)
            .flat_map(|y| (20..58).map(move |x| (x, y)))
            .filter_map(|position| buffer.cell(position))
            .find(|cell| {
                cell.symbol() != " "
                    && cell.fg == muted.fg.expect("monochrome muted foreground")
                    && cell.modifier.contains(muted.add_modifier)
            })
            .expect("monochrome muted palette row");

        assert!(muted_cell.modifier.contains(muted.add_modifier));
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
