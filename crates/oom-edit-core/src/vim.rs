//! The hjkl wrapper — the ONLY module allowed to import `hjkl_*` crates.
//!
//! This module implements the `VimCore` contract over the pinned hjkl crate
//! family (`hjkl-engine`, `hjkl-buffer`, `hjkl-vim`). All hjkl types are
//! confined here; no hjkl type appears in any other module or any public
//! signature (R-2).
//!
//! See architecture §6.1 for the `VimCore` contract.

// ────────────────────────────────────────────────────────────────────────────
// SPIKE FINDINGS
// ────────────────────────────────────────────────────────────────────────────
//
// Spike performed against hjkl v0.39.0 vendored sources. Six integration
// points validated:
//
// 1. **Editor construction with vim discipline:**
//    `Editor::new(View::from_str(text), DefaultHost::new(), Options::default())`
//    then `install_vim_discipline(&mut editor)`.
//    — ✅ Works. `install_vim_discipline` is a `pub use vim::install` re-export.
//
// 2. **Feeding one key and observing mode:**
//    `feed_input(&mut editor, planned_input)` → `editor.vim_mode()` returns
//    `hjkl_engine::VimMode`. `feed_input` returns `bool` (consumed).
//    Also: `dispatch_input(&mut editor, input)` for raw `Input` events.
//    — ✅ Works. `feed_input` wraps `decode_planned_input` + `dispatch_input`
//      + cursor-shape emission. `dispatch_input` is the raw entry point.
//
// 3. **Reading buffer text and cursor:**
//    `editor.buffer().as_string()` for full text.
//    `editor.buffer().cursor()` → `hjkl_buffer::Position { row, col }`.
//    `editor.line(row)` → `Option<String>`.
//    — ✅ Works. `editor.buffer()` returns `&View`; `View::as_string()` joins
//      all lines with `\n`. `View::cursor()` returns the charwise cursor.
//
// 4. **Receiving content-edit notifications:**
//    `editor.take_content_edits()` → `Vec<ContentEdit>` with byte offsets.
//    Each `ContentEdit` has `start_byte`, `old_end_byte`, `new_end_byte`
//    plus position tuples `(row, col_byte)`.
//    — ✅ Works. Edits are drained atomically; subsequent calls return empty.
//    Map to `TextEdit { range: start_byte..old_end_byte, new_text_len }`.
//
// 5. **Reading visual-mode selections:**
//    `editor.vim_char_highlight()` → `Option<((usize,usize),(usize,usize))>`
//    for char-visual; `vim_line_highlight()` for linewise;
//    `vim_block_highlight()` for blockwise.
//    Also: `editor.buffer_selection()` → `Option<hjkl_buffer::Selection>`.
//    — ✅ Works. The `VimEditorExt` trait provides all three highlight
//      accessors. Convert row/col selections to byte ranges via
//      `buffer_byte_of_row` + line char counts.
//
// 6. **Observing ex-command submission (`:w`):**
//    The engine's command-line mode (`hjkl_vim::Mode::CommandLine`) is
//    accessible via `editor.vim_mode()`. The command text is available via
//    `editor.buffer()` while in command line mode (the command is typed
//    into the buffer).
//    — ⚠️ Partial. hjkl-vim's command-line mode stores the command in the
//      buffer itself. We detect `CommandLine` mode and read the buffer text
//      as the command. `Esc` exits to Normal; `Enter` commits. We intercept
//      `Enter` to capture the command before hjkl processes it.
//
// Mode mapping (hjkl → oom-edit):
//   hjkl::VimMode::Normal      → oom_edit::Mode::Normal
//   hjkl::VimMode::Insert      → oom_edit::Mode::Insert
//   hjkl::VimMode::Visual      → oom_edit::Mode::Visual
//   hjkl::VimMode::Replace     → oom_edit::Mode::Insert (replace → insert)
//   hjkl::VimMode::Command     → oom_edit::Mode::Command
//   (hjkl has no VisualLine/VisualBlock in VimMode — those are in
//    hjkl_vim::Mode which is internal; we use vim_mode() for the coarse
//    mode and track visual sub-kinds via the selection highlight checks.)

use std::ops::Range;
use std::sync::Arc;

use hjkl_buffer::{ContentEdit, View};
use hjkl_engine::types::{CursorShape, DefaultHost, Host, Options, Viewport};
use hjkl_engine::{Editor, PlannedInput, SpecialKey, VimMode as HjklVimMode};
use hjkl_vim::vim::{Operator as HjklOperator, Pending as HjklPending, VimState as HjklVimState};
use hjkl_vim::{feed_input, install_vim_discipline, VimEditorExt};

// ── vim.rs internal types ──────────────────────────────────────────────────

/// Host adapter that exposes engine clipboard writes as drainable events.
struct ClipboardCapturingHost {
    inner: DefaultHost,
    pending_writes: Vec<String>,
}

impl ClipboardCapturingHost {
    fn new() -> Self {
        Self {
            inner: DefaultHost::new(),
            pending_writes: Vec::new(),
        }
    }

    fn take_clipboard_writes(&mut self) -> Vec<String> {
        std::mem::take(&mut self.pending_writes)
    }
}

impl Host for ClipboardCapturingHost {
    type Intent = ();

    fn write_clipboard(&mut self, text: String) {
        self.inner.write_clipboard(text.clone());
        self.pending_writes.push(text);
    }

    fn read_clipboard(&mut self) -> Option<String> {
        self.inner.read_clipboard()
    }

    fn now(&self) -> core::time::Duration {
        self.inner.now()
    }

    fn prompt_search(&mut self) -> Option<String> {
        self.inner.prompt_search()
    }

    fn emit_cursor_shape(&mut self, shape: CursorShape) {
        self.inner.emit_cursor_shape(shape);
    }

    fn viewport(&self) -> &Viewport {
        self.inner.viewport()
    }

    fn viewport_mut(&mut self) -> &mut Viewport {
        self.inner.viewport_mut()
    }

    fn emit_intent(&mut self, _intent: Self::Intent) {}
}

/// Effects emitted by `VimCore::handle_key`. Translated from hjkl state
/// changes into a form the session layer can act on.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) enum VimEffect {
    /// Buffer was modified; contains byte-range edits for the highlighter.
    Edited { edits: Vec<TextEdit> },
    /// Cursor moved (may also indicate a mode-dependent cursor change).
    CursorMoved,
    /// Mode changed. The session layer uses this to update its Mode state.
    ModeChanged(Mode),
    /// An ex command was entered (in Command mode, user pressed Enter).
    ExCommand { command: String },
    /// Command line mode cancelled (user pressed Esc while in Command mode).
    CommandCancelled,
    /// Yanked text to clipboard register.
    ClipboardYank(String),
    /// Search wrapped around buffer end.
    SearchWrapped,
    /// Vim bell (for unbound keys in Normal mode).
    Bell,
}

/// A text edit describing a byte-range replacement in the document.
/// Matches the shape tree-sitter's `InputEdit` consumes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextEdit {
    /// Byte range in the document that was replaced.
    pub range: Range<usize>,
    /// Length of the replacement text in bytes.
    pub new_text_len: usize,
    /// The new text that replaces the range (empty for deletes).
    pub new_text: String,
}

// ── Mode ───────────────────────────────────────────────────────────────────

/// Terminal-agnostic mode enum. Mirrors the seven modes from plan §6.1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Mode {
    Normal,
    Insert,
    Visual,
    VisualLine,
    VisualBlock,
    Command,
}

#[allow(dead_code)]
impl Mode {
    /// Check if this mode allows buffer mutations.
    pub(crate) fn is_editable(self) -> bool {
        matches!(
            self,
            Mode::Normal
                | Mode::Insert
                | Mode::Visual
                | Mode::VisualLine
                | Mode::VisualBlock
                | Mode::Command
        )
    }
}

// ── KeyCode / Modifiers / KeyInput ─────────────────────────────────────────

/// Terminal-agnostic key representation (mirrors, but does not expose,
/// crossterm's model).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct KeyCode {
    /// The key code kind.
    pub kind: KeyCodeKind,
}

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

/// A terminal-agnostic key event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct KeyInput {
    /// The key code.
    pub code: KeyCode,
    /// The modifiers.
    pub mods: Modifiers,
}

// ── VimCore ────────────────────────────────────────────────────────────────

/// The hjkl wrapper implementing the `VimCore` contract.
///
/// This is the one module allowed to import `hjkl_*` crates. All hjkl types
/// are confined here; the public API (defined in session.rs) uses our own
/// types.
pub(crate) struct VimCore {
    /// The hjkl editor instance.
    editor: Editor<hjkl_buffer::View, ClipboardCapturingHost>,
    /// The current oom-edit mode, derived from hjkl state.
    mode: Mode,
    /// The last saved undo mark (for dirty tracking).
    save_point_dirty_gen: u64,
    /// Stable sequence of the undo node active at the last save point.
    save_point_undo_seq: u64,
    /// Exact content at the last save point.
    save_point_content: Arc<String>,
    /// Whether the current undo state differs from the save point.
    modified_since_save: bool,
    /// Whether the preceding Normal-mode key was the `g` history prefix.
    history_prefix_pending: bool,
}

impl VimCore {
    /// Create a new `VimCore` from initial text. Starts in Normal mode.
    pub(crate) fn new(text: &str) -> Self {
        let view = View::from_str(text);
        // oom-edit promises stock Vim `s`/`S` substitution semantics; hjkl's
        // optional sneak motion is deliberately outside that conformance set.
        let options = Options {
            motion_sneak: false,
            ..Options::default()
        };
        let mut editor = Editor::new(view, ClipboardCapturingHost::new(), options);
        install_vim_discipline(&mut editor);
        let save_point_dirty_gen = editor.buffer().dirty_gen();
        let save_point_undo_seq = editor.buffer().current_undo_seq();
        let save_point_content = editor.buffer().content_joined();
        Self {
            editor,
            mode: Mode::Normal,
            save_point_dirty_gen,
            save_point_undo_seq,
            save_point_content,
            modified_since_save: false,
            history_prefix_pending: false,
        }
    }

    /// Handle a key input. Returns zero or more effects.
    pub(crate) fn handle_key(&mut self, key: KeyInput) -> Vec<VimEffect> {
        // Normalize: <C-[> (Ctrl+LeftBracket) is ASCII 0x1b, identical to Esc.
        // hjkl-vim only recognizes Key::Esc for exit-insert; it does NOT treat
        // Char('[', ctrl=true) as Esc. Normalize here so callers can use either.
        let key = self.normalize_ctrl_bracket(key);

        // Convert our KeyInput → hjkl Input
        let hjkl_input = self.key_input_to_hjkl(key);

        // Check if we're in command-line mode and need to intercept
        if self.mode == Mode::Command {
            return self.handle_command_mode_key(hjkl_input, key);
        }

        let cursor_before = self.cursor();
        let repeat_search_wrapped = if self.mode == Mode::Normal
            && key.mods == Modifiers::default()
            && self.editor.last_search_pattern().is_some()
        {
            let direction = match key.code.kind {
                KeyCodeKind::Char('n') => Some(self.editor.last_search_forward()),
                KeyCodeKind::Char('N') => Some(!self.editor.last_search_forward()),
                _ => None,
            };
            direction.map(|forward| {
                let count = self.hjkl_state().count.max(1);
                self.repeat_search_will_wrap(forward, count, cursor_before)
            })
        } else {
            None
        };

        let system_register_selected = self.system_clipboard_register_selected();
        let system_yank_pending = system_register_selected && self.yank_operator_pending();
        let system_yank_step =
            system_register_selected && (system_yank_pending || Self::is_yank_key(key));

        let history_traversal = self.is_history_traversal_key(key);
        let history_text_before = history_traversal.then(|| self.text());
        let undo_seq_before = history_traversal.then(|| self.editor.buffer().current_undo_seq());
        let next_history_prefix = self.mode == Mode::Normal
            && !self.history_prefix_pending
            && matches!(key.code.kind, KeyCodeKind::Char('g'))
            && key.mods == Modifiers::default();

        // Feed the key through hjkl
        let consumed = if self.is_visual_register_prefix(key) {
            // hjkl handles a pending register selector in every visual mode,
            // but only opens the `"{register}` chord from Normal mode. Seed
            // that same pending state here so Visual yanks can target `+`/`*`.
            self.hjkl_state_mut().pending = HjklPending::SelectRegister;
            true
        } else {
            feed_input(&mut self.editor, hjkl_input)
        };

        if !consumed {
            self.history_prefix_pending = false;
            self.editor.host_mut().take_clipboard_writes();
            // Key not consumed — could be a bell (unbound key in Normal)
            return vec![VimEffect::Bell];
        }
        self.history_prefix_pending = next_history_prefix;

        // Drain any content edits
        let mut edits = self.drain_content_edits();
        if self.editor.take_content_reset() {
            let old_text = history_text_before.as_deref().unwrap_or("");
            let new_text = self.text();
            edits = vec![TextEdit {
                range: 0..old_text.len(),
                new_text_len: new_text.len(),
                new_text,
            }];
        }

        let history_moved =
            undo_seq_before.is_some_and(|before| self.editor.buffer().current_undo_seq() != before);
        if history_moved {
            self.modified_since_save = !self.is_at_save_point();
        } else if !edits.is_empty() {
            self.modified_since_save = true;
        }

        // Check for mode change
        let current_mode = self.compute_mode();
        let mut effects = Vec::new();
        if current_mode != self.mode {
            effects.push(VimEffect::ModeChanged(current_mode));
            self.mode = current_mode;
        }

        // Check for cursor movement
        if !edits.is_empty() {
            effects.push(VimEffect::Edited { edits });
        } else {
            effects.push(VimEffect::CursorMoved);
        }

        let writes = self.editor.host_mut().take_clipboard_writes();
        if system_yank_pending
            && writes.is_empty()
            && !self.yank_operator_pending()
            && self.editor.search_prompt_state().is_none()
        {
            self.hjkl_state_mut().pending_register = None;
        }
        effects.extend(self.clipboard_effects(writes, system_yank_step));

        if repeat_search_wrapped == Some(true) {
            effects.push(VimEffect::SearchWrapped);
        }

        effects
    }

    fn clipboard_effects(&self, writes: Vec<String>, system_yank_step: bool) -> Vec<VimEffect> {
        if !system_yank_step || writes.is_empty() {
            return Vec::new();
        }

        let clipboard_text = self
            .editor
            .with_registers(|registers| registers.read('+').map(|slot| slot.text.clone()));

        writes
            .into_iter()
            .filter(|text| clipboard_text.as_ref() == Some(text))
            .map(VimEffect::ClipboardYank)
            .collect()
    }

    /// Determine whether a counted repeat search crosses a buffer boundary.
    /// The engine processes the whole count in one step, so comparing only the
    /// final cursor with the starting cursor loses wraps that happen midway.
    fn repeat_search_will_wrap(
        &mut self,
        forward: bool,
        count: usize,
        cursor: (usize, usize),
    ) -> bool {
        let Some(pattern) = self.editor.last_search_pattern() else {
            return false;
        };
        self.editor.push_search_pattern(&pattern);
        if !self.editor.search_state().wrap_around {
            return false;
        }

        let line_count = self.text().split('\n').count();
        let mut total_matches = 0;
        let mut matches_before_boundary = 0;
        for row in 0..line_count {
            let line = self.line(row).unwrap_or_default();
            for range in self.search_matches_for_line(row) {
                total_matches += 1;
                let byte_start = range.start.min(line.len());
                let position = (row, line[..byte_start].chars().count());
                if (forward && position > cursor) || (!forward && position < cursor) {
                    matches_before_boundary += 1;
                }
            }
        }

        total_matches > 0 && count > matches_before_boundary
    }

    fn hjkl_state(&self) -> &HjklVimState {
        self.editor
            .discipline()
            .as_any()
            .downcast_ref::<HjklVimState>()
            .expect("vim discipline must remain installed")
    }

    fn hjkl_state_mut(&mut self) -> &mut HjklVimState {
        self.editor
            .discipline_mut()
            .as_any_mut()
            .downcast_mut::<HjklVimState>()
            .expect("vim discipline must remain installed")
    }

    fn system_clipboard_register_selected(&self) -> bool {
        matches!(self.hjkl_state().pending_register, Some('+') | Some('*'))
    }

    fn is_yank_key(key: KeyInput) -> bool {
        match key.code.kind {
            KeyCodeKind::Char('y') => key.mods == Modifiers::default(),
            KeyCodeKind::Char('Y') => !key.mods.ctrl && !key.mods.alt,
            _ => false,
        }
    }

    fn is_visual_register_prefix(&self, key: KeyInput) -> bool {
        matches!(
            self.mode,
            Mode::Visual | Mode::VisualLine | Mode::VisualBlock
        ) && self.hjkl_state().pending == HjklPending::None
            && matches!(key.code.kind, KeyCodeKind::Char('"'))
            && key.mods == Modifiers::default()
    }

    fn yank_operator_pending(&self) -> bool {
        if self
            .editor
            .search_prompt_state()
            .and_then(|prompt| prompt.operator.as_ref())
            .is_some_and(|(op, _, _)| *op == HjklOperator::Yank)
        {
            return true;
        }

        match &self.hjkl_state().pending {
            HjklPending::Op { op, .. }
            | HjklPending::OpTextObj { op, .. }
            | HjklPending::OpG { op, .. }
            | HjklPending::OpFind { op, .. }
            | HjklPending::OpSquareBracketOpen { op, .. }
            | HjklPending::OpSquareBracketClose { op, .. }
            | HjklPending::OpSneakFirst { op, .. }
            | HjklPending::OpSneakSecond { op, .. } => *op == HjklOperator::Yank,
            _ => false,
        }
    }

    /// Return the current mode.
    #[allow(dead_code)]
    pub(crate) fn mode(&self) -> Mode {
        self.mode
    }

    /// Return the full buffer text.
    pub(crate) fn text(&self) -> String {
        self.editor.buffer().as_string()
    }

    /// Return line `idx` (0-based), or `None` if out of range.
    pub(crate) fn line(&self, idx: usize) -> Option<String> {
        self.editor.line(idx)
    }

    /// Return cursor position as `(row, col)` — 0-based char indices.
    pub(crate) fn cursor(&self) -> (usize, usize) {
        let pos = self.editor.buffer().cursor();
        (pos.row, pos.col)
    }

    /// Return visual-mode selection byte ranges. Empty vec when not in
    /// visual mode.
    pub(crate) fn selections(&self) -> Vec<Range<usize>> {
        let _ = self.cursor();
        let buffer_text = self.text();

        match self.mode {
            Mode::Visual => {
                if let Some(((start_row, start_col), (end_row, end_col))) =
                    self.editor.char_highlight()
                {
                    let start_byte = self.row_col_to_byte(start_row, start_col, &buffer_text);
                    let end_byte = self.row_col_to_byte(end_row, end_col, &buffer_text);
                    return std::iter::once(start_byte..end_byte).collect();
                }
                vec![]
            }
            Mode::VisualLine => {
                if let Some((start_row, end_row)) = self.editor.line_highlight() {
                    let start_byte = self.row_col_to_byte(start_row, 0, &buffer_text);
                    // For linewise, include the newline
                    let end_byte = if end_row + 1 < self.editor.buffer().row_count() {
                        self.row_col_to_byte(end_row + 1, 0, &buffer_text)
                    } else {
                        buffer_text.len()
                    };
                    return std::iter::once(start_byte..end_byte).collect();
                }
                vec![]
            }
            Mode::VisualBlock => {
                if let Some((top, bot, left, right)) = self.editor.block_highlight() {
                    let mut ranges = Vec::new();
                    for r in top..=bot.min(self.editor.buffer().row_count().saturating_sub(1)) {
                        let line_text = self.line(r).unwrap_or_default();
                        let start_byte = self.row_col_to_byte(r, left, &buffer_text);
                        let end_col = left
                            .min(line_text.chars().count())
                            .saturating_add(right - left + 1);
                        let end_byte = self.row_col_to_byte(r, end_col, &buffer_text);
                        if start_byte < end_byte {
                            ranges.push(start_byte..end_byte);
                        }
                    }
                    return ranges;
                }
                vec![]
            }
            _ => vec![],
        }
    }

    /// Return active search-highlight byte ranges for one buffer line.
    pub(crate) fn search_matches_for_line(&mut self, row: usize) -> Vec<Range<usize>> {
        self.editor
            .highlights_for_line(row as u32)
            .into_iter()
            .filter_map(|highlight| {
                matches!(
                    highlight.kind,
                    hjkl_engine::types::HighlightKind::SearchMatch
                        | hjkl_engine::types::HighlightKind::IncSearch
                )
                .then_some(highlight.range.start.col as usize..highlight.range.end.col as usize)
            })
            .collect()
    }

    /// Clear the active search pattern used for match highlighting.
    pub(crate) fn clear_search_highlight(&mut self) {
        self.editor.set_search_pattern(None);
    }

    /// Publish the current source viewport for screen- and pagewise motions.
    pub(crate) fn set_viewport(&mut self, top_row: usize, height: u16) {
        self.editor.set_viewport_height(height);
        self.editor.set_viewport_top(top_row);
    }

    /// Command-line mode is not supported by the hjkl wrapper.
    /// Returns `None` always.
    pub(crate) fn command_line(&self) -> Option<String> {
        None
    }

    /// Check if the buffer has been modified since the last save point.
    pub(crate) fn is_modified_since(&self, save_point: UndoMark) -> bool {
        self.editor.buffer().dirty_gen() != save_point.0 && self.modified_since_save
    }

    /// Take a new save point (current dirty generation).
    pub(crate) fn save_point(&mut self) -> UndoMark {
        self.save_point_dirty_gen = self.editor.buffer().dirty_gen();
        self.save_point_undo_seq = self.editor.buffer().current_undo_seq();
        self.save_point_content = self.editor.buffer().content_joined();
        self.modified_since_save = false;
        UndoMark(self.save_point_dirty_gen)
    }

    /// Apply one Ex substitution as a single undoable engine transaction.
    pub(crate) fn substitute(
        &mut self,
        args: &str,
        start_row: usize,
        end_row: usize,
    ) -> Result<Vec<TextEdit>, String> {
        let command = hjkl_engine::substitute::parse_substitute(args)?;
        let old_text = self.text();
        let outcome = hjkl_engine::substitute::apply_substitute(
            &mut self.editor,
            &command,
            start_row as u32..=end_row as u32,
        )?;
        if outcome.replacements == 0 {
            return Ok(Vec::new());
        }

        self.modified_since_save = true;
        let new_text = self.text();
        Ok(vec![TextEdit {
            range: 0..old_text.len(),
            new_text_len: new_text.len(),
            new_text,
        }])
    }

    /// Move the cursor to the given (row, col) position.
    pub(crate) fn jump_to(&mut self, row: usize, col: usize) {
        use hjkl_buffer::Position;
        let pos = Position {
            row,
            col: col.min(
                self.editor
                    .line(row)
                    .map(|l| l.chars().count())
                    .unwrap_or(0),
            ),
        };
        self.editor.buffer_mut().set_cursor(pos);
    }

    /// Insert text at the current cursor position (for paste operations).
    ///
    /// This inserts the text directly into the buffer without going through
    /// the hjkl input pipeline, so it counts as a single undo step.
    /// Returns the edits for the highlighter.
    pub(crate) fn insert_text(&mut self, text: &str) -> Vec<TextEdit> {
        self.modified_since_save = true;
        let buffer = self.editor.buffer_mut();
        let cursor = buffer.cursor();
        let line = cursor.row;
        let col = cursor.col;

        // Get the current line text using rope_line_str helper
        let current_line = {
            let rope = buffer.rope();
            hjkl_buffer::rope_line_str(&rope, line)
        };

        // Convert col (character index) to byte offset within the line
        let byte_offset: usize = current_line.chars().take(col).map(|c| c.len_utf8()).sum();

        // Calculate global byte offset
        let mut global_offset = 0;
        for i in 0..line {
            let rope = buffer.rope();
            let l = hjkl_buffer::rope_line_str(&rope, i);
            global_offset += l.len() + 1; // +1 for newline
        }
        global_offset += byte_offset;

        // Insert the text at the byte offset
        let full_text = buffer.as_string();
        let mut new_text = full_text[..global_offset].to_string();
        new_text.push_str(text);
        new_text.push_str(&full_text[global_offset..]);

        // Replace the entire buffer
        buffer.replace_all(&new_text);

        // Move cursor forward by the inserted text length (in chars)
        let char_len = text.chars().count();
        let new_line = if text.contains('\n') {
            // If multi-line, cursor goes to the end of the last line of pasted text
            let new_lines: Vec<&str> = new_text.split('\n').collect();
            let last_line_idx = new_lines.len().saturating_sub(1).min(new_lines.len() - 1);
            // Cursor at end of the last line of pasted text
            let last_line = new_lines[last_line_idx];
            (last_line_idx, last_line.chars().count())
        } else {
            // Single line: advance col within the same line
            let max_col = new_text
                .split('\n')
                .nth(line)
                .map(|l| l.chars().count())
                .unwrap_or(0);
            (line, (col + char_len).min(max_col))
        };

        buffer.set_cursor(hjkl_buffer::Position {
            row: new_line.0,
            col: new_line.1,
        });

        // Collect the content edits for the highlighter
        let edits = self.drain_content_edits();
        if edits.is_empty() {
            vec![TextEdit {
                range: global_offset..global_offset,
                new_text_len: text.len(),
                new_text: text.to_string(),
            }]
        } else {
            edits
        }
    }

    /// Apply the engine's right-to-left multi-split operation for syntax
    /// regression tests, then translate its emitted content edits through the
    /// same wrapper path used by normal key handling.
    #[cfg(test)]
    pub(crate) fn split_lines_for_test(
        &mut self,
        row: usize,
        cols: Vec<usize>,
        inserted_spaces: Vec<bool>,
    ) -> Vec<TextEdit> {
        self.editor.mutate_edit(hjkl_buffer::Edit::SplitLines {
            row,
            cols,
            inserted_spaces,
        });
        self.drain_content_edits()
    }

    /// Apply a sequence of single-character engine replacements for syntax
    /// regression tests, then translate the accumulated content-edit batch
    /// through the normal wrapper drain path.
    #[cfg(test)]
    pub(crate) fn replace_chars_for_test(
        &mut self,
        replacements: &[(usize, usize, char)],
    ) -> Vec<TextEdit> {
        use hjkl_buffer::{Edit, Position};

        for &(row, col, replacement) in replacements {
            self.editor.mutate_edit(Edit::Replace {
                start: Position::new(row, col),
                end: Position::new(row, col + 1),
                with: replacement.to_string(),
            });
        }
        self.drain_content_edits()
    }

    // ── Internal helpers ─────────────────────────────────────────────

    /// Return whether `key` traverses the undo tree in Normal mode.
    fn is_history_traversal_key(&self, key: KeyInput) -> bool {
        if self.mode != Mode::Normal {
            return false;
        }

        match key.code.kind {
            KeyCodeKind::Char('u') => {
                !self.history_prefix_pending && key.mods == Modifiers::default()
            }
            KeyCodeKind::Char('r') => {
                !self.history_prefix_pending && key.mods.ctrl && !key.mods.alt && !key.mods.shift
            }
            KeyCodeKind::Char('-' | '+') => {
                self.history_prefix_pending && !key.mods.ctrl && !key.mods.alt
            }
            _ => false,
        }
    }

    /// Check stable undo-node identity and exact within-node contents.
    fn is_at_save_point(&self) -> bool {
        self.editor.buffer().current_undo_seq() == self.save_point_undo_seq
            && self.editor.buffer().content_joined().as_str() == self.save_point_content.as_str()
    }

    /// Normalize <C-[> (Ctrl+LeftBracket) to Esc.
    ///
    /// ASCII 0x1b is the Escape character. Terminals send the same byte for
    /// both the Esc key and Ctrl+[. hjkl-vim only matches `Key::Esc` for
    /// exit-insert — it does NOT treat `Char('[', ctrl=true)` as Esc.
    /// Normalize here so callers can use either notation.
    fn normalize_ctrl_bracket(&self, key: KeyInput) -> KeyInput {
        if key.mods.ctrl
            && matches!(key.code.kind, KeyCodeKind::Char('['))
            && !key.mods.alt
            && !key.mods.shift
        {
            KeyInput {
                code: KeyCode {
                    kind: KeyCodeKind::Esc,
                },
                mods: Modifiers::default(),
            }
        } else {
            key
        }
    }

    /// Convert our KeyInput to an hjkl PlannedInput.
    fn key_input_to_hjkl(&self, key: KeyInput) -> PlannedInput {
        let mods = key.mods;
        let code = key.code.kind;

        use hjkl_engine::types::Modifiers as HjklMods;
        let hjkl_mods = HjklMods {
            ctrl: mods.ctrl,
            shift: mods.shift,
            alt: mods.alt,
            super_: false,
        };

        match code {
            KeyCodeKind::Char(c) => PlannedInput::Char(c, hjkl_mods),
            KeyCodeKind::Enter => PlannedInput::Key(SpecialKey::Enter, hjkl_mods),
            KeyCodeKind::Esc => PlannedInput::Key(SpecialKey::Esc, hjkl_mods),
            KeyCodeKind::Backspace => PlannedInput::Key(SpecialKey::Backspace, hjkl_mods),
            KeyCodeKind::Tab => PlannedInput::Key(SpecialKey::Tab, hjkl_mods),
            KeyCodeKind::BackTab => PlannedInput::Key(SpecialKey::BackTab, hjkl_mods),
            KeyCodeKind::Up => PlannedInput::Key(SpecialKey::Up, hjkl_mods),
            KeyCodeKind::Down => PlannedInput::Key(SpecialKey::Down, hjkl_mods),
            KeyCodeKind::Left => PlannedInput::Key(SpecialKey::Left, hjkl_mods),
            KeyCodeKind::Right => PlannedInput::Key(SpecialKey::Right, hjkl_mods),
            KeyCodeKind::Home => PlannedInput::Key(SpecialKey::Home, hjkl_mods),
            KeyCodeKind::End => PlannedInput::Key(SpecialKey::End, hjkl_mods),
            KeyCodeKind::PageUp => PlannedInput::Key(SpecialKey::PageUp, hjkl_mods),
            KeyCodeKind::PageDown => PlannedInput::Key(SpecialKey::PageDown, hjkl_mods),
            KeyCodeKind::Delete => PlannedInput::Key(SpecialKey::Delete, hjkl_mods),
            KeyCodeKind::F(n) => PlannedInput::Key(SpecialKey::F(n), hjkl_mods),
        }
    }

    /// Compute the oom-edit mode from hjkl's internal state.
    fn compute_mode(&self) -> Mode {
        let hjkl_mode = self.editor.vim_mode();
        match hjkl_mode {
            HjklVimMode::Normal => Mode::Normal,
            HjklVimMode::Insert => Mode::Insert,
            HjklVimMode::Visual => {
                // Determine visual sub-kind from the highlight type
                if self.editor.block_highlight().is_some() {
                    Mode::VisualBlock
                } else if self.editor.line_highlight().is_some() {
                    Mode::VisualLine
                } else {
                    Mode::Visual
                }
            }
            HjklVimMode::VisualLine => Mode::VisualLine,
            HjklVimMode::VisualBlock => Mode::VisualBlock,
        }
    }

    /// Drain content edits from the editor and convert to our TextEdit shape.
    fn drain_content_edits(&mut self) -> Vec<TextEdit> {
        let hjkl_edits = self.editor.take_content_edits();
        let text = self.editor.buffer().as_string();
        hjkl_edits
            .iter()
            .enumerate()
            .map(|(index, ce)| {
                let new_text = replacement_text_from_final_buffer(&text, &hjkl_edits, index);
                TextEdit {
                    range: ce.start_byte..ce.old_end_byte,
                    new_text_len: ce
                        .new_end_byte
                        .checked_sub(ce.start_byte)
                        .expect("hjkl replacement end must not precede its start"),
                    new_text: new_text.to_string(),
                }
            })
            .collect()
    }

    /// Convert (row, col) to a byte offset in the document text.
    fn row_col_to_byte(&self, row: usize, col: usize, text: &str) -> usize {
        let lines: Vec<&str> = text.split('\n').collect();
        if row >= lines.len() {
            return text.len();
        }
        let mut offset: usize = lines[..row].iter().map(|l| l.len() + 1).sum(); // +1 for \n
        let line = lines[row];
        // col is a character index, not byte index
        offset += line.chars().take(col).map(|c| c.len_utf8()).sum::<usize>();
        offset
    }

    /// Handle keys while in Command mode.
    /// Note: hjkl doesn't expose command-line mode through its public API,
    /// so this is a simplified implementation.
    fn handle_command_mode_key(
        &mut self,
        hjkl_input: PlannedInput,
        key: KeyInput,
    ) -> Vec<VimEffect> {
        match key.code.kind {
            KeyCodeKind::Esc => {
                let _ = feed_input(&mut self.editor, hjkl_input);
                let current_mode = self.compute_mode();
                if current_mode != self.mode {
                    self.mode = current_mode;
                }
                vec![VimEffect::CommandCancelled]
            }
            KeyCodeKind::Enter => {
                // In hjkl, command-line mode is not exposed through the public API.
                // We treat Enter as a normal key that may trigger an ex command.
                let consumed = feed_input(&mut self.editor, hjkl_input);
                if consumed {
                    let current_mode = self.compute_mode();
                    if current_mode != self.mode {
                        self.mode = current_mode;
                    }
                    vec![VimEffect::CursorMoved]
                } else {
                    vec![VimEffect::Bell]
                }
            }
            _ => {
                let consumed = feed_input(&mut self.editor, hjkl_input);
                if consumed {
                    vec![VimEffect::CursorMoved]
                } else {
                    vec![VimEffect::Bell]
                }
            }
        }
    }
}

/// Extract one sequential edit's replacement from the engine's final buffer.
///
/// hjkl emits fan-out batches in the order they must be consumed, while the
/// buffer text available after the command already contains every edit. Later
/// entries located before `edit` have therefore shifted its replacement in
/// the final text even though a sequential consumer has not applied them yet.
/// Rebase by those later entries' net byte deltas without changing batch order.
fn replacement_text_from_final_buffer<'a>(
    final_text: &'a str,
    edits: &[ContentEdit],
    index: usize,
) -> &'a str {
    let edit = &edits[index];
    if edit.start_byte == edit.new_end_byte {
        return "";
    }

    let shift = edits[index + 1..]
        .iter()
        .filter(|later| later.start_byte < edit.start_byte)
        .fold(0_i128, |total, later| {
            total + later.new_end_byte as i128 - later.old_end_byte as i128
        });
    let rebased_start = usize::try_from(edit.start_byte as i128 + shift)
        .expect("hjkl replacement start must remain non-negative after rebasing");
    let rebased_end = usize::try_from(edit.new_end_byte as i128 + shift)
        .expect("hjkl replacement end must remain non-negative after rebasing");

    final_text
        .get(rebased_start..rebased_end)
        .unwrap_or_else(|| {
            panic!(
                "hjkl replacement range {rebased_start}..{rebased_end} is not a valid UTF-8 slice of the {}-byte final buffer",
                final_text.len()
            )
        })
}

#[cfg(test)]
mod dirty_tracking_tests {
    use super::*;

    fn key(ch: char) -> KeyInput {
        KeyInput {
            code: KeyCode {
                kind: KeyCodeKind::Char(ch),
            },
            mods: Modifiers::default(),
        }
    }

    fn ctrl_key(ch: char) -> KeyInput {
        KeyInput {
            code: KeyCode {
                kind: KeyCodeKind::Char(ch),
            },
            mods: Modifiers {
                ctrl: true,
                ..Modifiers::default()
            },
        }
    }

    fn shift_key(ch: char) -> KeyInput {
        KeyInput {
            code: KeyCode {
                kind: KeyCodeKind::Char(ch),
            },
            mods: Modifiers {
                shift: true,
                ..Modifiers::default()
            },
        }
    }

    fn special_key(kind: KeyCodeKind) -> KeyInput {
        KeyInput {
            code: KeyCode { kind },
            mods: Modifiers::default(),
        }
    }

    fn feed(vim: &mut VimCore, keys: &str) -> Vec<VimEffect> {
        keys.chars()
            .flat_map(|ch| vim.handle_key(key(ch)))
            .collect()
    }

    fn clipboard_writes(effects: Vec<VimEffect>) -> Vec<String> {
        effects
            .into_iter()
            .filter_map(|effect| match effect {
                VimEffect::ClipboardYank(text) => Some(text),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn system_clipboard_yank_emits_exact_payload_once_and_drains() {
        let mut vim = VimCore::new("hello\nworld");

        assert_eq!(clipboard_writes(feed(&mut vim, "\"+yy")), ["hello\n"]);

        let next_effects = vim.handle_key(key('l'));
        assert!(!next_effects
            .iter()
            .any(|effect| matches!(effect, VimEffect::ClipboardYank(_))));

        assert_eq!(clipboard_writes(feed(&mut vim, "\"+yy")), ["hello\n"]);
    }

    #[test]
    fn counted_and_uppercase_system_yanks_emit_exact_payloads() {
        let mut counted = VimCore::new("hello\nworld\nthird");
        assert_eq!(
            clipboard_writes(feed(&mut counted, "\"+2yy")),
            ["hello\nworld\n"]
        );

        let mut uppercase = VimCore::new("hello\nworld\nthird");
        assert!(clipboard_writes(feed(&mut uppercase, "\"+")).is_empty());

        assert_eq!(
            clipboard_writes(uppercase.handle_key(shift_key('Y'))),
            ["hello"]
        );
    }

    #[test]
    fn operator_motion_and_visual_system_yanks_emit_exact_payloads() {
        let mut operator_motion = VimCore::new("hello world");
        assert_eq!(
            clipboard_writes(feed(&mut operator_motion, "\"+yw")),
            ["hello "]
        );

        let mut visual = VimCore::new("hello world");
        assert!(clipboard_writes(feed(&mut visual, "v6l\"+")).is_empty());
        assert_eq!(clipboard_writes(visual.handle_key(key('y'))), ["hello w"]);
    }

    #[test]
    fn non_system_yanks_deletes_and_changes_do_not_emit_clipboard_effects() {
        for keys in ["yy", "dd", "x", "cw", "\"_yy"] {
            let mut vim = VimCore::new("hello world\nsecond line");

            let effects = feed(&mut vim, keys);

            assert!(
                !effects
                    .iter()
                    .any(|effect| matches!(effect, VimEffect::ClipboardYank(_))),
                "{keys:?} unexpectedly emitted a system clipboard write"
            );
        }
    }

    #[test]
    fn invalid_system_yank_does_not_arm_a_later_ordinary_yank() {
        let mut vim = VimCore::new("hello\nworld");
        assert_eq!(clipboard_writes(feed(&mut vim, "\"+yy")), ["hello\n"]);

        assert!(clipboard_writes(feed(&mut vim, "\"+yz")).is_empty());
        assert!(clipboard_writes(feed(&mut vim, "yy")).is_empty());
    }

    #[test]
    fn system_clipboard_search_yank_emits_exact_payload() {
        let mut vim = VimCore::new("hello world\nsecond world");
        assert!(clipboard_writes(feed(&mut vim, "\"+y/world")).is_empty());

        let effects = vim.handle_key(special_key(KeyCodeKind::Enter));

        assert_eq!(clipboard_writes(effects), ["hello "]);
    }

    #[test]
    fn canceled_system_clipboard_search_does_not_arm_ordinary_yank() {
        let mut vim = VimCore::new("hello world\nsecond world");
        assert!(clipboard_writes(feed(&mut vim, "\"+y/world")).is_empty());

        let cancel_effects = vim.handle_key(special_key(KeyCodeKind::Esc));

        assert!(clipboard_writes(cancel_effects).is_empty());
        assert!(clipboard_writes(feed(&mut vim, "yy")).is_empty());
    }

    #[test]
    fn undo_sequence_identifies_the_save_point_across_history_pruning() {
        let mut vim = VimCore::new("abcdef");
        vim.editor.settings_mut().undo_levels = 2;

        vim.handle_key(key('x'));
        vim.handle_key(key('x'));
        let mark = vim.save_point();
        assert_eq!(vim.text(), "cdef");

        vim.handle_key(key('x'));
        assert!(vim.is_modified_since(mark));
        vim.handle_key(key('u'));
        assert_eq!(vim.text(), "cdef");
        assert!(!vim.is_modified_since(mark));

        vim.handle_key(ctrl_key('r'));
        vim.handle_key(key('x'));
        vim.handle_key(key('x'));
        vim.handle_key(key('u'));
        vim.handle_key(key('u'));
        assert_ne!(vim.text(), "cdef");
        assert!(
            vim.is_modified_since(mark),
            "a pruned save-point sequence must not be confused with a later node"
        );
    }
}

// ── UndoMark ───────────────────────────────────────────────────────────────

/// Opaque marker for an undo save point. Wraps the buffer's dirty generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UndoMark(pub u64);

// ── Severity ───────────────────────────────────────────────────────────────

/// Message severity for effects.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[allow(dead_code)]
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
