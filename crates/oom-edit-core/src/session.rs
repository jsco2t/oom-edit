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
    /// Help was requested (from `:help` or `F1`).
    ///
    /// The TUI opens its command palette with the Vim reference section.
    /// Headless hosts may ignore this effect.
    HelpRequested,
    /// A new tab was requested (from `:tabnew {path}`).
    TabNewRequested {
        /// File path to open in the new tab.
        path: std::path::PathBuf,
    },
    /// Close a tab (from `:tabclose` or `:tabclose!`).
    TabCloseRequested {
        /// Tab index to close; `None` = active tab.
        index: Option<usize>,
        /// Force close (discard unsaved changes).
        force: bool,
    },
    /// Switch to the next tab (from `gt`).
    TabNext,
    /// Switch to the previous tab (from `gT`).
    TabPrev,
    /// Jump to a specific tab by 1-based index (from `{count}gt`).
    TabJump {
        /// 1-based tab index.
        index: usize,
    },
    /// Quit all tabs (from `:qa` or `:qa!`).
    QuitAllRequested {
        /// Force quit (discard unsaved changes).
        force: bool,
    },
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
use crate::syntax::Highlighter;
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
    /// The width used when the layout was last built.
    last_width: u16,
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
            last_width: 0,
            cursor: ViewCursor::new(0),
            search: None,
            fm_collapsed: false,
            count: 0,
        }
    }

    /// Create a new ViewState with a pre-built layout.
    fn with_layout(layout: ViewLayout) -> Self {
        Self {
            layout_cache: Some(layout),
            last_width: 0, // Will be set on first render_view call
            cursor: ViewCursor::new(0),
            search: None,
            fm_collapsed: false,
            count: 0,
        }
    }

    /// Get the layout, building or rebuilding it if necessary.
    ///
    /// Rebuilds when the layout is missing or when `width` differs from
    /// the width used for the last build (`last_width`).
    fn get_layout(&mut self, text: &str, highlighter: &Highlighter, width: u16) -> &ViewLayout {
        if self.layout_cache.is_none() || self.last_width != width {
            let model = BlockModel::build(text, None);
            let layout = ViewLayout::build(&model, width, highlighter);
            self.layout_cache = Some(layout);
            self.last_width = width;
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
    /// Syntax highlighter — kept in sync with buffer edits.
    highlighter: Highlighter,
}

impl EditorSession {
    /// Create a new session from initial text. Starts in Normal mode.
    ///
    /// # Example
    ///
    /// ```
    /// use oom_edit_core::session::EditorSession;
    ///
    /// let session = EditorSession::from_text("# Hello\n\nWorld\n");
    /// assert_eq!(session.mode(), oom_edit_core::session::Mode::Normal);
    /// assert_eq!(session.line_count(), 4);
    /// ```
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
            highlighter: Highlighter::new(text),
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
            highlighter: Highlighter::new(&text),
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
        // Rebuild the highlighter with saved text
        self.highlighter = Highlighter::new(&text);
        // Invalidate view layout cache on save
        if let Some(ref mut vs) = self.view_state {
            vs.invalidate();
        }
        Ok(())
    }

    /// Handle a key input. Returns zero or more effects.
    ///
    /// # Example
    ///
    /// ```
    /// use oom_edit_core::session::{EditorSession, KeyInput, KeyCode, KeyCodeKind, Modifiers};
    ///
    /// let mut session = EditorSession::from_text("hello");
    /// let key = KeyInput {
    ///     code: KeyCode { kind: KeyCodeKind::Char('i') },
    ///     mods: Modifiers::default(),
    /// };
    /// let effects = session.handle_key(key);
    /// assert!(effects.iter().any(|e| matches!(e, oom_edit_core::session::Effect::ModeChanged(_))));
    /// assert_eq!(session.mode(), oom_edit_core::session::Mode::Insert);
    /// ```
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
                VimEffect::Edited { edits } => {
                    effects.push(Effect::Edited);
                    // Apply edits to the highlighter for incremental re-parse
                    self.highlighter.apply_edit(&edits);
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

    /// Return the syntax highlighter.
    pub fn highlighter(&self) -> &Highlighter {
        &self.highlighter
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
    ///
    /// The `width` parameter is used to rebuild the layout when the terminal
    /// width changes. Pass the current terminal width on each call.
    pub fn view_layout_mut(&mut self, width: u16) -> Option<&ViewLayout> {
        if self.mode == Mode::View {
            if let Some(ref mut vs) = self.view_state {
                let text = self.vim.text();
                Some(vs.get_layout(&text, &self.highlighter, width))
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

    /// Insert text at the current cursor position as a single paste operation.
    ///
    /// This is used for bracketed paste (FR-5.5): the text is inserted as one
    /// undo step with no per-character processing. The text is always inserted
    /// in Insert mode — if not in Insert mode, no action is taken.
    ///
    /// Returns `Effect::Edited` if text was inserted, `Effect::Message` if
    /// ignored (not in Insert mode).
    pub fn insert_paste(&mut self, text: &str) -> Vec<Effect> {
        // Only paste in Insert mode (FR-5.5)
        if self.mode != Mode::Insert {
            return vec![Effect::Message {
                text: "paste only works in insert mode".to_string(),
                severity: Severity::Info,
            }];
        }

        let edits = self.vim.insert_text(text);
        // Apply edits to the highlighter
        self.highlighter.apply_edit(&edits);
        // Invalidate view layout cache on edit
        if let Some(ref mut vs) = self.view_state {
            vs.invalidate();
        }
        vec![Effect::Edited]
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

    /// Render the source editor frame for the given viewport.
    ///
    /// Produces a [`crate::style::SourceFrame`] containing:
    /// - Highlighted styled lines (exactly `viewport.height` lines, padded)
    /// - Cursor position in viewport-relative `(row, col)` coordinates
    /// - Visual-mode selection ranges (clipped to viewport)
    /// - Search-match ranges (if any)
    ///
    /// The `Viewport.top_line` is owned by the host; the core does not
    /// modify it. The host keeps the cursor visible by adjusting
    /// `top_line` based on [`Self::cursor`] output.
    ///
    /// # Example
    ///
    /// ```
    /// use oom_edit_core::session::{EditorSession, Viewport};
    ///
    /// let mut session = EditorSession::from_text("# Hello\n\nWorld\n");
    /// let vp = Viewport { top_line: 0, height: 10, width: 80 };
    /// let frame = session.render_source(vp);
    /// assert_eq!(frame.lines.len(), 10); // padded to viewport height
    /// assert!(!frame.lines[0].text.is_empty()); // first line has content
    /// ```
    pub fn render_source(&mut self, vp: Viewport) -> crate::style::SourceFrame {
        let text = self.vim.text();
        let line_count = self.line_count();
        let (cursor_line, cursor_col) = self.cursor();

        // Compute which document lines are visible
        let first_visible = vp.top_line;
        let last_visible = first_visible.saturating_add(vp.height as usize);

        // Highlight the visible lines (pad to viewport height)
        let start_line = first_visible.min(line_count);
        let end_line = last_visible.min(line_count);
        let highlighted = self.highlighter.highlight_lines(start_line..end_line);

        // Build the lines vector, padding if needed
        let mut lines = Vec::with_capacity(vp.height as usize);

        // Lines before the visible region are blank (for padding above)
        let padding_before = if first_visible > line_count {
            vp.height as usize
        } else {
            0
        };

        for _ in 0..padding_before {
            lines.push(crate::style::StyledLine {
                text: String::new(),
                spans: Vec::new(),
            });
        }

        // Add highlighted lines
        for styled_line in &highlighted {
            lines.push(styled_line.clone());
        }

        // Pad with blank lines if we have fewer lines than viewport height
        while lines.len() < vp.height as usize {
            lines.push(crate::style::StyledLine {
                text: String::new(),
                spans: Vec::new(),
            });
        }

        // Truncate to exactly viewport.height (in case we over-highlighted)
        lines.truncate(vp.height as usize);

        // Compute viewport-relative cursor position
        let cursor_row = if cursor_line >= first_visible {
            (cursor_line - first_visible) as u16
        } else {
            0
        };
        let cursor_col_u16 = cursor_col as u16;

        // Collect visual-mode selections, clipped to viewport
        let selections = self.vim.selections();
        let viewport_selections: Vec<std::ops::Range<usize>> = selections
            .into_iter()
            .filter(|r| {
                // Include selections that overlap with the visible region
                let sel_start_line = text[..r.start].chars().filter(|&c| c == '\n').count();
                let sel_end_line = text[..r.end].chars().filter(|&c| c == '\n').count();
                sel_start_line < last_visible && sel_end_line >= first_visible
            })
            .collect();

        crate::style::SourceFrame {
            lines,
            first_line_number: first_visible + 1,
            cursor: (cursor_row.min(vp.height.saturating_sub(1)), cursor_col_u16),
            selections: viewport_selections,
        }
    }

    /// Render the View mode layout for the given width.
    ///
    /// Builds (or returns a reference to the cached layout for) a
    /// [`crate::style::ViewLayout`] from the current document text,
    /// highlighter, and block model.
    ///
    /// The layout is invalidated on edits and width changes. Callers should
    /// pass the current terminal width on each call.
    ///
    /// # Example
    ///
    /// ```
    /// use oom_edit_core::session::EditorSession;
    ///
    /// let mut session = EditorSession::from_text("# Hello\n\n* item\n");
    /// let layout = session.render_view(80);
    /// assert!(!layout.lines.is_empty());
    /// ```
    pub fn render_view(&mut self, width: u16) -> &crate::style::ViewLayout {
        // Ensure view_state exists
        if self.view_state.is_none() {
            let text = self.vim.text();
            let model = BlockModel::build(&text, None);
            let layout = ViewLayout::build(&model, width, &self.highlighter);
            self.view_state = Some(ViewState::with_layout(layout));
        }

        // Check if layout needs rebuilding
        if let Some(ref mut vs) = self.view_state {
            if vs.layout_cache.is_none() || vs.last_width != width {
                let text = self.vim.text();
                let model = BlockModel::build(&text, None);
                let layout = ViewLayout::build(&model, width, &self.highlighter);
                vs.layout_cache = Some(layout);
                vs.last_width = width;
            }
            vs.layout_cache.as_ref().unwrap()
        } else {
            // SAFETY: The block above always sets view_state to Some when it
            // was None, so this branch is unreachable. Kept for borrow-checker
            // compatibility; if the compiler ever allowed both paths, there
            // would be conflicting borrows on self.
            unreachable!("view_state was just set above")
        }
    }

    /// Return the view cursor line position.
    ///
    /// Used by the host to implement scrolling: the host keeps the cursor
    /// visible by adjusting `Viewport.top_line` based on this value.
    pub fn view_cursor_line(&self) -> usize {
        self.view_state
            .as_ref()
            .map(|vs| vs.cursor.line)
            .unwrap_or(0)
    }

    /// Remap the view cursor from edit coordinates (used on resize).
    ///
    /// When the terminal width changes, the view layout re-wraps and view
    /// line indices shift. This remaps the view cursor to the same content
    /// line using the core's `enter_view` pure function.
    pub fn remap_view_cursor_from_edit(&mut self, edit_line: usize, edit_col: usize) {
        if let Some(ref mut vs) = self.view_state {
            let layout = vs.layout_cache.as_ref().unwrap();
            let text = self.vim.text();
            vs.cursor = nav::enter_view(edit_line, edit_col, layout, &text);
        }
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
                let layout = vs.get_layout(&text, &self.highlighter, 80);
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
            let layout = ViewLayout::build(&model, 80, &self.highlighter);
            let cursor = nav::enter_view(edit_line, edit_col, &layout, &text);

            self.view_state = Some(ViewState {
                layout_cache: Some(layout),
                last_width: 0,
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
            let layout = vs.get_layout(&text, &self.highlighter, 80).clone();
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
                &text,
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

            // Apply layout dirty flag
            if result.layout_dirty {
                vs.invalidate();
            }

            // Apply fm_collapsed toggle
            if result.fm_collapsed_toggled {
                vs.fm_collapsed = !vs.fm_collapsed;
            }

            // Apply status message
            if let Some(msg) = result.message {
                effects.push(Effect::Message {
                    text: msg,
                    severity: Severity::Info,
                });
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
            let layout = vs.get_layout(&text, &self.highlighter, 80);
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
            "help" => vec![Effect::HelpRequested],
            "qa" => vec![Effect::QuitAllRequested { force: args.1 }],
            "tabnew" => {
                if let Some(path) = args.0 {
                    vec![Effect::TabNewRequested {
                        path: std::path::PathBuf::from(path),
                    }]
                } else {
                    vec![Effect::Message {
                        text: ":tabnew requires a file path".to_string(),
                        severity: Severity::Warning,
                    }]
                }
            }
            "tabclose" => vec![Effect::TabCloseRequested {
                index: None,
                force: args.1,
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
