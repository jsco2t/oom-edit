//! App — the single source of truth for the running TUI.
//!
//! Holds the tab stack, scroll positions, last status message, overlay
//! state, and the quit flag. After each event: drain effects, scroll-follow,
//! update status message.
//!
//! ## Key routing order (arch §7.1)
//!
//! 1. Overlay open → overlay's key handler (take-and-return-bool).
//! 2. Mode ∈ {Normal, Select}: try registry keymap projections and app chords.
//!    On match → `execute_command`. No match → fall through.
//! 3. Everything else → active session's `handle_key(key)`, then drain `Effect`s.

use std::time::Instant;

use crossterm::event::{Event, KeyCode as CrosstermKeyCode, KeyEvent, KeyModifiers};
use ratatui::Frame;

use oom_edit_core::clipboard::ClipboardSink;
use oom_edit_core::session::{EditorSession, Effect, KeyCode, KeyCodeKind, KeyInput, Modifiers};

use crossterm::event::MouseEventKind;

use crate::command::{keymap::PendingChord, Command, Keymap};
use crate::config::ConfigStore;
use crate::overlay::Overlay;
use crate::screens::editor::{render_editor, render_status_row, source_text_width, EditorViewport};
use crate::screens::rendered::render_rendered;
use crate::theme::{self, ResolvedTheme, Theme, Tier};
use crate::widgets::status_bar;
use crate::widgets::which_key;

/// Scrolloff: keep this many lines of context around the cursor.
const SCROLLOFF: usize = 3;
/// Horizontal scrolloff in no-wrap mode.
const HSCROLLOFF: usize = 5;

fn body_height(total_height: u16, has_multiple_tabs: bool) -> u16 {
    let tab_bar_height = u16::from(has_multiple_tabs);
    total_height.saturating_sub(tab_bar_height + 1)
}

fn rendered_scroll_top(
    cursor: usize,
    viewport_height: usize,
    layout_height: usize,
    current_top: usize,
) -> usize {
    if viewport_height == 0 || layout_height == 0 {
        return 0;
    }
    let cursor = cursor.min(layout_height.saturating_sub(1));
    let max_top = layout_height.saturating_sub(viewport_height);
    let center_start = viewport_height / 3;
    let center_end = 2 * viewport_height / 3;
    if cursor < center_start {
        0
    } else if cursor >= max_top + center_end {
        max_top
    } else if cursor < current_top + center_start {
        cursor.saturating_sub(center_start)
    } else if cursor >= current_top + center_end {
        cursor.saturating_sub(center_end)
    } else {
        current_top.min(max_top)
    }
}

/// A single tab entry: an [`EditorSession`] with per-tab UI state.
pub(crate) struct TabEntry {
    /// The core editing session for this tab.
    session: EditorSession,
    /// The first visible line (owned by the TUI for scroll-follow).
    top_line: usize,
    /// First visible character column when wrapping is disabled.
    left_col: usize,
    /// Visual rows skipped within `top_line` when wrapping is enabled.
    skip_rows: usize,
    /// The first visible rendered row (owned by the TUI for scroll-follow in rendered mode).
    rendered_top: usize,
}

impl TabEntry {
    fn new(session: EditorSession) -> Self {
        Self {
            session,
            top_line: 0,
            left_col: 0,
            skip_rows: 0,
            rendered_top: 0,
        }
    }

    /// Get a mutable reference to the session.
    #[cfg(test)]
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
    /// Source text viewport width, excluding the line-number gutter.
    viewport_width: usize,
    /// Follow requested by a state transition that returns before the normal
    /// event tail (tab switches and registry-dispatched session commands).
    pending_scroll_follow: bool,
    /// Geometry used by the most recent follow, so paint only follows again
    /// when the body dimensions actually change.
    last_follow_geometry: Option<(usize, usize)>,
    /// Runtime source-wrap option, initialized from config.
    wrap_enabled: bool,
    /// Whether rendered Normal, Select, and Command use hybrid-relative numbers.
    relative_line_numbers: bool,
    /// Current time (injected by tick for testability of which-key delay gate).
    now: Instant,
    /// Active transient message with TTL expiry.
    transient: Option<status_bar::Transient>,
    /// Active theme name (for CycleTheme).
    #[cfg(test)]
    pub theme_name: String,
    #[cfg(not(test))]
    theme_name: String,
    /// Whether the active display mode was resolved as light at startup.
    is_light: bool,
    /// Explicitly injected persistence for theme changes.
    config_store: Box<dyn ConfigStore>,
    /// Active capability tier.
    tier: Tier,
    /// Clipboard sink for OSC 52 clipboard writes (T16).
    clipboard_sink: Box<dyn ClipboardSink>,
    #[cfg(test)]
    scroll_follow_count: usize,
}

impl App {
    /// Create a new App from an open session (starts with one tab).
    pub fn new(
        session: EditorSession,
        resolved_theme: ResolvedTheme,
        wrap_enabled: bool,
        relative_line_numbers: bool,
        clipboard_sink: Box<dyn ClipboardSink>,
        config_store: Box<dyn ConfigStore>,
    ) -> Self {
        let is_light = resolved_theme.is_light();
        let tier = resolved_theme.capability;
        let theme_name = resolved_theme.name;
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
            viewport_width: 76,
            pending_scroll_follow: true,
            last_follow_geometry: None,
            wrap_enabled,
            relative_line_numbers,
            now: Instant::now(),
            transient: None,
            theme_name,
            is_light,
            config_store,
            tier,
            clipboard_sink,
            #[cfg(test)]
            scroll_follow_count: 0,
        }
    }

    /// Get a reference to the active tab entry.
    fn active(&self) -> Option<&TabEntry> {
        self.tabs.get(self.active_tab)
    }

    /// Get a mutable reference to the active tab entry.
    #[cfg(test)]
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
    #[cfg(test)]
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

        let viewport_height = body_height as usize;
        let viewport_width = self
            .active()
            .map(|entry| source_text_width(area.width, entry.session.line_count()) as usize)
            .unwrap_or(area.width as usize);
        self.viewport_height = viewport_height;
        self.viewport_width = viewport_width;
        let geometry = (viewport_height, viewport_width);
        if self.last_follow_geometry != Some(geometry) {
            self.pending_scroll_follow = true;
            self.last_follow_geometry = Some(geometry);
        }
        if self.pending_scroll_follow {
            self.pending_scroll_follow = false;
            self.scroll_follow();
        }

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
            if entry.session.mode() != oom_edit_core::session::Mode::Insert {
                render_rendered(
                    frame,
                    &mut entry.session,
                    entry.rendered_top,
                    self.relative_line_numbers,
                    body_area,
                    active_theme,
                    self.tier,
                );
            } else {
                render_editor(
                    frame,
                    &mut entry.session,
                    EditorViewport::new(
                        entry.top_line,
                        self.wrap_enabled,
                        entry.left_col,
                        entry.skip_rows,
                    ),
                    self.relative_line_numbers,
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
        self.render_which_key(frame, status_area);

        // Render overlay on top if open.
        if self.overlay.is_some() {
            self.overlay.render(frame, active_theme, self.tier);
        }
    }

    /// Render the which-key hint bar.
    ///
    /// Pure gate + pure build + thin render: the which-key popup appears
    /// only after 150ms of pending Space prefix, in Normal/rendered mode,
    /// and only when there are ≥2 continuations.
    /// Return the [`Contexts`] bitset for the current session mode.
    fn mode_context(&self) -> crate::command::registry::Contexts {
        match self.session().map(|s| s.mode()) {
            Some(oom_edit_core::session::Mode::Normal) => {
                crate::command::registry::Contexts::NORMAL
            }
            Some(oom_edit_core::session::Mode::Insert) => {
                crate::command::registry::Contexts::INSERT
            }
            Some(oom_edit_core::session::Mode::Select) => {
                crate::command::registry::Contexts::SELECT
            }
            Some(oom_edit_core::session::Mode::Command) => {
                crate::command::registry::Contexts::COMMAND
            }
            None => crate::command::registry::Contexts::NORMAL,
        }
    }

    /// Return true when the current mode supports Space-chord and g-chord keymaps.
    fn in_chord_context(&self) -> bool {
        self.session()
            .map(|s| {
                matches!(
                    s.mode(),
                    oom_edit_core::session::Mode::Normal | oom_edit_core::session::Mode::Select
                )
            })
            .unwrap_or(false)
    }

    /// Render the which-key hint bar.
    ///
    /// Pure gate + pure build + thin render: the which-key popup appears
    /// only after 150ms of pending Space prefix, in Normal/rendered mode,
    /// and only when there are ≥2 continuations.
    fn render_which_key(&self, frame: &mut Frame<'_>, status_area: ratatui::layout::Rect) {
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
            let badge_width = crate::widgets::status_bar::MODE_BADGE_COLS.min(status_area.width);
            let flexible_area = ratatui::layout::Rect::new(
                status_area.x.saturating_add(badge_width),
                status_area.y,
                status_area.width.saturating_sub(badge_width),
                status_area.height,
            );
            which_key::render(frame, flexible_area, &text);
        }
    }

    /// Get the active tab's top_line.
    #[cfg_attr(not(test), expect(dead_code))]
    pub fn top_line(&self) -> usize {
        self.active().map(|t| t.top_line).unwrap_or(0)
    }

    /// Set the active tab's top_line.
    fn set_top_line(&mut self, val: usize) {
        if let Some(entry) = self.tabs.get_mut(self.active_tab) {
            if entry.top_line != val {
                entry.skip_rows = 0;
            }
            entry.top_line = val;
        }
    }

    /// Get the active tab's horizontal offset.
    #[cfg_attr(not(test), expect(dead_code))]
    pub fn left_col(&self) -> usize {
        self.active().map(|t| t.left_col).unwrap_or(0)
    }

    /// Set the active tab's horizontal offset.
    fn set_left_col(&mut self, val: usize) {
        if let Some(entry) = self.tabs.get_mut(self.active_tab) {
            entry.left_col = val;
        }
    }

    /// Get the active tab's wrapped-row offset.
    #[cfg_attr(not(test), expect(dead_code))]
    pub fn skip_rows(&self) -> usize {
        self.active().map(|t| t.skip_rows).unwrap_or(0)
    }

    /// Set the active tab's wrapped-row offset.
    fn set_skip_rows(&mut self, val: usize) {
        if let Some(entry) = self.tabs.get_mut(self.active_tab) {
            entry.skip_rows = val;
        }
    }

    /// Get the active tab's rendered_top.
    #[expect(dead_code)]
    fn rendered_top(&self) -> usize {
        self.active().map(|t| t.rendered_top).unwrap_or(0)
    }

    /// Set the active tab's rendered_top.
    fn set_rendered_top(&mut self, val: usize) {
        if let Some(entry) = self.tabs.get_mut(self.active_tab) {
            entry.rendered_top = val;
        }
    }

    /// Handle a crossterm event, following arch §7.1 fixed order.
    pub fn handle_event(&mut self, event: &Event) {
        // Handle resize events — rebuild rendered layout on width change.
        if let Event::Resize(_width, height) = event {
            // Clamp viewport height using the same chrome rows as render().
            self.viewport_height = body_height(*height, self.has_multiple_tabs()) as usize;
            self.viewport_width = self
                .active()
                .map(|entry| source_text_width(*_width, entry.session.line_count()) as usize)
                .unwrap_or(*_width as usize);
            self.last_follow_geometry = Some((self.viewport_height, self.viewport_width));
            if let Some(ref mut entry) = self.tabs.get_mut(self.active_tab) {
                if entry.session.mode() != oom_edit_core::session::Mode::Insert {
                    let text_width = source_text_width(*_width, entry.session.line_count());
                    entry.session.render_layout(text_width);
                }
            }
            self.scroll_follow();
            self.pending_scroll_follow = false;
            return;
        }

        if let Event::Paste(text) = event {
            let effects = self
                .tabs
                .get_mut(self.active_tab)
                .map(|entry| entry.session.insert_paste(text))
                .unwrap_or_default();
            for effect in effects {
                self.handle_effect(effect);
            }
            self.pending_scroll_follow = true;
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

        // Unsupported terminal keys are global no-ops. Consume them before
        // overlays and chord state so they cannot cancel an in-progress key
        // sequence or otherwise mutate TUI state.
        if key_input.code.kind == KeyCodeKind::Noop {
            return;
        }

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

        // 2. Mode ∈ {Normal, Select}: try app keymap.
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
                        // Not a tab chord: replay both keys so rendered Vim
                        // sequences such as `gg` are never swallowed.
                        self.forward_session_key(KeyCodeKind::Char('g'));
                        self.forward_session_input(key_input);
                        return;
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
                        let tab_num = digit.to_digit(10).unwrap_or(0) as usize;
                        if tab_num >= 1 && tab_num <= self.tab_count() {
                            self.pending_chord.reset();
                            self.jump_to_tab(tab_num);
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

            // Check if this key starts a 'g' chord (only in Normal/Select).
            if let KeyCodeKind::Char('g') = key_input.code.kind {
                if !key_input.mods.ctrl
                    && !key_input.mods.alt
                    && !key_input.mods.shift
                    && self.tab_count() > 1
                {
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

        // Coalesce all input received before the next paint into one follow.
        self.pending_scroll_follow = true;
    }

    /// Open a new tab with the given file path.
    fn open_tab(&mut self, path: &std::path::Path) {
        match EditorSession::open(path) {
            Ok(session) => {
                let idx = self.tabs.len();
                self.tabs.push(TabEntry::new(session));
                self.active_tab = idx;
                self.pending_scroll_follow = true;
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
        self.pending_scroll_follow = true;
    }

    /// Switch to tab by 1-based index.
    fn jump_to_tab(&mut self, index: usize) {
        // Convert 1-based to 0-based.
        let idx = index.saturating_sub(1);
        if idx < self.tabs.len() {
            self.active_tab = idx;
            self.pending_scroll_follow = true;
        }
    }

    /// Next tab (wrap).
    fn next_tab(&mut self) {
        if self.tabs.len() <= 1 {
            return;
        }
        self.active_tab = (self.active_tab + 1) % self.tabs.len();
        self.pending_scroll_follow = true;
    }

    /// Previous tab (wrap).
    fn prev_tab(&mut self) {
        if self.tabs.len() <= 1 {
            return;
        }
        self.active_tab = (self.active_tab + self.tabs.len() - 1) % self.tabs.len();
        self.pending_scroll_follow = true;
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
                                    entry.rendered_top = 0;
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
            Command::EnterCharacterSelect => self.forward_session_key(KeyCodeKind::Char('v')),
            Command::EnterLineSelect => self.forward_session_key(KeyCodeKind::Char('V')),
            Command::EnterBlockSelect => self.forward_session_input(KeyInput {
                code: KeyCode {
                    kind: KeyCodeKind::Char('v'),
                },
                mods: Modifiers {
                    ctrl: true,
                    ..Modifiers::default()
                },
            }),
            Command::CancelSelect => self.forward_session_key(KeyCodeKind::Esc),
            Command::SelectYank => self.forward_session_key(KeyCodeKind::Char('y')),
            Command::SelectDelete => self.forward_session_key(KeyCodeKind::Char('d')),
            Command::SelectChange => self.forward_session_key(KeyCodeKind::Char('c')),
            Command::SelectIndent => self.forward_session_key(KeyCodeKind::Char('>')),
            Command::SelectOutdent => self.forward_session_key(KeyCodeKind::Char('<')),
            Command::SelectSwapAnchor => self.forward_session_key(KeyCodeKind::Char('o')),
            Command::Help => {
                // Open the command palette.
                self.overlay = Overlay::open_palette(self.mode_context());
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
                let next = theme::cycle_theme(&self.theme_name, self.is_light);
                self.theme_name = next.to_string();
                // Persist to config.
                let mut config = self.config_store.load();
                if self.is_light {
                    config.theme.light = next.to_string();
                } else {
                    config.theme.dark = next.to_string();
                }
                if let Err(e) = self.config_store.save(&config) {
                    eprintln!("oom-edit: failed to save config: {e}");
                }
                self.set_transient(
                    if next == "accessible" {
                        "theme: accessible (monochrome)".to_string()
                    } else {
                        format!("theme: {next}")
                    },
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

    fn forward_session_key(&mut self, kind: KeyCodeKind) {
        self.forward_session_input(KeyInput {
            code: KeyCode { kind },
            mods: Modifiers::default(),
        });
    }

    fn forward_session_input(&mut self, input: KeyInput) {
        let effects = self
            .tabs
            .get_mut(self.active_tab)
            .map(|entry| entry.session.handle_key(input))
            .unwrap_or_default();
        for effect in effects {
            self.handle_effect(effect);
        }
        self.pending_scroll_follow = true;
    }

    /// Handle a single core effect.
    fn handle_effect(&mut self, effect: Effect) {
        match effect {
            Effect::SaveRequested {
                path,
                force,
                retarget,
                then_quit,
            } => {
                if let Some(ref mut entry) = self.tabs.get_mut(self.active_tab) {
                    let save_result = if let Some(copy_path) = path.as_deref().filter(|_| !retarget)
                    {
                        entry.session.save_copy(copy_path)
                    } else {
                        entry.session.save(path.as_deref(), force)
                    };
                    match save_result {
                        Ok(()) => {
                            if retarget || path.is_none() {
                                entry.session.save_point();
                            }
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
                                entry.left_col = 0;
                                entry.skip_rows = 0;
                                entry.rendered_top = 0;
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
                                entry.left_col = 0;
                                entry.skip_rows = 0;
                                entry.rendered_top = 0;
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
                } else {
                    self.set_transient(
                        "yanked to register".to_string(),
                        oom_edit_core::session::Severity::Info,
                    );
                }
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
            Effect::SetOption { key, value } => {
                if key == "wrap" {
                    self.wrap_enabled = value;
                    if value {
                        self.set_left_col(0);
                    }
                    self.set_transient(
                        if value { "wrap" } else { "nowrap" }.to_string(),
                        oom_edit_core::session::Severity::Info,
                    );
                } else {
                    self.set_transient(
                        format!("Unknown option: {key}"),
                        oom_edit_core::session::Severity::Warning,
                    );
                }
                self.pending_scroll_follow = true;
            }
            Effect::HelpRequested => {
                // Open command palette.
                self.overlay = Overlay::open_palette(self.mode_context());
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
    /// `SCROLLOFF` lines of context on either rendered or source surfaces.
    pub fn scroll_follow(&mut self) {
        #[cfg(test)]
        {
            self.scroll_follow_count += 1;
        }
        // Determine the mode first, then apply scroll-follow.
        let is_rendered = self
            .active()
            .map(|e| e.session.mode() != oom_edit_core::session::Mode::Insert)
            .unwrap_or(false);

        if is_rendered {
            let viewport_width = self.viewport_width.min(usize::from(u16::MAX)) as u16;
            if let Some(entry) = self.tabs.get_mut(self.active_tab) {
                // Insert edits invalidate the rendered cache. Rebuild before
                // reading the rendered cursor so Escape can follow a cursor
                // that moved beyond the current rendered viewport.
                entry.session.render_layout(viewport_width);
                let cursor_line = entry.session.rendered_cursor_line();
                let layout = entry.session.rendered_layout();
                let layout_height = layout.map(|l| l.lines.len()).unwrap_or(0);
                let rendered_top = entry.rendered_top;

                if layout_height == 0 || self.viewport_height == 0 {
                    return;
                }

                let new_top = rendered_scroll_top(
                    cursor_line,
                    self.viewport_height,
                    layout_height,
                    rendered_top,
                );
                self.set_rendered_top(new_top);
            }
        } else {
            if let Some(entry) = self.active() {
                let (top_line, skip_rows, left_col) = Self::source_scroll_position(
                    entry,
                    self.wrap_enabled,
                    self.viewport_height,
                    self.viewport_width,
                );
                self.set_top_line(top_line);
                self.set_skip_rows(skip_rows);
                self.set_left_col(left_col);
            }
        }
    }

    fn source_scroll_position(
        entry: &TabEntry,
        wrap: bool,
        viewport_height: usize,
        viewport_width: usize,
    ) -> (usize, usize, usize) {
        let (cursor_line, cursor_col) = entry.session.cursor();
        let line_count = entry.session.line_count();
        if viewport_height == 0 || line_count == 0 {
            return (entry.top_line, 0, if wrap { 0 } else { entry.left_col });
        }

        if !wrap {
            let mut top_line = entry.top_line.min(line_count.saturating_sub(1));
            if cursor_line < top_line.saturating_add(SCROLLOFF) {
                top_line = cursor_line.saturating_sub(SCROLLOFF);
            } else if cursor_line
                >= top_line.saturating_add(viewport_height.saturating_sub(SCROLLOFF))
            {
                top_line =
                    cursor_line.saturating_sub(viewport_height.saturating_sub(SCROLLOFF + 1));
            }
            if top_line.saturating_add(viewport_height) > line_count {
                top_line = line_count.saturating_sub(viewport_height);
            }
            top_line = top_line.min(line_count.saturating_sub(1));

            let mut left_col = entry.left_col;
            if viewport_width == 0 {
                left_col = 0;
            } else {
                let margin = HSCROLLOFF.min(viewport_width.saturating_sub(1) / 2);
                if cursor_col < left_col.saturating_add(margin) {
                    left_col = cursor_col.saturating_sub(margin);
                } else {
                    let right_margin = viewport_width.saturating_sub(margin + 1);
                    if cursor_col > left_col.saturating_add(right_margin) {
                        left_col = cursor_col.saturating_sub(right_margin);
                    }
                }
            }
            return (top_line, 0, left_col);
        }

        if viewport_width == 0 {
            return (cursor_line, 0, 0);
        }

        let width = viewport_width.min(u16::MAX as usize) as u16;
        let scrolloff = SCROLLOFF.min(viewport_height.saturating_sub(1) / 2);
        let bottom_target = viewport_height.saturating_sub(scrolloff + 1);
        let mut top_line = entry.top_line.min(line_count.saturating_sub(1));
        let mut skip_rows = if cursor_line == top_line {
            entry.skip_rows
        } else {
            0
        };

        if cursor_line < top_line {
            top_line = cursor_line;
            skip_rows = 0;
        }

        let (cursor_visual_row, cursor_line_height) =
            entry
                .session
                .visual_row_info(cursor_line, cursor_col, width, true);
        let mut cursor_screen_row = cursor_visual_row;
        for line in top_line..cursor_line {
            let (_, rows) = entry.session.visual_row_info(line, 0, width, true);
            cursor_screen_row = cursor_screen_row.saturating_add(rows);
        }
        cursor_screen_row = cursor_screen_row.saturating_sub(skip_rows);

        if cursor_screen_row < scrolloff {
            let mut candidate_top = cursor_line;
            let mut candidate_cursor_row = cursor_visual_row;
            while candidate_top > 0 && candidate_cursor_row < scrolloff {
                let previous = candidate_top - 1;
                let (_, previous_rows) = entry.session.visual_row_info(previous, 0, width, true);
                if candidate_cursor_row.saturating_add(previous_rows) > bottom_target {
                    break;
                }
                candidate_top = previous;
                candidate_cursor_row = candidate_cursor_row.saturating_add(previous_rows);
            }
            top_line = candidate_top;
            skip_rows = 0;
        } else if cursor_screen_row > bottom_target {
            let mut candidate_top = cursor_line;
            let mut candidate_cursor_row = cursor_visual_row;
            while candidate_top > 0 {
                let previous = candidate_top - 1;
                let (_, previous_rows) = entry.session.visual_row_info(previous, 0, width, true);
                if candidate_cursor_row.saturating_add(previous_rows) > bottom_target {
                    break;
                }
                candidate_top = previous;
                candidate_cursor_row = candidate_cursor_row.saturating_add(previous_rows);
            }
            top_line = candidate_top;
            skip_rows = 0;
        }

        if top_line == cursor_line && cursor_line_height > viewport_height {
            if cursor_visual_row < skip_rows.saturating_add(scrolloff) {
                skip_rows = cursor_visual_row.saturating_sub(scrolloff);
            } else if cursor_visual_row > skip_rows.saturating_add(bottom_target) {
                skip_rows = cursor_visual_row.saturating_sub(bottom_target);
            }
            skip_rows = skip_rows.min(cursor_line_height.saturating_sub(1));
        } else {
            skip_rows = 0;
        }

        (top_line, skip_rows, 0)
    }

    /// Scroll up by `lines` rows — Vim Ctrl-e / Ctrl-y style (viewport moves,
    /// cursor stays put). Rendered modes scroll `rendered_top`.
    fn scroll_up(&mut self, lines: usize) {
        if let Some(entry) = self.active() {
            match entry.session.mode() {
                oom_edit_core::session::Mode::Normal
                | oom_edit_core::session::Mode::Select
                | oom_edit_core::session::Mode::Command => {
                    self.set_rendered_top(entry.rendered_top.saturating_sub(lines));
                }
                _ => {
                    self.set_top_line(entry.top_line.saturating_sub(lines));
                }
            }
        }
    }

    /// Scroll down by `lines` rows — Vim Ctrl-e style (viewport moves,
    /// cursor stays put). Rendered modes scroll `rendered_top`.
    fn scroll_down(&mut self, lines: usize) {
        if let Some(entry) = self.active() {
            match entry.session.mode() {
                oom_edit_core::session::Mode::Normal
                | oom_edit_core::session::Mode::Select
                | oom_edit_core::session::Mode::Command => {
                    let layout = entry.session.rendered_layout();
                    let layout_height = layout.map(|l| l.lines.len()).unwrap_or(0);
                    let max_top = layout_height.saturating_sub(self.viewport_height);
                    self.set_rendered_top((entry.rendered_top + lines).min(max_top));
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
            kind: KeyCodeKind::Noop,
        },
        CrosstermKeyCode::F(n) => KeyCode {
            kind: KeyCodeKind::F(n),
        },
        CrosstermKeyCode::Null => KeyCode {
            kind: KeyCodeKind::Noop,
        },
        CrosstermKeyCode::Esc => KeyCode {
            kind: KeyCodeKind::Esc,
        },
        CrosstermKeyCode::CapsLock => KeyCode {
            kind: KeyCodeKind::Noop,
        },
        CrosstermKeyCode::Menu => KeyCode {
            kind: KeyCodeKind::Noop,
        },
        CrosstermKeyCode::ScrollLock => KeyCode {
            kind: KeyCodeKind::Noop,
        },
        CrosstermKeyCode::Pause => KeyCode {
            kind: KeyCodeKind::Noop,
        },
        CrosstermKeyCode::NumLock => KeyCode {
            kind: KeyCodeKind::Noop,
        },
        CrosstermKeyCode::PrintScreen => KeyCode {
            kind: KeyCodeKind::Noop,
        },
        CrosstermKeyCode::KeypadBegin => KeyCode {
            kind: KeyCodeKind::Noop,
        },
        CrosstermKeyCode::Media(_) => KeyCode {
            kind: KeyCodeKind::Noop,
        },
        CrosstermKeyCode::Modifier(_) => KeyCode {
            kind: KeyCodeKind::Noop,
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
    use crate::command::Contexts;
    use crossterm::event::{MediaKeyCode, ModifierKeyCode};
    use oom_edit_core::clipboard::{ClipboardError, ClipboardSink, RecordingClipboardSink};
    use oom_edit_core::Mode;

    struct FailingClipboardSink;

    impl ClipboardSink for FailingClipboardSink {
        fn copy(&mut self, _text: &str) -> Result<(), ClipboardError> {
            Err(ClipboardError::NotSupported)
        }
    }

    /// Create a test App with a recording clipboard sink.
    fn test_app(mut session: EditorSession) -> App {
        session.render_layout(74);
        App::new(
            session,
            theme::ResolvedTheme::injected("default-dark", false, Tier::TrueColor),
            true,
            false,
            Box::new(RecordingClipboardSink::default()),
            Box::new(crate::config::DisabledConfigStore),
        )
    }

    fn large_source_fixture() -> String {
        const TARGET: usize = 128 * 1024;
        const LINE: &str =
            "paragraph text for source scrolling and incremental editing 0123456789\n\n";
        let mut text = String::with_capacity(TARGET);
        while text.len() + LINE.len() <= TARGET {
            text.push_str(LINE);
        }
        text.push_str(&"x".repeat(TARGET - text.len()));
        text
    }

    fn yank_current_line_to_system_clipboard(app: &mut App) {
        for ch in ['v', '"', '+', 'y'] {
            let key = KeyEvent::new(CrosstermKeyCode::Char(ch), KeyModifiers::NONE);
            app.handle_event(&Event::Key(key));
        }
    }

    fn enter_insert(app: &mut App) {
        app.handle_event(&Event::Key(KeyEvent::new(
            CrosstermKeyCode::Char('i'),
            KeyModifiers::NONE,
        )));
    }

    fn press(app: &mut App, code: CrosstermKeyCode) {
        app.handle_event(&Event::Key(KeyEvent::new(code, KeyModifiers::NONE)));
        app.pending_scroll_follow = false;
        app.scroll_follow();
    }

    fn open_palette_with_space_h(app: &mut App) {
        for ch in [' ', 'h'] {
            app.handle_event(&Event::Key(KeyEvent::new(
                CrosstermKeyCode::Char(ch),
                KeyModifiers::NONE,
            )));
        }
    }

    fn test_app_with_tabs(tab_count: usize) -> App {
        assert!(tab_count >= 1);

        let mut app = test_app(EditorSession::from_text("tab 1"));
        for tab_num in 2..=tab_count {
            app.tabs
                .push(TabEntry::new(EditorSession::from_text(&format!(
                    "tab {tab_num}"
                ))));
        }
        app
    }

    fn unmapped_special_keys() -> [CrosstermKeyCode; 11] {
        [
            CrosstermKeyCode::Insert,
            CrosstermKeyCode::Menu,
            CrosstermKeyCode::Null,
            CrosstermKeyCode::CapsLock,
            CrosstermKeyCode::ScrollLock,
            CrosstermKeyCode::Pause,
            CrosstermKeyCode::NumLock,
            CrosstermKeyCode::PrintScreen,
            CrosstermKeyCode::KeypadBegin,
            CrosstermKeyCode::Media(MediaKeyCode::Play),
            CrosstermKeyCode::Modifier(ModifierKeyCode::LeftShift),
        ]
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

    #[test]
    fn clipboard_error_preserves_error_transient() {
        let mut session = EditorSession::from_text("hello\n");
        session.render_layout(74);
        let mut app = App::new(
            session,
            theme::ResolvedTheme::injected("default-dark", false, Tier::TrueColor),
            true,
            false,
            Box::new(FailingClipboardSink),
            Box::new(crate::config::DisabledConfigStore),
        );

        yank_current_line_to_system_clipboard(&mut app);

        let transient = app.transient.as_ref().expect("transient should be set");
        assert!(
            transient.text.contains("Clipboard error"),
            "expected clipboard error message, got: {}",
            transient.text
        );
        assert!(!transient.text.contains("yanked to register"));
        assert_eq!(
            transient.severity,
            oom_edit_core::session::Severity::Warning
        );
    }

    #[test]
    fn clipboard_success_sets_success_transient() {
        let mut app = test_app(EditorSession::from_text("hello\n"));

        yank_current_line_to_system_clipboard(&mut app);

        let transient = app.transient.as_ref().expect("transient should be set");
        assert_eq!(transient.text, "yanked to register");
        assert_eq!(transient.severity, oom_edit_core::session::Severity::Info);
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

        enter_insert(&mut app);
        for _ in 0..50 {
            press(&mut app, CrosstermKeyCode::Down);
        }
        app.scroll_follow();

        // Scroll follow should have adjusted top_line
        assert!(app.top_line() > 0, "top_line should have scrolled down");
    }

    #[test]
    fn app_navigation_follows_once_per_frame() {
        use crossterm::event::{MouseEvent, MouseEventKind};

        let fixture = large_source_fixture();
        let mut app = test_app(EditorSession::from_text(&fixture));
        let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(100, 12))
            .expect("test terminal");
        terminal
            .draw(|frame| app.render(frame))
            .expect("initial frame");
        app.scroll_follow_count = 0;
        let started = std::time::Instant::now();

        let before_rendered = app.scroll_follow_count;
        for code in [
            CrosstermKeyCode::Char('2'),
            CrosstermKeyCode::Char('0'),
            CrosstermKeyCode::Char('j'),
        ] {
            app.handle_event(&Event::Key(KeyEvent::new(code, KeyModifiers::NONE)));
        }
        assert_eq!(app.scroll_follow_count, before_rendered);
        terminal
            .draw(|frame| app.render(frame))
            .expect("coalesced rendered frame");
        assert_eq!(app.scroll_follow_count - before_rendered, 1);
        let entry = &app.tabs[0];
        let rendered_cursor = entry.session.rendered_cursor_line();
        assert!(entry.rendered_top > 0);
        assert!(rendered_cursor >= entry.rendered_top);
        assert!(rendered_cursor < entry.rendered_top + app.viewport_height);
        terminal
            .draw(|frame| app.render(frame))
            .expect("idle rendered frame");
        assert_eq!(app.scroll_follow_count - before_rendered, 1);

        let before_upward = app.scroll_follow_count;
        for code in [CrosstermKeyCode::Char('g'), CrosstermKeyCode::Char('g')] {
            app.handle_event(&Event::Key(KeyEvent::new(code, KeyModifiers::NONE)));
        }
        assert_eq!(app.scroll_follow_count, before_upward);
        terminal
            .draw(|frame| app.render(frame))
            .expect("coalesced upward frame");
        assert_eq!(app.scroll_follow_count - before_upward, 1);
        assert_eq!(app.tabs[0].rendered_top, 0);
        assert_eq!(app.tabs[0].session.rendered_cursor_line(), 0);

        let before_source = app.scroll_follow_count;
        app.handle_event(&Event::Key(KeyEvent::new(
            CrosstermKeyCode::Char('i'),
            KeyModifiers::NONE,
        )));
        for _ in 0..20 {
            app.handle_event(&Event::Key(KeyEvent::new(
                CrosstermKeyCode::Down,
                KeyModifiers::NONE,
            )));
        }
        assert_eq!(app.scroll_follow_count, before_source);
        terminal
            .draw(|frame| app.render(frame))
            .expect("coalesced source frame");
        assert_eq!(app.scroll_follow_count - before_source, 1);
        assert_eq!(app.session().unwrap().mode(), Mode::Insert);
        let source_cursor = app.session().unwrap().cursor().0;
        assert!(app.top_line() > 0);
        assert!(source_cursor >= app.top_line());
        assert!(source_cursor < app.top_line() + app.viewport_height);

        let before_paste = app.scroll_follow_count;
        app.handle_event(&Event::Paste("λ\ninserted".to_string()));
        assert_eq!(app.scroll_follow_count, before_paste);
        terminal
            .draw(|frame| app.render(frame))
            .expect("paste frame");
        assert_eq!(app.scroll_follow_count - before_paste, 1);
        let source_cursor = app.session().unwrap().cursor().0;
        assert!(source_cursor >= app.top_line());
        assert!(source_cursor < app.top_line() + app.viewport_height);

        let before_mouse = app.scroll_follow_count;
        app.handle_event(&Event::Mouse(MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: 1,
            row: 1,
            modifiers: KeyModifiers::NONE,
        }));
        terminal
            .draw(|frame| app.render(frame))
            .expect("mouse frame");
        assert_eq!(app.scroll_follow_count - before_mouse, 0);

        app.handle_effect(Effect::SetOption {
            key: "wrap".to_string(),
            value: false,
        });
        let before_option_frame = app.scroll_follow_count;
        terminal
            .draw(|frame| app.render(frame))
            .expect("option frame");
        assert_eq!(app.scroll_follow_count - before_option_frame, 1);
        let source_cursor = app.session().unwrap().cursor().0;
        assert!(source_cursor >= app.top_line());
        assert!(source_cursor < app.top_line() + app.viewport_height);

        let before_resize = app.scroll_follow_count;
        app.handle_event(&Event::Resize(80, 10));
        let mut resized = ratatui::Terminal::new(ratatui::backend::TestBackend::new(80, 10))
            .expect("resized terminal");
        resized
            .draw(|frame| app.render(frame))
            .expect("resize frame");
        assert_eq!(app.scroll_follow_count - before_resize, 1);

        let before_escape = app.scroll_follow_count;
        app.handle_event(&Event::Key(KeyEvent::new(
            CrosstermKeyCode::Esc,
            KeyModifiers::NONE,
        )));
        assert_eq!(app.scroll_follow_count, before_escape);
        resized
            .draw(|frame| app.render(frame))
            .expect("escape frame");
        assert_eq!(app.scroll_follow_count - before_escape, 1);
        assert_eq!(app.session().unwrap().mode(), Mode::Normal);
        let entry = &app.tabs[0];
        let rendered_cursor = entry.session.rendered_cursor_line();
        assert!(rendered_cursor >= entry.rendered_top);
        assert!(rendered_cursor < entry.rendered_top + app.viewport_height);
        assert!(
            started.elapsed() < std::time::Duration::from_secs(15),
            "large-fixture production path exceeded relaxed smoke ceiling"
        );
    }

    #[test]
    fn insert_edit_escape_rebuilds_rendered_cursor_before_scroll_follow() {
        let text: String = (0..40).map(|line| format!("line {line}\n")).collect();
        let mut app = test_app(EditorSession::from_text(&text));
        app.viewport_width = 20;
        app.viewport_height = 5;
        enter_insert(&mut app);
        for _ in 0..30 {
            press(&mut app, CrosstermKeyCode::Down);
        }
        press(&mut app, CrosstermKeyCode::Char('X'));
        press(&mut app, CrosstermKeyCode::Esc);

        let entry = &app.tabs[0];
        let cursor = entry.session.rendered_cursor_line();
        assert_eq!(entry.session.mode(), Mode::Normal);
        assert!(entry.rendered_top > 0);
        assert!(cursor >= entry.rendered_top);
        assert!(cursor < entry.rendered_top + app.viewport_height);
        assert!(entry.session.line(30).unwrap().starts_with('X'));

        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(20, 7)).unwrap();
        terminal.draw(|frame| app.render(frame)).unwrap();
        let entry = &app.tabs[0];
        let cursor = entry.session.rendered_cursor_line();
        assert!(cursor >= entry.rendered_top);
        assert!(cursor < entry.rendered_top + app.viewport_height);
    }

    #[test]
    fn scroll_follow_horizontal_adjusts_left_col() {
        let mut app = test_app(EditorSession::from_text(&"x".repeat(100)));
        app.wrap_enabled = false;
        app.viewport_width = 20;
        enter_insert(&mut app);
        press(&mut app, CrosstermKeyCode::End);
        assert!(app.left_col() > 0);
    }

    #[test]
    fn scroll_follow_horizontal_scrolloff_margin() {
        let mut app = test_app(EditorSession::from_text(&"x".repeat(100)));
        app.wrap_enabled = false;
        app.viewport_width = 20;
        enter_insert(&mut app);
        press(&mut app, CrosstermKeyCode::End);
        assert_eq!(app.left_col(), 85);
    }

    #[test]
    fn scroll_follow_horizontal_left_clamp() {
        let mut app = test_app(EditorSession::from_text(&"x".repeat(100)));
        app.wrap_enabled = false;
        app.viewport_width = 20;
        enter_insert(&mut app);
        press(&mut app, CrosstermKeyCode::End);
        press(&mut app, CrosstermKeyCode::Home);
        assert_eq!(app.left_col(), 0);
    }

    #[test]
    fn scroll_follow_wrap_visual_row_awareness() {
        let text = format!("{}\nsecond", "x".repeat(45));
        let mut app = test_app(EditorSession::from_text(&text));
        app.viewport_width = 10;
        app.viewport_height = 4;

        enter_insert(&mut app);
        press(&mut app, CrosstermKeyCode::Down);
        assert_eq!(app.top_line(), 1, "the wrapped first line no longer fits");
    }

    #[test]
    fn scroll_follow_wrap_upward_crossing_uses_top_margin() {
        let text = (0..30)
            .map(|line| line.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        let mut app = test_app(EditorSession::from_text(&text));
        app.viewport_width = 20;
        app.viewport_height = 10;
        for ch in [':', '1', '0'] {
            app.handle_event(&Event::Key(KeyEvent::new(
                CrosstermKeyCode::Char(ch),
                KeyModifiers::NONE,
            )));
        }
        app.handle_event(&Event::Key(KeyEvent::new(
            CrosstermKeyCode::Enter,
            KeyModifiers::NONE,
        )));
        enter_insert(&mut app);
        app.tabs[0].top_line = 10;

        app.scroll_follow();

        assert_eq!(app.top_line(), 6);
    }

    #[test]
    fn scroll_follow_wrap_advances_past_very_long_line() {
        let text = format!("{}\nsecond", "x".repeat(100));
        let mut app = test_app(EditorSession::from_text(&text));
        app.viewport_width = 10;
        app.viewport_height = 6;
        enter_insert(&mut app);
        press(&mut app, CrosstermKeyCode::Down);
        assert_eq!(app.top_line(), 1);
    }

    #[test]
    fn scroll_follow_wrap_skip_rows_for_tall_line() {
        let mut app = test_app(EditorSession::from_text(&"x".repeat(100)));
        app.viewport_width = 10;
        app.viewport_height = 5;
        enter_insert(&mut app);
        press(&mut app, CrosstermKeyCode::End);
        assert_eq!(app.skip_rows(), 7);
    }

    #[test]
    fn scroll_follow_skip_rows_resets_on_line_change() {
        let text = format!("{}\nsecond", "x".repeat(100));
        let mut app = test_app(EditorSession::from_text(&text));
        app.viewport_width = 10;
        app.viewport_height = 5;
        enter_insert(&mut app);
        press(&mut app, CrosstermKeyCode::End);
        assert!(app.skip_rows() > 0);
        press(&mut app, CrosstermKeyCode::Down);
        assert_eq!(app.top_line(), 1);
        assert_eq!(app.skip_rows(), 0);
    }

    #[test]
    fn scroll_follow_skip_rows_scrolloff() {
        let mut app = test_app(EditorSession::from_text(&"x".repeat(100)));
        app.viewport_width = 10;
        app.viewport_height = 5;
        enter_insert(&mut app);
        press(&mut app, CrosstermKeyCode::End);
        let (cursor_line, cursor_col) = app.session().unwrap().cursor();
        let (visual_row, _) =
            app.session()
                .unwrap()
                .visual_row_info(cursor_line, cursor_col, 10, true);
        assert_eq!(visual_row - app.skip_rows(), 2);
    }

    #[test]
    fn app_set_wrap_toggles_runtime_state() {
        let mut app = test_app(EditorSession::from_text("text"));
        app.wrap_enabled = false;
        app.handle_effect(Effect::SetOption {
            key: "wrap".to_string(),
            value: true,
        });
        assert!(app.wrap_enabled);
        assert_eq!(app.transient.as_ref().unwrap().text, "wrap");
    }

    #[test]
    fn app_set_nowrap_triggers_horizontal_follow() {
        let mut app = test_app(EditorSession::from_text(&"x".repeat(100)));
        app.viewport_width = 20;
        enter_insert(&mut app);
        press(&mut app, CrosstermKeyCode::End);
        app.handle_effect(Effect::SetOption {
            key: "wrap".to_string(),
            value: false,
        });
        let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(24, 6))
            .expect("test terminal");
        terminal
            .draw(|frame| app.render(frame))
            .expect("render pending follow");
        assert!(!app.wrap_enabled);
        assert!(app.left_col() > 0);
        assert_eq!(app.transient.as_ref().unwrap().text, "nowrap");
    }

    #[test]
    fn app_set_wrap_resets_left_col() {
        let mut app = test_app(EditorSession::from_text("text"));
        app.wrap_enabled = false;
        app.set_left_col(20);
        app.handle_effect(Effect::SetOption {
            key: "wrap".to_string(),
            value: true,
        });
        assert_eq!(app.left_col(), 0);
    }

    #[test]
    fn app_set_wrap_not_persisted() {
        let temp_dir = tempfile::tempdir().unwrap();
        let config_path = temp_dir.path().join("config.toml");
        crate::config::Config::default()
            .save_to_path(&config_path)
            .unwrap();
        let mut app = test_app(EditorSession::from_text("text"));
        app.handle_effect(Effect::SetOption {
            key: "wrap".to_string(),
            value: false,
        });
        assert!(
            crate::config::Config::load_from_path(&config_path)
                .editor
                .wrap
        );
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

    #[test]
    fn test_unmapped_keys_produce_noop() {
        for crossterm_code in unmapped_special_keys() {
            let key = KeyEvent::new(crossterm_code, KeyModifiers::NONE);
            let core = crossterm_key_to_core(&key);
            assert_eq!(
                core.code.kind,
                KeyCodeKind::Noop,
                "crossterm {crossterm_code:?} should map to Noop"
            );
        }
    }

    #[test]
    fn test_noop_not_space_or_esc() {
        for crossterm_code in unmapped_special_keys() {
            let key = KeyEvent::new(crossterm_code, KeyModifiers::NONE);
            let kind = crossterm_key_to_core(&key).code.kind;
            assert_ne!(kind, KeyCodeKind::Char(' '));
            assert_ne!(kind, KeyCodeKind::Char('\0'));
            assert_ne!(kind, KeyCodeKind::Esc);
        }
    }

    #[test]
    fn test_noop_has_no_behavioral_effect() {
        let mut normal = EditorSession::from_text("hello");

        let mut insert = EditorSession::from_text("hello");
        insert.handle_key(KeyInput {
            code: KeyCode {
                kind: KeyCodeKind::Char('i'),
            },
            mods: Modifiers::default(),
        });
        assert_eq!(insert.mode(), oom_edit_core::session::Mode::Insert);

        let mut command = EditorSession::from_text("hello");
        command.handle_key(KeyInput {
            code: KeyCode {
                kind: KeyCodeKind::Char(':'),
            },
            mods: Modifiers::default(),
        });
        assert_eq!(command.mode(), oom_edit_core::session::Mode::Command);

        for (mode_name, session) in [
            ("Normal", &mut normal),
            ("Insert", &mut insert),
            ("Command", &mut command),
        ] {
            let document_before = session.document();
            let cursor_before = session.cursor();
            let mode_before = session.mode();

            let effects = session.handle_key(KeyInput {
                code: KeyCode {
                    kind: KeyCodeKind::Noop,
                },
                mods: Modifiers::default(),
            });

            assert!(
                effects.is_empty(),
                "Noop emitted effects in {mode_name} mode"
            );
            assert_eq!(session.document(), document_before);
            assert_eq!(session.cursor(), cursor_before);
            assert_eq!(session.mode(), mode_before);
        }
    }

    #[test]
    fn test_unmapped_keys_have_no_app_behavioral_effect() {
        let mut normal = test_app(EditorSession::from_text("hello"));

        let mut insert = test_app(EditorSession::from_text("hello"));
        insert.handle_event(&Event::Key(KeyEvent::new(
            CrosstermKeyCode::Char('i'),
            KeyModifiers::NONE,
        )));
        assert_eq!(
            insert.session().unwrap().mode(),
            oom_edit_core::session::Mode::Insert
        );

        let mut command = test_app(EditorSession::from_text("hello"));
        command.handle_event(&Event::Key(KeyEvent::new(
            CrosstermKeyCode::Char(':'),
            KeyModifiers::NONE,
        )));
        assert_eq!(
            command.session().unwrap().mode(),
            oom_edit_core::session::Mode::Command
        );

        for (mode_name, app) in [
            ("Normal", &mut normal),
            ("Insert", &mut insert),
            ("Command", &mut command),
        ] {
            for crossterm_code in unmapped_special_keys() {
                let document_before = app.session().unwrap().document();
                let cursor_before = app.session().unwrap().cursor();
                let mode_before = app.session().unwrap().mode();
                let status_before = app.status_message.clone();

                app.handle_event(&Event::Key(KeyEvent::new(
                    crossterm_code,
                    KeyModifiers::NONE,
                )));

                assert_eq!(app.session().unwrap().document(), document_before);
                assert_eq!(app.session().unwrap().cursor(), cursor_before);
                assert_eq!(app.session().unwrap().mode(), mode_before);
                assert_eq!(app.status_message, status_before);
                assert!(!app.should_quit, "{crossterm_code:?} quit in {mode_name}");
                assert!(!app.overlay.is_some());
                assert!(app.transient.is_none());
            }
        }
    }

    #[test]
    fn test_unmapped_key_preserves_pending_space_chord() {
        let mut app = test_app(EditorSession::from_text("hello"));

        app.handle_event(&Event::Key(KeyEvent::new(
            CrosstermKeyCode::Char(' '),
            KeyModifiers::NONE,
        )));
        assert!(app.pending_chord.since.is_some());

        app.handle_event(&Event::Key(KeyEvent::new(
            CrosstermKeyCode::Insert,
            KeyModifiers::NONE,
        )));
        assert!(app.pending_chord.since.is_some());

        app.handle_event(&Event::Key(KeyEvent::new(
            CrosstermKeyCode::Char('h'),
            KeyModifiers::NONE,
        )));
        assert!(app.overlay.is_palette());
    }

    #[test]
    fn test_unmapped_key_preserves_pending_g_chord() {
        let mut app = test_app(EditorSession::from_text("first"));
        app.tabs
            .push(TabEntry::new(EditorSession::from_text("second")));

        app.handle_event(&Event::Key(KeyEvent::new(
            CrosstermKeyCode::Char('g'),
            KeyModifiers::NONE,
        )));
        assert!(app.pending_g);

        app.handle_event(&Event::Key(KeyEvent::new(
            CrosstermKeyCode::Insert,
            KeyModifiers::NONE,
        )));
        assert!(app.pending_g);

        app.handle_event(&Event::Key(KeyEvent::new(
            CrosstermKeyCode::Char('t'),
            KeyModifiers::NONE,
        )));
        assert_eq!(app.active_tab, 1);
    }

    #[test]
    fn app_f1_does_not_open_palette() {
        let session = EditorSession::from_text("hello");
        let mut app = test_app(session);
        let key = KeyEvent::new(CrosstermKeyCode::F(1), KeyModifiers::NONE);
        app.handle_event(&Event::Key(key));

        assert!(!app.overlay.is_some());
    }

    #[test]
    fn app_space_h_opens_palette() {
        let session = EditorSession::from_text("hello");
        let mut app = test_app(session);
        open_palette_with_space_h(&mut app);

        assert!(app.overlay.is_palette());
    }

    #[test]
    fn app_plain_v_enters_select() {
        let session = EditorSession::from_text("hello");
        let mut app = test_app(session);
        let v = KeyEvent::new(CrosstermKeyCode::Char('v'), KeyModifiers::NONE);
        app.handle_event(&Event::Key(v));
        assert_eq!(
            app.session().unwrap().mode(),
            oom_edit_core::session::Mode::Select
        );
    }

    #[test]
    fn replacing_a_session_resets_rendered_scroll_to_the_canonical_top() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("short.md");
        std::fs::write(&path, "short\n").unwrap();
        let mut app = test_app(EditorSession::from_text(
            "one\n\ntwo\n\nthree\n\nfour\n\nfive\n",
        ));
        app.set_rendered_top(8);

        app.handle_effect(Effect::OpenRequested { path, force: true });

        assert_eq!(app.active().unwrap().rendered_top, 0);
        assert_eq!(app.session().unwrap().cursor(), (0, 0));
    }

    #[test]
    fn app_dispatches_every_select_binding_in_select_context() {
        fn send(app: &mut App, ch: char) {
            app.handle_event(&Event::Key(KeyEvent::new(
                CrosstermKeyCode::Char(ch),
                KeyModifiers::NONE,
            )));
        }

        let mut cancel = test_app(EditorSession::from_text("# one\n# two\n"));
        send(&mut cancel, 'v');
        cancel.handle_event(&Event::Key(KeyEvent::new(
            CrosstermKeyCode::Esc,
            KeyModifiers::NONE,
        )));
        assert_eq!(cancel.session().unwrap().mode(), Mode::Normal);

        let mut yank = test_app(EditorSession::from_text("# one\n# two\n"));
        send(&mut yank, 'V');
        send(&mut yank, 'y');
        send(&mut yank, 'p');
        assert_eq!(yank.session().unwrap().document(), "# one\n# one\n# two\n");

        for operator in ['d', 'x'] {
            let mut delete = test_app(EditorSession::from_text("# one\n# two\n"));
            send(&mut delete, 'V');
            send(&mut delete, operator);
            assert_eq!(delete.session().unwrap().document(), "# two\n");
        }

        let mut change = test_app(EditorSession::from_text("# one\n# two\n"));
        send(&mut change, 'V');
        send(&mut change, 'c');
        assert_eq!(change.session().unwrap().mode(), Mode::Insert);

        let mut indent = test_app(EditorSession::from_text("# one\n# two\n"));
        send(&mut indent, 'V');
        send(&mut indent, '>');
        assert_eq!(indent.session().unwrap().document(), "    # one\n# two\n");
        indent.session_mut().unwrap().render_layout(74);
        send(&mut indent, 'V');
        send(&mut indent, '<');
        assert_eq!(indent.session().unwrap().document(), "# one\n# two\n");

        let mut swap = test_app(EditorSession::from_text("# one\n# two\n"));
        send(&mut swap, 'v');
        send(&mut swap, 'j');
        let before = swap.session().unwrap().rendered_selection().unwrap();
        send(&mut swap, 'o');
        let after = swap.session().unwrap().rendered_selection().unwrap();
        assert_eq!((after.anchor, after.active), (before.active, before.anchor));
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

    #[test]
    fn test_space_digit_jumps_to_correct_tab() {
        let mut app = test_app_with_tabs(9);
        assert_eq!(app.tab_count(), 9);

        for tab_num in 1..=9 {
            app.handle_event(&Event::Key(KeyEvent::new(
                CrosstermKeyCode::Char(' '),
                KeyModifiers::NONE,
            )));
            app.handle_event(&Event::Key(KeyEvent::new(
                CrosstermKeyCode::Char(char::from_digit(tab_num as u32, 10).unwrap()),
                KeyModifiers::NONE,
            )));

            assert_eq!(
                app.active_tab,
                tab_num - 1,
                "Space+{tab_num} should activate tab {tab_num}"
            );
        }
    }

    #[test]
    fn test_space_digit_noop_when_tab_missing() {
        let mut app = test_app_with_tabs(3);
        app.active_tab = 1;

        app.handle_event(&Event::Key(KeyEvent::new(
            CrosstermKeyCode::Char(' '),
            KeyModifiers::NONE,
        )));
        app.handle_event(&Event::Key(KeyEvent::new(
            CrosstermKeyCode::Char('5'),
            KeyModifiers::NONE,
        )));

        assert_eq!(app.active_tab, 1);
    }

    #[test]
    fn test_space_9_works() {
        let mut app = test_app_with_tabs(9);

        app.handle_event(&Event::Key(KeyEvent::new(
            CrosstermKeyCode::Char(' '),
            KeyModifiers::NONE,
        )));
        app.handle_event(&Event::Key(KeyEvent::new(
            CrosstermKeyCode::Char('9'),
            KeyModifiers::NONE,
        )));

        assert_eq!(app.active_tab, 8);
    }

    #[test]
    fn test_effect_tab_jump_uses_one_based_index() {
        let mut app = test_app_with_tabs(3);

        app.handle_effect(Effect::TabJump { index: 3 });

        assert_eq!(app.active_tab, 2);
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

        open_palette_with_space_h(&mut app);
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

        open_palette_with_space_h(&mut app);
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

        open_palette_with_space_h(&mut app);
        assert!(app.overlay.is_palette());

        // Enter executes the selected command (first row = character Select).
        let enter = KeyEvent::new(CrosstermKeyCode::Enter, KeyModifiers::NONE);
        app.handle_event(&Event::Key(enter));

        // Palette should be closed and character Select should have executed.
        assert!(!app.overlay.is_some());
        assert_eq!(
            app.session().unwrap().mode(),
            oom_edit_core::session::Mode::Select
        );
    }

    /// T12: Palette reference entry — Enter on Vim reference shows status.
    #[test]
    fn app_palette_reference_entry() {
        let session = EditorSession::from_text("hello");
        let mut app = test_app(session);

        open_palette_with_space_h(&mut app);
        assert!(app.overlay.is_palette(), "palette should be open");

        // Navigate down past every executable command to the first reference row.
        for _ in 0..crate::command::registry::COMMANDS.len() {
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

        open_palette_with_space_h(&mut app);
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

        // Space h should open the palette through keymap dispatch.
        open_palette_with_space_h(&mut app);

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
        let temp_dir = tempfile::tempdir().unwrap();
        app.config_store = Box::new(crate::config::FileConfigStore::new(
            temp_dir.path().join("oom-edit/config.toml"),
        ));

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

    #[test]
    fn generic_test_app_cannot_persist_config() {
        let mut app = test_app(EditorSession::from_text("hello"));
        let original = app.theme_name.clone();
        app.execute_command(Command::CycleTheme);
        assert_ne!(app.theme_name, original);
        assert!(app
            .transient
            .as_ref()
            .is_some_and(|message| message.text.contains("theme:")));
        // `test_app` injects DisabledConfigStore, whose save operation has no
        // filesystem path and therefore cannot resolve the production config.
        assert!(app
            .config_store
            .save(&crate::config::Config::default())
            .is_ok());
    }

    #[test]
    fn app_cycle_theme_never_persists_opposite_mode_theme() {
        struct Case {
            name: &'static str,
            current_theme: &'static str,
            is_light: bool,
            expected_theme: &'static str,
        }

        for case in [
            Case {
                name: "dark mode cycles to accessible",
                current_theme: "default-dark",
                is_light: false,
                expected_theme: "accessible",
            },
            Case {
                name: "dark mode recovers an incompatible current theme",
                current_theme: "default-light",
                is_light: false,
                expected_theme: "default-dark",
            },
            Case {
                name: "light mode cycles to accessible",
                current_theme: "default-light",
                is_light: true,
                expected_theme: "accessible",
            },
            Case {
                name: "light mode recovers an incompatible current theme",
                current_theme: "default-dark",
                is_light: true,
                expected_theme: "default-light",
            },
        ] {
            let temp_dir = tempfile::tempdir().unwrap();
            let config_path = temp_dir.path().join("oom-edit/config.toml");
            let mut initial_config = crate::config::Config::default();
            initial_config.theme.mode = Some(if case.is_light { "light" } else { "dark" }.into());
            initial_config.theme.dark = "saved-dark".to_string();
            initial_config.theme.light = "saved-light".to_string();
            initial_config.relative_line_numbers = true;
            initial_config.editor.wrap = false;
            initial_config.save_to_path(&config_path).unwrap();

            let mut app = App::new(
                EditorSession::from_text(""),
                theme::ResolvedTheme::injected(case.current_theme, case.is_light, Tier::TrueColor),
                initial_config.editor.wrap,
                initial_config.relative_line_numbers,
                Box::new(RecordingClipboardSink::default()),
                Box::new(crate::config::FileConfigStore::new(config_path.clone())),
            );

            app.execute_command(Command::CycleTheme);

            let persisted = crate::config::Config::load_from_path(&config_path);
            let mut expected_config = initial_config.clone();
            if case.is_light {
                expected_config.theme.light = case.expected_theme.to_string();
            } else {
                expected_config.theme.dark = case.expected_theme.to_string();
            }
            assert_eq!(app.theme_name, case.expected_theme, "{}", case.name);
            assert_eq!(persisted, expected_config, "{}", case.name);
        }
    }

    #[test]
    fn app_cycle_theme_keeps_mode_appropriate_slot_after_accessible_restart() {
        for (mode, expected_theme) in [("dark", "default-dark"), ("light", "default-light")] {
            let temp_dir = tempfile::tempdir().unwrap();
            let config_path = temp_dir.path().join("oom-edit/config.toml");
            let mut initial_config = crate::config::Config::default();
            initial_config.theme.mode = Some(mode.to_string());
            initial_config.theme.dark = if mode == "dark" {
                "accessible".to_string()
            } else {
                "saved-dark".to_string()
            };
            initial_config.theme.light = if mode == "light" {
                "accessible".to_string()
            } else {
                "saved-light".to_string()
            };
            initial_config.save_to_path(&config_path).unwrap();

            let resolved = theme::resolve_theme(
                None,
                initial_config.theme.mode.as_deref(),
                Some(&initial_config.theme.dark),
                Some(&initial_config.theme.light),
                &theme::EnvParts::default(),
            );
            let mut app = App::new(
                EditorSession::from_text(""),
                resolved.clone(),
                true,
                false,
                Box::new(RecordingClipboardSink::default()),
                Box::new(crate::config::FileConfigStore::new(config_path.clone())),
            );

            app.execute_command(Command::CycleTheme);

            let persisted = crate::config::Config::load_from_path(&config_path);
            let mut expected_config = initial_config.clone();
            if resolved.is_light() {
                expected_config.theme.light = expected_theme.to_string();
            } else {
                expected_config.theme.dark = expected_theme.to_string();
            }
            assert_eq!(app.theme_name, expected_theme, "{mode} mode");
            assert_eq!(persisted, expected_config, "{mode} mode");
        }
    }

    /// Resize in rendered Normal remaps cursor to the same source content.
    #[test]
    fn app_resize_rendered_remaps_cursor() {
        // Multi-line text that wraps differently at different widths.
        let text = "# Intro\n\nThis opening paragraph is deliberately long enough to wrap at forty columns but not at eighty columns.\n\n## Target heading\n\nThis trailing paragraph is also deliberately long enough to make the narrow layout visibly different.\n";
        let session = EditorSession::from_text(text);
        let mut app = test_app(session);

        app.handle_event(&Event::Resize(80, 6));
        assert_eq!(
            app.session().unwrap().mode(),
            oom_edit_core::session::Mode::Normal
        );

        // Navigate to the target heading in rendered mode.
        let down = KeyEvent::new(CrosstermKeyCode::Down, KeyModifiers::NONE);
        let content_line = |app: &App| {
            let session = app.session().unwrap();
            let cursor = session.rendered_cursor_line();
            let source_start = session.rendered_layout().unwrap().lines[cursor]
                .source
                .start;
            text[..source_start]
                .bytes()
                .filter(|byte| *byte == b'\n')
                .count()
        };
        while content_line(&app) < 4 {
            app.handle_event(&Event::Key(down));
        }
        assert_eq!(content_line(&app), 4);
        let wide_line_count = app
            .session()
            .unwrap()
            .rendered_layout()
            .unwrap()
            .lines
            .len();

        // Simulate a narrow resize that changes wrapping.
        let resize = Event::Resize(40, 6);
        app.handle_event(&resize);

        let narrow_line_count = app
            .session()
            .unwrap()
            .rendered_layout()
            .unwrap()
            .lines
            .len();
        assert!(
            narrow_line_count > wide_line_count,
            "narrow resize should reflow the rendered layout"
        );
        assert_eq!(
            content_line(&app),
            4,
            "rendered cursor should remain on the target heading's logical source line"
        );
        let cursor = app.session().unwrap().rendered_cursor_line();
        let rendered_top = app.active().unwrap().rendered_top;
        assert!(
            (rendered_top..rendered_top + app.viewport_height).contains(&cursor),
            "remapped rendered cursor should remain visible after resize"
        );
    }

    #[test]
    fn app_resize_with_tabs_keeps_rendered_cursor_visible() {
        let text = "# Intro\n\nThis opening paragraph is deliberately long enough to wrap at forty columns but not at eighty columns.\n\n## Target heading\n\nThis trailing paragraph is also deliberately long enough to make the narrow layout visibly different.\n";
        let mut app = test_app(EditorSession::from_text(text));

        app.handle_event(&Event::Resize(80, 6));

        let wide_line_count = app
            .session()
            .unwrap()
            .rendered_layout()
            .unwrap()
            .lines
            .len();
        let down = KeyEvent::new(CrosstermKeyCode::Down, KeyModifiers::NONE);
        for _ in 0..wide_line_count {
            app.handle_event(&Event::Key(down));
        }
        assert_eq!(
            app.session().unwrap().rendered_cursor_line(),
            wide_line_count - 1
        );

        app.tabs
            .push(TabEntry::new(EditorSession::from_text("second tab")));
        app.handle_event(&Event::Resize(40, 6));

        assert_eq!(app.viewport_height, 4);
        let cursor = app.session().unwrap().rendered_cursor_line();
        let rendered_top = app.active().unwrap().rendered_top;
        assert!(
            (rendered_top..rendered_top + app.viewport_height).contains(&cursor),
            "remapped rendered cursor should remain visible below the tab bar"
        );
    }

    #[test]
    fn app_render_propagates_light_theme_to_editor_status_and_tabs() {
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;

        let mut app = App::new(
            EditorSession::from_text("first\n\nplain"),
            theme::ResolvedTheme::injected("default-light", true, Tier::TrueColor),
            true,
            false,
            Box::new(RecordingClipboardSink::default()),
            Box::new(crate::config::DisabledConfigStore),
        );
        app.tabs
            .push(TabEntry::new(EditorSession::from_text("second")));
        let mut terminal = Terminal::new(TestBackend::new(80, 10)).unwrap();

        terminal.draw(|frame| app.render(frame)).unwrap();

        let buffer = terminal.backend().buffer();
        let theme = &theme::DEFAULT_LIGHT;
        let text_style = theme.style(Tier::TrueColor, oom_edit_core::SemanticStyle::Text);
        let cursor_style = theme.ui_style(Tier::TrueColor, theme::UiSlot::CursorLine);
        let body_cells = (1..9).flat_map(|y| (6..80).filter_map(move |x| buffer.cell((x, y))));
        let cells: Vec<_> = body_cells.collect();
        assert!(cells.iter().any(|cell| {
            cell.fg == text_style.fg.expect("light text foreground")
                && cell.bg == cursor_style.bg.expect("light cursor-line background")
        }));
        assert!(cells.iter().any(|cell| {
            cell.symbol() != " "
                && cell.fg == text_style.fg.expect("light text foreground")
                && cell.bg != cursor_style.bg.expect("light cursor-line background")
        }));

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
            theme::ResolvedTheme::injected("default-dark", false, Tier::Monochrome),
            true,
            false,
            Box::new(RecordingClipboardSink::default()),
            Box::new(crate::config::DisabledConfigStore),
        );
        app.overlay = Overlay::open_palette(Contexts::NORMAL);
        for ch in "rendered row".chars() {
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
        let muted_cell = (3..21)
            .flat_map(|y| (8..72).map(move |x| (x, y)))
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
