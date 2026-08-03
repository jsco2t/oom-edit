//! EditorSession — the core editing façade.
//!
//! This module owns the `EditorSession` type, the `Mode` machine, and the
//! session-facing type definitions (`KeyInput`, `KeyCode`, `Modifiers`,
//! `Effect`, `Viewport`). It composes `VimCore` (the hjkl wrapper) with the
//! document model and highlighting pipeline.

// ── Mode ───────────────────────────────────────────────────────────────────

/// The seven editor modes. Editing modes (Normal, Insert, Visual,
/// VisualLine, VisualBlock, Command) are owned by the hjkl engine;
/// View is owned by oom-edit-core's session layer.
///
/// See plan §6.1 / FR-1.1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Mode {
    /// Normal mode — modal editing with Vim motions.
    Normal,
    /// Insert mode — direct text entry.
    Insert,
    /// Visual mode — character-wise selection.
    Visual,
    /// Visual line mode — line-wise selection.
    VisualLine,
    /// Visual block mode — block-wise selection.
    VisualBlock,
    /// Command mode — ex-command entry (e.g. `:w`).
    Command,
    /// View mode — read-only rendered view.
    View,
}

// ── KeyInput / KeyCode / Modifiers ─────────────────────────────────────────

/// Terminal-agnostic key representation (mirrors, but does not expose,
/// crossterm's model).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct KeyInput {
    /// The key code.
    pub code: KeyCode,
    /// The modifiers.
    pub mods: Modifiers,
}

/// A key code — either a printable character or a special key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct KeyCode {
    /// The key code kind.
    pub kind: KeyCodeKind,
}

/// A key code — either a printable character or a special key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KeyCodeKind {
    /// A printable character.
    Char(char),
    /// Enter / Return key.
    Enter,
    /// Escape key.
    Esc,
    /// Backspace key.
    Backspace,
    /// Tab key.
    Tab,
    /// Shift-Tab key.
    BackTab,
    /// Up arrow key.
    Up,
    /// Down arrow key.
    Down,
    /// Left arrow key.
    Left,
    /// Right arrow key.
    Right,
    /// Home key.
    Home,
    /// End key.
    End,
    /// Page Up key.
    PageUp,
    /// Page Down key.
    PageDown,
    /// Delete key.
    Delete,
    /// Function key F1-F24.
    F(u8),
}

/// Keyboard modifier bits accompanying every keystroke.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Modifiers {
    /// Ctrl modifier pressed.
    pub ctrl: bool,
    /// Alt/Option modifier pressed.
    pub alt: bool,
    /// Shift modifier pressed.
    pub shift: bool,
}

// ── Effect ─────────────────────────────────────────────────────────────────

/// Effects emitted by `EditorSession::handle_key`. The host drains these
/// after each key to decide what to render or act on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    /// A save was requested (from `:w`, `:wq`, etc.).
    SaveRequested {
        /// File path, or `None` for the current buffer.
        path: Option<std::path::PathBuf>,
        /// Force save (ignore read-only flag).
        force: bool,
        /// Quit after saving.
        then_quit: bool,
    },
    /// A quit was requested (from `:q`, `:q!`, etc.).
    QuitRequested {
        /// Force quit (ignore unsaved changes).
        force: bool,
    },
    /// An open-file was requested (from `:e`, `:e!`, etc.).
    OpenRequested {
        /// File path to open.
        path: std::path::PathBuf,
        /// Force open (ignore unsaved changes).
        force: bool,
    },
    /// Yanked text to the system clipboard (e.g. `"+y`).
    ClipboardWrite(String),
    /// Mode changed.
    ModeChanged(Mode),
    /// A status message to display.
    Message {
        /// The message text.
        text: String,
        /// The message severity.
        severity: Severity,
    },
    /// Cursor moved (render-invalidation hint).
    CursorMoved,
    /// Buffer was edited (dirty may have changed).
    Edited,
}

// ── Severity ───────────────────────────────────────────────────────────────

/// Message severity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Severity {
    /// Informational message.
    Info,
    /// Success message.
    Success,
    /// Warning message.
    Warning,
    /// Error message.
    Error,
}

// ── Viewport ───────────────────────────────────────────────────────────────

/// Viewport specification for rendering.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Viewport {
    /// The 0-based index of the first visible line.
    pub top_line: usize,
    /// The height of the viewport in lines.
    pub height: u16,
    /// The width of the viewport in columns.
    pub width: u16,
}

// ── VimCore re-export (internal) ──────────────────────────────────────────

use crate::vim::{UndoMark, VimCore, VimEffect};

// ── Document (internal) ───────────────────────────────────────────────────

use crate::document::Document;
use crate::error::{OpenError, SaveError};
use crate::style::{SearchDirection, ViewCursor, ViewLayout, ViewSearch};
use crate::view::nav;
use crate::view::BlockModel;

// ── ViewState ──────────────────────────────────────────────────────────────

/// State for View mode (read-only rendered view).
///
/// Holds a cached layout, cursor position, search state, and front-matter
/// panel collapse state. The layout is invalidated on edits.
#[allow(dead_code)]
struct ViewState {
    /// Cached view layout (None = needs rebuild).
    layout_cache: Option<ViewLayout>,
    /// Current cursor position in view coordinates.
    cursor: ViewCursor,
    /// Active search state (if in search mode).
    search: Option<ViewSearch>,
    /// Whether the front-matter panel is collapsed.
    fm_collapsed: bool,
    /// Accumulated numeric count for navigation commands.
    count: usize,
}

#[allow(dead_code)]
impl ViewState {
    fn new() -> Self {
        Self {
            layout_cache: None,
            cursor: ViewCursor::new(0),
            search: None,
            fm_collapsed: false,
            count: 0,
        }
    }

    /// Get the layout, building it if necessary.
    fn get_layout(&mut self, text: &str) -> &ViewLayout {
        if self.layout_cache.is_none() {
            let model = BlockModel::build(text, None);
            // Use a default width of 80 for the layout
            // The actual width comes from the viewport in the TUI layer
            self.layout_cache = Some(ViewLayout::build(
                &model,
                80,
                &crate::syntax::Highlighter::new(text),
            ));
        }
        self.layout_cache.as_ref().unwrap()
    }

    /// Invalidate the layout cache.
    fn invalidate(&mut self) {
        self.layout_cache = None;
    }

    /// Clear search state.
    fn clear_search(&mut self) {
        self.search = None;
    }
}

// ── EditorSession ──────────────────────────────────────────────────────────

/// The core editing session. This is the public façade through which a host
/// feeds keys, drains effects, and queries state.
///
/// See architecture §6 for the full API contract.
pub struct EditorSession {
    /// The hjkl wrapper — owns the modal editing engine.
    vim: VimCore,
    /// The current mode (View is session-owned, not hjkl-owned).
    mode: Mode,
    /// Dirty generation at last save.
    save_point: UndoMark,
    /// Buffer for ex-command text in Command mode.
    command_buffer: String,
    /// The document model — text, path, front matter, I/O state.
    document: Document,
    /// View mode state (only present when in View mode).
    view_state: Option<ViewState>,
}

impl EditorSession {
    /// Create a new session from initial text. Starts in Normal mode.
    pub fn from_text(text: &str) -> Self {
        let mut document = Document::from_text(text);
        let save_point = document.save_point();
        Self {
            vim: VimCore::new(text),
            mode: Mode::Normal,
            save_point,
            command_buffer: String::new(),
            document,
            view_state: None,
        }
    }

    /// Open a session from a file path.
    ///
    /// If the file does not exist, creates a new-buffer session with empty
    /// text and the path retained (FR-6.10 / new-file semantics).
    ///
    /// Per FR-5.1: invalid UTF-8 is refused with the byte offset of the
    /// first bad byte.
    pub fn open(path: &std::path::Path) -> Result<Self, OpenError> {
        let mut document = Document::open(path)?;
        let text = document.text().to_string();
        let save_point = document.save_point();
        Ok(Self {
            vim: VimCore::new(&text),
            mode: Mode::Normal,
            save_point,
            command_buffer: String::new(),
            document,
            view_state: None,
        })
    }

    /// Save the document to its path (or the given override path).
    ///
    /// `force: true` bypasses external-modification detection (FR-5.7).
    ///
    /// Returns `SaveError::ExternallyModified` if the file was externally
    /// modified and `force` is `false` (FR-5.7).
    pub fn save(&mut self, path: Option<&std::path::Path>, force: bool) -> Result<(), SaveError> {
        // Get the current text from the vim buffer
        let text = self.vim.text();
        // Save using the document's I/O logic, passing the vim buffer text
        self.document.save_with_text(&text, path, force)?;
        // Update the vim core's save point to match
        self.save_point = self.document.save_point();
        // Invalidate view layout cache on save
        if let Some(ref mut vs) = self.view_state {
            vs.invalidate();
        }
        Ok(())
    }

    /// Handle a key input. Returns zero or more effects.
    pub fn handle_key(&mut self, key: KeyInput) -> Vec<Effect> {
        // View mode is read-only (FR-1.6) — most keys are no-ops
        if self.mode == Mode::View {
            return self.handle_view_mode_key(key);
        }

        // Command mode: hjkl doesn't expose it via public API,
        // so we intercept `:` in Normal mode and manage it here.
        if self.mode == Mode::Command {
            return self.handle_command_mode_key(key);
        }

        // Translate our KeyInput → VimCore's internal KeyInput
        let vim_key = self.key_input_to_vim(key);

        // Special handling: `:` in Normal mode enters Command mode
        // (hjkl's vim_mode() doesn't expose command-line mode).
        // We must check BEFORE feeding to hjkl, because hjkl may
        // change its internal mode when `:` is pressed.
        let was_normal = self.mode == Mode::Normal;

        // Feed through vim core
        let vim_effects = self.vim.handle_key(vim_key);

        // Translate VimEffects → Effects
        let mut effects = Vec::new();
        for vef in vim_effects {
            match vef {
                VimEffect::ModeChanged(vim_mode) => {
                    self.mode = vim_mode.into();
                    effects.push(Effect::ModeChanged(self.mode));
                }
                VimEffect::Edited { edits: _ } => {
                    effects.push(Effect::Edited);
                    // Invalidate view layout cache on edit
                    if let Some(ref mut vs) = self.view_state {
                        vs.invalidate();
                    }
                }
                VimEffect::CursorMoved => {
                    effects.push(Effect::CursorMoved);
                }
                VimEffect::ExCommand { command } => {
                    effects.extend(self.process_ex_command(&command));
                }
                VimEffect::CommandCancelled => {
                    // Mode will be updated via ModeChanged
                }
                VimEffect::ClipboardYank(text) => {
                    effects.push(Effect::ClipboardWrite(text));
                }
                VimEffect::SearchWrapped => {
                    effects.push(Effect::Message {
                        text: "Search wrapped around buffer".to_string(),
                        severity: Severity::Info,
                    });
                }
                VimEffect::Bell => {
                    effects.push(Effect::Message {
                        text: "".to_string(),
                        severity: Severity::Warning,
                    });
                }
            }
        }

        // Enter Command mode if `:` was pressed in Normal mode
        if was_normal
            && matches!(key.code.kind, KeyCodeKind::Char(':'))
            && !key.mods.ctrl
            && !key.mods.alt
            && !key.mods.shift
        {
            self.mode = Mode::Command;
            effects.push(Effect::ModeChanged(Mode::Command));
        }

        effects
    }

    /// Return the current mode.
    pub fn mode(&self) -> Mode {
        self.mode
    }

    /// Return the full document text.
    pub fn document(&self) -> String {
        self.vim.text()
    }

    /// Return a reference to the document model.
    pub fn document_ref(&self) -> &Document {
        &self.document
    }

    /// Return cursor position as `(line, col)` — 0-based.
    pub fn cursor(&self) -> (usize, usize) {
        self.vim.cursor()
    }

    /// Return visual-mode selection byte ranges.
    pub fn selections(&self) -> Vec<std::ops::Range<usize>> {
        self.vim.selections()
    }

    /// Return the command-line text, or `None` when not in Command mode.
    pub fn command_line(&self) -> Option<String> {
        self.vim.command_line()
    }

    /// Return the view cursor position, or `None` when not in View mode.
    pub fn view_cursor(&self) -> Option<ViewCursor> {
        self.view_state.as_ref().map(|vs| vs.cursor)
    }

    /// Return the view layout, or `None` when not in View mode.
    pub fn view_layout(&self) -> Option<&ViewLayout> {
        self.view_state
            .as_ref()
            .and_then(|vs| vs.layout_cache.as_ref())
    }

    /// Return the view layout (building it if necessary), or `None` when not in View mode.
    pub fn view_layout_mut(&mut self) -> Option<&ViewLayout> {
        if self.mode == Mode::View {
            if let Some(ref mut vs) = self.view_state {
                let text = self.vim.text();
                Some(vs.get_layout(&text))
            } else {
                None
            }
        } else {
            None
        }
    }

    /// Return the view search state, or `None` when not in View mode.
    pub fn view_search(&self) -> Option<&ViewSearch> {
        self.view_state.as_ref().and_then(|vs| vs.search.as_ref())
    }

    /// Check if the buffer is dirty (modified since last save).
    pub fn is_dirty(&self) -> bool {
        self.vim.is_modified_since(self.save_point)
    }

    /// Take a save point (marks current state as clean).
    pub fn save_point(&mut self) {
        self.save_point = self.vim.save_point();
    }

    /// Return the number of lines in the document.
    pub fn line_count(&self) -> usize {
        // Count newlines + 1 (at least one line)
        let text = self.vim.text();
        if text.is_empty() {
            return 1;
        }
        text.matches('\n').count() + 1
    }

    /// Return a specific line (0-based), or `None` if out of range.
    pub fn line(&self, idx: usize) -> Option<String> {
        self.vim.line(idx)
    }

    /// Toggle View mode. If in an editing mode, enter View. If in View,
    /// return to Normal.
    pub fn toggle_view(&mut self) -> Vec<Effect> {
        if self.mode == Mode::View {
            // Exit View → Normal
            // Apply leave_view to map cursor back to edit coordinates
            let (edit_line, _edit_col) = if let Some(ref mut vs) = self.view_state {
                let text = self.vim.text();
                let cursor_line = vs.cursor.line;
                let layout = vs.get_layout(&text);
                let cursor = ViewCursor::new(cursor_line);
                nav::leave_view(&cursor, layout, &text)
            } else {
                (0, 0)
            };
            // Move edit cursor to mapped position
            self.vim.jump_to(edit_line, 0);
            if let Some(ref mut vs) = self.view_state {
                vs.clear_search();
                vs.count = 0;
            }
            self.mode = Mode::Normal;
            let mut effects = vec![Effect::ModeChanged(Mode::Normal)];
            effects.push(Effect::CursorMoved);
            effects
        } else {
            // Enter View from Normal/Insert/Visual/etc.
            // FR-1.6: Insert/Visual cannot transition directly to View
            // — they first go to Normal
            let (edit_line, edit_col) = self.vim.cursor();
            let text = self.vim.text();
            let model = BlockModel::build(&text, None);
            let layout = ViewLayout::build(&model, 80, &crate::syntax::Highlighter::new(&text));
            let cursor = nav::enter_view(edit_line, edit_col, &layout);

            self.view_state = Some(ViewState {
                layout_cache: Some(layout),
                cursor,
                search: None,
                fm_collapsed: false,
                count: 0,
            });
            self.mode = Mode::View;
            vec![Effect::ModeChanged(Mode::View)]
        }
    }

    // ── Internal helpers ─────────────────────────────────────────────

    /// Handle keys in Command mode (ex-command entry).
    fn handle_command_mode_key(&mut self, key: KeyInput) -> Vec<Effect> {
        let mut effects = Vec::new();
        match key.code.kind {
            KeyCodeKind::Esc => {
                // Cancel command-line and return to Normal
                self.command_buffer.clear();
                self.mode = Mode::Normal;
                effects.push(Effect::ModeChanged(Mode::Normal));
            }
            KeyCodeKind::Enter => {
                // Execute the command from the buffer
                let cmd = self.command_buffer.trim().to_string();
                self.command_buffer.clear();
                effects.extend(self.process_ex_command(&cmd));
                // Only default to Normal if the ex command didn't already change mode
                if !effects.iter().any(|e| matches!(e, Effect::ModeChanged(_))) {
                    self.mode = Mode::Normal;
                    effects.push(Effect::ModeChanged(Mode::Normal));
                }
            }
            KeyCodeKind::Backspace => {
                // Remove last character from command buffer
                self.command_buffer.pop();
            }
            _ => {
                // Collect printable characters in the command buffer
                if let KeyCodeKind::Char(c) = key.code.kind {
                    if !key.mods.ctrl && !key.mods.alt && !key.mods.shift {
                        self.command_buffer.push(c);
                    }
                }
            }
        }
        effects
    }

    /// Handle keys in View mode (FR-1.6: read-only, most keys are no-ops).
    fn handle_view_mode_key(&mut self, key: KeyInput) -> Vec<Effect> {
        // FR-1.6 exception: i/a/o in View jump to editing at mapped position
        match key.code.kind {
            KeyCodeKind::Char('i') if !key.mods.ctrl && !key.mods.alt && !key.mods.shift => {
                return self.exit_view_to_edit(0);
            }
            KeyCodeKind::Char('a') if !key.mods.ctrl && !key.mods.alt && !key.mods.shift => {
                return self.exit_view_to_edit(1);
            }
            KeyCodeKind::Char('o') if !key.mods.ctrl && !key.mods.alt && !key.mods.shift => {
                return self.exit_view_to_edit(self.line_count());
            }
            KeyCodeKind::Esc => {
                return self.toggle_view();
            }
            _ => {}
        }

        // Accumulate numeric count for navigation commands
        if let KeyCodeKind::Char(c) = key.code.kind {
            if c.is_ascii_digit() && !key.mods.ctrl && !key.mods.alt && !key.mods.shift {
                let digit = c.to_digit(10).unwrap() as usize;
                if let Some(vs) = self.view_state.as_mut() {
                    vs.count = vs.count * 10 + digit;
                }
                return Vec::new();
            }
        }

        // Handle navigation keys via nav module
        let mut effects = Vec::new();

        if let Some(ref mut vs) = self.view_state {
            // Extract all needed data before getting mutable layout reference
            let cursor_line = vs.cursor.line;
            let search_pattern = vs.search.as_ref().map(|s| s.pattern.clone());
            let search_direction = vs.search.as_ref().map(|s| s.last_direction);
            let prev_count = vs.count;

            // Reset count before getting layout
            vs.count = 0;

            let text = self.vim.text();
            // Clone the layout to avoid holding a borrow on vs
            let layout = vs.get_layout(&text).clone();
            let jump_targets = layout.jump_targets.clone();
            let max_view_lines = layout.lines.len();

            let cursor = ViewCursor::new(cursor_line);
            let search = search_pattern.as_ref().map(|p| {
                let mut s = ViewSearch::new(p);
                s.set_direction(search_direction.unwrap_or(SearchDirection::Forward));
                s
            });

            let result = nav::handle_key(
                key,
                &cursor,
                search.as_ref(),
                max_view_lines,
                &jump_targets,
                &layout,
                prev_count,
            );

            // Apply search state changes
            if result.search_changed {
                if let Some(new_search) = result.new_search {
                    if new_search.pattern.is_empty() {
                        // Search mode activated but no pattern yet
                        vs.search = Some(new_search);
                    } else {
                        // Pattern entered, perform search
                        vs.search = Some(new_search.clone());
                        if let Some(match_line) = nav::find_next_match(
                            &new_search,
                            &cursor,
                            &layout,
                            &text,
                            new_search.direction(),
                        ) {
                            vs.cursor.line = match_line;
                            effects.push(Effect::CursorMoved);
                        }
                    }
                } else {
                    vs.clear_search();
                }
            } else if let Some(ref mut search_state) = vs.search {
                // Check if we're in search mode and need to accumulate pattern
                if let KeyCodeKind::Char(c) = key.code.kind {
                    if c.is_ascii_alphanumeric() || c == ' ' || c == '.' || c == '_' {
                        search_state.pattern.push(c);
                        // Perform search with updated pattern
                        if let Some(match_line) = nav::find_next_match(
                            search_state,
                            &cursor,
                            &layout,
                            &text,
                            search_state.direction(),
                        ) {
                            vs.cursor.line = match_line;
                            effects.push(Effect::CursorMoved);
                        }
                        return effects;
                    } else if key.code.kind == KeyCodeKind::Esc {
                        // Escape exits search mode
                        vs.clear_search();
                        return effects;
                    }
                }
            }

            // Apply cursor changes
            if result.cursor_moved {
                if let Some(new_cursor) = result.new_cursor {
                    vs.cursor = new_cursor;
                    effects.push(Effect::CursorMoved);
                }
            }
        }

        if effects.is_empty() {
            vec![Effect::Message {
                text: "read-only view — Esc to edit".to_string(),
                severity: Severity::Info,
            }]
        } else {
            effects
        }
    }

    /// Exit View mode and enter Normal mode at a mapped edit position.
    ///
    /// `offset` is the column offset to apply after mapping:
    /// - 0: start of line (i key)
    /// - 1: after character (a key)
    /// - line_count: next line after current (o key)
    fn exit_view_to_edit(&mut self, _offset: usize) -> Vec<Effect> {
        let (edit_line, _edit_col) = if let Some(ref mut vs) = self.view_state {
            let text = self.vim.text();
            let cursor_line = vs.cursor.line;
            let layout = vs.get_layout(&text);
            let cursor = ViewCursor::new(cursor_line);
            nav::leave_view(&cursor, layout, &text)
        } else {
            (0, 0)
        };

        if let Some(ref mut vs) = self.view_state {
            self.vim.jump_to(edit_line, 0);
            vs.clear_search();
            vs.count = 0;
        }
        self.mode = Mode::Normal;
        let mut effects = vec![Effect::ModeChanged(Mode::Normal)];
        effects.push(Effect::CursorMoved);
        effects
    }

    /// Process an ex command text and produce effects.
    ///
    /// Handles: :w, :w!, :w {path}, :wq, :x, :q, :q!, :e, :e!, :e {path},
    /// :saveas, :{number}, :s, :noh, :view, :help, and unknown commands.
    fn process_ex_command(&mut self, command: &str) -> Vec<Effect> {
        let cmd = command.trim();
        let (base, args) = Self::parse_ex_command(cmd);

        match base {
            "w" | "wq" | "x" => {
                let force = base != "w" || args.1;
                if base == "w" && args.0.is_some() {
                    // :w {path} — save copy without retargeting
                    vec![Effect::SaveRequested {
                        path: args.0.map(std::path::PathBuf::from),
                        force: args.1,
                        then_quit: false,
                    }]
                } else {
                    vec![Effect::SaveRequested {
                        path: None,
                        force,
                        then_quit: base != "w",
                    }]
                }
            }
            "q" => vec![Effect::QuitRequested { force: args.1 }],
            "e" => vec![Effect::OpenRequested {
                path: args.0.map(std::path::PathBuf::from).unwrap_or_default(),
                force: args.1,
            }],
            "saveas" => vec![Effect::SaveRequested {
                path: args.0.map(std::path::PathBuf::from),
                force: false,
                then_quit: false,
            }],
            _ if !args.0.is_none() && base.chars().all(|c| c.is_ascii_digit()) => {
                // :{number} — jump to line
                if let Ok(line) = base.parse::<usize>() {
                    vec![Effect::Message {
                        text: format!("Jump to line {}", line + 1),
                        severity: Severity::Info,
                    }]
                } else {
                    vec![Effect::Message {
                        text: format!("Invalid line number: {}", base),
                        severity: Severity::Warning,
                    }]
                }
            }
            "s" | "substitute" => {
                // :[range]s/pattern/replacement/[flags]
                // Try to extract substitute args from the rest of the command
                let sub_args = if let Some(rest) = args.0 {
                    Self::parse_substitute(rest)
                } else {
                    None
                };
                match sub_args {
                    Some((pattern, replacement, flags)) => {
                        let global = flags.contains('g');
                        let text = self.vim.text();
                        let new_text = if global {
                            Self::substitute_global(&text, pattern, replacement)
                        } else {
                            Self::substitute_first(&text, pattern, replacement)
                        };
                        if new_text != text {
                            self.vim.set_text(&new_text);
                            vec![Effect::Edited]
                        } else {
                            vec![Effect::Message {
                                text: "No replacement done".to_string(),
                                severity: Severity::Info,
                            }]
                        }
                    }
                    None => vec![Effect::Message {
                        text: "Invalid substitute command".to_string(),
                        severity: Severity::Warning,
                    }],
                }
            }
            "noh" => vec![Effect::Message {
                text: "Search highlighting cleared".to_string(),
                severity: Severity::Info,
            }],
            "view" => {
                self.mode = Mode::View;
                vec![Effect::ModeChanged(Mode::View)]
            }
            "help" => vec![Effect::Message {
                text: "Help not yet implemented".to_string(),
                severity: Severity::Info,
            }],
            _ => vec![Effect::Message {
                text: format!("Unknown command: {}", base),
                severity: Severity::Warning,
            }],
        }
    }

    /// Parse an ex command into (base_command, (path_arg, force_flag)).
    fn parse_ex_command(cmd: &str) -> (&str, (Option<&str>, bool)) {
        let cmd = cmd.trim_start_matches(':');

        // Special case: substitute commands like "s/pat/rep/" or ":%s/pat/rep/g"
        // have no whitespace separator between base and args. Detect and extract base.
        let (base, rest_str) = if cmd.starts_with("s/") || cmd.starts_with("substitute/") {
            if cmd.starts_with("substitute/") {
                // "substitute/pat/rep/" → base="substitute", rest="/pat/rep/"
                ("substitute", Some(&cmd[10..]))
            } else {
                // "s/pat/rep/" → base="s", rest="/pat/rep/"
                ("s", Some(&cmd[1..]))
            }
        } else if cmd.contains("s/") || cmd.contains("substitute/") {
            // Might be a substitute with range prefix like "%s/pat/rep/g" or "1,2s/pat/rep/"
            // Find the 's/' or 'substitute/' after any range prefix
            if let Some(s_pos) = cmd.find("substitute/") {
                ("substitute", Some(&cmd[s_pos + 10..]))
            } else if let Some(s_pos) = cmd.find("s/") {
                ("s", Some(&cmd[s_pos + 1..]))
            } else {
                let mut parts = cmd.splitn(2, char::is_whitespace);
                (parts.next().unwrap_or(cmd), parts.next())
            }
        } else {
            let mut parts = cmd.splitn(2, char::is_whitespace);
            let b = parts.next().unwrap_or(cmd);
            (b, parts.next())
        };

        // Check for ! suffix on base command
        let (base, force) = if let Some(stripped) = base.strip_suffix('!') {
            (stripped, true)
        } else {
            (base, false)
        };

        // Check for ! in args (e.g., :w!)
        let (args, force) = if let Some(a) = rest_str {
            if a.trim().ends_with('!') {
                (Some(&a[..a.trim().len() - 1]), true)
            } else {
                (rest_str, force)
            }
        } else {
            (rest_str, force)
        };

        // Extract path argument (first word of rest)
        let path = args.and_then(|a| {
            let a = a.trim();
            if a.is_empty() {
                None
            } else {
                a.split_whitespace().next()
            }
        });

        (base, (path, force))
    }

    /// Parse substitute command arguments: "pattern/replacement/flags"
    fn parse_substitute(args: &str) -> Option<(&str, &str, String)> {
        if args.is_empty() {
            return None;
        }
        let delim = args.chars().next()?;
        let closing = args[1..].find(delim)?;
        let pattern = &args[1..1 + closing];
        let rest = &args[1 + closing + 1..];
        let rep_end = rest.find(delim)?;
        let replacement = &rest[..rep_end];
        let flags = rest[rep_end + 1..].to_string();
        Some((pattern, replacement, flags))
    }

    /// Substitute the first occurrence of pattern in text with replacement.
    fn substitute_first(text: &str, pattern: &str, replacement: &str) -> String {
        if let Some(pos) = text.find(pattern) {
            let mut result = text[..pos].to_string();
            result.push_str(replacement);
            result.push_str(&text[pos + pattern.len()..]);
            result
        } else {
            text.to_string()
        }
    }

    /// Substitute all occurrences of pattern in text with replacement.
    fn substitute_global(text: &str, pattern: &str, replacement: &str) -> String {
        text.replace(pattern, replacement)
    }

    /// Translate our KeyInput → VimCore's internal KeyInput.
    fn key_input_to_vim(&self, key: KeyInput) -> crate::vim::KeyInput {
        crate::vim::KeyInput {
            code: crate::vim::KeyCode {
                kind: self.key_code_kind_to_vim(key.code.kind),
            },
            mods: crate::vim::Modifiers {
                ctrl: key.mods.ctrl,
                alt: key.mods.alt,
                shift: key.mods.shift,
            },
        }
    }

    fn key_code_kind_to_vim(&self, kind: KeyCodeKind) -> crate::vim::KeyCodeKind {
        match kind {
            KeyCodeKind::Char(c) => crate::vim::KeyCodeKind::Char(c),
            KeyCodeKind::Enter => crate::vim::KeyCodeKind::Enter,
            KeyCodeKind::Esc => crate::vim::KeyCodeKind::Esc,
            KeyCodeKind::Backspace => crate::vim::KeyCodeKind::Backspace,
            KeyCodeKind::Tab => crate::vim::KeyCodeKind::Tab,
            KeyCodeKind::BackTab => crate::vim::KeyCodeKind::BackTab,
            KeyCodeKind::Up => crate::vim::KeyCodeKind::Up,
            KeyCodeKind::Down => crate::vim::KeyCodeKind::Down,
            KeyCodeKind::Left => crate::vim::KeyCodeKind::Left,
            KeyCodeKind::Right => crate::vim::KeyCodeKind::Right,
            KeyCodeKind::Home => crate::vim::KeyCodeKind::Home,
            KeyCodeKind::End => crate::vim::KeyCodeKind::End,
            KeyCodeKind::PageUp => crate::vim::KeyCodeKind::PageUp,
            KeyCodeKind::PageDown => crate::vim::KeyCodeKind::PageDown,
            KeyCodeKind::Delete => crate::vim::KeyCodeKind::Delete,
            KeyCodeKind::F(n) => crate::vim::KeyCodeKind::F(n),
        }
    }
}

// ── Mode conversion ────────────────────────────────────────────────────────

impl From<crate::vim::Mode> for Mode {
    fn from(m: crate::vim::Mode) -> Self {
        match m {
            crate::vim::Mode::Normal => Mode::Normal,
            crate::vim::Mode::Insert => Mode::Insert,
            crate::vim::Mode::Visual => Mode::Visual,
            crate::vim::Mode::VisualLine => Mode::VisualLine,
            crate::vim::Mode::VisualBlock => Mode::VisualBlock,
            crate::vim::Mode::Command => Mode::Command,
        }
    }
}

impl From<Mode> for crate::vim::Mode {
    fn from(m: Mode) -> Self {
        match m {
            Mode::Normal => crate::vim::Mode::Normal,
            Mode::Insert => crate::vim::Mode::Insert,
            Mode::Visual => crate::vim::Mode::Visual,
            Mode::VisualLine => crate::vim::Mode::VisualLine,
            Mode::VisualBlock => crate::vim::Mode::VisualBlock,
            Mode::Command => crate::vim::Mode::Command,
            Mode::View => crate::vim::Mode::Normal, // View is session-owned; fall back to Normal for hjkl
        }
    }
}
