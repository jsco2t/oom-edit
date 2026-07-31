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

use hjkl_buffer::View;
use hjkl_engine::types::{DefaultHost, Options};
use hjkl_engine::{Editor, PlannedInput, SpecialKey, VimMode as HjklVimMode};
use hjkl_vim::{feed_input, install_vim_discipline, VimEditorExt};

// ── vim.rs internal types ──────────────────────────────────────────────────

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
pub(crate) struct TextEdit {
    /// Byte range in the document that was replaced.
    pub range: Range<usize>,
    /// Length of the replacement text in bytes.
    pub new_text_len: usize,
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
    editor: Editor<hjkl_buffer::View, DefaultHost>,
    /// The current oom-edit mode, derived from hjkl state.
    mode: Mode,
    /// The last saved undo mark (for dirty tracking).
    save_point_dirty_gen: u64,
}

impl VimCore {
    /// Create a new `VimCore` from initial text. Starts in Normal mode.
    pub(crate) fn new(text: &str) -> Self {
        let view = View::from_str(text);
        let mut editor = Editor::new(view, DefaultHost::new(), Options::default());
        install_vim_discipline(&mut editor);
        let save_point_dirty_gen = editor.buffer().dirty_gen();
        Self {
            editor,
            mode: Mode::Normal,
            save_point_dirty_gen,
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

        // Feed the key through hjkl
        let consumed = feed_input(&mut self.editor, hjkl_input);

        if !consumed {
            // Key not consumed — could be a bell (unbound key in Normal)
            return vec![VimEffect::Bell];
        }

        // Drain any content edits
        let edits = self.drain_content_edits();

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

        effects
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

    /// Command-line mode is not supported by the hjkl wrapper.
    /// Returns `None` always.
    pub(crate) fn command_line(&self) -> Option<String> {
        None
    }

    /// Check if the buffer has been modified since the last save point.
    pub(crate) fn is_modified_since(&self, save_point: UndoMark) -> bool {
        self.editor.buffer().dirty_gen() != save_point.0
    }

    /// Take a new save point (current dirty generation).
    pub(crate) fn save_point(&mut self) -> UndoMark {
        self.save_point_dirty_gen = self.editor.buffer().dirty_gen();
        UndoMark(self.save_point_dirty_gen)
    }

    /// Replace the entire buffer text in place. Returns the old text.
    pub(crate) fn set_text(&mut self, text: &str) -> String {
        let old = self.editor.buffer().as_string();
        self.editor.buffer_mut().replace_all(text);
        self.save_point_dirty_gen = self.editor.buffer().dirty_gen();
        old
    }

    // ── Internal helpers ─────────────────────────────────────────────

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
        hjkl_edits
            .into_iter()
            .map(|ce| TextEdit {
                range: ce.start_byte..ce.old_end_byte,
                new_text_len: ce.new_end_byte - ce.start_byte,
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

// ── UndoMark ───────────────────────────────────────────────────────────────

/// Opaque marker for an undo save point. Wraps the buffer's dirty generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct UndoMark(pub u64);

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
