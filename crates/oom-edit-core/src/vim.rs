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
// Private mode mapping (hjkl → this wrapper only):
//   hjkl::VimMode::Normal      → vim::Mode::Normal
//   hjkl::VimMode::Insert      → vim::Mode::Insert
//   hjkl::VimMode::Visual      → vim::Mode::Visual
//   hjkl::VimMode::Replace     → vim::Mode::Insert (replace → insert)
//   hjkl::VimMode::Command     → vim::Mode::Command
//   (hjkl has no VisualLine/VisualBlock in VimMode — those are in
//    hjkl_vim::Mode which is internal; we use vim_mode() for the coarse
//    mode and track visual sub-kinds via the selection highlight checks.)

use std::ops::Range;
use std::sync::Arc;

#[cfg(test)]
use std::cell::Cell;

use hjkl_buffer::{ContentEdit, View};
use hjkl_engine::types::{CursorShape, DefaultHost, Host, Options, Query, Viewport};
use hjkl_engine::{Editor, PlannedInput, SpecialKey, VimMode as HjklVimMode};
use hjkl_vim::vim::{
    InsertReason as HjklInsertReason, InsertSession as HjklInsertSession, Mode as HjklInternalMode,
    Operator as HjklOperator, Pending as HjklPending, VimState as HjklVimState,
};
use hjkl_vim::{feed_input, install_vim_discipline, VimEditorExt};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::style::{RenderedSelection, SelectionShape};

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

macro_rules! edits_reproduce {
    ($before:expr, $edits:expr, $after:expr) => {{
        let mut working = $before.clone();
        let mut valid = true;
        for edit in $edits {
            let start = edit.range.start.min(edit.range.end);
            let end = edit.range.start.max(edit.range.end);
            let (Ok(start_char), Ok(end_char)) = (
                working.try_byte_to_char(start),
                working.try_byte_to_char(end),
            ) else {
                valid = false;
                break;
            };
            if working.char_to_byte(start_char) != start
                || working.char_to_byte(end_char) != end
                || edit.new_text_len != edit.new_text.len()
            {
                valid = false;
                break;
            }
            working.remove(start_char..end_char);
            working.insert(start_char, &edit.new_text);
        }
        valid && working == *$after
    }};
}

fn validate_content_edits(
    edits: Vec<TextEdit>,
    before_len: usize,
    content_reset: bool,
    reproduces_after: impl FnOnce(&[TextEdit]) -> bool,
    fallback_text: impl FnOnce() -> String,
) -> Vec<TextEdit> {
    if !content_reset && reproduces_after(&edits) {
        return edits;
    }

    let new_text = fallback_text();
    vec![TextEdit {
        range: 0..before_len,
        new_text_len: new_text.len(),
        new_text,
    }]
}

// ── Mode ───────────────────────────────────────────────────────────────────

/// Private engine modes; these never escape the wrapper.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Mode {
    Normal,
    Insert,
    Visual,
    VisualLine,
    VisualBlock,
    Command,
}

/// Whole-line operator requested by the rendered Select surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RangeOperator {
    Yank,
    Delete,
    Change,
    Indent,
    Outdent,
}

/// Register selected by a rendered operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Register {
    Unnamed,
    System,
    Named(char),
    BlackHole,
}

impl Register {
    pub(crate) fn selector(self) -> Option<char> {
        match self {
            Self::Unnamed => None,
            Self::System => Some('+'),
            Self::Named(name) => Some(name),
            Self::BlackHole => Some('_'),
        }
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
    /// Whole-document materializations observed by wrapper hot paths.
    #[cfg(test)]
    full_materializations: Cell<usize>,
    /// Edit batches replayed by the correctness validator.
    #[cfg(test)]
    edit_replays: Cell<usize>,
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
            #[cfg(test)]
            full_materializations: Cell::new(0),
            #[cfg(test)]
            edit_replays: Cell::new(0),
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
        let undo_seq_before = history_traversal.then(|| self.editor.buffer().current_undo_seq());
        if let Some(effects) = self.try_display_block_put(key) {
            return effects;
        }
        let before = self.editor.buffer().rope();
        let dirty_gen_before = self.editor.buffer().dirty_gen();
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
        let content_reset = self.editor.take_content_reset();
        if self.editor.buffer().dirty_gen() != dirty_gen_before {
            let after = self.editor.buffer().rope();
            #[cfg(test)]
            self.edit_replays.set(self.edit_replays.get() + 1);
            edits = validate_content_edits(
                edits,
                before.len_bytes(),
                content_reset,
                |candidate| edits_reproduce!(&before, candidate, &after),
                || self.materialize_text(),
            );
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

    /// Paste renderer-created block registers using display-cell geometry.
    /// hjkl's stock block paste measures segment width with `chars().count()`,
    /// which cannot represent rectangles containing a mix of narrow and wide
    /// atoms. Keep that adaptation private to this engine boundary.
    fn try_display_block_put(&mut self, key: KeyInput) -> Option<Vec<VimEffect>> {
        let before = match key.code.kind {
            KeyCodeKind::Char('p')
                if self.mode == Mode::Normal && !key.mods.ctrl && !key.mods.alt =>
            {
                false
            }
            KeyCodeKind::Char('P')
                if self.mode == Mode::Normal && !key.mods.ctrl && !key.mods.alt =>
            {
                true
            }
            _ => return None,
        };
        let selector = self.hjkl_state().pending_register;
        let slot = self.editor.with_registers(|registers| {
            selector
                .and_then(|register| registers.read(register))
                .unwrap_or(&registers.unnamed)
                .clone()
        });
        if !slot.blockwise {
            return None;
        }
        let count = self.hjkl_state().count.max(1);
        self.hjkl_state_mut().clear_pending_prefix();
        if slot.text.is_empty() {
            return Some(Vec::new());
        }

        let old_text = self.materialize_text();
        let trailing_newline = old_text.ends_with('\n');
        let mut lines: Vec<String> = old_text.split('\n').map(str::to_string).collect();
        if trailing_newline {
            lines.pop();
        }
        let (start_row, cursor_col) = self.cursor();
        let start_line = lines.get(start_row).map_or("", String::as_str);
        let target_display_col = display_width_through_cursor(start_line, cursor_col, !before);
        let segments: Vec<&str> = slot.text.split('\n').collect();
        let mut insertion_columns = Vec::with_capacity(segments.len());

        for (index, segment) in segments.iter().enumerate() {
            let row = start_row + index;
            while row >= lines.len() {
                lines.push(String::new());
            }
            let (insert_col, missing_width) =
                char_index_at_display_column(&lines[row], target_display_col);
            let characters: Vec<char> = lines[row].chars().collect();
            let head: String = characters[..insert_col].iter().collect();
            let tail: String = characters[insert_col..].iter().collect();
            let virtual_padding = " ".repeat(missing_width);
            let piece = if tail.is_empty() {
                segment.repeat(count)
            } else {
                let padding = slot
                    .block_width
                    .saturating_sub(UnicodeWidthStr::width(*segment));
                format!("{segment}{}", " ".repeat(padding)).repeat(count)
            };
            insertion_columns.push(insert_col + missing_width);
            lines[row] = format!("{head}{virtual_padding}{piece}{tail}");
        }

        let mut new_text = lines.join("\n");
        if trailing_newline {
            new_text.push('\n');
        }
        self.editor.push_undo();
        self.editor.buffer_mut().replace_all(&new_text);
        self.editor.mark_content_dirty();
        let first_insert_col = insertion_columns.first().copied().unwrap_or(cursor_col);
        let last_insert_col = insertion_columns
            .last()
            .copied()
            .unwrap_or(first_insert_col);
        self.editor.jump_cursor(start_row, first_insert_col);
        self.editor.set_mark('[', (start_row, first_insert_col));
        self.editor.set_mark(
            ']',
            (
                start_row + segments.len().saturating_sub(1),
                last_insert_col,
            ),
        );
        let _ = self.editor.take_content_edits();
        let _ = self.editor.take_content_reset();
        self.modified_since_save = true;
        Some(vec![
            VimEffect::Edited {
                edits: vec![TextEdit {
                    range: 0..old_text.len(),
                    new_text_len: new_text.len(),
                    new_text,
                }],
            },
            VimEffect::CursorMoved,
        ])
    }

    /// Enter Insert after a wrapper-owned range change without creating a
    /// second undo checkpoint. The range deletion already pushed the undo
    /// snapshot, so the following typed insertion belongs to that same Vim
    /// change transaction.
    fn enter_insert_after_change_noundo(&mut self) -> VimEffect {
        let (row, col) = self.cursor();
        let before_rope = Query::rope(self.editor.buffer());
        let state = self.hjkl_state_mut();
        state.insert_session = Some(HjklInsertSession {
            count: 1,
            row_min: row,
            row_max: row,
            before_rope,
            reason: HjklInsertReason::AfterChange,
            start_row: row,
            start_col: col,
        });
        state.mode = HjklInternalMode::Insert;
        state.current_mode = HjklVimMode::Insert;
        self.mode = Mode::Insert;
        VimEffect::ModeChanged(Mode::Insert)
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
        self.materialize_text()
    }

    fn materialize_text(&self) -> String {
        #[cfg(test)]
        self.full_materializations
            .set(self.full_materializations.get() + 1);
        self.editor.buffer().as_string()
    }

    #[cfg(test)]
    pub(crate) fn reset_work_counters(&self) {
        self.full_materializations.set(0);
        self.edit_replays.set(0);
    }

    #[cfg(test)]
    pub(crate) fn work_counters(&self) -> (usize, usize) {
        (self.full_materializations.get(), self.edit_replays.get())
    }

    /// Convert a source `(line, character-column)` into a byte offset using
    /// the buffer rope without materializing the document.
    pub(crate) fn byte_offset_for_position(&self, line: usize, column: usize) -> usize {
        let rope = self.editor.buffer().rope();
        let line = line.min(rope.len_lines().saturating_sub(1));
        let line_start_char = rope.line_to_char(line);
        let rope_line = rope.line(line);
        let mut content_chars = rope_line.len_chars();
        if content_chars > 0 && rope_line.char(content_chars - 1) == '\n' {
            content_chars -= 1;
            if content_chars > 0 && rope_line.char(content_chars - 1) == '\r' {
                content_chars -= 1;
            }
        }
        rope.char_to_byte(line_start_char + column.min(content_chars))
    }

    /// Convert a source byte offset into `(line, character-column)` using
    /// the buffer rope without materializing the document.
    pub(crate) fn position_for_byte_offset(&self, offset: usize) -> (usize, usize) {
        let rope = self.editor.buffer().rope();
        let offset = offset.min(rope.len_bytes());
        let line = rope.byte_to_line(offset);
        let line_start = rope.line_to_byte(line);
        let column = rope.byte_to_char(offset) - rope.byte_to_char(line_start);
        (line, column)
    }

    pub(crate) fn cursor_byte_offset(&self) -> usize {
        let (line, column) = self.cursor();
        self.byte_offset_for_position(line, column)
    }

    pub(crate) fn byte_before_is_newline(&self, offset: usize) -> bool {
        let rope = self.editor.buffer().rope();
        offset > 0 && offset <= rope.len_bytes() && rope.byte(offset - 1) == b'\n'
    }

    pub(crate) fn line_count(&self) -> usize {
        self.editor.buffer().rope().len_lines()
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

    /// Apply a renderer-projected character, line, or block selection.
    ///
    /// The projection contains only core-owned display/source types. Exact
    /// register metadata and all engine mutation stay inside this wrapper.
    pub(crate) fn apply_selection(
        &mut self,
        selection: RenderedSelection,
        operator: RangeOperator,
        register: Register,
    ) -> Vec<VimEffect> {
        if matches!(operator, RangeOperator::Indent | RangeOperator::Outdent) {
            let Some(first) = selection.source_ranges.first() else {
                return Vec::new();
            };
            let Some(last) = selection.source_ranges.last() else {
                return Vec::new();
            };
            return self.apply_line_range(first.start..last.end, operator, register);
        }
        if selection.source_ranges.is_empty() {
            return Vec::new();
        }

        if self.mode != Mode::Normal {
            let _ = self.handle_key(Self::plain_key(KeyCodeKind::Esc));
        }

        let old_text = self.text();
        let target = register.selector();
        let block_width = selection.block_width.unwrap_or(0);
        let payload = if selection.shape == SelectionShape::Block {
            selection
                .rows
                .iter()
                .map(|row| {
                    let mut text = row
                        .source_ranges
                        .iter()
                        .filter_map(|range| old_text.get(range.clone()))
                        .collect::<String>();
                    let selected_width = row.columns.end.saturating_sub(row.columns.start);
                    let raw_width = UnicodeWidthStr::width(text.as_str());
                    let padding = block_width.saturating_sub(selected_width.max(raw_width));
                    text.push_str(&" ".repeat(padding));
                    text
                })
                .collect::<Vec<_>>()
                .join("\n")
        } else {
            selection
                .source_ranges
                .iter()
                .filter_map(|range| old_text.get(range.clone()))
                .collect()
        };
        match operator {
            RangeOperator::Yank => {
                if selection.shape == SelectionShape::Block {
                    self.editor
                        .record_yank_block(payload.clone(), block_width, target);
                } else {
                    self.editor.record_yank(
                        payload.clone(),
                        selection.shape == SelectionShape::Line,
                        target,
                    );
                }
                let mut effects = vec![VimEffect::CursorMoved];
                if matches!(register, Register::System) {
                    effects.push(VimEffect::ClipboardYank(payload));
                }
                return effects;
            }
            RangeOperator::Delete | RangeOperator::Change => {
                if selection.shape == SelectionShape::Block {
                    self.editor
                        .record_delete_block(payload.clone(), block_width, target);
                } else {
                    self.editor.record_delete(
                        payload.clone(),
                        selection.shape == SelectionShape::Line,
                        target,
                    );
                }
            }
            RangeOperator::Indent | RangeOperator::Outdent => unreachable!(),
        }

        let earliest = selection.source_ranges[0].start.min(old_text.len());
        let mut new_text = old_text.clone();
        let mut ranges = selection.source_ranges;
        ranges.sort_by_key(|range| (range.start, range.end));
        for range in ranges.into_iter().rev() {
            let start = range.start.min(new_text.len());
            let end = range.end.min(new_text.len());
            if start < end {
                new_text.replace_range(start..end, "");
            }
        }

        {
            let _undo_group = self.editor.undo_group();
            self.editor.push_undo();
            self.editor.buffer_mut().replace_all(&new_text);
        }
        let _ = self.editor.take_content_edits();
        self.modified_since_save = true;
        let cursor_offset = earliest.min(new_text.len());
        let row = new_text[..cursor_offset]
            .bytes()
            .filter(|byte| *byte == b'\n')
            .count();
        let line_start = new_text[..cursor_offset]
            .rfind('\n')
            .map_or(0, |newline| newline + 1);
        let column = new_text[line_start..cursor_offset].chars().count();
        self.jump_to(row, column);

        let mut effects = vec![
            VimEffect::Edited {
                edits: vec![TextEdit {
                    range: 0..old_text.len(),
                    new_text_len: new_text.len(),
                    new_text,
                }],
            },
            VimEffect::CursorMoved,
        ];
        if matches!(register, Register::System) {
            effects.push(VimEffect::ClipboardYank(payload));
        }
        if operator == RangeOperator::Change {
            effects.push(self.enter_insert_after_change_noundo());
        }
        effects
    }

    /// Apply a linewise source range through hjkl's VisualLine machinery.
    ///
    /// The caller supplies only core-owned byte coordinates. All engine
    /// modes, selections, registers, undo state, and edit notifications stay
    /// confined to this wrapper.
    pub(crate) fn apply_line_range(
        &mut self,
        range: Range<usize>,
        operator: RangeOperator,
        register: Register,
    ) -> Vec<VimEffect> {
        let text = self.text();
        let start = range.start.min(text.len());
        let end = range.end.min(text.len());
        if start >= end {
            return Vec::new();
        }

        if self.mode != Mode::Normal {
            let _ = self.handle_key(Self::plain_key(KeyCodeKind::Esc));
        }

        let start_row = text[..start].bytes().filter(|byte| *byte == b'\n').count();
        let last_byte = end.saturating_sub(1);
        let end_row = text[..last_byte]
            .bytes()
            .filter(|byte| *byte == b'\n')
            .count();
        self.jump_to(start_row, 0);

        let mut effects = self.handle_key(Self::plain_key(KeyCodeKind::Char('V')));
        for _ in start_row..end_row {
            effects.extend(self.handle_key(Self::plain_key(KeyCodeKind::Char('j'))));
        }
        if let Some(selector) = register.selector() {
            effects.extend(self.handle_key(Self::plain_key(KeyCodeKind::Char('"'))));
            effects.extend(self.handle_key(Self::plain_key(KeyCodeKind::Char(selector))));
        }
        let operator_key = match operator {
            RangeOperator::Yank => 'y',
            RangeOperator::Delete => 'd',
            RangeOperator::Change => 'c',
            RangeOperator::Indent => '>',
            RangeOperator::Outdent => '<',
        };
        effects.extend(self.handle_key(Self::plain_key(KeyCodeKind::Char(operator_key))));

        if matches!(operator, RangeOperator::Indent | RangeOperator::Outdent)
            && self.mode != Mode::Normal
        {
            effects.extend(self.handle_key(Self::plain_key(KeyCodeKind::Esc)));
        }
        effects
    }

    fn plain_key(kind: KeyCodeKind) -> KeyInput {
        KeyInput {
            code: KeyCode { kind },
            mods: Modifiers::default(),
        }
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
        if hjkl_edits.is_empty() {
            return Vec::new();
        }
        let text = self.materialize_text();
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

/// Return the display-cell boundary immediately before or after the cursor.
fn display_width_through_cursor(line: &str, cursor_col: usize, include_cursor: bool) -> usize {
    let character_count = cursor_col.saturating_add(usize::from(include_cursor));
    line.chars()
        .take(character_count)
        .map(|character| UnicodeWidthChar::width(character).unwrap_or(0))
        .sum()
}

/// Locate a display-cell column in a source line.
///
/// The returned tuple is `(character_index, virtual_space_padding)`. A target
/// beyond the line is represented by padding; a target inside a wide glyph is
/// rounded to the glyph's trailing boundary because a terminal glyph cannot be
/// split into cells.
fn char_index_at_display_column(line: &str, target: usize) -> (usize, usize) {
    let mut display_col = 0;
    let mut character_count = 0;
    for (index, character) in line.chars().enumerate() {
        let character_width = UnicodeWidthChar::width(character).unwrap_or(0);
        if display_col == target && character_width > 0 {
            return (index, 0);
        }
        display_col += character_width;
        character_count = index + 1;
        if display_col > target {
            return (character_count, 0);
        }
    }
    (character_count, target.saturating_sub(display_col))
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
    fn vim_cursor_keys_do_not_materialize_or_validate_content() {
        let text = "αβγ line\n".repeat(20_000);
        let mut vim = VimCore::new(&text);
        vim.handle_key(key('i'));
        vim.full_materializations.set(0);
        vim.edit_replays.set(0);

        for kind in [KeyCodeKind::Down, KeyCodeKind::Up, KeyCodeKind::Esc] {
            vim.handle_key(special_key(kind));
        }

        assert_eq!(vim.full_materializations.get(), 0);
        assert_eq!(vim.edit_replays.get(), 0);
        assert_eq!(vim.text(), text);
    }

    #[test]
    fn vim_edit_validation_falls_back_only_on_reset_or_invalid_batch() {
        let before = View::from_str("aé\nz").rope();
        let valid = vec![
            TextEdit {
                range: 1..3,
                new_text_len: 1,
                new_text: "x".to_string(),
            },
            TextEdit {
                range: 2..2,
                new_text_len: 4,
                new_text: "🙂".to_string(),
            },
        ];
        let after = View::from_str("ax🙂\nz").rope();
        let fallback_calls = Cell::new(0);
        let validated = validate_content_edits(
            valid.clone(),
            before.len_bytes(),
            false,
            |candidate| edits_reproduce!(&before, candidate, &after),
            || {
                fallback_calls.set(fallback_calls.get() + 1);
                "fallback".to_string()
            },
        );
        assert_eq!(validated, valid);
        assert_eq!(fallback_calls.get(), 0);

        let malformed_boundary = vec![TextEdit {
            range: 2..3,
            new_text_len: 2,
            new_text: "é".to_string(),
        }];
        let rounded_after = View::from_str("aé\nz").rope();
        assert!(!edits_reproduce!(
            &before,
            &malformed_boundary,
            &rounded_after
        ));
        let wrong_length = vec![TextEdit {
            range: 0..1,
            new_text_len: 9,
            new_text: "x".to_string(),
        }];
        for invalid in [malformed_boundary, wrong_length] {
            let validated = validate_content_edits(
                invalid,
                before.len_bytes(),
                false,
                |candidate| edits_reproduce!(&before, candidate, &after),
                || {
                    fallback_calls.set(fallback_calls.get() + 1);
                    "fallback".to_string()
                },
            );
            assert_eq!(
                validated,
                [TextEdit {
                    range: 0..before.len_bytes(),
                    new_text_len: 8,
                    new_text: "fallback".to_string(),
                }]
            );
        }

        let reset = validate_content_edits(
            valid,
            before.len_bytes(),
            true,
            |_| panic!("reset must not replay the incremental batch"),
            || {
                fallback_calls.set(fallback_calls.get() + 1);
                "reset text".to_string()
            },
        );
        assert_eq!(
            reset,
            [TextEdit {
                range: 0..before.len_bytes(),
                new_text_len: 10,
                new_text: "reset text".to_string(),
            }]
        );
        assert_eq!(fallback_calls.get(), 3);
    }

    #[test]
    fn rope_coordinates_round_trip_unicode_and_line_boundaries() {
        let vim = VimCore::new("aé🙂\r\ncombining e\u{301}\nlast");
        for position in [(0, 0), (0, 1), (0, 2), (0, 3), (1, 11), (2, 4)] {
            let offset = vim.byte_offset_for_position(position.0, position.1);
            assert_eq!(vim.position_for_byte_offset(offset), position);
        }
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

/// Conformance for the private hjkl wrapper surface retained by plan §6.2.
///
/// These tests intentionally live in `vim.rs`: exercising the wrapped
/// Normal/Visual machinery anywhere else would leak engine concepts across
/// the single-wrapper boundary.
#[cfg(test)]
mod private_conformance_tests {
    use super::*;

    const REQUIRED_ROWS: &[&str] = &[
        "V-M1", "V-M2", "V-M3", "V-M4", "V-M5", "V-M6", "V-M7", "V-M8", "V-M9", "V-S1", "V-S2",
        "V-S3", "V-S4", "V-S5", "V-E1", "V-E2", "V-E3", "V-E4", "V-E5", "V-E6", "V-E7", "V-E8",
        "V-O1", "V-O2", "V-O3", "V-O4", "V-O5", "V-T1", "V-T2", "V-T3", "V-T4", "V-T5", "V-R1",
        "V-R2", "V-R3", "V-V1", "V-V2", "V-V3", "V-V4", "V-V5",
    ];

    type Case = (&'static str, fn());
    const CASES: &[Case] = &[
        ("V-M1", v_m1_char_line_and_arrow_motions),
        ("V-M2", v_m2_word_motions),
        ("V-M3", v_m3_big_word_motions),
        ("V-M4", v_m4_line_boundary_motions),
        ("V-M5", v_m5_file_and_counted_line_motions),
        ("V-M6", v_m6_half_page_motions),
        ("V-M7", v_m7_full_page_motions),
        ("V-M8", v_m8_paragraph_motions),
        ("V-M9", v_m9_matching_pair_motion),
        ("V-S1", v_s1_forward_search),
        ("V-S2", v_s2_backward_search),
        ("V-S3", v_s3_repeat_search_both_directions),
        ("V-S4", v_s4_clear_search_highlight),
        ("V-S5", v_s5_search_operator_target),
        ("V-E1", v_e1_delete_under_and_before_cursor),
        ("V-E2", v_e2_replace_character),
        ("V-E3", v_e3_toggle_case),
        ("V-E4", v_e4_join_lines),
        ("V-E5", v_e5_delete_and_change_to_eol),
        ("V-E6", v_e6_substitute_character_and_line),
        ("V-E7", v_e7_undo_redo),
        ("V-E8", v_e8_repeat_change),
        ("V-O1", v_o1_delete_motion_and_line),
        ("V-O2", v_o2_change_motion_and_line),
        ("V-O3", v_o3_yank_motion_and_line),
        ("V-O4", v_o4_indent_and_outdent),
        ("V-O5", v_o5_case_operators),
        ("V-T1", v_t1_word_text_objects),
        ("V-T2", v_t2_big_word_text_objects),
        ("V-T3", v_t3_quote_text_objects),
        ("V-T4", v_t4_pair_text_objects),
        ("V-T5", v_t5_paragraph_text_objects),
        ("V-R1", v_r1_put_before_and_after),
        ("V-R2", v_r2_unnamed_register),
        ("V-R3", v_r3_system_clipboard_register),
        ("V-V1", v_v1_visual_motion_extends_selection),
        ("V-V2", v_v2_visual_operators),
        ("V-V3", v_v3_visual_line_indent),
        ("V-V4", v_v4_swap_visual_endpoints),
        ("V-V5", v_v5_visual_block_insert),
    ];

    fn key(ch: char) -> KeyInput {
        KeyInput {
            code: KeyCode {
                kind: KeyCodeKind::Char(ch),
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

    fn ctrl(ch: char) -> KeyInput {
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

    fn feed(vim: &mut VimCore, keys: &str) -> Vec<VimEffect> {
        keys.chars()
            .flat_map(|ch| vim.handle_key(key(ch)))
            .collect()
    }

    fn search(vim: &mut VimCore, prefix: char, pattern: &str) -> Vec<VimEffect> {
        vim.handle_key(key(prefix));
        feed(vim, pattern);
        vim.handle_key(special(KeyCodeKind::Enter))
    }

    #[test]
    fn private_manifest_covers_every_retained_wrapper_row_exactly_once() {
        let required: std::collections::BTreeSet<_> = REQUIRED_ROWS.iter().copied().collect();
        let covered: std::collections::BTreeSet<_> = CASES.iter().map(|(row, _)| *row).collect();
        assert_eq!(covered, required);
        assert_eq!(CASES.len(), required.len(), "duplicate private row mapping");
    }

    #[test]
    fn v_m1_char_line_and_arrow_motions() {
        let mut vim = VimCore::new("abcd\nxy\n1234");
        feed(&mut vim, "lljkh");
        assert_eq!(vim.cursor(), (0, 1));
        vim.handle_key(special(KeyCodeKind::Right));
        vim.handle_key(special(KeyCodeKind::Down));
        vim.handle_key(special(KeyCodeKind::Up));
        vim.handle_key(special(KeyCodeKind::Left));
        assert_eq!(vim.cursor(), (0, 1));
    }

    #[test]
    fn v_m2_word_motions() {
        let mut vim = VimCore::new("one, two three");
        feed(&mut vim, "we");
        assert!(vim.cursor().1 >= 4);
        feed(&mut vim, "b");
        assert!(vim.cursor().1 < 6);
    }

    #[test]
    fn v_m3_big_word_motions() {
        let mut vim = VimCore::new("one,two three");
        feed(&mut vim, "W");
        assert_eq!(vim.cursor().1, 8);
        feed(&mut vim, "BE");
        assert_eq!(vim.cursor().1, 6);
    }

    #[test]
    fn v_m4_line_boundary_motions() {
        let mut vim = VimCore::new("  alpha beta");
        feed(&mut vim, "$0^");
        assert_eq!(vim.cursor(), (0, 2));
    }

    #[test]
    fn v_m5_file_and_counted_line_motions() {
        let mut vim = VimCore::new("one\ntwo\nthree\nfour");
        feed(&mut vim, "Ggg3G2gg");
        assert_eq!(vim.cursor().0, 1);
    }

    #[test]
    fn v_m6_half_page_motions() {
        let mut vim = VimCore::new("1\n2\n3\n4\n5\n6\n7\n8\n9\n10");
        vim.set_viewport(0, 6);
        vim.handle_key(ctrl('d'));
        assert!(vim.cursor().0 > 0);
        vim.handle_key(ctrl('u'));
        assert_eq!(vim.cursor().0, 0);
    }

    #[test]
    fn v_m7_full_page_motions() {
        let text = (1..=40)
            .map(|line| format!("line {line}"))
            .collect::<Vec<_>>()
            .join("\n");
        let mut vim = VimCore::new(&text);
        vim.set_viewport(0, 10);
        vim.handle_key(ctrl('f'));
        assert_eq!(vim.cursor().0, 8);
        feed(&mut vim, "G");
        vim.handle_key(ctrl('b'));
        assert_eq!(vim.cursor().0, 31);
    }

    #[test]
    fn v_m8_paragraph_motions() {
        let mut vim = VimCore::new("one\n\ntwo\n\nthree");
        feed(&mut vim, "}}");
        assert_eq!(vim.cursor().0, 4);
        feed(&mut vim, "{");
        assert!(vim.cursor().0 < 4);
    }

    #[test]
    fn v_m9_matching_pair_motion() {
        let mut vim = VimCore::new("(alpha [beta])");
        feed(&mut vim, "%");
        assert_eq!(vim.cursor().1, 13);
        feed(&mut vim, "%");
        assert_eq!(vim.cursor().1, 0);
    }

    #[test]
    fn v_s1_forward_search() {
        let mut vim = VimCore::new("one target\ntarget");
        search(&mut vim, '/', "target");
        assert_eq!(vim.cursor(), (0, 4));
    }

    #[test]
    fn v_s2_backward_search() {
        let mut vim = VimCore::new("one target\ntarget");
        feed(&mut vim, "G$");
        search(&mut vim, '?', "target");
        assert_eq!(vim.cursor(), (1, 0));
    }

    #[test]
    fn v_s3_repeat_search_both_directions() {
        let mut vim = VimCore::new("target x target x target");
        search(&mut vim, '/', "target");
        feed(&mut vim, "n");
        let forward = vim.cursor();
        feed(&mut vim, "N");
        assert!(vim.cursor().1 < forward.1);
    }

    #[test]
    fn v_s4_clear_search_highlight() {
        let mut vim = VimCore::new("target target");
        search(&mut vim, '/', "target");
        assert!(!vim.search_matches_for_line(0).is_empty());
        vim.clear_search_highlight();
        assert!(vim.search_matches_for_line(0).is_empty());
    }

    #[test]
    fn v_s5_search_operator_target() {
        let mut vim = VimCore::new("delete until target remains");
        feed(&mut vim, "d/target");
        vim.handle_key(special(KeyCodeKind::Enter));
        assert_eq!(vim.text(), "target remains");
    }

    #[test]
    fn v_e1_delete_under_and_before_cursor() {
        let mut under = VimCore::new("abc");
        feed(&mut under, "x");
        assert_eq!(under.text(), "bc");
        let mut before = VimCore::new("abc");
        feed(&mut before, "lX");
        assert_eq!(before.text(), "bc");
    }

    #[test]
    fn v_e2_replace_character() {
        let mut vim = VimCore::new("abc");
        feed(&mut vim, "rZ");
        assert_eq!(vim.text(), "Zbc");
    }

    #[test]
    fn v_e3_toggle_case() {
        let mut vim = VimCore::new("aB");
        feed(&mut vim, "~~");
        assert_eq!(vim.text(), "Ab");
    }

    #[test]
    fn v_e4_join_lines() {
        let mut vim = VimCore::new("one\n  two");
        feed(&mut vim, "J");
        assert_eq!(vim.text(), "one two");
    }

    #[test]
    fn v_e5_delete_and_change_to_eol() {
        let mut delete = VimCore::new("one two");
        feed(&mut delete, "wD");
        assert_eq!(delete.text(), "one ");
        let mut change = VimCore::new("one two");
        feed(&mut change, "wCX");
        change.handle_key(special(KeyCodeKind::Esc));
        assert_eq!(change.text(), "one X");
    }

    #[test]
    fn v_e6_substitute_character_and_line() {
        let mut character = VimCore::new("one");
        feed(&mut character, "sX");
        character.handle_key(special(KeyCodeKind::Esc));
        assert_eq!(character.text(), "Xne");
        let mut line = VimCore::new("  one\ntwo");
        feed(&mut line, "SX");
        line.handle_key(special(KeyCodeKind::Esc));
        assert_eq!(line.text(), "  X\ntwo");
    }

    #[test]
    fn v_e7_undo_redo() {
        let mut vim = VimCore::new("abc");
        feed(&mut vim, "x");
        assert_eq!(vim.text(), "bc");
        feed(&mut vim, "u");
        assert_eq!(vim.text(), "abc");
        vim.handle_key(ctrl('r'));
        assert_eq!(vim.text(), "bc");
    }

    #[test]
    fn v_e8_repeat_change() {
        let mut vim = VimCore::new("abcd");
        feed(&mut vim, "xl.");
        assert_eq!(vim.text(), "bd");
    }

    #[test]
    fn v_o1_delete_motion_and_line() {
        let mut motion = VimCore::new("one two");
        feed(&mut motion, "dw");
        assert_eq!(motion.text(), "two");
        let mut line = VimCore::new("one\ntwo");
        feed(&mut line, "dd");
        assert_eq!(line.text(), "two");
    }

    #[test]
    fn v_o2_change_motion_and_line() {
        let mut motion = VimCore::new("one two");
        feed(&mut motion, "cwX");
        motion.handle_key(special(KeyCodeKind::Esc));
        assert_eq!(motion.text(), "X two");
        let mut line = VimCore::new("  one\ntwo");
        feed(&mut line, "ccX");
        line.handle_key(special(KeyCodeKind::Esc));
        assert_eq!(line.text(), "  X\ntwo");
    }

    #[test]
    fn v_o3_yank_motion_and_line() {
        let mut motion = VimCore::new("one two");
        feed(&mut motion, "ywp");
        assert_eq!(motion.text(), "oone ne two");
        let mut line = VimCore::new("one\ntwo");
        feed(&mut line, "yyp");
        assert_eq!(line.text(), "one\none\ntwo");
    }

    #[test]
    fn v_o4_indent_and_outdent() {
        let mut vim = VimCore::new("one\ntwo");
        feed(&mut vim, ">>");
        assert_eq!(vim.text(), "    one\ntwo");
        feed(&mut vim, "<<");
        assert_eq!(vim.text(), "one\ntwo");
    }

    #[test]
    fn v_o5_case_operators() {
        let mut lower = VimCore::new("ONE two");
        feed(&mut lower, "guw");
        assert_eq!(lower.text(), "one two");
        let mut upper = VimCore::new("one two");
        feed(&mut upper, "gUw");
        assert_eq!(upper.text(), "ONE two");
    }

    #[test]
    fn v_t1_word_text_objects() {
        let mut inner = VimCore::new("one two");
        feed(&mut inner, "diw");
        assert_eq!(inner.text(), " two");
        let mut around = VimCore::new("one two");
        feed(&mut around, "daw");
        assert_eq!(around.text(), "two");
    }

    #[test]
    fn v_t2_big_word_text_objects() {
        let mut inner = VimCore::new("one,two three");
        feed(&mut inner, "diW");
        assert_eq!(inner.text(), " three");
        let mut around = VimCore::new("one,two three");
        feed(&mut around, "daW");
        assert_eq!(around.text(), "three");
    }

    #[test]
    fn v_t3_quote_text_objects() {
        let mut inner = VimCore::new("say \"hello\" now");
        feed(&mut inner, "f\"ldi\"");
        assert_eq!(inner.text(), "say \"\" now");
        let mut around = VimCore::new("say 'hello' now");
        feed(&mut around, "f'lda'");
        assert_eq!(around.text(), "say now");
    }

    #[test]
    fn v_t4_pair_text_objects() {
        let mut paren = VimCore::new("x (alpha) y");
        feed(&mut paren, "f(di(");
        assert_eq!(paren.text(), "x () y");
        let mut bracket = VimCore::new("x [alpha] y");
        feed(&mut bracket, "f[da[");
        assert_eq!(bracket.text(), "x  y");
    }

    #[test]
    fn v_t5_paragraph_text_objects() {
        let mut inner = VimCore::new("one\ntwo\n\nthree");
        feed(&mut inner, "dip");
        assert!(!inner.text().contains("one"));
        let mut around = VimCore::new("one\ntwo\n\nthree");
        feed(&mut around, "dap");
        assert_eq!(around.text(), "three");
    }

    #[test]
    fn v_r1_put_before_and_after() {
        let mut after = VimCore::new("one\ntwo");
        feed(&mut after, "yyp");
        assert_eq!(after.text(), "one\none\ntwo");
        let mut before = VimCore::new("one\ntwo");
        feed(&mut before, "yyjP");
        assert_eq!(before.text(), "one\none\ntwo");
    }

    #[test]
    fn v_r2_unnamed_register() {
        let mut vim = VimCore::new("one\ntwo");
        feed(&mut vim, "ddp");
        assert_eq!(vim.text(), "two\none");
    }

    #[test]
    fn v_r3_system_clipboard_register() {
        let mut vim = VimCore::new("one\ntwo");
        let effects = feed(&mut vim, "\"+yy");
        assert!(effects
            .iter()
            .any(|effect| matches!(effect, VimEffect::ClipboardYank(text) if text == "one\n")));
    }

    #[test]
    fn v_v1_visual_motion_extends_selection() {
        let mut vim = VimCore::new("one two");
        feed(&mut vim, "vww");
        assert_eq!(vim.mode(), Mode::Visual);
        assert!(vim.editor.buffer_selection().is_some());
    }

    #[test]
    fn v_v2_visual_operators() {
        let mut delete = VimCore::new("one two");
        feed(&mut delete, "vwd");
        assert_eq!(delete.mode(), Mode::Normal);
        assert_ne!(delete.text(), "one two");
        let mut yank = VimCore::new("one two");
        feed(&mut yank, "vwyP");
        assert!(yank.text().len() > "one two".len());
        let mut change = VimCore::new("one two");
        feed(&mut change, "vwcX");
        change.handle_key(special(KeyCodeKind::Esc));
        assert!(change.text().contains('X'));
    }

    #[test]
    fn v_v3_visual_line_indent() {
        let mut vim = VimCore::new("one\ntwo");
        feed(&mut vim, "Vj>");
        assert_eq!(vim.text(), "    one\n    two");
    }

    #[test]
    fn v_v4_swap_visual_endpoints() {
        let mut vim = VimCore::new("one two");
        feed(&mut vim, "vww");
        let active = vim.cursor();
        feed(&mut vim, "o");
        assert_ne!(vim.cursor(), active);
        assert_eq!(vim.mode(), Mode::Visual);
    }

    #[test]
    fn v_v5_visual_block_insert() {
        let mut vim = VimCore::new("aa\nbb\ncc");
        vim.handle_key(ctrl('v'));
        feed(&mut vim, "jjIX");
        vim.handle_key(special(KeyCodeKind::Esc));
        assert_eq!(vim.text(), "Xaa\nXbb\nXcc");
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
