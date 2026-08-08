//! Command palette (FR-6.6/6.7).
//!
//! A fuzzy-filtered, executable command palette with two sections:
//! - **App commands** — from the registry, dimmed when context-disabled
//! - **Vim reference** — static, non-executable, rendered muted
//!
//! Floor geometry: 40×12. Close with Esc; execute with Enter.

use ratatui::{
    style::Style,
    text::Line,
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use crate::command::{Command, Contexts, Keymap};
use crate::theme::{Theme, Tier};

// ── Vim reference table ─────────────────────────────────────────────────────

/// Static Vim reference entries for the palette's non-executable section.
///
/// Transcribed from plan §6.2 — every row of every table, exactly.
/// Format: `(keys, description, row-id)`.
pub static VIM_REFERENCE: &[(&str, &str, &str)] = &[
    // ── Motions ──────────────────────────────────────────────────────────
    ("h, ←", "Char/line movement left. No line wrap.", "V-M1"),
    (
        "j, ↓",
        "Line down, preserves desired column on short lines.",
        "V-M1",
    ),
    (
        "k, ↑",
        "Line up, preserves desired column on short lines.",
        "V-M1",
    ),
    ("l, →", "Char/line movement right. No line wrap.", "V-M1"),
    (
        "w",
        "Word forward (Vim word rules: punctuation runs are words).",
        "V-M2",
    ),
    ("b", "Word backward.", "V-M2"),
    ("e", "End of word.", "V-M2"),
    ("W", "WORD forward (whitespace-delimited).", "V-M3"),
    ("B", "WORD backward.", "V-M3"),
    ("E", "End of WORD.", "V-M3"),
    ("0", "Hard beginning-of-line.", "V-M4"),
    ("^", "First non-blank character of line.", "V-M4"),
    ("$", "End-of-line (last character).", "V-M4"),
    ("gg", "First line of document.", "V-M5"),
    (
        "G",
        "Last line of document; {count}G jumps to line.",
        "V-M5",
    ),
    ("<C-d>", "Half-page down.", "V-M6"),
    ("<C-u>", "Half-page up.", "V-M6"),
    ("<C-f>", "Full-page down.", "V-M7"),
    ("<C-b>", "Full-page up.", "V-M7"),
    (
        "{",
        "Paragraph backward (blank-line-delimited block).",
        "V-M8",
    ),
    ("}", "Paragraph forward.", "V-M8"),
    ("%", "Jump between matching (){}[] pairs.", "V-M9"),
    // ── Search ───────────────────────────────────────────────────────────
    (
        "/pattern⏎",
        "Forward search (regex); moves to next match.",
        "V-S1",
    ),
    ("?pattern⏎", "Backward search.", "V-S2"),
    (
        "n",
        "Repeat last search in same direction, with wraparound.",
        "V-S3",
    ),
    ("N", "Repeat last search in opposite direction.", "V-S3"),
    (":noh", "Clear search-match highlighting.", "V-S4"),
    // ── Editing ──────────────────────────────────────────────────────────
    (
        "x",
        "Delete char under cursor (into unnamed register).",
        "V-E1",
    ),
    ("X", "Delete char before cursor.", "V-E1"),
    ("r{char}", "Replace char under cursor.", "V-E2"),
    ("~", "Toggle case of char under cursor, advance.", "V-E3"),
    ("J", "Join line below with one space.", "V-E4"),
    ("D", "Delete to end-of-line.", "V-E5"),
    ("C", "Change to end-of-line.", "V-E5"),
    ("s", "Substitute char (delete + Insert).", "V-E6"),
    ("S", "Change whole line (delete line + Insert).", "V-E6"),
    (
        "u",
        "Undo (one step per Insert session / operator / ex command).",
        "V-E7",
    ),
    ("<C-r>", "Redo.", "V-E7"),
    (".", "Repeat last change.", "V-E8"),
    // ── Operators ────────────────────────────────────────────────────────
    ("d{motion}", "Delete — operator.", "V-O1"),
    ("dd", "Delete linewise.", "V-O1"),
    ("c{motion}", "Change (delete + Insert) — operator.", "V-O2"),
    ("cc", "Change whole line (preserves indent).", "V-O2"),
    ("y{motion}", "Yank — operator.", "V-O3"),
    ("yy", "Yank linewise.", "V-O3"),
    (">{motion}", "Indent by one shift width (4 spaces).", "V-O4"),
    ("<{motion}", "Dedent by one shift width.", "V-O4"),
    (">>", "Indent current line.", "V-O4"),
    ("<<", "Dedent current line.", "V-O4"),
    ("gu{motion}", "Lowercase.", "V-O5"),
    ("gU{motion}", "Uppercase.", "V-O5"),
    // ── Text objects ─────────────────────────────────────────────────────
    ("iw", "Inner word.", "V-T1"),
    ("aw", "Around word.", "V-T1"),
    ("iW", "Inner WORD (whitespace-delimited).", "V-T2"),
    ("aW", "Around WORD.", "V-T2"),
    ("i\"/a\"", "Inner / around double-quoted string.", "V-T3"),
    ("i'/a'", "Inner / around single-quoted string.", "V-T3"),
    ("i`/a`", "Inner / around backtick-delimited string.", "V-T3"),
    ("i(/a(", "Inner / around parens (+ ib for braces).", "V-T4"),
    ("i[/a[", "Inner / around brackets.", "V-T4"),
    ("i{/a{", "Inner / around braces (+ iB).", "V-T4"),
    ("i</a<", "Inner / around angle brackets.", "V-T4"),
    ("ip", "Inner paragraph.", "V-T5"),
    ("ap", "Around paragraph.", "V-T5"),
    // ── Registers ────────────────────────────────────────────────────────
    (
        "p",
        "Put after cursor; linewise yanks put linewise.",
        "V-R1",
    ),
    ("P", "Put before cursor.", "V-R1"),
    (
        "\"",
        "Unnamed register — default for all deletes/yanks.",
        "V-R2",
    ),
    ("\"+y", "Yank to system-clipboard register.", "V-R3"),
    ("\"+p", "Put from system-clipboard register.", "V-R3"),
    // ── Visual mode ──────────────────────────────────────────────────────
    ("v", "Characterwise visual mode.", "V-V1"),
    ("V", "Linewise visual mode.", "V-V1"),
    ("<C-v>", "Block visual mode.", "V-V1"),
    (
        "d/x/c/y",
        "Apply operator to selection; selection collapses.",
        "V-V2",
    ),
    ("><", "Indent/dedent selection (linewise).", "V-V3"),
    ("o", "Swap cursor and anchor.", "V-V4"),
    // ── Ex commands ──────────────────────────────────────────────────────
    (":w", "Save (atomic).", "V-X1"),
    (
        ":w {path}",
        "Save a copy to path without retargeting buffer.",
        "V-X1",
    ),
    (":q", "Quit; refuses if dirty.", "V-X2"),
    (":q!", "Quit; discards changes.", "V-X2"),
    (":wq", "Save then quit.", "V-X3"),
    (":x", "Save then quit (if changed).", "V-X3"),
    (
        ":e {path}",
        "Open file; refuses if dirty without !.",
        "V-X4",
    ),
    (":e!", "Reload current file from disk.", "V-X4"),
    (
        ":saveas {path}",
        "Save to path and retarget buffer.",
        "V-X5",
    ),
    (":{number}", "Jump to line.", "V-X6"),
    (":s/pat/rep/", "Substitute on current line.", "V-X7"),
    (":s/pat/rep/g", "Substitute all on current line.", "V-X7"),
    (":%s/pat/rep/g", "Substitute all in document.", "V-X7"),
    (":noh", "Clear search-match highlighting.", "V-X8"),
    (":view", "Enter View mode.", "V-X8"),
    (":help", "Open help / command palette.", "V-X8"),
];

// ── Palette state ───────────────────────────────────────────────────────────

/// The command palette state machine.
#[derive(Debug, Default)]
pub struct PaletteState {
    /// Current filter text (case-insensitive).
    filter: String,
    /// Selected row index within the filtered+sectioned list.
    selected: usize,
    /// Whether the last Enter was on a Vim reference entry (non-executable).
    #[allow(dead_code)]
    last_was_reference: bool,
}

impl PaletteState {
    /// Get the current filter text (test-only).
    #[cfg(test)]
    pub fn filter_text(&self) -> &str {
        &self.filter
    }
}

/// A single row in the palette.
#[derive(Debug)]
pub(crate) enum PaletteRow {
    /// An executable app command.
    Command {
        id: Command,
        name: String,
        desc: String,
        keys: String,
        disabled: bool,
    },
    /// A non-executable Vim reference entry.
    Reference {
        keys: String,
        desc: String,
        row_id: String,
    },
}

impl PaletteState {
    /// Build the full row list from the registry and Vim reference table.
    pub(crate) fn build_rows(&self, ctx: Contexts, km: &Keymap) -> Vec<PaletteRow> {
        let mut rows = Vec::new();

        // App commands section.
        for spec in crate::command::COMMANDS {
            let enabled = spec.contexts.contains(ctx);
            let keys = km
                .rendered_keys(spec.id)
                .unwrap_or_else(|| "(no binding)".to_string());

            rows.push(PaletteRow::Command {
                id: spec.id,
                name: spec.name.to_string(),
                desc: spec.desc.to_string(),
                keys,
                disabled: !enabled,
            });
        }

        // Vim reference section.
        for (keys, desc, row_id) in VIM_REFERENCE {
            rows.push(PaletteRow::Reference {
                keys: keys.to_string(),
                desc: desc.to_string(),
                row_id: row_id.to_string(),
            });
        }

        rows
    }

    /// Filter rows by the current filter text using fuzzy subsequence matching.
    fn filter_rows(&self, rows: &[PaletteRow]) -> Vec<usize> {
        if self.filter.is_empty() {
            return (0..rows.len()).collect();
        }

        let lower = self.filter.to_lowercase();
        let mut indices = Vec::new();

        for (i, row) in rows.iter().enumerate() {
            let searchable = match row {
                PaletteRow::Command {
                    name, desc, keys, ..
                } => format!("{} {} {}", name, desc, keys).to_lowercase(),
                PaletteRow::Reference { keys, desc, row_id } => {
                    format!("{} {} {}", keys, desc, row_id).to_lowercase()
                }
            };

            if fuzzy_match(&lower, &searchable) {
                indices.push(i);
            }
        }

        indices
    }

    /// Get the visible (filtered) row at the selected index.
    #[allow(dead_code)]
    fn visible_row<'a>(&'a self, rows: &'a [PaletteRow]) -> Option<&'a PaletteRow> {
        let visible = self.filter_rows(rows);
        if self.selected < visible.len() {
            let i = visible[self.selected];
            return rows.get(i);
        }
        None
    }

    /// Handle a key event. Returns true if consumed.
    pub fn handle_key(&mut self, key: &oom_edit_core::session::KeyInput) -> bool {
        use oom_edit_core::session::KeyCodeKind;

        let code = &key.code.kind;

        match code {
            KeyCodeKind::Esc => {
                // Esc closes the palette.
                return false; // pass through to caller
            }
            KeyCodeKind::Char('c') if key.mods.ctrl => {
                // Ctrl-C also closes.
                return false;
            }
            KeyCodeKind::Enter => {
                // Enter executes or marks reference — pass through to caller
                // which handles the execution logic.
                return false;
            }
            KeyCodeKind::Up | KeyCodeKind::BackTab => {
                self.selected = self.selected.saturating_sub(1);
                return true;
            }
            KeyCodeKind::Down | KeyCodeKind::Tab => {
                let rows = self.build_rows(Contexts::ALL, &Keymap::default());
                let visible_count = self.filter_rows(&rows).len();
                if self.selected.saturating_add(1) < visible_count {
                    self.selected += 1;
                }
                return true;
            }
            KeyCodeKind::Char(c) if !key.mods.ctrl && !key.mods.alt => {
                // Append to filter.
                self.filter.push(*c);
                self.selected = 0;
                return true;
            }
            KeyCodeKind::Backspace => {
                self.filter.pop();
                self.selected = 0;
                return true;
            }
            _ => {}
        }

        false
    }

    /// Render the palette.
    pub fn render(&self, frame: &mut Frame<'_>, theme: &Theme, tier: Tier) {
        let area = centered_area(40, 12, frame.area());
        let block = Block::default()
            .borders(Borders::ALL)
            .title(" Command Palette ");

        frame.render_widget(block.clone(), area);

        // Build rows.
        let km = Keymap::default();
        let all_rows = self.build_rows(Contexts::ALL, &km);
        let visible_indices = self.filter_rows(&all_rows);

        // Build display lines.
        let mut lines: Vec<Line<'_>> = Vec::new();

        // Filter input line.
        let filter_text = format!("> {}", self.filter);
        lines.push(Line::raw(filter_text));
        lines.push(Line::raw("")); // separator

        for (idx, &row_idx) in visible_indices.iter().enumerate() {
            let row = &all_rows[row_idx];
            let line = match row {
                PaletteRow::Command {
                    name,
                    desc,
                    keys,
                    disabled,
                    ..
                } => {
                    let style = if *disabled {
                        theme.style(tier, oom_edit_core::SemanticStyle::Muted)
                    } else {
                        Style::default()
                    };
                    let is_selected = idx == self.selected;
                    let prefix = if is_selected { "▸ " } else { "  " };
                    Line::styled(
                        format!("{}{:<20} {:<30} {}", prefix, name, desc, keys),
                        style,
                    )
                }
                PaletteRow::Reference { keys, desc, row_id } => {
                    let is_selected = idx == self.selected;
                    let prefix = if is_selected { "▸ " } else { "  " };
                    Line::styled(
                        format!("{}{:<20} {:<35} {}", prefix, keys, desc, row_id),
                        theme.style(tier, oom_edit_core::SemanticStyle::Muted),
                    )
                }
            };
            lines.push(line);

            // Stop at max visible rows (floor 12 minus filter/header).
            if lines.len() >= 12 {
                break;
            }
        }

        let paragraph =
            Paragraph::new(lines).block(Block::default().borders(Borders::NONE).title(""));

        let inner = block.inner(area);
        frame.render_widget(paragraph, inner);
    }

    /// Preferred centered geometry (width, height).
    #[allow(dead_code)]
    pub fn geometry(&self) -> (u16, u16) {
        (40, 12)
    }

    /// Hint string.
    #[allow(dead_code)]
    pub fn hints(&self) -> &'static str {
        "↑↓ navigate · type to filter · Enter execute · Esc close"
    }

    /// Get the command to execute (if the selected row is a Command).
    pub fn selected_command(&self) -> Option<Command> {
        let km = Keymap::default();
        let all_rows = self.build_rows(Contexts::ALL, &km);
        let visible_indices = self.filter_rows(&all_rows);
        if let Some(&row_idx) = visible_indices.get(self.selected) {
            if let PaletteRow::Command { id, .. } = &all_rows[row_idx] {
                return Some(*id);
            }
        }
        None
    }
}

/// Check if `pattern` is a fuzzy subsequence of `text`.
///
/// Simple case-insensitive subsequence match: every character in `pattern` must
/// appear in `text` in order.
fn fuzzy_match(pattern: &str, text: &str) -> bool {
    let pattern_lower = pattern.to_lowercase();
    let text_lower = text.to_lowercase();

    let mut pattern_chars = pattern_lower.chars();
    let mut pattern_char = match pattern_chars.next() {
        Some(c) => c,
        None => return true, // empty pattern matches everything
    };

    for tc in text_lower.chars() {
        if tc == pattern_char {
            if let Some(next) = pattern_chars.next() {
                pattern_char = next;
            } else {
                // Pattern fully matched in order.
                return true;
            }
        }
    }

    false
}

// ── Layout helpers ──────────────────────────────────────────────────────────

/// Compute a centered rectangle of the given size within the parent area.
fn centered_area(width: u16, height: u16, parent: ratatui::layout::Rect) -> ratatui::layout::Rect {
    let x = parent.width.saturating_sub(width).saturating_sub(1) / 2;
    let y = parent.height.saturating_sub(height).saturating_sub(1) / 2;
    ratatui::layout::Rect::new(
        x,
        y,
        width.min(parent.width.saturating_sub(x)),
        height.min(parent.height.saturating_sub(y)),
    )
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use oom_edit_core::session::{KeyCode, KeyCodeKind, KeyInput, Modifiers};

    fn key(kind: KeyCodeKind) -> KeyInput {
        KeyInput {
            code: KeyCode { kind },
            mods: Modifiers::default(),
        }
    }

    #[test]
    fn fuzzy_match_exact() {
        assert!(fuzzy_match("help", "help"));
    }

    #[test]
    fn fuzzy_match_subsequence() {
        assert!(fuzzy_match("hlp", "help"));
        assert!(fuzzy_match("tgv", "toggle-view"));
    }

    #[test]
    fn fuzzy_match_case_insensitive() {
        assert!(fuzzy_match("HELP", "help"));
        assert!(fuzzy_match("HELP", "Help"));
    }

    #[test]
    fn fuzzy_match_no_false_positive_short() {
        // "hi" is not a subsequence of "help" (no 'i' in "help").
        assert!(!fuzzy_match("hi", "help"));
    }

    #[test]
    fn fuzzy_match_rejects_unrelated() {
        assert!(!fuzzy_match("xyz", "help"));
        assert!(!fuzzy_match("abc", "toggle-view"));
    }

    #[test]
    fn fuzzy_match_empty_pattern() {
        assert!(fuzzy_match("", "anything"));
    }

    #[test]
    fn test_palette_down_clamps_at_last_row() {
        let mut palette = PaletteState {
            filter: "V-M2".to_string(),
            ..PaletteState::default()
        };
        let rows = palette.build_rows(Contexts::ALL, &Keymap::default());
        assert_eq!(palette.filter_rows(&rows).len(), 3);

        let down = key(KeyCodeKind::Down);
        for _ in 0..5 {
            assert!(palette.handle_key(&down));
        }
        assert_eq!(palette.selected, 2);

        assert!(palette.handle_key(&key(KeyCodeKind::Tab)));
        assert_eq!(palette.selected, 2);
    }

    #[test]
    fn test_palette_selected_command_at_boundary() {
        let mut palette = PaletteState {
            filter: "(no binding)".to_string(),
            ..PaletteState::default()
        };
        let rows = palette.build_rows(Contexts::ALL, &Keymap::default());
        let visible_rows = palette.filter_rows(&rows);
        assert_eq!(
            visible_rows.len(),
            6,
            "unexpected filtered rows: {:?}",
            visible_rows
                .iter()
                .map(|&index| &rows[index])
                .collect::<Vec<_>>()
        );

        for _ in 0..8 {
            assert!(palette.handle_key(&key(KeyCodeKind::Down)));
        }
        assert_eq!(palette.selected, 5);
        assert!(palette.handle_key(&key(KeyCodeKind::Tab)));
        assert_eq!(palette.selected, 5);

        assert_eq!(palette.selected_command(), Some(Command::QuitAll));
    }

    #[test]
    fn test_palette_down_with_zero_visible() {
        let mut palette = PaletteState {
            filter: "zzzz-no-visible-row".to_string(),
            ..PaletteState::default()
        };
        let rows = palette.build_rows(Contexts::ALL, &Keymap::default());
        assert!(palette.filter_rows(&rows).is_empty());

        assert!(palette.handle_key(&key(KeyCodeKind::Down)));
        assert_eq!(palette.selected, 0);
    }

    #[test]
    fn vim_reference_has_entries() {
        assert!(!VIM_REFERENCE.is_empty());
        // Check that every entry has all three fields.
        for (keys, desc, row_id) in VIM_REFERENCE {
            assert!(!keys.is_empty(), "keys should not be empty");
            assert!(!desc.is_empty(), "desc should not be empty");
            assert!(!row_id.is_empty(), "row_id should not be empty");
        }
    }

    #[test]
    fn vim_reference_covers_all_sections() {
        let row_ids: Vec<&str> = VIM_REFERENCE.iter().map(|(_, _, id)| *id).collect();
        let sections = ["V-M", "V-S", "V-E", "V-O", "V-T", "V-R", "V-V", "V-X"];
        for section in sections {
            assert!(
                row_ids.iter().any(|id| id.starts_with(section)),
                "VIM_REFERENCE should cover section {}",
                section
            );
        }
    }
}
