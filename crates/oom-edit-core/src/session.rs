//! EditorSession — the core editing façade.
//!
//! This module owns the `EditorSession` type, the `Mode` machine, and the
//! session-facing type definitions (`KeyInput`, `KeyCode`, `Modifiers`,
//! `Effect`, `Viewport`). It composes `VimCore` (the hjkl wrapper) with the
//! document model and highlighting pipeline.

// ── Mode ───────────────────────────────────────────────────────────────────

/// The four user-visible editor modes.
///
/// Normal and Select are rendered Markdown surfaces owned by this session.
/// Insert uses the private Vim wrapper for raw-source editing, and Command
/// owns ex-command entry. Private hjkl modal states never escape
/// `vim.rs`.
///
/// See plan §6.1 / FR-1.1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Mode {
    /// Rendered Normal mode — navigation and editing transitions.
    Normal,
    /// Raw-source Insert mode — direct text entry.
    Insert,
    /// Rendered character-, line-, or block-wise Select mode.
    Select,
    /// Command mode — ex-command entry (e.g. `:w`).
    Command,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(c: char) -> KeyInput {
        KeyInput {
            code: KeyCode {
                kind: KeyCodeKind::Char(c),
            },
            mods: Modifiers::default(),
        }
    }

    fn esc() -> KeyInput {
        KeyInput {
            code: KeyCode {
                kind: KeyCodeKind::Esc,
            },
            mods: Modifiers::default(),
        }
    }

    fn special(kind: KeyCodeKind) -> KeyInput {
        KeyInput {
            code: KeyCode { kind },
            mods: Modifiers::default(),
        }
    }

    #[test]
    fn session_starts_in_rendered_normal() {
        let mut session = EditorSession::from_text("# Heading\n\nBody\n");
        assert_eq!(session.mode(), Mode::Normal);
        assert_eq!(session.cursor(), (0, 0));
        assert!(!session.render_layout(31).lines.is_empty());
        assert_eq!(session.rendered_cursor_line(), 0);
    }

    #[test]
    fn rendered_navigation_updates_canonical_source_cursor() {
        let mut session = EditorSession::from_text("# Heading\n\nBody\n");
        session.render_layout(40);
        session.handle_key(key('j'));
        assert_eq!(session.cursor(), (0, 0));
        session.handle_key(key('j'));
        assert_eq!(session.cursor().0, 2);
    }

    #[test]
    fn navigation_and_frames_reuse_materialization_layout_and_line_index_work() {
        let text = "plain paragraph content for cached work counters\n\n".repeat(500);
        let mut session = EditorSession::from_text(&text);
        session.render_layout(80);
        assert_eq!(session.rendered_state.layout_builds, 1);
        session.vim.reset_work_counters();

        for input in [
            key('j'),
            key('k'),
            key('2'),
            key('0'),
            key('j'),
            key('G'),
            key('g'),
            key('g'),
        ] {
            session.handle_key(input);
            session.render_layout(80);
        }
        assert_eq!(session.vim.work_counters(), (0, 0));
        assert_eq!(session.rendered_state.layout_builds, 1);

        session.handle_key(key('i'));
        assert_eq!(session.mode(), Mode::Insert);
        session.vim.reset_work_counters();
        let line_index_builds = crate::syntax::line_index_build_count();
        for _ in 0..20 {
            session.handle_key(special(KeyCodeKind::Down));
        }
        let top_line = session.cursor().0.saturating_sub(5);
        for _ in 0..3 {
            let _ = session.render_source(Viewport {
                top_line,
                height: 10,
                width: 80,
                wrap: true,
                left_col: 0,
                skip_rows: 0,
            });
        }
        assert_eq!(session.vim.work_counters(), (0, 0));
        assert_eq!(crate::syntax::line_index_build_count(), line_index_builds);

        session.handle_key(key('x'));
        let _ = session.render_source(Viewport {
            top_line,
            height: 10,
            width: 80,
            wrap: true,
            left_col: 0,
            skip_rows: 0,
        });
        assert_eq!(session.vim.work_counters(), (1, 1));
        assert_eq!(crate::syntax::line_index_build_count(), line_index_builds);

        session.handle_key(esc());
        session.vim.reset_work_counters();
        session.render_layout(80);
        assert_eq!(session.rendered_state.layout_builds, 2);
        assert_eq!(session.vim.work_counters(), (1, 0));
    }

    #[test]
    fn first_actual_width_preserves_source_anchor() {
        let text = "# Heading\n\nA paragraph with enough words to wrap at a narrow width.\n";
        let mut session = EditorSession::from_text(text);
        session.vim.jump_to(2, 12);
        let before = session.cursor();
        session.render_layout(23);
        assert_eq!(session.cursor(), before);
        session.render_layout(61);
        assert_eq!(session.cursor(), before);
    }

    #[test]
    fn insert_select_command_roundtrip_preserves_source_position() {
        let mut session = EditorSession::from_text("# Heading\n\nBody\n");
        session.render_layout(40);
        session.handle_key(key('j'));
        session.handle_key(key('j'));
        let body = session.cursor();

        session.handle_key(key('V'));
        assert_eq!(session.mode(), Mode::Select);
        session.handle_key(esc());
        assert_eq!(session.mode(), Mode::Normal);
        assert_eq!(session.cursor(), body);

        session.handle_key(key(':'));
        assert_eq!(session.mode(), Mode::Command);
        session.handle_key(esc());
        assert_eq!(session.mode(), Mode::Normal);
        assert_eq!(session.cursor(), body);

        session.handle_key(key('i'));
        assert_eq!(session.mode(), Mode::Insert);
        session.handle_key(esc());
        assert_eq!(session.mode(), Mode::Normal);
        assert_eq!(session.cursor(), body);
    }

    #[test]
    fn select_yank_is_non_destructive_and_line_aligned() {
        let text = "# Heading\n\nFirst\nSecond\n";
        let mut session = EditorSession::from_text(text);
        session.render_layout(40);
        session.handle_key(key('V'));
        session.handle_key(key('j'));
        let selection = session.rendered_selection().unwrap();
        assert_eq!(selection.source_ranges, vec![0..10]);
        assert_eq!(&text[selection.source_ranges[0].clone()], "# Heading\n");
        session.handle_key(key('y'));
        assert_eq!(session.mode(), Mode::Normal);
        assert_eq!(session.document(), text);
    }
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
    /// An input that is intentionally ignored.
    Noop,
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
        /// Whether a path argument becomes the buffer's new path.
        /// False means copy-out (`:w {path}`); true means `:saveas`.
        retarget: bool,
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
    /// A host-owned boolean option was changed by an ex command.
    SetOption {
        /// Stable option key understood by the host.
        key: String,
        /// New option value.
        value: bool,
    },
    /// Help was requested through the core command line (`:help`).
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
    /// Whether long source lines wrap into visual rows.
    pub wrap: bool,
    /// Source-window character offset when wrapping is disabled. When this is
    /// nonzero, the left edge indicator replaces the first window character.
    pub left_col: usize,
    /// Visual rows skipped within `top_line` when wrapping is enabled.
    pub skip_rows: usize,
}

// ── VimCore re-export (internal) ──────────────────────────────────────────

use crate::vim::{RangeOperator, Register, UndoMark, VimCore, VimEffect};

// ── Document (internal) ───────────────────────────────────────────────────

use crate::document::Document;
use crate::error::{OpenError, SaveError};
use crate::rendered::nav;
use crate::rendered::BlockModel;
use crate::style::{
    RenderedCursor, RenderedLayout, RenderedPoint, RenderedSearch, RenderedSelection,
    SearchDirection, SelectionShape,
};
use crate::syntax::Highlighter;
use std::ops::Range;

/// Vim action applied after mapping a rendered cursor to source editing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RenderedExitAction {
    Insert,
    Append,
    InsertLineStart,
    AppendLineEnd,
    OpenBelow,
    OpenAbove,
}

impl RenderedExitAction {
    fn key(self) -> KeyInput {
        let action = match self {
            Self::Insert => 'i',
            Self::Append => 'a',
            Self::InsertLineStart => 'I',
            Self::AppendLineEnd => 'A',
            Self::OpenBelow => 'o',
            Self::OpenAbove => 'O',
        };

        KeyInput {
            code: KeyCode {
                kind: KeyCodeKind::Char(action),
            },
            mods: Modifiers::default(),
        }
    }
}

// ── RenderedState ──────────────────────────────────────────────────────────

/// Persistent state shared by rendered Normal, Select, and Command.
///
/// Holds a cached layout, cursor position, search state, and front-matter
/// panel collapse state. The layout is invalidated on edits.
struct RenderedState {
    /// Cached rendered layout (None = needs rebuild).
    layout_cache: Option<RenderedLayout>,
    /// The width used when the layout was last built.
    last_width: u16,
    /// Current cursor position in rendered coordinates.
    cursor: RenderedCursor,
    /// Select anchor display point. Present exactly while public mode is Select.
    select_anchor: Option<RenderedPoint>,
    /// Active Select shape.
    selection_shape: SelectionShape,
    /// Canonical source position for the Select anchor.
    ///
    /// The rendered row is derived from this position whenever wrapping
    /// changes, so a resize cannot silently move one endpoint of a selection.
    select_anchor_source: Option<(usize, usize)>,
    /// Exact source atom under the Select anchor.
    select_anchor_atom: Option<Range<usize>>,
    /// Whether the anchor is source-backed rather than synthetic.
    select_anchor_atom_exact: bool,
    /// Exact source atom under the active Select endpoint.
    select_active_atom: Option<Range<usize>>,
    /// Whether the active endpoint is source-backed rather than synthetic.
    select_active_atom_exact: bool,
    /// Source-line identity and wrapped-row ordinal for the Select anchor.
    select_anchor_line: Option<(Range<usize>, usize)>,
    /// Source-line identity and wrapped-row ordinal for the active endpoint.
    select_active_line: Option<(Range<usize>, usize)>,
    /// Stable character-wise source projection. This is deliberately not
    /// used for line- or block-wise selection, whose geometry is recomputed.
    select_character_ranges: Vec<Range<usize>>,
    /// Explicit register prefix pending in Select.
    pending_register: Option<Register>,
    /// Whether Select has received the opening `"` register prefix.
    register_prefix_pending: bool,
    /// Active search state (if in search mode).
    search: Option<RenderedSearch>,
    /// Whether typed characters are currently extending the search pattern.
    search_input_active: bool,
    /// Cursor position captured when the active search prompt was opened.
    search_origin: Option<RenderedCursor>,
    /// Whether the front-matter panel is collapsed.
    fm_collapsed: bool,
    /// Accumulated numeric count for navigation commands.
    count: usize,
    /// Whether the first `g` of rendered `gg` is pending.
    pending_g: bool,
    /// First bracket of a rendered `[[` or `]]` heading motion.
    pending_heading_bracket: Option<char>,
    /// Actual rendered layout builds, exposed only to regression tests.
    #[cfg(test)]
    layout_builds: usize,
}

impl RenderedState {
    fn new() -> Self {
        Self {
            layout_cache: None,
            last_width: 0,
            cursor: RenderedCursor::new(0),
            select_anchor: None,
            selection_shape: SelectionShape::Character,
            select_anchor_source: None,
            select_anchor_atom: None,
            select_anchor_atom_exact: false,
            select_active_atom: None,
            select_active_atom_exact: false,
            select_anchor_line: None,
            select_active_line: None,
            select_character_ranges: Vec::new(),
            pending_register: None,
            register_prefix_pending: false,
            search: None,
            search_input_active: false,
            search_origin: None,
            fm_collapsed: false,
            count: 0,
            pending_g: false,
            pending_heading_bracket: None,
            #[cfg(test)]
            layout_builds: 0,
        }
    }

    fn needs_layout(&self, width: u16) -> bool {
        self.layout_cache.is_none() || self.last_width != width
    }

    /// Invalidate the layout cache.
    fn invalidate(&mut self) {
        self.layout_cache = None;
    }

    /// Clear search state.
    fn clear_search(&mut self) {
        self.search = None;
        self.search_input_active = false;
        self.search_origin = None;
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
    /// The current public mode.
    mode: Mode,
    /// Dirty generation at last save.
    save_point: UndoMark,
    /// Buffer for ex-command text in Command mode.
    command_buffer: String,
    /// The document model — text, path, front matter, I/O state.
    document: Document,
    /// Persistent rendered navigation and Select state.
    rendered_state: RenderedState,
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
            rendered_state: RenderedState::new(),
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
            rendered_state: RenderedState::new(),
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
        // The vim engine owns the authoritative dirty generation. Capture it
        // once after the save succeeds, then keep the session and standalone
        // document I/O state synchronized to that same mark.
        let mark = self.vim.save_point();
        self.save_point = mark;
        self.document.set_save_point(mark);
        // Rebuild the highlighter with saved text
        self.highlighter = Highlighter::new(&text);
        self.rendered_state.invalidate();
        Ok(())
    }

    /// Atomically save a copy without retargeting the buffer or clearing its
    /// dirty state (`:w {path}`).
    pub fn save_copy(&self, path: &std::path::Path) -> Result<(), SaveError> {
        self.document.save_copy_with_text(&self.vim.text(), path)
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
        // Unsupported terminal keys are consumed without reaching any mode
        // handler or the Vim engine, where fallback mappings could otherwise
        // cause edits, cursor movement, mode changes, or command effects.
        if key.code.kind == KeyCodeKind::Noop {
            return Vec::new();
        }

        match self.mode {
            Mode::Normal => self.handle_rendered_normal_key(key),
            Mode::Select => self.handle_rendered_select_key(key),
            Mode::Insert => self.handle_insert_key(key),
            Mode::Command => self.handle_command_mode_key(key),
        }
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

    /// Return the unprefixed command-line text, or `None` outside Command mode.
    pub fn command_line(&self) -> Option<String> {
        (self.mode == Mode::Command).then(|| self.command_buffer.clone())
    }

    /// Return the active rendered-search prompt, including `/` or `?` prefix.
    ///
    /// Submitted or cancelled prompts return `None` even though the last
    /// search remains available for `n`/`N`.
    pub fn rendered_search_prompt(&self) -> Option<String> {
        if !self.rendered_state.search_input_active {
            return None;
        }
        let search = self.rendered_state.search.as_ref()?;
        let prefix = match search.last_direction {
            SearchDirection::Forward => '/',
            SearchDirection::Backward => '?',
        };
        Some(format!("{prefix}{}", search.pattern))
    }

    /// Return the rendered cursor position.
    pub fn rendered_cursor(&self) -> RenderedPoint {
        self.rendered_state.cursor.point()
    }

    /// Return the cached rendered layout, if a host width has been supplied.
    pub fn rendered_layout(&self) -> Option<&RenderedLayout> {
        self.rendered_state.layout_cache.as_ref()
    }

    /// Return the rendered layout, building it at the supplied text width.
    pub fn rendered_layout_mut(&mut self, width: u16) -> &RenderedLayout {
        self.render_layout(width)
    }

    /// Return the retained rendered search state.
    pub fn rendered_search(&self) -> Option<&RenderedSearch> {
        self.rendered_state.search.as_ref()
    }

    /// Return renderer-neutral Select metadata, or `None` outside Select.
    pub fn rendered_selection(&self) -> Option<RenderedSelection> {
        let anchor = self.rendered_state.select_anchor?;
        let anchor_source = self.rendered_state.select_anchor_source?;
        let layout = self.rendered_state.layout_cache.as_ref()?;
        let mut selection = nav::project_selection_from_source_positions(
            anchor,
            self.rendered_state.cursor.point(),
            self.rendered_state.selection_shape,
            anchor_source,
            self.vim.cursor(),
            layout,
            &self.vim.text(),
        );
        if selection.shape == SelectionShape::Character {
            selection.source_ranges = self.rendered_state.select_character_ranges.clone();
        }
        Some(selection)
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
        self.rendered_state.invalidate();
        vec![Effect::Edited]
    }

    /// Return the number of lines in the document.
    pub fn line_count(&self) -> usize {
        self.vim.line_count()
    }

    /// Return a specific line (0-based), or `None` if out of range.
    pub fn line(&self, idx: usize) -> Option<String> {
        self.vim.line(idx)
    }

    /// Return the cursor's visual row within a document line and that line's
    /// total wrapped height at `width`.
    ///
    /// When wrapping is disabled, the result is always `(0, 1)`.
    /// For the active cursor at an exact full-width end-of-line in Insert
    /// mode, the result includes the synthetic blank continuation row used to
    /// display the insertion point.
    pub fn visual_row_info(
        &self,
        doc_line: usize,
        doc_col: usize,
        width: u16,
        wrap: bool,
    ) -> (usize, usize) {
        if !wrap {
            return (0, 1);
        }

        let line = self.line(doc_line).unwrap_or_default();
        let styled = crate::style::StyledLine {
            text: line.clone(),
            spans: Vec::new(),
        };
        let mut wrapped = crate::rendered::wrap_source_line(&styled, width);
        if (doc_line, doc_col) == self.cursor()
            && self.mode() == Mode::Insert
            && Self::cursor_needs_blank_continuation(&line, &wrapped, doc_col, width)
        {
            wrapped.push(crate::style::StyledLine {
                text: String::new(),
                spans: Vec::new(),
            });
        }
        let (row, _) = Self::wrapped_cursor_position(&line, &wrapped, doc_col);
        (row, wrapped.len().max(1))
    }

    /// Render the source editor frame for the given viewport.
    ///
    /// Produces a [`crate::style::SourceFrame`] containing:
    /// - Highlighted styled lines (exactly `viewport.height` lines, padded)
    /// - Cursor position in viewport-relative `(row, col)` coordinates
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
    /// let vp = Viewport {
    ///     top_line: 0,
    ///     height: 10,
    ///     width: 80,
    ///     wrap: true,
    ///     left_col: 0,
    ///     skip_rows: 0,
    /// };
    /// let frame = session.render_source(vp);
    /// assert_eq!(frame.lines.len(), 10); // padded to viewport height
    /// assert!(!frame.lines[0].text.is_empty()); // first line has content
    /// ```
    pub fn render_source(&mut self, vp: Viewport) -> crate::style::SourceFrame {
        self.vim.set_viewport(vp.top_line, vp.height);
        let line_count = self.line_count();
        let (cursor_line, cursor_col) = self.cursor();

        // Compute which document lines are visible
        let first_visible = vp.top_line;
        let last_visible = first_visible.saturating_add(vp.height as usize);

        // Highlight the visible lines (pad to viewport height)
        let start_line = first_visible.min(line_count);
        let end_line = last_visible.min(line_count);
        let mut highlighted = self.highlighter.highlight_lines(start_line..end_line);
        for (offset, styled_line) in highlighted.iter_mut().enumerate() {
            for search_match in self.vim.search_matches_for_line(start_line + offset) {
                Self::overlay_search_match(styled_line, search_match);
            }
        }

        // Build visual rows and their gutter metadata.
        let mut lines = Vec::with_capacity(vp.height as usize);
        let mut line_numbers = Vec::with_capacity(vp.height as usize);
        let mut screen_cursor = (0usize, 0usize);

        if vp.wrap {
            for (offset, styled_line) in highlighted.iter().enumerate() {
                let doc_line = start_line + offset;
                let mut wrapped = crate::rendered::wrap_source_line(styled_line, vp.width);
                if doc_line == cursor_line
                    && self.mode() == Mode::Insert
                    && Self::cursor_needs_blank_continuation(
                        &styled_line.text,
                        &wrapped,
                        cursor_col,
                        vp.width,
                    )
                {
                    wrapped.push(crate::style::StyledLine {
                        text: String::new(),
                        spans: Vec::new(),
                    });
                }
                let skip = if offset == 0 {
                    vp.skip_rows.min(wrapped.len().saturating_sub(1))
                } else {
                    0
                };
                let first_screen_row = lines.len();

                if doc_line == cursor_line {
                    let (wrapped_row, wrapped_col) =
                        Self::wrapped_cursor_position(&styled_line.text, &wrapped, cursor_col);
                    screen_cursor = (
                        first_screen_row + wrapped_row.saturating_sub(skip),
                        wrapped_col,
                    );
                }

                for (wrapped_row, row) in wrapped.into_iter().enumerate().skip(skip) {
                    if lines.len() == vp.height as usize {
                        break;
                    }
                    line_numbers.push(if wrapped_row == 0 {
                        Some(doc_line + 1)
                    } else {
                        None
                    });
                    lines.push(row);
                }

                if lines.len() == vp.height as usize {
                    break;
                }
            }
        } else {
            for (offset, styled_line) in highlighted.iter().enumerate() {
                if lines.len() == vp.height as usize {
                    break;
                }
                let doc_line = start_line + offset;
                if doc_line == cursor_line {
                    screen_cursor = (
                        lines.len(),
                        cursor_col
                            .saturating_sub(vp.left_col)
                            .min(vp.width.saturating_sub(1) as usize),
                    );
                }
                lines.push(Self::horizontal_window(styled_line, vp.left_col, vp.width));
                line_numbers.push(Some(doc_line + 1));
            }
        }

        // Pad with blank lines if we have fewer lines than viewport height
        while lines.len() < vp.height as usize {
            lines.push(crate::style::StyledLine {
                text: String::new(),
                spans: Vec::new(),
            });
            line_numbers.push(None);
        }

        // Truncate to exactly viewport.height (in case we over-highlighted)
        lines.truncate(vp.height as usize);
        line_numbers.truncate(vp.height as usize);

        crate::style::SourceFrame {
            lines,
            line_numbers,
            first_line_number: first_visible + 1,
            cursor: (
                screen_cursor.0.min(vp.height.saturating_sub(1) as usize) as u16,
                screen_cursor.1.min(vp.width.saturating_sub(1) as usize) as u16,
            ),
        }
    }

    fn wrapped_cursor_position(
        source: &str,
        wrapped: &[crate::style::StyledLine],
        doc_col: usize,
    ) -> (usize, usize) {
        let chars: Vec<char> = source.chars().collect();
        let doc_col = doc_col.min(chars.len());
        let mut source_pos = 0usize;

        for (row, styled) in wrapped.iter().enumerate() {
            let row_start = source_pos;
            let row_len = styled.text.chars().count();
            let row_end = (row_start + row_len).min(chars.len());

            if doc_col < row_end {
                return (row, doc_col.saturating_sub(row_start));
            }
            if doc_col == row_end {
                if row + 1 < wrapped.len() {
                    return (row + 1, 0);
                }
                return (row, row_len);
            }

            source_pos = row_end;
        }

        let last = wrapped.len().saturating_sub(1);
        (
            last,
            wrapped
                .get(last)
                .map_or(0, |line| line.text.chars().count()),
        )
    }

    fn cursor_needs_blank_continuation(
        source: &str,
        wrapped: &[crate::style::StyledLine],
        doc_col: usize,
        width: u16,
    ) -> bool {
        width > 0
            && doc_col == source.chars().count()
            && wrapped.last().is_some_and(|line| {
                unicode_width::UnicodeWidthStr::width(line.text.as_str()) >= width as usize
            })
    }

    fn horizontal_window(
        styled_line: &crate::style::StyledLine,
        left_col: usize,
        width: u16,
    ) -> crate::style::StyledLine {
        let chars: Vec<char> = styled_line.text.chars().collect();
        let width = width as usize;
        if width == 0 || left_col >= chars.len() {
            return crate::style::StyledLine {
                text: String::new(),
                spans: Vec::new(),
            };
        }

        let end_col = left_col.saturating_add(width).min(chars.len());
        let text: String = chars[left_col..end_col].iter().collect();
        let spans = styled_line
            .spans
            .iter()
            .filter_map(|span| {
                let start = span.start_col.max(left_col);
                let end = span.end_col.min(end_col);
                (start < end).then_some(crate::style::Span {
                    start_col: start - left_col,
                    end_col: end - left_col,
                    style: span.style,
                })
            })
            .collect();
        let mut window = crate::style::StyledLine { text, spans };

        if left_col > 0 {
            Self::replace_window_character(&mut window, 0, '«', crate::style::SemanticStyle::Muted);
        }
        if chars.len() > left_col.saturating_add(width) {
            Self::replace_window_character(
                &mut window,
                width.saturating_sub(1),
                '»',
                crate::style::SemanticStyle::Muted,
            );
        }

        window
    }

    fn replace_window_character(
        line: &mut crate::style::StyledLine,
        col: usize,
        replacement: char,
        style: crate::style::SemanticStyle,
    ) {
        let mut chars: Vec<char> = line.text.chars().collect();
        if col >= chars.len() {
            return;
        }
        chars[col] = replacement;
        line.text = chars.into_iter().collect();

        let mut spans = Vec::with_capacity(line.spans.len() + 1);
        for span in &line.spans {
            if span.end_col <= col || span.start_col > col {
                spans.push(span.clone());
                continue;
            }
            if span.start_col < col {
                spans.push(crate::style::Span {
                    start_col: span.start_col,
                    end_col: col,
                    style: span.style,
                });
            }
            if span.end_col > col + 1 {
                spans.push(crate::style::Span {
                    start_col: col + 1,
                    end_col: span.end_col,
                    style: span.style,
                });
            }
        }
        spans.push(crate::style::Span {
            start_col: col,
            end_col: col + 1,
            style,
        });
        spans.sort_by_key(|span| span.start_col);
        line.spans = spans;
    }

    /// Render the Normal/Select Markdown layout at the host's text width.
    ///
    /// Builds (or returns a reference to the cached layout for) a
    /// [`crate::style::RenderedLayout`] from the current document text,
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
    /// let layout = session.render_layout(80);
    /// assert!(!layout.lines.is_empty());
    /// ```
    pub fn render_layout(&mut self, width: u16) -> &crate::style::RenderedLayout {
        if self.rendered_state.needs_layout(width) {
            let source_anchor = self.vim.cursor();
            let select_anchor_source = self.rendered_state.select_anchor_source;
            let select_anchor_atom = self.rendered_state.select_anchor_atom.clone();
            let select_active_atom = self.rendered_state.select_active_atom.clone();
            let select_anchor_line = self.rendered_state.select_anchor_line.clone();
            let select_active_line = self.rendered_state.select_active_line.clone();
            let character_selection =
                self.rendered_state.selection_shape == SelectionShape::Character;
            let block_selection = self.rendered_state.selection_shape == SelectionShape::Block;
            let active_atom_remap = character_selection
                || (block_selection && self.rendered_state.select_active_atom_exact);
            let anchor_atom_remap = character_selection
                || (block_selection && self.rendered_state.select_anchor_atom_exact);
            let text = self.vim.text();
            let fm_span = crate::frontmatter::front_matter_span(&text);
            let model = BlockModel::build(&text, fm_span);
            let layout = RenderedLayout::build_with_front_matter_state(
                &model,
                width,
                &self.highlighter,
                self.rendered_state.fm_collapsed,
            );
            let cursor = active_atom_remap
                .then(|| {
                    select_active_atom
                        .as_ref()
                        .and_then(|source| nav::point_for_source_range(source, &layout))
                })
                .flatten()
                .or_else(|| {
                    (block_selection && !active_atom_remap)
                        .then(|| {
                            select_active_line.as_ref().and_then(|(source, ordinal)| {
                                nav::point_for_line_identity(
                                    source,
                                    *ordinal,
                                    self.rendered_state.cursor.column,
                                    &layout,
                                )
                            })
                        })
                        .flatten()
                })
                .map(RenderedCursor::at)
                .unwrap_or_else(|| {
                    nav::enter_rendered(source_anchor.0, source_anchor.1, &layout, &text)
                });
            let select_anchor = anchor_atom_remap
                .then(|| {
                    select_anchor_atom
                        .as_ref()
                        .and_then(|source| nav::point_for_source_range(source, &layout))
                })
                .flatten()
                .or_else(|| {
                    (block_selection && !anchor_atom_remap)
                        .then(|| {
                            select_anchor_line.as_ref().and_then(|(source, ordinal)| {
                                nav::point_for_line_identity(
                                    source,
                                    *ordinal,
                                    self.rendered_state
                                        .select_anchor
                                        .map_or(0, |point| point.column),
                                    &layout,
                                )
                            })
                        })
                        .flatten()
                })
                .or_else(|| {
                    select_anchor_source
                        .map(|(line, col)| nav::enter_rendered(line, col, &layout, &text).point())
                });
            self.rendered_state.layout_cache = Some(layout);
            self.rendered_state.last_width = width;
            self.rendered_state.cursor = cursor;
            self.rendered_state.select_anchor = select_anchor;
            #[cfg(test)]
            {
                self.rendered_state.layout_builds += 1;
            }
        }
        self.rendered_state
            .layout_cache
            .as_ref()
            .expect("rendered layout must be cached after building")
    }

    /// Return the rendered cursor row.
    ///
    /// Used by the host to implement scrolling: the host keeps the cursor
    /// visible by adjusting `Viewport.top_line` based on this value.
    pub fn rendered_cursor_line(&self) -> usize {
        self.rendered_state.cursor.line
    }

    /// Remap the rendered cursor from canonical source coordinates.
    ///
    /// When the terminal width changes, the layout re-wraps and rendered row
    /// indices shift. This remaps the rendered cursor to the same content
    /// line using the core's `enter_rendered` pure function.
    pub fn remap_rendered_cursor(&mut self, edit_line: usize, edit_col: usize) {
        let Some(layout) = self.rendered_state.layout_cache.as_ref() else {
            return;
        };
        let text = self.vim.text();
        self.rendered_state.cursor = nav::enter_rendered(edit_line, edit_col, layout, &text);
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

    fn handle_insert_key(&mut self, key: KeyInput) -> Vec<Effect> {
        let vim_key = self.key_input_to_vim(key);
        let vim_effects = self.vim.handle_key(vim_key);
        self.translate_vim_effects(vim_effects)
    }

    fn handle_rendered_normal_key(&mut self, key: KeyInput) -> Vec<Effect> {
        if self.rendered_state.search_input_active {
            return self.handle_rendered_search_input(key);
        }
        if self.rendered_state.register_prefix_pending {
            self.rendered_state.register_prefix_pending = false;
            if let KeyCodeKind::Char(selector) = key.code.kind {
                if key.mods == Modifiers::default() {
                    self.rendered_state.pending_register = Self::rendered_register(selector);
                }
            }
            return Vec::new();
        }
        if self.first_rendered_g_is_pending(key) {
            return Vec::new();
        }
        if self.first_heading_bracket_is_pending(key) {
            return Vec::new();
        }
        if key.mods.ctrl && matches!(key.code.kind, KeyCodeKind::Char('v' | 'V')) {
            return self.enter_select(SelectionShape::Block);
        }
        if key.mods == Modifiers::default() {
            match key.code.kind {
                KeyCodeKind::Char('v') => {
                    return self.enter_select(SelectionShape::Character);
                }
                KeyCodeKind::Char('V') => {
                    return self.enter_select(SelectionShape::Line);
                }
                KeyCodeKind::Char('"') => {
                    self.rendered_state.register_prefix_pending = true;
                    return Vec::new();
                }
                KeyCodeKind::Char(':') => {
                    self.command_buffer.clear();
                    self.mode = Mode::Command;
                    return vec![Effect::ModeChanged(Mode::Command)];
                }
                KeyCodeKind::Char('i') => {
                    return self.enter_insert_from_rendered(RenderedExitAction::Insert)
                }
                KeyCodeKind::Char('a') => {
                    return self.enter_insert_from_rendered(RenderedExitAction::Append)
                }
                KeyCodeKind::Char('I') => {
                    return self.enter_insert_from_rendered(RenderedExitAction::InsertLineStart)
                }
                KeyCodeKind::Char('A') => {
                    return self.enter_insert_from_rendered(RenderedExitAction::AppendLineEnd)
                }
                KeyCodeKind::Char('o') => {
                    return self.enter_insert_from_rendered(RenderedExitAction::OpenBelow)
                }
                KeyCodeKind::Char('O') => {
                    return self.enter_insert_from_rendered(RenderedExitAction::OpenAbove)
                }
                KeyCodeKind::Char('p') | KeyCodeKind::Char('P') | KeyCodeKind::Char('u') => {
                    let mut vim_effects = Vec::new();
                    if matches!(key.code.kind, KeyCodeKind::Char('p' | 'P')) {
                        if let Some(selector) = self
                            .rendered_state
                            .pending_register
                            .take()
                            .and_then(Register::selector)
                        {
                            vim_effects.extend(self.vim.handle_key(self.key_input_to_vim(
                                KeyInput {
                                    code: KeyCode {
                                        kind: KeyCodeKind::Char('"'),
                                    },
                                    mods: Modifiers::default(),
                                },
                            )));
                            vim_effects.extend(self.vim.handle_key(self.key_input_to_vim(
                                KeyInput {
                                    code: KeyCode {
                                        kind: KeyCodeKind::Char(selector),
                                    },
                                    mods: Modifiers::default(),
                                },
                            )));
                        }
                    }
                    vim_effects.extend(self.vim.handle_key(self.key_input_to_vim(key)));
                    let mut effects = self.translate_vim_effects(vim_effects);
                    self.mode = Mode::Normal;
                    effects.retain(|effect| !matches!(effect, Effect::ModeChanged(_)));
                    return effects;
                }
                _ => {}
            }
        }
        if key.mods.ctrl && matches!(key.code.kind, KeyCodeKind::Char('r')) {
            let vim_effects = self.vim.handle_key(self.key_input_to_vim(key));
            let mut effects = self.translate_vim_effects(vim_effects);
            self.mode = Mode::Normal;
            effects.retain(|effect| !matches!(effect, Effect::ModeChanged(_)));
            return effects;
        }
        self.handle_rendered_navigation_key(key)
    }

    fn handle_rendered_select_key(&mut self, key: KeyInput) -> Vec<Effect> {
        if self.rendered_state.search_input_active {
            return self.handle_rendered_search_input(key);
        }
        if self.rendered_state.register_prefix_pending {
            self.rendered_state.register_prefix_pending = false;
            if let KeyCodeKind::Char(selector) = key.code.kind {
                if key.mods == Modifiers::default() {
                    self.rendered_state.pending_register = Self::rendered_register(selector);
                }
            }
            return Vec::new();
        }
        if self.first_rendered_g_is_pending(key) {
            return Vec::new();
        }
        if self.first_heading_bracket_is_pending(key) {
            return Vec::new();
        }
        if key.mods.ctrl && matches!(key.code.kind, KeyCodeKind::Char('c')) {
            return self.finish_select(Mode::Normal, Vec::new());
        }
        if key.mods.ctrl && matches!(key.code.kind, KeyCodeKind::Char('v' | 'V')) {
            return self.switch_or_cancel_selection_shape(SelectionShape::Block);
        }
        if key.mods == Modifiers::default() {
            match key.code.kind {
                KeyCodeKind::Esc => return self.finish_select(Mode::Normal, Vec::new()),
                KeyCodeKind::Char('v') => {
                    return self.switch_or_cancel_selection_shape(SelectionShape::Character)
                }
                KeyCodeKind::Char('V') => {
                    return self.switch_or_cancel_selection_shape(SelectionShape::Line)
                }
                KeyCodeKind::Char('o') => {
                    let old_anchor = self
                        .rendered_state
                        .select_anchor
                        .unwrap_or(self.rendered_state.cursor.point());
                    let old_anchor_source = self
                        .rendered_state
                        .select_anchor_source
                        .unwrap_or(self.vim.cursor());
                    let old_active_source = self.vim.cursor();
                    let old_anchor_atom = self.rendered_state.select_anchor_atom.clone();
                    let old_active_atom = self.rendered_state.select_active_atom.clone();
                    let old_anchor_atom_exact = self.rendered_state.select_anchor_atom_exact;
                    let old_active_atom_exact = self.rendered_state.select_active_atom_exact;
                    let old_anchor_line = self.rendered_state.select_anchor_line.clone();
                    let old_active_line = self.rendered_state.select_active_line.clone();
                    self.rendered_state.select_anchor = Some(self.rendered_state.cursor.point());
                    self.rendered_state.select_anchor_source = Some(old_active_source);
                    self.rendered_state.select_anchor_atom = old_active_atom;
                    self.rendered_state.select_active_atom = old_anchor_atom;
                    self.rendered_state.select_anchor_atom_exact = old_active_atom_exact;
                    self.rendered_state.select_active_atom_exact = old_anchor_atom_exact;
                    self.rendered_state.select_anchor_line = old_active_line;
                    self.rendered_state.select_active_line = old_anchor_line;
                    self.rendered_state.cursor = RenderedCursor::at(old_anchor);
                    self.vim.jump_to(old_anchor_source.0, old_anchor_source.1);
                    return vec![Effect::CursorMoved];
                }
                KeyCodeKind::Char('"') => {
                    self.rendered_state.register_prefix_pending = true;
                    return Vec::new();
                }
                KeyCodeKind::Char('y') => return self.apply_select_operator(RangeOperator::Yank),
                KeyCodeKind::Char('d') | KeyCodeKind::Char('x') => {
                    return self.apply_select_operator(RangeOperator::Delete)
                }
                KeyCodeKind::Char('c') => return self.apply_select_operator(RangeOperator::Change),
                KeyCodeKind::Char('>') => return self.apply_select_operator(RangeOperator::Indent),
                KeyCodeKind::Char('<') => {
                    return self.apply_select_operator(RangeOperator::Outdent)
                }
                _ => {
                    self.rendered_state.register_prefix_pending = false;
                    self.rendered_state.pending_register = None;
                }
            }
        }
        self.handle_rendered_navigation_key(key)
    }

    fn rendered_register(selector: char) -> Option<Register> {
        match selector {
            '+' | '*' => Some(Register::System),
            '_' => Some(Register::BlackHole),
            name @ ('a'..='z' | 'A'..='Z' | '0'..='9' | '-') => Some(Register::Named(name)),
            _ => None,
        }
    }

    /// Hold the first `g` so only the complete rendered `gg` motion jumps.
    fn first_rendered_g_is_pending(&mut self, key: KeyInput) -> bool {
        let is_plain_g =
            key.mods == Modifiers::default() && matches!(key.code.kind, KeyCodeKind::Char('g'));
        if !is_plain_g {
            self.rendered_state.pending_g = false;
            return false;
        }
        if self.rendered_state.pending_g {
            self.rendered_state.pending_g = false;
            false
        } else {
            self.rendered_state.pending_g = true;
            true
        }
    }

    /// Hold the first bracket of `[[`/`]]`; the renderer uses count two to
    /// distinguish the completed heading motion from an unbound bracket.
    fn first_heading_bracket_is_pending(&mut self, key: KeyInput) -> bool {
        let bracket = match key.code.kind {
            KeyCodeKind::Char(c @ ('[' | ']')) if key.mods == Modifiers::default() => c,
            _ => {
                self.rendered_state.pending_heading_bracket = None;
                return false;
            }
        };
        if self.rendered_state.pending_heading_bracket == Some(bracket) {
            self.rendered_state.pending_heading_bracket = None;
            self.rendered_state.count = self.rendered_state.count.saturating_add(1).max(2);
            false
        } else {
            self.rendered_state.pending_heading_bracket = Some(bracket);
            true
        }
    }

    fn handle_rendered_navigation_key(&mut self, key: KeyInput) -> Vec<Effect> {
        if let KeyCodeKind::Char(c) = key.code.kind {
            if c.is_ascii_digit()
                && key.mods == Modifiers::default()
                && (c != '0' || self.rendered_state.count > 0)
            {
                let digit = c.to_digit(10).unwrap() as usize;
                self.rendered_state.count = self.rendered_state.count.saturating_mul(10) + digit;
                return Vec::new();
            }
        }

        let Some(layout) = self.rendered_state.layout_cache.as_ref() else {
            return Vec::new();
        };
        let text = nav::key_inspects_source(key).then(|| self.vim.text());
        let cursor = self.rendered_state.cursor;
        let search = self.rendered_state.search.clone();
        let count = std::mem::take(&mut self.rendered_state.count);
        let result = nav::handle_key(
            key,
            &cursor,
            search.as_ref(),
            layout.lines.len(),
            &layout.jump_targets,
            layout,
            count,
            text.as_deref().unwrap_or_default(),
        );
        let mut effects = Vec::new();
        if result.search_changed {
            if let Some(new_search) = result.new_search {
                self.rendered_state.search_input_active = new_search.pattern.is_empty();
                self.rendered_state.search_origin =
                    self.rendered_state.search_input_active.then_some(cursor);
                self.rendered_state.search = Some(new_search);
            } else {
                self.rendered_state.clear_search();
            }
        }
        if let Some(new_cursor) = result.new_cursor.filter(|_| result.cursor_moved) {
            self.rendered_state.cursor = new_cursor;
            self.commit_rendered_cursor();
            self.refresh_character_selection();
            effects.push(Effect::CursorMoved);
        }
        let collapse_hides_selection = result.fm_collapsed_toggled
            && !self.rendered_state.fm_collapsed
            && self.mode == Mode::Select
            && crate::frontmatter::front_matter_span(
                text.as_deref().unwrap_or_else(|| self.highlighter.text()),
            )
            .is_some_and(|front_matter| {
                self.rendered_selection().is_some_and(|selection| {
                    selection.source_ranges.iter().any(|source| {
                        source.start < front_matter.end && front_matter.start < source.end
                    })
                })
            });
        if result.layout_dirty {
            self.rendered_state.invalidate();
        }
        if result.fm_collapsed_toggled {
            self.rendered_state.fm_collapsed = !self.rendered_state.fm_collapsed;
        }
        if let Some(message) = result.message {
            effects.push(Effect::Message {
                text: message,
                severity: Severity::Info,
            });
        }
        if collapse_hides_selection {
            return self.finish_select(Mode::Normal, effects);
        }
        effects
    }

    /// Handle pattern entry while a rendered search prompt is active.
    fn handle_rendered_search_input(&mut self, key: KeyInput) -> Vec<Effect> {
        match key.code.kind {
            KeyCodeKind::Esc => {
                self.rendered_state.clear_search();
                Vec::new()
            }
            KeyCodeKind::Enter => {
                self.rendered_state.search_input_active = false;
                self.rendered_state.search_origin = None;
                Vec::new()
            }
            KeyCodeKind::Char(c)
                if !key.mods.ctrl
                    && !key.mods.alt
                    && !key.mods.shift
                    && (c.is_ascii_alphanumeric() || c == ' ' || c == '.' || c == '_') =>
            {
                let cursor = self
                    .rendered_state
                    .search_origin
                    .unwrap_or(self.rendered_state.cursor);
                let text = self.vim.text();
                let Some(layout) = self.rendered_state.layout_cache.as_ref() else {
                    return Vec::new();
                };
                let mut search_state = self
                    .rendered_state
                    .search
                    .as_ref()
                    .expect("active rendered search input must have search state")
                    .clone();
                search_state.pattern.push(c);
                let match_line = nav::find_next_match(
                    &search_state,
                    &cursor,
                    layout,
                    &text,
                    search_state.direction(),
                );
                self.rendered_state.search = Some(search_state);
                if let Some(match_line) = match_line {
                    self.rendered_state.cursor = nav::cursor_for_row(
                        match_line,
                        self.rendered_state.cursor.desired_column,
                        layout,
                    );
                    self.commit_rendered_cursor();
                    self.refresh_character_selection();
                    vec![Effect::CursorMoved]
                } else {
                    Vec::new()
                }
            }
            _ => Vec::new(),
        }
    }

    fn enter_insert_from_rendered(&mut self, action: RenderedExitAction) -> Vec<Effect> {
        self.commit_rendered_cursor();
        self.rendered_state.clear_search();
        self.rendered_state.count = 0;
        let vim_effects = self.vim.handle_key(self.key_input_to_vim(action.key()));
        self.translate_vim_effects(vim_effects)
    }

    fn enter_select(&mut self, shape: SelectionShape) -> Vec<Effect> {
        self.rendered_state.select_anchor = Some(self.rendered_state.cursor.point());
        self.rendered_state.select_anchor_source = Some(self.vim.cursor());
        let atom = self
            .rendered_state
            .layout_cache
            .as_ref()
            .and_then(|layout| nav::source_for_point(self.rendered_state.cursor.point(), layout));
        self.rendered_state.select_anchor_atom = atom.clone();
        self.rendered_state.select_active_atom = atom;
        self.rendered_state.select_anchor_atom_exact =
            self.rendered_state.select_anchor_atom.is_some();
        self.rendered_state.select_active_atom_exact =
            self.rendered_state.select_active_atom.is_some();
        let line = self
            .rendered_state
            .layout_cache
            .as_ref()
            .and_then(|layout| {
                nav::line_identity_for_point(self.rendered_state.cursor.point(), layout)
            });
        self.rendered_state.select_anchor_line = line.clone();
        self.rendered_state.select_active_line = line;
        self.rendered_state.selection_shape = shape;
        self.rendered_state.select_character_ranges.clear();
        self.rendered_state.pending_register = None;
        self.rendered_state.register_prefix_pending = false;
        self.mode = Mode::Select;
        self.refresh_character_selection();
        vec![Effect::ModeChanged(Mode::Select)]
    }

    fn switch_or_cancel_selection_shape(&mut self, shape: SelectionShape) -> Vec<Effect> {
        if self.rendered_state.selection_shape == shape {
            self.finish_select(Mode::Normal, Vec::new())
        } else {
            self.rendered_state.selection_shape = shape;
            self.refresh_character_selection();
            vec![Effect::CursorMoved]
        }
    }

    fn apply_select_operator(&mut self, operator: RangeOperator) -> Vec<Effect> {
        let Some(layout) = self.rendered_state.layout_cache.as_ref() else {
            return Vec::new();
        };
        let anchor = self
            .rendered_state
            .select_anchor
            .unwrap_or(self.rendered_state.cursor.point());
        let mut selection = nav::project_selection_from_source_positions(
            anchor,
            self.rendered_state.cursor.point(),
            self.rendered_state.selection_shape,
            self.rendered_state
                .select_anchor_source
                .unwrap_or(self.vim.cursor()),
            self.vim.cursor(),
            layout,
            &self.vim.text(),
        );
        if selection.shape == SelectionShape::Character {
            selection.source_ranges = self.rendered_state.select_character_ranges.clone();
        }
        if selection.source_ranges.is_empty() {
            return Vec::new();
        }
        let register = self
            .rendered_state
            .pending_register
            .unwrap_or(Register::Unnamed);
        let vim_effects = self.vim.apply_selection(selection, operator, register);
        let effects = self.translate_vim_effects(vim_effects);
        self.remap_active_cursor_from_canonical();
        let target_mode = if operator == RangeOperator::Change {
            Mode::Insert
        } else {
            Mode::Normal
        };
        self.finish_select(target_mode, effects)
    }

    fn finish_select(&mut self, mode: Mode, mut effects: Vec<Effect>) -> Vec<Effect> {
        self.rendered_state.select_anchor = None;
        self.rendered_state.select_anchor_source = None;
        self.rendered_state.select_anchor_atom = None;
        self.rendered_state.select_anchor_atom_exact = false;
        self.rendered_state.select_active_atom = None;
        self.rendered_state.select_active_atom_exact = false;
        self.rendered_state.select_anchor_line = None;
        self.rendered_state.select_active_line = None;
        self.rendered_state.select_character_ranges.clear();
        self.rendered_state.pending_register = None;
        self.rendered_state.register_prefix_pending = false;
        self.mode = mode;
        effects.retain(|effect| !matches!(effect, Effect::ModeChanged(_)));
        effects.push(Effect::ModeChanged(mode));
        effects
    }

    /// Recompute the displayed endpoint from the canonical source cursor.
    ///
    /// Line-range operations move the wrapped Vim cursor even when the
    /// document is unchanged (notably yank), so the cached rendered endpoint
    /// must be updated before returning to Normal.
    fn remap_active_cursor_from_canonical(&mut self) {
        let Some(layout) = self.rendered_state.layout_cache.as_ref() else {
            return;
        };
        let source = self.vim.cursor();
        self.rendered_state.cursor = nav::enter_rendered_at_offset(
            source.0,
            self.vim.cursor_byte_offset(),
            layout,
            |offset| self.vim.position_for_byte_offset(offset).0,
            |offset| self.vim.byte_before_is_newline(offset),
        );
    }

    fn commit_rendered_cursor(&mut self) {
        let Some(layout) = self.rendered_state.layout_cache.as_ref() else {
            return;
        };
        let source_offset = nav::canonical_source_offset_for_row(
            &self.rendered_state.cursor,
            self.vim.cursor_byte_offset(),
            layout,
        );
        let source = self.vim.position_for_byte_offset(source_offset);
        self.vim.jump_to(source.0, source.1);
    }

    fn refresh_character_selection(&mut self) {
        if self.mode != Mode::Select {
            return;
        }
        let Some(layout) = self.rendered_state.layout_cache.as_ref() else {
            return;
        };
        if let Some(atom) = nav::source_for_point(self.rendered_state.cursor.point(), layout) {
            self.rendered_state.select_active_atom = Some(atom);
            self.rendered_state.select_active_atom_exact = true;
        } else {
            self.rendered_state.select_active_atom_exact = false;
        }
        self.rendered_state.select_active_line =
            nav::line_identity_for_point(self.rendered_state.cursor.point(), layout);
        if self.rendered_state.selection_shape == SelectionShape::Character {
            let anchor = self
                .rendered_state
                .select_anchor
                .unwrap_or(self.rendered_state.cursor.point());
            self.rendered_state.select_character_ranges = nav::project_selection(
                anchor,
                self.rendered_state.cursor.point(),
                SelectionShape::Character,
                layout,
                &self.vim.text(),
            )
            .source_ranges;
        } else {
            self.rendered_state.select_character_ranges.clear();
        }
    }

    fn translate_vim_effects(&mut self, vim_effects: Vec<VimEffect>) -> Vec<Effect> {
        let mut effects = Vec::new();
        let mut left_insert = false;
        for effect in vim_effects {
            match effect {
                VimEffect::ModeChanged(vim_mode) => {
                    let mode = if vim_mode == crate::vim::Mode::Insert {
                        Mode::Insert
                    } else {
                        Mode::Normal
                    };
                    if mode != self.mode {
                        left_insert = self.mode == Mode::Insert && mode == Mode::Normal;
                        self.mode = mode;
                        effects.push(Effect::ModeChanged(mode));
                    }
                }
                VimEffect::Edited { edits } => {
                    self.highlighter.apply_edit(&edits);
                    self.rendered_state.invalidate();
                    effects.push(Effect::Edited);
                }
                VimEffect::CursorMoved => effects.push(Effect::CursorMoved),
                VimEffect::ExCommand { command } => {
                    effects.extend(self.process_ex_command(&command))
                }
                VimEffect::CommandCancelled => {}
                VimEffect::ClipboardYank(text) => effects.push(Effect::ClipboardWrite(text)),
                VimEffect::SearchWrapped => effects.push(Effect::Message {
                    text: "Search wrapped around buffer".to_string(),
                    severity: Severity::Info,
                }),
                VimEffect::Bell => {}
            }
        }
        if left_insert {
            // Insert-mode motions are owned by the canonical Vim cursor.
            // Re-enter rendered coordinates before the next rendered motion;
            // when an edit invalidated the cache, render_layout performs this
            // same remap from the canonical source anchor after rebuilding.
            self.remap_active_cursor_from_canonical();
        }
        effects
    }

    /// Process an ex command text and produce effects.
    ///
    /// Handles: :w, :w!, :w {path}, :wq, :x, :q, :q!, :e, :e!, :e {path},
    /// :saveas, :{number}, :s, :noh, :help, :set, and unknown commands.
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
                        retarget: false,
                        then_quit: false,
                    }]
                } else {
                    vec![Effect::SaveRequested {
                        path: None,
                        force,
                        retarget: false,
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
                retarget: true,
                then_quit: false,
            }],
            _ if args.0.is_none()
                && !base.is_empty()
                && base.chars().all(|c| c.is_ascii_digit()) =>
            {
                // :{number} — jump to a 1-based line, clamped at EOF.
                match base.parse::<usize>() {
                    Ok(0) | Err(_) => vec![Effect::Message {
                        text: format!("Invalid line number: {}", base),
                        severity: Severity::Warning,
                    }],
                    Ok(line) => {
                        let row = line.min(self.line_count()) - 1;
                        self.vim.jump_to(row, 0);
                        self.remap_rendered_cursor(row, 0);
                        vec![Effect::CursorMoved]
                    }
                }
            }
            "s" | "substitute" => {
                // :[range]s/pattern/replacement/[flags]
                let Some(substitute_args) = args.0 else {
                    return vec![Effect::Message {
                        text: "Invalid substitute command".to_string(),
                        severity: Severity::Warning,
                    }];
                };
                let Some((start_row, end_row)) =
                    Self::parse_substitute_range(cmd, self.cursor().0, self.line_count())
                else {
                    return vec![Effect::Message {
                        text: "Invalid substitute range".to_string(),
                        severity: Severity::Warning,
                    }];
                };
                match self.vim.substitute(substitute_args, start_row, end_row) {
                    Ok(edits) if edits.is_empty() => vec![Effect::Message {
                        text: "No replacement done".to_string(),
                        severity: Severity::Info,
                    }],
                    Ok(edits) => {
                        self.highlighter.apply_edit(&edits);
                        self.rendered_state.invalidate();
                        vec![Effect::Edited]
                    }
                    Err(_) => vec![Effect::Message {
                        text: "Invalid substitute command".to_string(),
                        severity: Severity::Warning,
                    }],
                }
            }
            "noh" => {
                self.vim.clear_search_highlight();
                self.rendered_state.clear_search();
                vec![Effect::Message {
                    text: "Search highlighting cleared".to_string(),
                    severity: Severity::Info,
                }]
            }
            "help" => vec![Effect::HelpRequested],
            "set" => match args.0 {
                Some("wrap") => vec![Effect::SetOption {
                    key: "wrap".to_string(),
                    value: true,
                }],
                Some("nowrap") => vec![Effect::SetOption {
                    key: "wrap".to_string(),
                    value: false,
                }],
                Some(unknown) => vec![Effect::Message {
                    text: format!("Unknown option: {unknown}"),
                    severity: Severity::Warning,
                }],
                None => vec![Effect::Message {
                    text: "Usage: :set <option>".to_string(),
                    severity: Severity::Warning,
                }],
            },
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

    /// Resolve a substitute command's optional 1-based line range.
    /// An omitted range targets the cursor line; `%` targets the whole buffer.
    fn parse_substitute_range(
        command: &str,
        cursor_row: usize,
        line_count: usize,
    ) -> Option<(usize, usize)> {
        let command = command.trim_start_matches(':');
        let substitute_pos = if command.starts_with("substitute/") || command.starts_with("s/") {
            0
        } else {
            command.find("substitute/").or_else(|| command.find("s/"))?
        };
        let prefix = command[..substitute_pos].trim();
        let last_row = line_count.saturating_sub(1);

        if prefix.is_empty() {
            let row = cursor_row.min(last_row);
            return Some((row, row));
        }
        if prefix == "%" {
            return Some((0, last_row));
        }

        let (start, end) = prefix.split_once(',').unwrap_or((prefix, prefix));
        let start = start.parse::<usize>().ok()?.checked_sub(1)?;
        let end = end.parse::<usize>().ok()?.checked_sub(1)?;
        (start <= end && end <= last_row).then_some((start, end))
    }

    fn overlay_search_match(
        line: &mut crate::style::StyledLine,
        byte_range: std::ops::Range<usize>,
    ) {
        let byte_start = byte_range.start.min(line.text.len());
        let byte_end = byte_range.end.min(line.text.len());
        if byte_start >= byte_end
            || !line.text.is_char_boundary(byte_start)
            || !line.text.is_char_boundary(byte_end)
        {
            return;
        }

        let start_col = line.text[..byte_start].chars().count();
        let end_col = line.text[..byte_end].chars().count();
        let mut spans = Vec::with_capacity(line.spans.len() + 1);
        for span in line.spans.drain(..) {
            if span.end_col <= start_col || span.start_col >= end_col {
                spans.push(span);
                continue;
            }
            if span.start_col < start_col {
                spans.push(crate::style::Span {
                    start_col: span.start_col,
                    end_col: start_col,
                    style: span.style,
                });
            }
            if span.end_col > end_col {
                spans.push(crate::style::Span {
                    start_col: end_col,
                    end_col: span.end_col,
                    style: span.style,
                });
            }
        }
        spans.push(crate::style::Span {
            start_col,
            end_col,
            style: crate::style::SemanticStyle::Match,
        });
        spans.sort_by_key(|span| span.start_col);
        line.spans = spans;
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
            KeyCodeKind::Noop => {
                unreachable!("Noop inputs are ignored before Vim key conversion")
            }
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
