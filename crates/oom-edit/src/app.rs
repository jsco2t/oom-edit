//! App — the single source of truth for the running TUI.
//!
//! Holds the tab stack, scroll positions, last status message, overlay
//! state, and the quit flag. After each event: drain effects, scroll-follow,
//! update status message.
//!
//! ## Key routing order (arch §7.1)
//!
//! 1. Overlay open → overlay's key handler (take-and-return-bool).
//! 2. Mode ∈ {Normal, Select}: apply the pure registry-derived Space-prefix
//!    transition. App commands execute here; every other key falls through.
//! 3. Everything else → active session's `handle_key(key)`, then drain `Effect`s.

use std::time::Instant;

use crossterm::event::{Event, KeyCode as CrosstermKeyCode, KeyEvent, KeyModifiers};
use ratatui::Frame;

use oom_edit_core::ClipboardSink;
use oom_edit_core::{EditorSession, Effect, KeyCode, KeyCodeKind, KeyInput, Modifiers};

use crossterm::event::MouseEventKind;

use crate::command::keymap::{
    resolve as resolve_app_input, AppInputTransition, PendingAppInput, TabAction,
};
use crate::command::AppCommand;
use crate::config::ConfigStore;
use crate::lifecycle::{
    CloseTabRequest, DirtyClosePolicy, LifecycleAction, SaveContinuation, SaveRequest,
};
use crate::overlay::{Overlay, SpellSuggestAction, TroubleAction, TroubleEntry, TroubleProgress};
use crate::screens::editor::{render_editor, render_status_row, source_text_width, EditorViewport};
use crate::screens::rendered::render_rendered;
use crate::spell_host::SpellHost;
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

/// Explicit host-side services injected when constructing the TUI state.
pub(crate) struct AppServices {
    clipboard_sink: Box<dyn ClipboardSink>,
    config_store: Box<dyn ConfigStore>,
    spell_host: SpellHost,
}

impl AppServices {
    pub(crate) fn new(
        clipboard_sink: Box<dyn ClipboardSink>,
        config_store: Box<dyn ConfigStore>,
        spell_host: SpellHost,
    ) -> Self {
        Self {
            clipboard_sink,
            config_store,
            spell_host,
        }
    }
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
    /// Sole App-owned pending input state.
    pub pending_input: PendingAppInput,
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
    /// One resumable spell engine shared by every tab.
    spell_host: SpellHost,
    /// Configured default applied independently to every newly-created session.
    spell_enabled_default: bool,
    /// Time of the most recently observed terminal input event.
    last_input: Instant,
    /// Active capability tier.
    tier: Tier,
    /// Clipboard sink for OSC 52 clipboard writes (T16).
    clipboard_sink: Box<dyn ClipboardSink>,
    #[cfg(test)]
    scroll_follow_count: usize,
}

impl App {
    /// Create a new App from an open session (starts with one tab).
    #[cfg(test)]
    pub fn new(
        session: EditorSession,
        resolved_theme: ResolvedTheme,
        wrap_enabled: bool,
        relative_line_numbers: bool,
        clipboard_sink: Box<dyn ClipboardSink>,
        config_store: Box<dyn ConfigStore>,
        initial_time: Instant,
    ) -> Self {
        Self::new_with_spell(
            session,
            resolved_theme,
            wrap_enabled,
            relative_line_numbers,
            AppServices::new(
                clipboard_sink,
                config_store,
                SpellHost::testing("a\nan\nand\nknown\nspell\ntext\nthe\nword\n"),
            ),
            true,
            initial_time,
        )
    }

    /// Create an App with explicit spell resources and configured session default.
    pub(crate) fn new_with_spell(
        session: EditorSession,
        resolved_theme: ResolvedTheme,
        wrap_enabled: bool,
        relative_line_numbers: bool,
        services: AppServices,
        spell_enabled_default: bool,
        initial_time: Instant,
    ) -> Self {
        let is_light = resolved_theme.is_light();
        let tier = resolved_theme.capability;
        let theme_name = resolved_theme.name;
        let session = Self::seed_spell_config(session, spell_enabled_default);
        Self {
            tabs: vec![TabEntry::new(session)],
            active_tab: 0,
            should_quit: false,
            status_message: String::new(),
            overlay: Overlay::default(),
            pending_input: PendingAppInput::Idle,
            viewport_height: 22,
            viewport_width: 76,
            pending_scroll_follow: true,
            last_follow_geometry: None,
            wrap_enabled,
            relative_line_numbers,
            now: initial_time,
            transient: None,
            theme_name,
            is_light,
            config_store: services.config_store,
            spell_host: services.spell_host,
            spell_enabled_default,
            last_input: initial_time,
            tier,
            clipboard_sink: services.clipboard_sink,
            #[cfg(test)]
            scroll_follow_count: 0,
        }
    }

    fn seed_spell_config(mut session: EditorSession, enabled: bool) -> EditorSession {
        session.set_spell_enabled(enabled);
        session
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
        let which_key_deadline = match self.pending_input {
            PendingAppInput::Space { since } => Some(since + std::time::Duration::from_millis(150)),
            PendingAppInput::Idle => None,
        };

        transient_deadline
            .into_iter()
            .chain(which_key_deadline)
            .min()
            .filter(|deadline| *deadline >= now)
    }

    /// Record the post-read timestamp for any terminal input event.
    pub(crate) fn record_input(&mut self, now: Instant) {
        self.last_input = now;
    }

    /// Return whether the app has been input-idle for at least `duration`.
    pub(crate) fn input_idle_for(&self, now: Instant, duration: std::time::Duration) -> bool {
        now.saturating_duration_since(self.last_input) >= duration
    }

    /// Advance one bounded host-build or active-session scan unit.
    pub(crate) fn on_idle_unit(&mut self, max_bytes: usize) -> bool {
        let enabled = self
            .tabs
            .get(self.active_tab)
            .is_some_and(|entry| entry.session.spell_enabled());
        let worked = if !enabled {
            false
        } else if self.spell_host.engine().is_none() {
            let worked = self.spell_host.advance(true, max_bytes);
            if let Some(message) = self.spell_host.take_unavailable_warning() {
                self.set_transient(message, oom_edit_core::Severity::Warning);
            }
            worked
        } else {
            let Some(engine) = self.spell_host.engine() else {
                return false;
            };
            self.tabs
                .get_mut(self.active_tab)
                .is_some_and(|entry| entry.session.spell_tick(engine, max_bytes))
        };
        self.refresh_trouble_snapshot();
        worked
    }

    #[cfg(test)]
    pub(crate) fn spell_host_phase(&self) -> &'static str {
        self.spell_host.phase_name()
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
            if entry.session.mode() != oom_edit_core::Mode::Insert {
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
            Some(oom_edit_core::Mode::Normal) => crate::command::registry::Contexts::NORMAL,
            Some(oom_edit_core::Mode::Insert) => crate::command::registry::Contexts::INSERT,
            Some(oom_edit_core::Mode::Select) => crate::command::registry::Contexts::SELECT,
            Some(oom_edit_core::Mode::Command) => crate::command::registry::Contexts::COMMAND,
            None => crate::command::registry::Contexts::NORMAL,
        }
    }

    /// Return true when the current mode supports Space-chord and g-chord keymaps.
    fn in_chord_context(&self) -> bool {
        self.session()
            .map(|s| {
                matches!(
                    s.mode(),
                    oom_edit_core::Mode::Normal | oom_edit_core::Mode::Select
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

        let PendingAppInput::Space { since } = self.pending_input else {
            return;
        };

        if !which_key::should_show(Some(since), self.now) {
            return;
        }

        let ctx = self.mode_context();
        if let Some(text) = which_key::build_hint(ctx) {
            let content_offset =
                crate::widgets::status_bar::STATUS_CONTENT_OFFSET.min(status_area.width);
            let flexible_area = ratatui::layout::Rect::new(
                status_area.x.saturating_add(content_offset),
                status_area.y,
                status_area.width.saturating_sub(content_offset),
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
    #[cfg(test)]
    pub fn handle_event(&mut self, event: &Event) {
        self.handle_event_at(event, self.now);
    }

    /// Handle one event stamped with the exact post-read time.
    pub fn handle_event_at(&mut self, event: &Event, now: Instant) {
        self.now = now;
        self.record_input(now);
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
                if entry.session.mode() != oom_edit_core::Mode::Insert {
                    let text_width = source_text_width(*_width, entry.session.line_count());
                    entry.session.render_layout(text_width);
                }
            }
            self.scroll_follow();
            self.pending_scroll_follow = false;
            return;
        }

        // Suggestion input is fully modal. Resize remains a presentation
        // event, but paste and mouse input cannot reach the document beneath.
        if (self.overlay.is_spell_suggest() || self.overlay.is_trouble())
            && matches!(event, Event::Paste(_) | Event::Mouse(_))
        {
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

        // 1. Confirmation input is exclusive and resolves semantic choices
        // immediately on y/w/n/o/r, Enter, Esc, or Ctrl-C.
        if self.overlay.is_confirmation() {
            if let Some(resolution) = self.overlay.handle_confirmation_key(&key_input) {
                self.overlay.close();
                self.pending_input = PendingAppInput::Idle;
                self.execute_confirmation(resolution);
            }
            return;
        }

        // The spelling modal owns every key while open. Its state machine
        // returns semantic requests; App performs session/host mutations.
        if let Some(action) = self.overlay.handle_spell_suggest_key(&key_input) {
            self.execute_spell_suggest_action(action);
            return;
        }

        // Trouble is equally modal: App revalidates requested jumps, while
        // every other key remains owned by its presentation state.
        if let Some(action) = self.overlay.handle_trouble_key(&key_input) {
            self.execute_trouble_action(action);
            return;
        }

        // Other overlays retain their own input protocol.
        if self.overlay.is_some() {
            // Esc closes the overlay (handled by returning false).
            let consumed = self.overlay.handle_key(&key_input);
            if !consumed {
                // Esc or other close key: close overlay and fall through.
                if matches!(event, Event::Key(key) if key.code == CrosstermKeyCode::Esc)
                    || matches!(key_input.code.kind, KeyCodeKind::Char('c') if key_input.mods.ctrl)
                {
                    self.overlay.close();
                    self.pending_input = PendingAppInput::Idle;
                    // Don't fall through — Esc on palette is consumed.
                    return;
                }

                if matches!(key_input.code.kind, KeyCodeKind::Enter) {
                    if let Some(cmd) = self.overlay.selected_command() {
                        self.overlay.close();
                        self.pending_input = PendingAppInput::Idle;
                        self.execute_command(cmd);
                    } else {
                        self.overlay.close();
                        self.pending_input = PendingAppInput::Idle;
                        self.set_transient(
                            "reference entry".to_string(),
                            oom_edit_core::Severity::Info,
                        );
                    }
                    return;
                }

                // Other keys are consumed by the palette's filter navigation.
                return;
            }
            return;
        }

        // 2. Normal/Select owns only Space chords. Every g/count/native Vim
        // key is forwarded immediately and unchanged to the core.
        let key_input = if self.in_chord_context() {
            let transition =
                resolve_app_input(self.pending_input, self.mode_context(), key_input, self.now);
            self.pending_input = PendingAppInput::Idle;
            match transition {
                AppInputTransition::Pending(pending) => {
                    self.pending_input = pending;
                    return;
                }
                AppInputTransition::AppCommand(command) => {
                    self.execute_command(command);
                    return;
                }
                AppInputTransition::TabAction(action) => {
                    self.execute_tab_action(action);
                    return;
                }
                AppInputTransition::Forward(input) => input,
            }
        } else {
            self.pending_input = PendingAppInput::Idle;
            key_input
        };

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
                let session = Self::seed_spell_config(session, self.spell_enabled_default);
                let idx = self.tabs.len();
                self.tabs.push(TabEntry::new(session));
                self.active_tab = idx;
                self.pending_scroll_follow = true;
                self.set_transient(
                    format!("Opened: {}", path.display()),
                    oom_edit_core::Severity::Info,
                );
            }
            Err(e) => {
                self.set_transient(format!("Open error: {e}"), oom_edit_core::Severity::Error);
            }
        }
    }

    /// Actually close a tab at the given index (no dirty check).
    fn do_close_tab(&mut self, idx: usize) {
        self.tabs.remove(idx);
        if idx < self.active_tab {
            self.active_tab -= 1;
        } else if self.active_tab >= self.tabs.len() {
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

    fn execute_tab_action(&mut self, action: TabAction) {
        match action {
            TabAction::Next => self.next_tab(),
            TabAction::Prev => self.prev_tab(),
            TabAction::Jump(one_based) if one_based.get() <= self.tab_count() => {
                self.jump_to_tab(one_based.get());
            }
            TabAction::Jump(one_based) => self.set_transient(
                format!("No tab {}", one_based.get()),
                oom_edit_core::Severity::Warning,
            ),
        }
    }

    fn execute_confirmation(&mut self, resolution: crate::overlay::ConfirmationResolution) {
        use crate::overlay::{DirtyCloseChoice, ExternalSaveChoice};

        match resolution {
            crate::overlay::ConfirmationResolution::DirtyClose { action, choice } => match choice {
                DirtyCloseChoice::SaveAndClose => {
                    self.execute_lifecycle(LifecycleAction::Save(SaveRequest {
                        target: action.target,
                        path: None,
                        force: false,
                        retarget: true,
                        continuation: SaveContinuation::CloseSavedTab,
                    }));
                }
                DirtyCloseChoice::Discard => {
                    if action.target < self.tabs.len() {
                        self.do_close_tab(action.target);
                    } else {
                        self.invalid_lifecycle_target(action.target);
                    }
                }
                DirtyCloseChoice::Cancel => {}
            },
            crate::overlay::ConfirmationResolution::ExternalSave {
                mut request,
                disk_path,
                choice,
            } => match choice {
                ExternalSaveChoice::Overwrite => {
                    request.force = true;
                    self.execute_lifecycle(LifecycleAction::Save(request));
                }
                ExternalSaveChoice::Reload => {
                    self.replace_tab_from_disk(request.target, &disk_path, true);
                }
                ExternalSaveChoice::Cancel => {}
            },
        }
    }

    /// Execute a command from the registry.
    fn execute_command(&mut self, cmd: AppCommand) {
        match cmd {
            AppCommand::Help => {
                // Open the command palette.
                self.overlay = Overlay::open_palette(self.mode_context());
            }
            AppCommand::Save => {
                self.execute_lifecycle(LifecycleAction::Save(SaveRequest {
                    target: self.active_tab,
                    path: None,
                    force: false,
                    retarget: true,
                    continuation: SaveContinuation::StayOpen,
                }));
            }
            AppCommand::Quit => {
                self.execute_lifecycle(LifecycleAction::CloseTab(CloseTabRequest {
                    target: self.active_tab,
                    force: false,
                    dirty_policy: DirtyClosePolicy::Confirm,
                }));
            }
            AppCommand::CycleTheme => {
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
                    oom_edit_core::Severity::Info,
                );
            }
            AppCommand::SpellSuggest => self.open_spell_suggestions(),
            AppCommand::SpellAdd => self.add_current_spelling_word(),
            AppCommand::SpellToggle => {
                let Some(entry) = self.tabs.get_mut(self.active_tab) else {
                    return;
                };
                let enabled = !entry.session.spell_enabled();
                entry.session.set_spell_enabled(enabled);
                self.set_transient(
                    if enabled {
                        "spell checking enabled".to_string()
                    } else {
                        "spell checking disabled".to_string()
                    },
                    oom_edit_core::Severity::Info,
                );
            }
            AppCommand::Trouble => self.open_trouble(),
        }
    }

    fn trouble_snapshot(&self) -> (Vec<TroubleEntry>, TroubleProgress) {
        let entries = self
            .session()
            .map(|session| {
                session
                    .diagnostics()
                    .iter()
                    .filter_map(|diagnostic| {
                        session
                            .position_for_offset(diagnostic.range.start)
                            .map(|position| TroubleEntry::new(diagnostic.clone(), position))
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        let progress = if !self.session().is_some_and(EditorSession::spell_enabled) {
            TroubleProgress::Complete
        } else if let Some(reason) = self.spell_host.unavailable_reason() {
            TroubleProgress::Unavailable(reason.to_string())
        } else if self.spell_host.engine().is_none()
            || self
                .session()
                .is_some_and(EditorSession::diagnostics_pending)
        {
            TroubleProgress::Pending
        } else {
            TroubleProgress::Complete
        };
        (entries, progress)
    }

    fn open_trouble(&mut self) {
        let (entries, progress) = self.trouble_snapshot();
        self.overlay = Overlay::open_trouble(entries, progress);
        self.pending_input = PendingAppInput::Idle;
    }

    fn refresh_trouble_snapshot(&mut self) {
        if !self.overlay.is_trouble() {
            return;
        }
        let (entries, progress) = self.trouble_snapshot();
        self.overlay.refresh_trouble(entries, progress);
    }

    fn execute_trouble_action(&mut self, action: TroubleAction) {
        match action {
            TroubleAction::StayOpen => {}
            TroubleAction::Close => {
                self.overlay.close();
                self.pending_input = PendingAppInput::Idle;
            }
            TroubleAction::Jump(diagnostic) => self.jump_to_trouble_diagnostic(&diagnostic),
        }
    }

    fn jump_to_trouble_diagnostic(&mut self, diagnostic: &oom_edit_core::Diagnostic) {
        let current = self.session().is_some_and(|session| {
            session
                .diagnostics()
                .iter()
                .any(|candidate| candidate == diagnostic)
                && session.text_for_range(diagnostic.range.clone()).as_deref()
                    == Some(diagnostic.source_text.as_str())
        });
        if !current {
            self.overlay
                .mark_trouble_stale("selected diagnostic is stale; jump cancelled");
            return;
        }

        let result = self
            .tabs
            .get_mut(self.active_tab)
            .map(|entry| entry.session.jump_to_offset(diagnostic.range.start));
        match result {
            Some(Ok(effects)) => {
                for effect in effects {
                    self.handle_effect(effect);
                }
                self.overlay.close();
                self.pending_input = PendingAppInput::Idle;
                self.pending_scroll_follow = true;
            }
            Some(Err(error)) => self
                .overlay
                .mark_trouble_stale(format!("selected diagnostic is stale: {error}")),
            None => self
                .overlay
                .mark_trouble_stale("selected diagnostic is stale; jump cancelled"),
        }
    }

    fn open_spell_suggestions(&mut self) {
        let Some(engine) = self.spell_host.engine() else {
            self.publish_spell_host_status();
            return;
        };
        let Some(session) = self.session() else {
            return;
        };
        let Some(diagnostic) = session.diagnostic_at_cursor().cloned() else {
            self.set_transient(
                "no spelling diagnostic under cursor".to_string(),
                oom_edit_core::Severity::Warning,
            );
            return;
        };
        let suggestions = session.spell_suggestions(
            engine,
            &diagnostic,
            crate::overlay::spell_suggest::MAX_SUGGESTIONS,
        );
        self.overlay = Overlay::open_spell_suggest(diagnostic, suggestions);
        self.pending_input = PendingAppInput::Idle;
    }

    fn add_current_spelling_word(&mut self) {
        if self.spell_host.engine().is_none() {
            self.publish_spell_host_status();
            return;
        }
        let Some(diagnostic) = self
            .session()
            .and_then(|session| session.diagnostic_at_cursor().cloned())
        else {
            self.set_transient(
                "no spelling diagnostic under cursor".to_string(),
                oom_edit_core::Severity::Warning,
            );
            return;
        };
        self.add_spelling_word(&diagnostic);
    }

    fn execute_spell_suggest_action(&mut self, action: SpellSuggestAction) {
        match action {
            SpellSuggestAction::StayOpen => {}
            SpellSuggestAction::Close => {
                self.overlay.close();
                self.pending_input = PendingAppInput::Idle;
            }
            SpellSuggestAction::Apply(replacement) => {
                let Some(diagnostic) = self.overlay.spell_suggest_diagnostic() else {
                    return;
                };
                if self.apply_spelling_replacement(&diagnostic, &replacement) {
                    self.overlay.close();
                    self.pending_input = PendingAppInput::Idle;
                }
            }
            SpellSuggestAction::AddWord => {
                let Some(diagnostic) = self.overlay.spell_suggest_diagnostic() else {
                    return;
                };
                if self.add_spelling_word(&diagnostic) {
                    self.overlay.close();
                    self.pending_input = PendingAppInput::Idle;
                }
            }
        }
    }

    fn apply_spelling_replacement(
        &mut self,
        diagnostic: &oom_edit_core::Diagnostic,
        replacement: &str,
    ) -> bool {
        let effects = self
            .tabs
            .get_mut(self.active_tab)
            .map(|entry| {
                entry
                    .session
                    .apply_spell_replacement(diagnostic, replacement)
            })
            .unwrap_or_default();
        let success = effects
            .iter()
            .any(|effect| matches!(effect, Effect::Edited));
        for effect in effects {
            self.handle_effect(effect);
        }
        if success {
            self.pending_scroll_follow = true;
        }
        success
    }

    fn add_spelling_word(&mut self, diagnostic: &oom_edit_core::Diagnostic) -> bool {
        let current = self.session().is_some_and(|session| {
            session
                .diagnostics()
                .iter()
                .any(|candidate| candidate == diagnostic)
                && session.text_for_range(diagnostic.range.clone()).as_deref()
                    == Some(diagnostic.source_text.as_str())
        });
        if !current {
            self.set_transient(
                "spelling diagnostic is stale; word was not added".to_string(),
                oom_edit_core::Severity::Warning,
            );
            return false;
        }

        match self.spell_host.add_personal_word(&diagnostic.source_text) {
            Ok(oom_spell::AddWordOutcome::Inserted { normalized }) => {
                self.set_transient(
                    format!("added '{normalized}' to personal dictionary"),
                    oom_edit_core::Severity::Info,
                );
                true
            }
            Ok(oom_spell::AddWordOutcome::AlreadyPresent { normalized }) => {
                self.set_transient(
                    format!("'{normalized}' is already in the personal dictionary"),
                    oom_edit_core::Severity::Info,
                );
                true
            }
            Ok(oom_spell::AddWordOutcome::Ignored) => {
                self.set_transient(
                    "word is not eligible for the personal dictionary".to_string(),
                    oom_edit_core::Severity::Warning,
                );
                false
            }
            Err(error) => {
                self.set_transient(
                    format!("failed to add word: {error}"),
                    oom_edit_core::Severity::Warning,
                );
                false
            }
        }
    }

    fn publish_spell_host_status(&mut self) {
        let message = self
            .spell_host
            .status_message()
            .unwrap_or_else(|| "spell dictionary is not ready".to_string());
        self.set_transient(message, oom_edit_core::Severity::Warning);
    }

    fn execute_lifecycle(&mut self, action: LifecycleAction) {
        match action {
            LifecycleAction::Save(request) => self.execute_save(request),
            LifecycleAction::CloseTab(request) => self.execute_close_tab(request),
            LifecycleAction::ReplaceTab {
                target,
                path,
                force,
            } => {
                if !force
                    && self
                        .tabs
                        .get(target)
                        .is_some_and(|entry| entry.session.is_dirty())
                {
                    self.set_transient(
                        "No write since last change (use :e! to override)".to_string(),
                        oom_edit_core::Severity::Error,
                    );
                } else {
                    self.replace_tab_from_disk(target, &path, false);
                }
            }
            LifecycleAction::OpenTab { path } => self.open_tab(&path),
            LifecycleAction::QuitAll { force } => {
                if !force && self.any_tab_dirty() {
                    let dirty_count = self.tabs.iter().filter(|t| t.session.is_dirty()).count();
                    self.set_transient(
                        format!("{dirty_count} unsaved tab(s) — use :qa! to discard"),
                        oom_edit_core::Severity::Error,
                    );
                } else {
                    self.should_quit = true;
                }
            }
        }
    }

    fn execute_save(&mut self, request: SaveRequest) {
        let Some(entry) = self.tabs.get_mut(request.target) else {
            self.invalid_lifecycle_target(request.target);
            return;
        };

        let result = if let Some(copy_path) = request.path.as_deref().filter(|_| !request.retarget)
        {
            entry.session.save_copy(copy_path)
        } else {
            entry.session.save(request.path.as_deref(), request.force)
        };

        match result {
            Ok(()) => {
                if request.retarget || request.path.is_none() {
                    entry.session.save_point();
                }
                let line_count = entry.session.line_count();
                let file_name = entry
                    .session
                    .path()
                    .and_then(|path| path.file_name())
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "buffer".to_string());
                self.set_transient(
                    format!("Saved {file_name} ({line_count} lines)"),
                    oom_edit_core::Severity::Success,
                );
                if request.continuation == SaveContinuation::CloseSavedTab {
                    self.do_close_tab(request.target);
                }
            }
            Err(oom_edit_core::SaveError::ExternallyModified(path)) if !request.force => {
                self.set_transient(
                    format!("File modified on disk: {}", path.display()),
                    oom_edit_core::Severity::Warning,
                );
                self.overlay = Overlay::open_confirm_overwrite(request, path);
            }
            Err(error) => {
                self.set_transient(
                    format!("Save error: {error}"),
                    oom_edit_core::Severity::Error,
                );
            }
        }
    }

    fn execute_close_tab(&mut self, request: CloseTabRequest) {
        let Some(entry) = self.tabs.get(request.target) else {
            self.invalid_lifecycle_target(request.target);
            return;
        };
        if !request.force && entry.session.is_dirty() {
            match request.dirty_policy {
                DirtyClosePolicy::Confirm => {
                    self.overlay = Overlay::open_confirm_quit(request);
                }
                DirtyClosePolicy::Refuse => {
                    self.set_transient(
                        "No write since last change (use :tabclose! to override)".to_string(),
                        oom_edit_core::Severity::Error,
                    );
                }
            }
            return;
        }
        self.do_close_tab(request.target);
    }

    fn replace_tab_from_disk(&mut self, target: usize, path: &std::path::Path, reloading: bool) {
        if target >= self.tabs.len() {
            self.invalid_lifecycle_target(target);
            return;
        }
        match EditorSession::open(path) {
            Ok(session) => {
                let session = Self::seed_spell_config(session, self.spell_enabled_default);
                let entry = &mut self.tabs[target];
                entry.session = session;
                entry.top_line = 0;
                entry.left_col = 0;
                entry.skip_rows = 0;
                entry.rendered_top = 0;
                self.pending_scroll_follow = true;
                self.set_transient(
                    if reloading {
                        "Reloaded from disk".to_string()
                    } else {
                        format!("Opened: {}", path.display())
                    },
                    oom_edit_core::Severity::Info,
                );
            }
            Err(error) => self.set_transient(
                format!(
                    "{} error: {error}",
                    if reloading { "Reload" } else { "Open" }
                ),
                oom_edit_core::Severity::Error,
            ),
        }
    }

    fn invalid_lifecycle_target(&mut self, target: usize) {
        self.set_transient(
            format!("Lifecycle target tab {} no longer exists", target + 1),
            oom_edit_core::Severity::Error,
        );
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
                self.execute_lifecycle(LifecycleAction::Save(SaveRequest {
                    target: self.active_tab,
                    path,
                    force,
                    retarget,
                    continuation: if then_quit {
                        SaveContinuation::CloseSavedTab
                    } else {
                        SaveContinuation::StayOpen
                    },
                }));
            }
            Effect::QuitRequested { force } => {
                self.execute_lifecycle(LifecycleAction::CloseTab(CloseTabRequest {
                    target: self.active_tab,
                    force,
                    dirty_policy: DirtyClosePolicy::Confirm,
                }));
            }
            Effect::OpenRequested { path, force } => {
                self.execute_lifecycle(LifecycleAction::ReplaceTab {
                    target: self.active_tab,
                    path,
                    force,
                });
            }
            Effect::ClipboardWrite(text) => {
                // T16: route to OSC 52 clipboard sink.
                if let Err(e) = self.clipboard_sink.copy(&text) {
                    self.set_transient(
                        format!("Clipboard error: {e}"),
                        oom_edit_core::Severity::Warning,
                    );
                } else {
                    self.set_transient(
                        "yanked to register".to_string(),
                        oom_edit_core::Severity::Info,
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
            Effect::SetWrap(enabled) => {
                self.wrap_enabled = enabled;
                if enabled {
                    self.set_left_col(0);
                }
                self.set_transient(
                    if enabled { "wrap" } else { "nowrap" }.to_string(),
                    oom_edit_core::Severity::Info,
                );
                self.pending_scroll_follow = true;
            }
            Effect::HelpRequested => {
                // Open command palette.
                self.overlay = Overlay::open_palette(self.mode_context());
            }
            Effect::TabNewRequested { path } => {
                self.execute_lifecycle(LifecycleAction::OpenTab { path });
            }
            Effect::TabCloseRequested { index, force } => {
                let idx = index.unwrap_or(self.active_tab);
                self.execute_lifecycle(LifecycleAction::CloseTab(CloseTabRequest {
                    target: idx,
                    force,
                    dirty_policy: DirtyClosePolicy::Refuse,
                }));
            }
            Effect::TabNext => self.execute_tab_action(TabAction::Next),
            Effect::TabPrev => self.execute_tab_action(TabAction::Prev),
            Effect::TabJump { one_based } => {
                self.execute_tab_action(TabAction::Jump(one_based));
            }
            Effect::QuitAllRequested { force } => {
                self.execute_lifecycle(LifecycleAction::QuitAll { force });
            }
        }
    }

    /// Set a transient status message with TTL expiry.
    #[cfg(test)]
    pub fn set_transient(&mut self, text: String, severity: oom_edit_core::Severity) {
        self.transient = Some(status_bar::Transient {
            text,
            severity,
            expires_at: self.now + status_bar::TRANSIENT_TTL,
        });
    }

    /// Set a transient status message with TTL expiry.
    #[cfg(not(test))]
    fn set_transient(&mut self, text: String, severity: oom_edit_core::Severity) {
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
            .map(|e| e.session.mode() != oom_edit_core::Mode::Insert)
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
                oom_edit_core::Mode::Normal
                | oom_edit_core::Mode::Select
                | oom_edit_core::Mode::Command => {
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
                oom_edit_core::Mode::Normal
                | oom_edit_core::Mode::Select
                | oom_edit_core::Mode::Command => {
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
    use oom_edit_core::Mode;
    use oom_edit_core::{ClipboardError, ClipboardSink, RecordingClipboardSink};

    struct FailingClipboardSink;

    impl ClipboardSink for FailingClipboardSink {
        fn copy(&mut self, _text: &str) -> Result<(), ClipboardError> {
            Err(ClipboardError::NotSupported)
        }
    }

    /// Create a test App with a recording clipboard sink.
    fn test_app(session: EditorSession) -> App {
        test_app_at(session, std::time::Instant::now())
    }

    fn test_app_at(mut session: EditorSession, initial_time: Instant) -> App {
        session.render_layout(74);
        App::new(
            session,
            theme::ResolvedTheme::injected("default-dark", false, Tier::TrueColor),
            true,
            false,
            Box::new(RecordingClipboardSink::default()),
            Box::new(crate::config::DisabledConfigStore),
            initial_time,
        )
    }

    fn test_app_with_spell_default(session: EditorSession, enabled: bool) -> App {
        test_app_with_spell_host(
            session,
            crate::spell_host::SpellHost::testing("known\n"),
            enabled,
        )
    }

    fn test_app_with_spell_host(
        session: EditorSession,
        spell_host: crate::spell_host::SpellHost,
        enabled: bool,
    ) -> App {
        App::new_with_spell(
            session,
            theme::ResolvedTheme::injected("default-dark", false, Tier::TrueColor),
            true,
            false,
            AppServices::new(
                Box::new(RecordingClipboardSink::default()),
                Box::new(crate::config::DisabledConfigStore),
                spell_host,
            ),
            enabled,
            Instant::now(),
        )
    }

    fn drain_app_spelling(app: &mut App) {
        for _ in 0..10_000 {
            app.on_idle_unit(crate::event::SPELL_WORK_UNIT_BYTES);
            if app.spell_host_phase() == "Ready"
                && app
                    .session()
                    .is_some_and(|session| !session.diagnostics_pending())
            {
                // The first Ready observation can precede the generation-mismatch
                // tick that starts the initial scan.
                app.on_idle_unit(crate::event::SPELL_WORK_UNIT_BYTES);
                if app
                    .session()
                    .is_some_and(|session| !session.diagnostics_pending())
                {
                    return;
                }
            }
        }
        panic!("test App spell work did not drain");
    }

    fn press_space_command(app: &mut App, continuation: char) {
        for ch in [' ', continuation] {
            app.handle_event(&Event::Key(KeyEvent::new(
                CrosstermKeyCode::Char(ch),
                KeyModifiers::NONE,
            )));
        }
    }

    #[test]
    fn every_session_creation_path_uses_config_default_not_runtime_toggle() {
        let directory = tempfile::tempdir().unwrap();
        let first_path = directory.path().join("first.md");
        let second_path = directory.path().join("second.md");
        std::fs::write(&first_path, "first\n").unwrap();
        std::fs::write(&second_path, "second\n").unwrap();

        let mut disabled =
            test_app_with_spell_default(EditorSession::open(&first_path).unwrap(), false);
        assert!(!disabled.tabs[0].session.spell_enabled());
        disabled.open_tab(&second_path);
        assert!(!disabled.tabs[1].session.spell_enabled());
        disabled.replace_tab_from_disk(0, &second_path, false);
        assert!(!disabled.tabs[0].session.spell_enabled());

        let mut enabled =
            test_app_with_spell_default(EditorSession::open(&first_path).unwrap(), true);
        enabled.tabs[0].session.set_spell_enabled(false);
        enabled.open_tab(&second_path);
        assert!(
            enabled.tabs[1].session.spell_enabled(),
            "new tabs must use config rather than inheriting the active runtime toggle"
        );
        enabled.tabs[1].session.set_spell_enabled(false);
        enabled.replace_tab_from_disk(1, &first_path, true);
        assert!(
            enabled.tabs[1].session.spell_enabled(),
            "replacement/reload must use the same session-construction funnel"
        );
    }

    #[test]
    fn spell_off_marker_tracks_only_the_active_tab() {
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;

        let mut app = test_app(EditorSession::from_text("first\n"));
        app.tabs[0].session.set_spell_enabled(false);
        let mut second = EditorSession::from_text("second\n");
        second.set_spell_enabled(true);
        app.tabs.push(TabEntry::new(second));

        let status_line = |app: &mut App| {
            let mut terminal = Terminal::new(TestBackend::new(80, 6)).unwrap();
            terminal.draw(|frame| app.render(frame)).unwrap();
            (0..80)
                .map(|column| {
                    terminal
                        .backend()
                        .buffer()
                        .cell((column, 5))
                        .unwrap()
                        .symbol()
                })
                .collect::<String>()
        };

        assert!(status_line(&mut app).contains("[spell off]"));
        app.active_tab = 1;
        assert!(!status_line(&mut app).contains("[spell off]"));
    }

    #[test]
    fn space_z_toggles_only_the_active_session() {
        let mut app = test_app(EditorSession::from_text("first\n"));
        app.tabs
            .push(TabEntry::new(EditorSession::from_text("second\n")));

        assert!(app.tabs[0].session.spell_enabled(), "first before Space z");
        assert!(app.tabs[1].session.spell_enabled(), "second before Space z");
        press_space_command(&mut app, 'z');
        assert!(!app.tabs[0].session.spell_enabled(), "first after Space z");
        assert!(
            app.tabs[1].session.spell_enabled(),
            "second remains enabled"
        );
        assert_eq!(
            app.transient.as_ref().map(|message| message.text.as_str()),
            Some("spell checking disabled")
        );
        press_space_command(&mut app, 'z');
        assert!(app.tabs[0].session.spell_enabled(), "first re-enabled");
        assert!(
            app.tabs[1].session.spell_enabled(),
            "second remains enabled"
        );
        assert_eq!(
            app.transient.as_ref().map(|message| message.text.as_str()),
            Some("spell checking enabled")
        );
    }

    #[test]
    fn space_s_opens_first_selected_suggestion_and_modal_input_is_exclusive() {
        let document = format!("teh\n{}", "known\n".repeat(40));
        let mut app = test_app_with_spell_host(
            EditorSession::from_text(&document),
            crate::spell_host::SpellHost::testing("known\ntea\nten\nthe\n"),
            true,
        );
        drain_app_spelling(&mut app);
        app.tabs[0].session.render_layout(74);

        press_space_command(&mut app, 's');
        let Overlay::SpellSuggest(state) = &app.overlay else {
            panic!("Space s must open the suggestion overlay");
        };
        assert_eq!(state.selected(), Some(0));
        assert!(!state.suggestions().is_empty());

        app.handle_event(&Event::Key(KeyEvent::new(
            CrosstermKeyCode::Char('i'),
            KeyModifiers::NONE,
        )));
        assert!(matches!(app.overlay, Overlay::SpellSuggest(_)));
        assert_eq!(app.session().unwrap().mode(), Mode::Normal);
        assert_eq!(app.session().unwrap().document(), document);

        app.handle_event(&Event::Paste("background edit".to_string()));
        let rendered_top = app.tabs[0].rendered_top;
        app.handle_event(&Event::Mouse(crossterm::event::MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: 0,
            row: 0,
            modifiers: KeyModifiers::NONE,
        }));
        assert!(matches!(app.overlay, Overlay::SpellSuggest(_)));
        assert_eq!(app.session().unwrap().document(), document);
        assert_eq!(
            app.tabs[0].rendered_top, rendered_top,
            "mouse scrolling must not move the document behind the modal"
        );
    }

    #[test]
    fn suggestion_digit_applies_one_undoable_replacement_and_closes() {
        let mut app = test_app_with_spell_host(
            EditorSession::from_text("teh\n"),
            crate::spell_host::SpellHost::testing("tea\nten\nthe\n"),
            true,
        );
        drain_app_spelling(&mut app);
        press_space_command(&mut app, 's');
        let expected = match &app.overlay {
            Overlay::SpellSuggest(state) => state.suggestions()[0].clone(),
            _ => panic!("suggestion overlay must be open"),
        };

        app.handle_event(&Event::Key(KeyEvent::new(
            CrosstermKeyCode::Char('1'),
            KeyModifiers::NONE,
        )));
        assert!(matches!(app.overlay, Overlay::None));
        assert_eq!(app.session().unwrap().document(), format!("{expected}\n"));

        app.handle_event(&Event::Key(KeyEvent::new(
            CrosstermKeyCode::Char('u'),
            KeyModifiers::NONE,
        )));
        assert_eq!(app.session().unwrap().document(), "teh\n");
    }

    #[test]
    fn empty_suggestions_stay_open_until_add_succeeds() {
        let mut app = test_app_with_spell_host(
            EditorSession::from_text("zzzzzz\n"),
            crate::spell_host::SpellHost::testing("known\n"),
            true,
        );
        drain_app_spelling(&mut app);
        press_space_command(&mut app, 's');
        let Overlay::SpellSuggest(state) = &app.overlay else {
            panic!("empty suggestion overlay must still open");
        };
        assert_eq!(state.selected(), None);
        assert!(state.suggestions().is_empty());

        app.handle_event(&Event::Key(KeyEvent::new(
            CrosstermKeyCode::Enter,
            KeyModifiers::NONE,
        )));
        assert!(matches!(app.overlay, Overlay::SpellSuggest(_)));
        app.handle_event(&Event::Key(KeyEvent::new(
            CrosstermKeyCode::Char('a'),
            KeyModifiers::NONE,
        )));
        assert!(matches!(app.overlay, Overlay::None));
        assert_eq!(
            app.transient.as_ref().map(|message| message.text.as_str()),
            Some("added 'zzzzzz' to personal dictionary")
        );
    }

    #[test]
    fn stale_replacement_warns_and_keeps_overlay_open() {
        let mut app = test_app_with_spell_host(
            EditorSession::from_text("teh\n"),
            crate::spell_host::SpellHost::testing("the\n"),
            true,
        );
        drain_app_spelling(&mut app);
        press_space_command(&mut app, 's');

        let entry = app.tabs.get_mut(0).unwrap();
        entry.session.handle_key(KeyInput {
            code: KeyCode {
                kind: KeyCodeKind::Char('i'),
            },
            mods: Modifiers::default(),
        });
        entry.session.handle_key(KeyInput {
            code: KeyCode {
                kind: KeyCodeKind::Char('x'),
            },
            mods: Modifiers::default(),
        });
        entry.session.handle_key(KeyInput {
            code: KeyCode {
                kind: KeyCodeKind::Esc,
            },
            mods: Modifiers::default(),
        });

        app.handle_event(&Event::Key(KeyEvent::new(
            CrosstermKeyCode::Enter,
            KeyModifiers::NONE,
        )));
        assert!(matches!(app.overlay, Overlay::SpellSuggest(_)));
        assert!(app
            .transient
            .as_ref()
            .is_some_and(|message| message.text.contains("stale")));
        assert_eq!(app.session().unwrap().document(), "xteh\n");
    }

    #[test]
    fn failed_personal_persistence_warns_and_keeps_overlay_open() {
        let mut app = test_app_with_spell_host(
            EditorSession::from_text("teh\n"),
            crate::spell_host::SpellHost::testing_with_failing_personal_save("the\n"),
            true,
        );
        drain_app_spelling(&mut app);
        press_space_command(&mut app, 's');
        app.handle_event(&Event::Key(KeyEvent::new(
            CrosstermKeyCode::Char('a'),
            KeyModifiers::NONE,
        )));

        assert!(matches!(app.overlay, Overlay::SpellSuggest(_)));
        assert_eq!(
            app.transient.as_ref().map(|message| message.text.as_str()),
            Some("failed to add word: scripted personal save failure")
        );
        assert_eq!(app.session().unwrap().diagnostics().len(), 1);
        drain_app_spelling(&mut app);
        assert_eq!(
            app.session().unwrap().diagnostics().len(),
            1,
            "failed disk persistence must not mutate the shared engine"
        );
    }

    #[test]
    fn stale_add_warns_keeps_overlay_open_and_does_not_mutate_engine() {
        let mut app = test_app_with_spell_host(
            EditorSession::from_text("teh\n"),
            crate::spell_host::SpellHost::testing("the\n"),
            true,
        );
        drain_app_spelling(&mut app);
        press_space_command(&mut app, 's');

        let entry = app.tabs.get_mut(0).unwrap();
        for kind in [
            KeyCodeKind::Char('i'),
            KeyCodeKind::Char('x'),
            KeyCodeKind::Esc,
        ] {
            entry.session.handle_key(KeyInput {
                code: KeyCode { kind },
                mods: Modifiers::default(),
            });
        }
        app.handle_event(&Event::Key(KeyEvent::new(
            CrosstermKeyCode::Char('a'),
            KeyModifiers::NONE,
        )));

        assert!(matches!(app.overlay, Overlay::SpellSuggest(_)));
        assert_eq!(
            app.transient.as_ref().map(|message| message.text.as_str()),
            Some("spelling diagnostic is stale; word was not added")
        );
        assert!(!app.spell_host.engine().unwrap().check("teh"));
    }

    #[test]
    fn escape_and_ctrl_c_close_suggestion_without_forwarding_to_document() {
        for (code, modifiers) in [
            (CrosstermKeyCode::Esc, KeyModifiers::NONE),
            (CrosstermKeyCode::Char('c'), KeyModifiers::CONTROL),
        ] {
            let mut app = test_app_with_spell_host(
                EditorSession::from_text("teh\n"),
                crate::spell_host::SpellHost::testing("the\n"),
                true,
            );
            drain_app_spelling(&mut app);
            press_space_command(&mut app, 's');
            app.handle_event(&Event::Key(KeyEvent::new(code, modifiers)));
            assert!(matches!(app.overlay, Overlay::None));
            assert_eq!(app.session().unwrap().document(), "teh\n");
            assert_eq!(app.session().unwrap().mode(), Mode::Normal);
        }
    }

    #[test]
    fn space_a_adds_without_opening_and_generation_clears_the_diagnostic() {
        let mut app = test_app_with_spell_host(
            EditorSession::from_text("teh\n"),
            crate::spell_host::SpellHost::testing("the\n"),
            true,
        );
        drain_app_spelling(&mut app);
        press_space_command(&mut app, 'a');
        assert!(matches!(app.overlay, Overlay::None));
        assert_eq!(
            app.transient.as_ref().map(|message| message.text.as_str()),
            Some("added 'teh' to personal dictionary")
        );
        drain_app_spelling(&mut app);
        assert!(app.session().unwrap().diagnostics().is_empty());
    }

    #[test]
    fn suggest_command_distinguishes_building_unavailable_and_no_diagnostic() {
        let mut building = test_app_with_spell_host(
            EditorSession::from_text("teh\n"),
            crate::spell_host::SpellHost::testing("the\n"),
            true,
        );
        while building.spell_host_phase() != "Building" {
            assert!(building.on_idle_unit(crate::event::SPELL_WORK_UNIT_BYTES));
        }
        press_space_command(&mut building, 's');
        assert!(matches!(building.overlay, Overlay::None));
        assert_eq!(
            building
                .transient
                .as_ref()
                .map(|message| message.text.as_str()),
            Some("spell dictionary building")
        );
        press_space_command(&mut building, 'a');
        assert_eq!(
            building
                .transient
                .as_ref()
                .map(|message| message.text.as_str()),
            Some("spell dictionary building")
        );

        let mut unavailable = test_app_with_spell_host(
            EditorSession::from_text("teh\n"),
            crate::spell_host::SpellHost::testing_unavailable("configured list failed"),
            true,
        );
        press_space_command(&mut unavailable, 's');
        assert!(matches!(unavailable.overlay, Overlay::None));
        assert_eq!(
            unavailable
                .transient
                .as_ref()
                .map(|message| message.text.as_str()),
            Some("spell unavailable: configured list failed")
        );
        press_space_command(&mut unavailable, 'a');
        assert_eq!(
            unavailable
                .transient
                .as_ref()
                .map(|message| message.text.as_str()),
            Some("spell unavailable: configured list failed")
        );

        let mut clean = test_app_with_spell_host(
            EditorSession::from_text("known\n"),
            crate::spell_host::SpellHost::testing("known\n"),
            true,
        );
        drain_app_spelling(&mut clean);
        press_space_command(&mut clean, 's');
        assert!(matches!(clean.overlay, Overlay::None));
        assert_eq!(
            clean
                .transient
                .as_ref()
                .map(|message| message.text.as_str()),
            Some("no spelling diagnostic under cursor")
        );
        press_space_command(&mut clean, 'a');
        assert_eq!(
            clean
                .transient
                .as_ref()
                .map(|message| message.text.as_str()),
            Some("no spelling diagnostic under cursor")
        );
    }

    #[test]
    fn space_d_opens_for_pending_complete_and_unavailable_diagnostics() {
        let mut pending = test_app_with_spell_host(
            EditorSession::from_text("teh\n"),
            crate::spell_host::SpellHost::testing("the\n"),
            true,
        );
        press_space_command(&mut pending, 'd');
        let Overlay::Trouble(state) = &pending.overlay else {
            panic!("Space d must open Trouble while dictionaries are pending");
        };
        assert_eq!(state.progress(), &TroubleProgress::Pending);
        assert!(state.entries().is_empty());

        let mut complete = test_app_with_spell_host(
            EditorSession::from_text("teh known\n"),
            crate::spell_host::SpellHost::testing("known\n"),
            true,
        );
        drain_app_spelling(&mut complete);
        press_space_command(&mut complete, 'd');
        let Overlay::Trouble(state) = &complete.overlay else {
            panic!("Space d must open completed Trouble results");
        };
        assert_eq!(state.progress(), &TroubleProgress::Complete);
        assert_eq!(state.entries().len(), 1);

        let mut unavailable = test_app_with_spell_host(
            EditorSession::from_text("teh\n"),
            crate::spell_host::SpellHost::testing_unavailable("configured list failed"),
            true,
        );
        press_space_command(&mut unavailable, 'd');
        let Overlay::Trouble(state) = &unavailable.overlay else {
            panic!("Space d must open unavailable Trouble state");
        };
        assert_eq!(
            state.progress(),
            &TroubleProgress::Unavailable("configured list failed".to_string())
        );

        let mut disabled = test_app_with_spell_default(EditorSession::from_text("teh\n"), false);
        press_space_command(&mut disabled, 'd');
        let Overlay::Trouble(state) = &disabled.overlay else {
            panic!("Space d must open a terminal empty state while spelling is disabled");
        };
        assert_eq!(state.progress(), &TroubleProgress::Complete);
        assert!(state.entries().is_empty());
        assert!(!disabled.on_idle_unit(crate::event::SPELL_WORK_UNIT_BYTES));
        assert_eq!(disabled.spell_host_phase(), "Unbuilt");
    }

    #[test]
    fn trouble_refreshes_after_idle_and_modal_input_is_exclusive() {
        let document = "teh known\n";
        let mut app = test_app_with_spell_host(
            EditorSession::from_text(document),
            crate::spell_host::SpellHost::testing("known\n"),
            true,
        );
        press_space_command(&mut app, 'd');

        drain_app_spelling(&mut app);
        let Overlay::Trouble(state) = &app.overlay else {
            panic!("Trouble must remain open while idle results refresh");
        };
        assert_eq!(state.progress(), &TroubleProgress::Complete);
        assert_eq!(state.entries().len(), 1);

        app.handle_event(&Event::Key(KeyEvent::new(
            CrosstermKeyCode::Char('i'),
            KeyModifiers::NONE,
        )));
        app.handle_event(&Event::Paste("background edit".to_string()));
        let rendered_top = app.tabs[0].rendered_top;
        app.handle_event(&Event::Mouse(crossterm::event::MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: 0,
            row: 0,
            modifiers: KeyModifiers::NONE,
        }));
        assert!(matches!(app.overlay, Overlay::Trouble(_)));
        assert_eq!(app.session().unwrap().mode(), Mode::Normal);
        assert_eq!(app.session().unwrap().document(), document);
        assert_eq!(app.tabs[0].rendered_top, rendered_top);
    }

    #[test]
    fn trouble_enter_atomically_jumps_cursor_ruler_and_wrapped_viewport_then_closes() {
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;

        let document = format!(
            "é {}teh\n{}wrng\n",
            "known ".repeat(12),
            "known\n".repeat(10)
        );
        let mut app = test_app_with_spell_host(
            EditorSession::from_text(&document),
            crate::spell_host::SpellHost::testing("known\n"),
            true,
        );
        drain_app_spelling(&mut app);

        let target = app.session().unwrap().diagnostics()[1].clone();
        let expected = app
            .session()
            .unwrap()
            .position_for_offset(target.range.start)
            .unwrap();

        let mut terminal = Terminal::new(TestBackend::new(24, 6)).unwrap();
        terminal.draw(|frame| app.render(frame)).unwrap();
        press_space_command(&mut app, 'd');
        app.handle_event(&Event::Key(KeyEvent::new(
            CrosstermKeyCode::Char('j'),
            KeyModifiers::NONE,
        )));
        app.handle_event(&Event::Key(KeyEvent::new(
            CrosstermKeyCode::Enter,
            KeyModifiers::NONE,
        )));

        assert!(matches!(app.overlay, Overlay::None));
        assert_eq!(
            app.session().unwrap().cursor(),
            (expected.line, expected.column)
        );
        let rendered = app.session().unwrap().rendered_cursor();
        assert!(rendered.row > 1, "the first long line must wrap");
        assert!(
            app.session().unwrap().rendered_layout().unwrap().lines[rendered.row]
                .atoms
                .iter()
                .any(|atom| atom.columns.contains(&rendered.column)
                    && atom
                        .source
                        .as_ref()
                        .is_some_and(|source| source.contains(&target.range.start)))
        );
        assert!(app.pending_scroll_follow);

        terminal.draw(|frame| app.render(frame)).unwrap();
        assert!(
            app.tabs[0].rendered_top > 0,
            "jump must follow the viewport"
        );
        assert!(
            app.tabs[0].rendered_top <= rendered.row
                && rendered.row < app.tabs[0].rendered_top + app.viewport_height,
            "jumped row must be inside the followed viewport"
        );
        let status_line = (0..24)
            .map(|column| {
                terminal
                    .backend()
                    .buffer()
                    .cell((column, 5))
                    .unwrap()
                    .symbol()
            })
            .collect::<String>();
        assert!(
            status_line.contains(&format!("{}:{}", expected.line + 1, expected.column + 1)),
            "ruler must use the canonical jumped position: {status_line:?}"
        );
    }

    #[test]
    fn stale_trouble_row_warns_and_closes_only_after_a_successful_jump() {
        let mut app = test_app_with_spell_host(
            EditorSession::from_text("teh\n"),
            crate::spell_host::SpellHost::testing("the\n"),
            true,
        );
        drain_app_spelling(&mut app);
        press_space_command(&mut app, 'd');

        let entry = app.tabs.get_mut(0).unwrap();
        for kind in [
            KeyCodeKind::Char('i'),
            KeyCodeKind::Char('x'),
            KeyCodeKind::Esc,
        ] {
            entry.session.handle_key(KeyInput {
                code: KeyCode { kind },
                mods: Modifiers::default(),
            });
        }

        app.tabs[0].session.render_layout(74);
        app.pending_scroll_follow = false;
        let before = (
            app.session().unwrap().cursor(),
            app.session().unwrap().rendered_cursor(),
            app.tabs[0].rendered_top,
            app.pending_scroll_follow,
        );

        app.handle_event(&Event::Key(KeyEvent::new(
            CrosstermKeyCode::Enter,
            KeyModifiers::NONE,
        )));
        let Overlay::Trouble(state) = &app.overlay else {
            panic!("a stale jump must keep Trouble open");
        };
        assert_eq!(
            state.warning(),
            Some("selected diagnostic is stale; jump cancelled")
        );
        assert_eq!(app.session().unwrap().document(), "xteh\n");
        assert_eq!(
            (
                app.session().unwrap().cursor(),
                app.session().unwrap().rendered_cursor(),
                app.tabs[0].rendered_top,
                app.pending_scroll_follow,
            ),
            before,
            "a stale Trouble row must not move either cursor or the viewport"
        );
    }

    #[test]
    fn unavailable_spell_host_emits_one_warning_then_stays_quiescent() {
        let initial = Instant::now();
        let mut app = App::new_with_spell(
            EditorSession::from_text("text\n"),
            theme::ResolvedTheme::injected("default-dark", false, Tier::TrueColor),
            true,
            false,
            AppServices::new(
                Box::new(RecordingClipboardSink::default()),
                Box::new(crate::config::DisabledConfigStore),
                crate::spell_host::SpellHost::testing_unavailable("configured list failed"),
            ),
            true,
            initial,
        );

        assert!(!app.on_idle_unit(crate::event::SPELL_WORK_UNIT_BYTES));
        let warning = app.transient.as_ref().expect("warning must be published");
        assert_eq!(warning.severity, oom_edit_core::Severity::Warning);
        assert_eq!(warning.text, "spell unavailable: configured list failed");

        app.transient = None;
        app.status_message.clear();
        assert!(!app.on_idle_unit(crate::event::SPELL_WORK_UNIT_BYTES));
        assert!(
            app.transient.is_none(),
            "terminal host warning must not repeat"
        );
        assert_eq!(app.spell_host_phase(), "Unavailable");
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

    fn file_backed_app(dir: &std::path::Path, files: &[(&str, &str)]) -> App {
        assert!(!files.is_empty());
        let paths: Vec<_> = files
            .iter()
            .map(|(name, contents)| {
                let path = dir.join(name);
                std::fs::write(&path, contents).unwrap();
                path
            })
            .collect();
        let mut app = test_app(EditorSession::open(&paths[0]).unwrap());
        for path in &paths[1..] {
            app.tabs
                .push(TabEntry::new(EditorSession::open(path).unwrap()));
        }
        app
    }

    fn dirty_tab(app: &mut App, index: usize, inserted: &str) {
        let session = &mut app.tabs[index].session;
        session.render_layout(74);
        session.handle_key(KeyInput {
            code: KeyCode {
                kind: KeyCodeKind::Char('i'),
            },
            mods: Modifiers::default(),
        });
        session.insert_paste(inserted);
        session.handle_key(KeyInput {
            code: KeyCode {
                kind: KeyCodeKind::Esc,
            },
            mods: Modifiers::default(),
        });
        assert!(session.is_dirty());
    }

    fn type_ex(app: &mut App, command: &str) {
        for key in std::iter::once(CrosstermKeyCode::Char(':'))
            .chain(command.chars().map(CrosstermKeyCode::Char))
            .chain(std::iter::once(CrosstermKeyCode::Enter))
        {
            app.handle_event(&Event::Key(KeyEvent::new(key, KeyModifiers::NONE)));
        }
    }

    fn type_chars(app: &mut App, characters: impl IntoIterator<Item = char>) {
        for character in characters {
            app.handle_event(&Event::Key(KeyEvent::new(
                CrosstermKeyCode::Char(character),
                KeyModifiers::NONE,
            )));
        }
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
            std::time::Instant::now(),
        );

        yank_current_line_to_system_clipboard(&mut app);

        let transient = app.transient.as_ref().expect("transient should be set");
        assert!(
            transient.text.contains("Clipboard error"),
            "expected clipboard error message, got: {}",
            transient.text
        );
        assert!(!transient.text.contains("yanked to register"));
        assert_eq!(transient.severity, oom_edit_core::Severity::Warning);
    }

    #[test]
    fn clipboard_success_sets_success_transient() {
        let mut app = test_app(EditorSession::from_text("hello\n"));

        yank_current_line_to_system_clipboard(&mut app);

        let transient = app.transient.as_ref().expect("transient should be set");
        assert_eq!(transient.text, "yanked to register");
        assert_eq!(transient.severity, oom_edit_core::Severity::Info);
    }

    /// App: typing 'i' enters Insert mode.
    #[test]
    fn app_handle_event_enters_insert() {
        let session = EditorSession::from_text("hello");
        let mut app = test_app(session);
        let key = KeyEvent::new(CrosstermKeyCode::Char('i'), KeyModifiers::NONE);
        app.handle_event(&Event::Key(key));
        assert_eq!(app.session().unwrap().mode(), oom_edit_core::Mode::Insert);
    }

    /// App: Escape returns to Normal mode.
    #[test]
    fn app_handle_event_escapes_to_normal() {
        let session = EditorSession::from_text("hello");
        let mut app = test_app(session);
        // Enter insert mode.
        let key = KeyEvent::new(CrosstermKeyCode::Char('i'), KeyModifiers::NONE);
        app.handle_event(&Event::Key(key));
        assert_eq!(app.session().unwrap().mode(), oom_edit_core::Mode::Insert);
        // Escape.
        let key = KeyEvent::new(CrosstermKeyCode::Esc, KeyModifiers::NONE);
        app.handle_event(&Event::Key(key));
        assert_eq!(app.session().unwrap().mode(), oom_edit_core::Mode::Normal);
    }

    /// App: `:w` triggers save request.
    #[test]
    fn app_handle_event_save_requested() {
        let session = EditorSession::from_text("hello");
        let mut app = test_app(session);

        // Enter command mode and type :w
        let key = KeyEvent::new(CrosstermKeyCode::Char(':'), KeyModifiers::NONE);
        app.handle_event(&Event::Key(key));
        assert_eq!(app.session().unwrap().mode(), oom_edit_core::Mode::Command);

        // Type 'w'
        let key = KeyEvent::new(CrosstermKeyCode::Char('w'), KeyModifiers::NONE);
        app.handle_event(&Event::Key(key));
        assert_eq!(app.session().unwrap().mode(), oom_edit_core::Mode::Command);

        // Press Enter to execute the command
        let key = KeyEvent::new(CrosstermKeyCode::Enter, KeyModifiers::NONE);
        app.handle_event(&Event::Key(key));
        assert_eq!(app.session().unwrap().mode(), oom_edit_core::Mode::Normal);

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

        app.handle_effect(Effect::SetWrap(false));
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
        app.handle_effect(Effect::SetWrap(true));
        assert!(app.wrap_enabled);
        assert_eq!(app.transient.as_ref().unwrap().text, "wrap");
    }

    #[test]
    fn app_set_nowrap_triggers_horizontal_follow() {
        let mut app = test_app(EditorSession::from_text(&"x".repeat(100)));
        app.viewport_width = 20;
        enter_insert(&mut app);
        press(&mut app, CrosstermKeyCode::End);
        app.handle_effect(Effect::SetWrap(false));
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
        app.handle_effect(Effect::SetWrap(true));
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
        app.handle_effect(Effect::SetWrap(false));
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
        assert_eq!(insert.mode(), oom_edit_core::Mode::Insert);

        let mut command = EditorSession::from_text("hello");
        command.handle_key(KeyInput {
            code: KeyCode {
                kind: KeyCodeKind::Char(':'),
            },
            mods: Modifiers::default(),
        });
        assert_eq!(command.mode(), oom_edit_core::Mode::Command);

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
            oom_edit_core::Mode::Insert
        );

        let mut command = test_app(EditorSession::from_text("hello"));
        command.handle_event(&Event::Key(KeyEvent::new(
            CrosstermKeyCode::Char(':'),
            KeyModifiers::NONE,
        )));
        assert_eq!(
            command.session().unwrap().mode(),
            oom_edit_core::Mode::Command
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
    fn unsupported_noop_key_preserves_pending_space_chord() {
        let mut app = test_app(EditorSession::from_text("hello"));

        app.handle_event(&Event::Key(KeyEvent::new(
            CrosstermKeyCode::Char(' '),
            KeyModifiers::NONE,
        )));
        assert!(matches!(app.pending_input, PendingAppInput::Space { .. }));

        app.handle_event(&Event::Key(KeyEvent::new(
            CrosstermKeyCode::Insert,
            KeyModifiers::NONE,
        )));
        assert!(matches!(app.pending_input, PendingAppInput::Space { .. }));

        app.handle_event(&Event::Key(KeyEvent::new(
            CrosstermKeyCode::Char('h'),
            KeyModifiers::NONE,
        )));
        assert!(app.overlay.is_palette());
    }

    #[test]
    fn app_forwards_g_and_numeric_prefixes_unchanged_to_core() {
        let document = "ONE TWO\nTHREE FOUR\nFIVE SIX\nSEVEN EIGHT\nNINE TEN\n";
        for sequence in ["gg", "guu", "gUU", "2gg", "3guu", "10j"] {
            let mut app = test_app(EditorSession::from_text(document));
            let mut direct = EditorSession::from_text(document);
            app.tabs[0].session.render_layout(40);
            direct.render_layout(40);

            for character in sequence.chars() {
                app.handle_event(&Event::Key(KeyEvent::new(
                    CrosstermKeyCode::Char(character),
                    KeyModifiers::NONE,
                )));
                direct.handle_key(KeyInput {
                    code: KeyCode {
                        kind: KeyCodeKind::Char(character),
                    },
                    mods: Modifiers::default(),
                });
                assert_eq!(app.pending_input, PendingAppInput::Idle, "{sequence}");
            }

            assert_eq!(
                app.tabs[0].session.document(),
                direct.document(),
                "{sequence}"
            );
            assert_eq!(app.tabs[0].session.mode(), direct.mode(), "{sequence}");
            assert_eq!(
                app.tabs[0].session.rendered_cursor(),
                direct.rendered_cursor(),
                "{sequence}"
            );
        }

        let mut tabs = test_app_with_tabs(4);
        tabs.active_tab = 2;
        type_chars(&mut tabs, ['1', 'g', 't']);
        assert_eq!(tabs.active_tab, 0);
        type_chars(&mut tabs, ['3', 'g', 't']);
        assert_eq!(tabs.active_tab, 2);
        type_chars(&mut tabs, ['9', 'g', 't']);
        assert_eq!(tabs.active_tab, 2);
        assert_eq!(tabs.transient.as_ref().unwrap().text, "No tab 9");
        type_chars(&mut tabs, ['g', 't']);
        assert_eq!(tabs.active_tab, 3);
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
        assert_eq!(app.session().unwrap().mode(), oom_edit_core::Mode::Select);
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

        app.handle_effect(Effect::TabJump {
            one_based: std::num::NonZeroUsize::new(3).unwrap(),
        });

        assert_eq!(app.active_tab, 2);
    }

    #[test]
    fn app_counted_gt_jumps_to_requested_tab() {
        let mut app = test_app_with_tabs(4);
        for key in ['3', 'g', 't'] {
            app.handle_event(&Event::Key(KeyEvent::new(
                CrosstermKeyCode::Char(key),
                KeyModifiers::NONE,
            )));
        }
        assert_eq!(app.active_tab, 2);
    }

    #[test]
    fn space_chord_before_first_tick_uses_injected_initial_time() {
        let initial = Instant::now();
        let mut app = test_app_at(EditorSession::from_text("hello"), initial);
        app.handle_event(&Event::Key(KeyEvent::new(
            CrosstermKeyCode::Char(' '),
            KeyModifiers::NONE,
        )));
        assert_eq!(app.pending_input, PendingAppInput::Space { since: initial });
    }

    #[test]
    fn tick_returns_exact_pending_chord_deadline() {
        let initial = Instant::now();
        let mut app = test_app_at(EditorSession::from_text("hello"), initial);
        app.handle_event(&Event::Key(KeyEvent::new(
            CrosstermKeyCode::Char(' '),
            KeyModifiers::NONE,
        )));
        assert_eq!(
            app.tick(initial),
            Some(initial + std::time::Duration::from_millis(150))
        );
        assert_eq!(
            app.tick(initial + std::time::Duration::from_millis(150)),
            Some(initial + std::time::Duration::from_millis(150)),
            "a due deadline must remain observable by the post-poll idle gate"
        );
        assert_eq!(
            app.tick(initial + std::time::Duration::from_millis(151)),
            None,
            "a previously-observed deadline must not create a busy loop"
        );
    }

    #[test]
    fn pending_space_resets_on_mode_overlay_completion_and_cancel() {
        let initial = Instant::now();
        let mut mode_app = test_app_at(EditorSession::from_text("hello"), initial);
        for key in [' ', 'i'] {
            mode_app.handle_event(&Event::Key(KeyEvent::new(
                CrosstermKeyCode::Char(key),
                KeyModifiers::NONE,
            )));
        }
        assert_eq!(mode_app.pending_input, PendingAppInput::Idle);
        assert_eq!(
            mode_app.session().unwrap().mode(),
            oom_edit_core::Mode::Insert
        );

        let mut overlay_app = test_app_at(EditorSession::from_text("hello"), initial);
        overlay_app.pending_input = PendingAppInput::Space { since: initial };
        overlay_app.overlay = Overlay::open_palette(Contexts::NORMAL);
        overlay_app.handle_event(&Event::Key(KeyEvent::new(
            CrosstermKeyCode::Esc,
            KeyModifiers::NONE,
        )));
        assert_eq!(overlay_app.pending_input, PendingAppInput::Idle);
        assert!(!overlay_app.overlay.is_some());

        let mut completed = test_app_at(EditorSession::from_text("hello"), initial);
        completed.pending_input = PendingAppInput::Space { since: initial };
        completed.overlay = Overlay::open_palette(Contexts::NORMAL);
        completed.handle_event(&Event::Key(KeyEvent::new(
            CrosstermKeyCode::Enter,
            KeyModifiers::NONE,
        )));
        assert_eq!(completed.pending_input, PendingAppInput::Idle);
        assert!(!completed.overlay.is_some());
    }

    /// T12: Space in Insert mode self-inserts (routing order proof).
    #[test]
    fn app_space_in_insert_self_inserts() {
        let session = EditorSession::from_text("hello");
        let mut app = test_app(session);

        // Enter insert mode.
        let i = KeyEvent::new(CrosstermKeyCode::Char('i'), KeyModifiers::NONE);
        app.handle_event(&Event::Key(i));
        assert_eq!(app.session().unwrap().mode(), oom_edit_core::Mode::Insert);

        // Space in Insert mode should NOT start a chord — it falls through
        // to the session as a self-insert.
        let space = KeyEvent::new(CrosstermKeyCode::Char(' '), KeyModifiers::NONE);
        app.handle_event(&Event::Key(space));

        // The pending chord should NOT be set.
        assert_eq!(app.pending_input, PendingAppInput::Idle);
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

        // Filter to an executable App-owned row before pressing Enter.
        for character in ['h', 'e', 'l', 'p'] {
            app.handle_event(&Event::Key(KeyEvent::new(
                CrosstermKeyCode::Char(character),
                KeyModifiers::NONE,
            )));
        }
        let enter = KeyEvent::new(CrosstermKeyCode::Enter, KeyModifiers::NONE);
        app.handle_event(&Event::Key(enter));

        // Executing Help opens a fresh palette.
        assert!(app.overlay.is_palette());
    }

    /// T12: Palette reference entry — Enter on Vim reference shows status.
    #[test]
    fn app_palette_reference_entry() {
        let session = EditorSession::from_text("hello");
        let mut app = test_app(session);

        open_palette_with_space_h(&mut app);
        assert!(app.overlay.is_palette(), "palette should be open");

        // The first registry row is a visibility-only core reference.

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
        let PendingAppInput::Space { since: instant } = app.pending_input else {
            panic!("Space must establish pending App input");
        };

        // Tick with a time immediately after Space (0ms delay).
        app.tick(instant);

        // should_show should return false at 0ms delay.
        assert!(!crate::widgets::which_key::should_show(
            Some(instant),
            instant
        ));

        // Tick with a time 200ms later.
        let later = instant + std::time::Duration::from_millis(200);
        app.tick(later);

        // should_show should return true at 200ms delay.
        assert!(crate::widgets::which_key::should_show(Some(instant), later));
    }

    #[test]
    fn app_which_key_preserves_mode_badge_gap() {
        let session = EditorSession::from_text("hello");
        let mut app = test_app(session);
        let space = KeyEvent::new(CrosstermKeyCode::Char(' '), KeyModifiers::NONE);
        app.handle_event(&Event::Key(space));
        let PendingAppInput::Space { since } = app.pending_input else {
            panic!("Space must establish pending App input");
        };
        app.tick(since + std::time::Duration::from_millis(200));

        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(70, 10)).unwrap();
        terminal.draw(|frame| app.render(frame)).unwrap();

        let buffer = terminal.backend().buffer();
        let status_y = 9;
        let row_style = theme::DEFAULT_DARK.ui_style(Tier::TrueColor, theme::UiSlot::StatusBar);
        let gap = buffer
            .cell((crate::widgets::status_bar::MODE_BADGE_COLS, status_y))
            .unwrap();
        assert_eq!(gap.symbol(), " ");
        assert_eq!(gap.fg, row_style.fg.unwrap());
        assert_eq!(gap.bg, row_style.bg.unwrap());

        let row: String = (0..70)
            .map(|x| buffer.cell((x, status_y)).unwrap().symbol())
            .collect();
        let which_key_x = row
            .find("Space:")
            .expect("delayed which-key text must render") as u16;
        assert_eq!(
            which_key_x,
            crate::widgets::status_bar::STATUS_CONTENT_OFFSET
        );
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
        assert_eq!(app.session().unwrap().mode(), oom_edit_core::Mode::Normal);
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
        assert_eq!(app.session().unwrap().mode(), oom_edit_core::Mode::Normal);
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
        app.execute_command(AppCommand::CycleTheme);
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
                std::time::Instant::now(),
            );

            app.execute_command(AppCommand::CycleTheme);

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
                std::time::Instant::now(),
            );

            app.execute_command(AppCommand::CycleTheme);

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
        assert_eq!(app.session().unwrap().mode(), oom_edit_core::Mode::Normal);

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
            std::time::Instant::now(),
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
            std::time::Instant::now(),
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

    #[test]
    fn space_quit_and_ex_quit_share_dirty_confirmation() {
        let dir = tempfile::tempdir().unwrap();
        let mut space = file_backed_app(dir.path(), &[("space.md", "one\n")]);
        let mut ex = file_backed_app(dir.path(), &[("ex.md", "one\n")]);
        dirty_tab(&mut space, 0, "space ");
        dirty_tab(&mut ex, 0, "ex ");

        type_chars(&mut space, [' ', 'q']);
        type_ex(&mut ex, "q");

        assert!(matches!(space.overlay, Overlay::ConfirmQuit(_)));
        assert!(matches!(ex.overlay, Overlay::ConfirmQuit(_)));
        assert!(!space.should_quit && !ex.should_quit);
    }

    #[test]
    fn dirty_close_discard_closes_only_captured_tab() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = file_backed_app(
            dir.path(),
            &[("first.md", "first\n"), ("second.md", "second\n")],
        );
        dirty_tab(&mut app, 0, "dirty ");
        app.execute_lifecycle(LifecycleAction::CloseTab(CloseTabRequest {
            target: 0,
            force: false,
            dirty_policy: DirtyClosePolicy::Confirm,
        }));
        type_chars(&mut app, ['n']);

        assert_eq!(app.tab_count(), 1);
        assert_eq!(app.tabs[0].session.document(), "second\n");
        assert_eq!(
            std::fs::read_to_string(dir.path().join("first.md")).unwrap(),
            "first\n"
        );
    }

    #[test]
    fn dirty_close_save_saves_and_closes_only_captured_tab() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = file_backed_app(
            dir.path(),
            &[("first.md", "first\n"), ("second.md", "second\n")],
        );
        dirty_tab(&mut app, 0, "saved ");
        app.execute_lifecycle(LifecycleAction::CloseTab(CloseTabRequest {
            target: 0,
            force: false,
            dirty_policy: DirtyClosePolicy::Confirm,
        }));
        type_chars(&mut app, ['y']);

        assert_eq!(app.tab_count(), 1);
        assert_eq!(app.tabs[0].session.document(), "second\n");
        assert_eq!(
            std::fs::read_to_string(dir.path().join("first.md")).unwrap(),
            "saved first\n"
        );
    }

    #[test]
    fn dirty_close_cancel_preserves_target_and_dirty_state() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = file_backed_app(dir.path(), &[("first.md", "first\n")]);
        dirty_tab(&mut app, 0, "dirty ");
        type_ex(&mut app, "q");
        app.handle_event(&Event::Key(KeyEvent::new(
            CrosstermKeyCode::Esc,
            KeyModifiers::NONE,
        )));

        assert_eq!(app.tab_count(), 1);
        assert!(app.tabs[0].session.is_dirty());
        assert!(!app.overlay.is_some());
        assert!(!app.should_quit);
    }

    #[test]
    fn dirty_last_tab_discard_sets_should_quit() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = file_backed_app(dir.path(), &[("only.md", "one\n")]);
        dirty_tab(&mut app, 0, "dirty ");
        type_ex(&mut app, "q");
        type_chars(&mut app, ['n']);
        assert!(app.should_quit);
        assert_eq!(app.tab_count(), 0);
    }

    #[test]
    fn dirty_last_tab_save_failure_does_not_quit() {
        let mut app = test_app(EditorSession::from_text("one\n"));
        dirty_tab(&mut app, 0, "dirty ");
        type_ex(&mut app, "q");
        type_chars(&mut app, ['y']);
        assert!(!app.should_quit);
        assert_eq!(app.tab_count(), 1);
        assert!(app.tabs[0].session.is_dirty());
        assert!(app.transient.as_ref().unwrap().text.contains("Save error"));
    }

    #[test]
    fn dirty_tabclose_refuses_without_overlay() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = file_backed_app(dir.path(), &[("only.md", "one\n")]);
        dirty_tab(&mut app, 0, "dirty ");
        app.handle_effect(Effect::TabCloseRequested {
            index: None,
            force: false,
        });
        assert_eq!(app.tab_count(), 1);
        assert!(!app.overlay.is_some());
        assert!(app.transient.as_ref().unwrap().text.contains(":tabclose!"));
    }

    #[test]
    fn forced_tabclose_discards_target() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = file_backed_app(
            dir.path(),
            &[("first.md", "first\n"), ("second.md", "second\n")],
        );
        dirty_tab(&mut app, 0, "dirty ");
        app.handle_effect(Effect::TabCloseRequested {
            index: Some(0),
            force: true,
        });
        assert_eq!(app.tab_count(), 1);
        assert_eq!(app.tabs[0].session.document(), "second\n");
        assert_eq!(
            std::fs::read_to_string(dir.path().join("first.md")).unwrap(),
            "first\n"
        );
    }

    #[test]
    fn indexed_dirty_tab_close_never_acts_on_active_tab() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = file_backed_app(
            dir.path(),
            &[("target.md", "target\n"), ("active.md", "active\n")],
        );
        dirty_tab(&mut app, 0, "dirty ");
        app.active_tab = 1;
        app.handle_effect(Effect::TabCloseRequested {
            index: Some(0),
            force: false,
        });
        assert_eq!(app.active_tab, 1);
        assert_eq!(app.tab_count(), 2);
        assert_eq!(app.tabs[1].session.document(), "active\n");
        assert!(app.tabs[0].session.is_dirty());
    }

    #[test]
    fn confirmation_modal_blocks_tab_mutation_until_resolution() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = file_backed_app(
            dir.path(),
            &[("first.md", "first\n"), ("second.md", "second\n")],
        );
        dirty_tab(&mut app, 0, "dirty ");
        type_ex(&mut app, "q");
        type_chars(&mut app, ['g', 't', ' ', '2']);
        assert_eq!(app.active_tab, 0);
        assert_eq!(app.tab_count(), 2);
        assert!(matches!(app.overlay, Overlay::ConfirmQuit(_)));
    }

    #[test]
    fn dirty_open_without_force_refuses_and_retains_session() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = file_backed_app(
            dir.path(),
            &[("first.md", "first\n"), ("other.md", "other\n")],
        );
        dirty_tab(&mut app, 0, "dirty ");
        app.handle_effect(Effect::OpenRequested {
            path: dir.path().join("other.md"),
            force: false,
        });
        assert_eq!(app.tabs[0].session.document(), "dirty first\n");
        assert!(app.tabs[0].session.is_dirty());
        assert!(!app.overlay.is_some());
        assert!(app.transient.as_ref().unwrap().text.contains(":e!"));
    }

    #[test]
    fn clean_and_forced_open_replace_only_target_and_reset_all_scroll_state() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = file_backed_app(
            dir.path(),
            &[
                ("first.md", "first\n"),
                ("second.md", "second\n"),
                ("new.md", "new\n"),
            ],
        );
        app.tabs.truncate(2);
        app.tabs[0].top_line = 4;
        app.tabs[0].left_col = 5;
        app.tabs[0].skip_rows = 6;
        app.tabs[0].rendered_top = 7;
        dirty_tab(&mut app, 0, "dirty ");
        app.handle_effect(Effect::OpenRequested {
            path: dir.path().join("new.md"),
            force: true,
        });
        assert_eq!(app.tabs[0].session.document(), "new\n");
        assert_eq!(app.tabs[1].session.document(), "second\n");
        assert_eq!(
            (
                app.tabs[0].top_line,
                app.tabs[0].left_col,
                app.tabs[0].skip_rows,
                app.tabs[0].rendered_top
            ),
            (0, 0, 0, 0)
        );

        let mut clean = file_backed_app(
            dir.path(),
            &[("clean.md", "clean\n"), ("peer.md", "peer\n")],
        );
        clean.tabs[0].top_line = 9;
        clean.tabs[0].left_col = 8;
        clean.tabs[0].skip_rows = 7;
        clean.tabs[0].rendered_top = 6;
        clean.handle_effect(Effect::OpenRequested {
            path: dir.path().join("new.md"),
            force: false,
        });
        assert_eq!(clean.tabs[0].session.document(), "new\n");
        assert_eq!(clean.tabs[1].session.document(), "peer\n");
        assert_eq!(
            (
                clean.tabs[0].top_line,
                clean.tabs[0].left_col,
                clean.tabs[0].skip_rows,
                clean.tabs[0].rendered_top
            ),
            (0, 0, 0, 0)
        );
    }

    #[test]
    fn quit_all_dirty_refuses_and_force_discards_all() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = file_backed_app(dir.path(), &[("one.md", "one\n"), ("two.md", "two\n")]);
        dirty_tab(&mut app, 1, "dirty ");
        app.handle_effect(Effect::QuitAllRequested { force: false });
        assert!(!app.should_quit);
        assert!(!app.overlay.is_some());
        app.handle_effect(Effect::QuitAllRequested { force: true });
        assert!(app.should_quit);
        assert_eq!(
            std::fs::read_to_string(dir.path().join("two.md")).unwrap(),
            "two\n"
        );
    }

    #[test]
    fn lifecycle_entry_points_normalize_to_same_final_state() {
        let dir = tempfile::tempdir().unwrap();
        let state = |app: &App, path: &std::path::Path| {
            (
                app.tab_count(),
                app.active_tab,
                app.should_quit,
                app.overlay.is_some(),
                app.pending_input,
                app.tabs
                    .iter()
                    .map(|tab| tab.session.is_dirty())
                    .collect::<Vec<_>>(),
                app.tabs
                    .iter()
                    .map(|tab| tab.session.document())
                    .collect::<Vec<_>>(),
                std::fs::read_to_string(path).unwrap(),
                app.transient
                    .as_ref()
                    .map(|message| (message.text.clone(), message.severity)),
                app.tabs
                    .iter()
                    .map(|tab| (tab.top_line, tab.left_col, tab.skip_rows, tab.rendered_top))
                    .collect::<Vec<_>>(),
            )
        };

        let make_apps = |case: &str| {
            ["space", "palette", "ex", "effect"].map(|entry| {
                let root = dir.path().join(format!("{case}-{entry}"));
                std::fs::create_dir_all(&root).unwrap();
                let app = file_backed_app(&root, &[("same.md", "one\n")]);
                (app, root.join("same.md"))
            })
        };

        let mut save = make_apps("save");
        for (app, _) in &mut save {
            dirty_tab(app, 0, "saved ");
        }
        type_chars(&mut save[0].0, [' ', 'w']);
        open_palette_with_space_h(&mut save[1].0);
        type_chars(&mut save[1].0, "save".chars());
        save[1].0.handle_event(&Event::Key(KeyEvent::new(
            CrosstermKeyCode::Enter,
            KeyModifiers::NONE,
        )));
        type_ex(&mut save[2].0, "w");
        save[3].0.handle_effect(Effect::SaveRequested {
            path: None,
            force: false,
            retarget: false,
            then_quit: false,
        });
        let expected_save = state(&save[0].0, &save[0].1);
        for (app, path) in &save[1..] {
            assert_eq!(state(app, path), expected_save);
        }

        let mut close = make_apps("close");
        for (app, _) in &mut close {
            dirty_tab(app, 0, "dirty ");
        }
        type_chars(&mut close[0].0, [' ', 'q']);
        open_palette_with_space_h(&mut close[1].0);
        type_chars(&mut close[1].0, "quit".chars());
        close[1].0.handle_event(&Event::Key(KeyEvent::new(
            CrosstermKeyCode::Enter,
            KeyModifiers::NONE,
        )));
        type_ex(&mut close[2].0, "q");
        close[3]
            .0
            .handle_effect(Effect::QuitRequested { force: false });
        for (app, _) in &close {
            assert!(matches!(app.overlay, Overlay::ConfirmQuit(_)));
        }
        for (app, _) in &mut close {
            type_chars(app, ['n']);
        }
        let expected_close = state(&close[0].0, &close[0].1);
        for (app, path) in &close[1..] {
            assert_eq!(state(app, path), expected_close);
        }
    }

    #[test]
    fn space_save_and_ex_save_share_success_message_and_save_point() {
        let dir = tempfile::tempdir().unwrap();
        let left = dir.path().join("left");
        let right = dir.path().join("right");
        std::fs::create_dir_all(&left).unwrap();
        std::fs::create_dir_all(&right).unwrap();
        let mut space = file_backed_app(&left, &[("same.md", "one\n")]);
        let mut ex = file_backed_app(&right, &[("same.md", "one\n")]);
        dirty_tab(&mut space, 0, "saved ");
        dirty_tab(&mut ex, 0, "saved ");
        type_chars(&mut space, [' ', 'w']);
        type_ex(&mut ex, "w");
        assert_eq!(
            space.transient.as_ref().unwrap().text,
            ex.transient.as_ref().unwrap().text
        );
        assert!(!space.tabs[0].session.is_dirty());
        assert!(!ex.tabs[0].session.is_dirty());
    }

    #[test]
    fn space_save_and_ex_save_share_external_modification_confirmation() {
        let dir = tempfile::tempdir().unwrap();
        let mut space = file_backed_app(dir.path(), &[("space.md", "one\n")]);
        let mut ex = file_backed_app(dir.path(), &[("ex.md", "one\n")]);
        dirty_tab(&mut space, 0, "space ");
        dirty_tab(&mut ex, 0, "ex ");
        std::fs::write(
            dir.path().join("space.md"),
            "changed externally and longer\n",
        )
        .unwrap();
        std::fs::write(dir.path().join("ex.md"), "changed externally and longer\n").unwrap();
        type_chars(&mut space, [' ', 'w']);
        type_ex(&mut ex, "w");
        assert!(matches!(space.overlay, Overlay::ConfirmOverwrite(_)));
        assert!(matches!(ex.overlay, Overlay::ConfirmOverwrite(_)));
    }

    #[test]
    fn overwrite_confirmation_preserves_write_quit_continuation() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("only.md");
        let mut app = file_backed_app(dir.path(), &[("only.md", "one\n")]);
        dirty_tab(&mut app, 0, "saved ");
        std::fs::write(&path, "changed externally and longer\n").unwrap();
        type_ex(&mut app, "wq");
        assert!(matches!(app.overlay, Overlay::ConfirmOverwrite(_)));
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "changed externally and longer\n"
        );
        type_chars(&mut app, ['o']);
        assert!(app.should_quit);
        assert_eq!(app.tab_count(), 0);
        assert_eq!(std::fs::read_to_string(path).unwrap(), "saved one\n");
    }

    #[test]
    fn overwrite_reload_targets_original_tab_and_resets_scroll() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("target.md");
        let mut app = file_backed_app(
            dir.path(),
            &[("target.md", "target\n"), ("active.md", "active\n")],
        );
        dirty_tab(&mut app, 0, "dirty ");
        app.tabs[0].top_line = 2;
        app.tabs[0].left_col = 3;
        app.tabs[0].skip_rows = 4;
        app.tabs[0].rendered_top = 5;
        std::fs::write(&target, "external replacement that is longer\n").unwrap();
        app.execute_lifecycle(LifecycleAction::Save(SaveRequest {
            target: 0,
            path: None,
            force: false,
            retarget: true,
            continuation: SaveContinuation::StayOpen,
        }));
        app.active_tab = 1;
        type_chars(&mut app, ['r']);
        assert_eq!(app.active_tab, 1);
        assert_eq!(
            app.tabs[0].session.document(),
            "external replacement that is longer\n"
        );
        assert_eq!(
            (
                app.tabs[0].top_line,
                app.tabs[0].left_col,
                app.tabs[0].skip_rows,
                app.tabs[0].rendered_top
            ),
            (0, 0, 0, 0)
        );
        assert_eq!(app.tabs[1].session.document(), "active\n");
    }

    #[test]
    fn confirm_shortcuts_execute_without_extra_enter() {
        for shortcut in ['y', 'w', 'n'] {
            let dir = tempfile::tempdir().unwrap();
            let mut app = file_backed_app(dir.path(), &[("only.md", "one\n")]);
            dirty_tab(&mut app, 0, "dirty ");
            type_ex(&mut app, "q");
            type_chars(&mut app, [shortcut]);
            assert!(
                app.should_quit,
                "shortcut {shortcut} did not resolve immediately"
            );
        }

        for shortcut in ['o', 'r'] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("only.md");
            let mut app = file_backed_app(dir.path(), &[("only.md", "one\n")]);
            dirty_tab(&mut app, 0, "dirty ");
            std::fs::write(&path, "external replacement that is longer\n").unwrap();
            type_ex(&mut app, "w");
            type_chars(&mut app, [shortcut]);
            assert!(
                !app.overlay.is_some(),
                "shortcut {shortcut} needs another key"
            );
        }

        let enter_dir = tempfile::tempdir().unwrap();
        let mut enter = file_backed_app(enter_dir.path(), &[("only.md", "one\n")]);
        dirty_tab(&mut enter, 0, "dirty ");
        type_ex(&mut enter, "q");
        enter.handle_event(&Event::Key(KeyEvent::new(
            CrosstermKeyCode::Enter,
            KeyModifiers::NONE,
        )));
        assert!(
            enter.should_quit,
            "Enter did not execute the highlighted choice"
        );

        for (code, modifiers) in [
            (CrosstermKeyCode::Esc, KeyModifiers::NONE),
            (CrosstermKeyCode::Char('c'), KeyModifiers::CONTROL),
        ] {
            let dir = tempfile::tempdir().unwrap();
            let mut app = file_backed_app(dir.path(), &[("only.md", "one\n")]);
            dirty_tab(&mut app, 0, "dirty ");
            type_ex(&mut app, "q");
            app.handle_event(&Event::Key(KeyEvent::new(code, modifiers)));
            assert!(!app.overlay.is_some());
            assert!(!app.should_quit);
            assert!(app.tabs[0].session.is_dirty());
        }

        let overwrite_dir = tempfile::tempdir().unwrap();
        let overwrite_path = overwrite_dir.path().join("only.md");
        let mut overwrite = file_backed_app(overwrite_dir.path(), &[("only.md", "one\n")]);
        dirty_tab(&mut overwrite, 0, "dirty ");
        std::fs::write(&overwrite_path, "external replacement that is longer\n").unwrap();
        type_ex(&mut overwrite, "w");
        overwrite.handle_event(&Event::Key(KeyEvent::new(
            CrosstermKeyCode::Enter,
            KeyModifiers::NONE,
        )));
        assert!(!overwrite.overlay.is_some());
        assert_eq!(
            std::fs::read_to_string(overwrite_path).unwrap(),
            "dirty one\n"
        );

        for (code, modifiers) in [
            (CrosstermKeyCode::Esc, KeyModifiers::NONE),
            (CrosstermKeyCode::Char('c'), KeyModifiers::CONTROL),
        ] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("only.md");
            let mut app = file_backed_app(dir.path(), &[("only.md", "one\n")]);
            dirty_tab(&mut app, 0, "dirty ");
            std::fs::write(&path, "external replacement that is longer\n").unwrap();
            type_ex(&mut app, "w");
            app.handle_event(&Event::Key(KeyEvent::new(code, modifiers)));
            assert!(!app.overlay.is_some());
            assert!(app.tabs[0].session.is_dirty());
            assert_eq!(
                std::fs::read_to_string(path).unwrap(),
                "external replacement that is longer\n"
            );
        }
    }

    #[test]
    fn invalid_confirmation_target_cancels_without_touching_tabs() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = file_backed_app(dir.path(), &[("only.md", "one\n")]);
        app.overlay = Overlay::open_confirm_quit(CloseTabRequest {
            target: 99,
            force: false,
            dirty_policy: DirtyClosePolicy::Confirm,
        });
        type_chars(&mut app, ['n']);
        assert_eq!(app.tab_count(), 1);
        assert_eq!(app.tabs[0].session.document(), "one\n");
        assert!(app
            .transient
            .as_ref()
            .unwrap()
            .text
            .contains("no longer exists"));
    }

    #[test]
    fn save_failure_never_runs_close_continuation() {
        let mut app = test_app(EditorSession::from_text("one\n"));
        dirty_tab(&mut app, 0, "dirty ");
        app.execute_lifecycle(LifecycleAction::Save(SaveRequest {
            target: 0,
            path: None,
            force: false,
            retarget: true,
            continuation: SaveContinuation::CloseSavedTab,
        }));
        assert!(!app.should_quit);
        assert_eq!(app.tab_count(), 1);
        assert!(app.tabs[0].session.is_dirty());
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
