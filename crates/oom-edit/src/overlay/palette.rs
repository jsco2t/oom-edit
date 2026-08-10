//! Command palette (FR-6.6/6.7).
//!
//! A fuzzy-filtered, executable command palette with two sections:
//! - **App commands** — from the registry, dimmed when context-disabled
//! - **Vim reference** — static, non-executable, rendered muted
//!
//! Floor geometry: 40×12. Close with Esc; execute with Enter.

use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::Style,
    text::Line,
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

use crate::command::{Command, Contexts, Keymap};
use crate::theme::{Theme, Tier};

// ── Vim reference table ─────────────────────────────────────────────────────

/// Static reference entries for the supported four-mode interaction model.
/// Format: `(keys, description, row-id)`.
pub static VIM_REFERENCE: &[(&str, &str, &str)] = &[
    ("j/k, ↑/↓", "Move by rendered row.", "R-N1"),
    ("gg / G", "Jump to the first / last rendered row.", "R-N2"),
    ("Tab / S-Tab", "Move between rendered jump targets.", "R-N3"),
    ("/pattern⏎", "Search rendered text forward.", "R-N4"),
    ("?pattern⏎", "Search rendered text backward.", "R-N4"),
    ("n / N", "Repeat the rendered search.", "R-N4"),
    ("i/a/I/A/o/O", "Enter source Insert mode.", "R-I1"),
    ("Esc", "Return from Insert to rendered Normal.", "R-I2"),
    ("u / <C-r>", "Undo / redo.", "R-E1"),
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
    (":help", "Open help / command palette.", "V-X8"),
];

// ── Palette state ───────────────────────────────────────────────────────────

const FLOOR_W: u16 = 40;
const FLOOR_H: u16 = 12;

/// The command palette state machine.
#[derive(Debug)]
pub struct PaletteState {
    /// Current filter text (case-insensitive).
    filter: String,
    /// Selected row index within the filtered+sectioned list.
    selected: usize,
    /// Mode context captured when the palette opened.
    context: Contexts,
    /// Whether the last Enter was on a Vim reference entry (non-executable).
    #[allow(dead_code)]
    last_was_reference: bool,
}

impl Default for PaletteState {
    fn default() -> Self {
        Self::new(Contexts::NORMAL)
    }
}

impl PaletteState {
    /// Create a palette whose command availability reflects `context`.
    pub fn new(context: Contexts) -> Self {
        Self {
            filter: String::new(),
            selected: 0,
            context,
            last_was_reference: false,
        }
    }

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
                let rows = self.build_rows(self.context, &Keymap::default());
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
        let area = palette_area(frame.area());
        frame.render_widget(Clear, area);

        let block = Block::default()
            .borders(Borders::ALL)
            .title(" Command Palette ");
        let inner = block.inner(area);
        frame.render_widget(block, area);

        if inner.width == 0 || inner.height < 3 {
            return;
        }

        let [filter_area, _spacer_area, list_area] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(1),
        ])
        .areas(inner);

        frame.render_widget(Paragraph::new(format!("> {}", self.filter)), filter_area);

        // Build rows.
        let km = Keymap::default();
        let all_rows = self.build_rows(self.context, &km);
        let visible_indices = self.filter_rows(&all_rows);

        // Build display lines.
        let mut lines: Vec<Line<'_>> = Vec::new();

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
        }

        let scroll = viewport_offset(self.selected, list_area.height);
        frame.render_widget(Paragraph::new(lines).scroll((scroll, 0)), list_area);
    }

    /// Preferred centered geometry (width, height).
    #[allow(dead_code)]
    pub fn geometry(&self) -> (u16, u16) {
        (FLOOR_W, FLOOR_H)
    }

    /// Hint string.
    #[allow(dead_code)]
    pub fn hints(&self) -> &'static str {
        "↑↓ navigate · type to filter · Enter execute · Esc close"
    }

    /// Get the command to execute (if the selected row is a Command).
    pub fn selected_command(&self) -> Option<Command> {
        let km = Keymap::default();
        let all_rows = self.build_rows(self.context, &km);
        let visible_indices = self.filter_rows(&all_rows);
        if let Some(&row_idx) = visible_indices.get(self.selected) {
            if let PaletteRow::Command { id, disabled, .. } = &all_rows[row_idx] {
                return (!disabled).then_some(*id);
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

/// Compute the responsive palette rectangle within the parent area.
fn palette_area(parent: Rect) -> Rect {
    let width = ((u32::from(parent.width) * 4 / 5) as u16)
        .max(FLOOR_W)
        .min(parent.width);
    let height = ((u32::from(parent.height) * 4 / 5) as u16)
        .max(FLOOR_H)
        .min(parent.height);

    centered_area(width, height, parent)
}

/// Compute a centered rectangle of the given size within the parent area.
fn centered_area(width: u16, height: u16, parent: Rect) -> Rect {
    let width = width.min(parent.width);
    let height = height.min(parent.height);
    Rect::new(
        parent
            .x
            .saturating_add(parent.width.saturating_sub(width) / 2),
        parent
            .y
            .saturating_add(parent.height.saturating_sub(height) / 2),
        width,
        height,
    )
}

/// Compute the minimal vertical scroll needed to keep the selected row visible.
fn viewport_offset(selected: usize, list_height: u16) -> u16 {
    let capacity = usize::from(list_height);
    let offset = selected.saturating_sub(capacity.saturating_sub(1));
    u16::try_from(offset).unwrap_or(u16::MAX)
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use oom_edit_core::session::{KeyCode, KeyCodeKind, KeyInput, Modifiers};
    use ratatui::{backend::TestBackend, Terminal};

    use crate::theme::{UiSlot, DEFAULT_DARK};

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
        assert!(fuzzy_match("ers", "enter-select"));
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
        assert!(!fuzzy_match("abc", "enter-select"));
    }

    #[test]
    fn fuzzy_match_empty_pattern() {
        assert!(fuzzy_match("", "anything"));
    }

    #[test]
    fn palette_area_uses_floor_and_grows_to_eighty_percent() {
        let cases = [
            (Rect::new(0, 0, 40, 12), Rect::new(0, 0, 40, 12)),
            (Rect::new(0, 0, 80, 24), Rect::new(8, 2, 64, 19)),
            (Rect::new(0, 0, 200, 60), Rect::new(20, 6, 160, 48)),
            (Rect::new(0, 0, 39, 11), Rect::new(0, 0, 39, 11)),
            (Rect::new(0, 0, 30, 24), Rect::new(0, 2, 30, 19)),
            (Rect::new(7, 11, 100, 50), Rect::new(17, 16, 80, 40)),
        ];

        for (parent, expected) in cases {
            let actual = palette_area(parent);
            assert_eq!(actual, expected, "unexpected palette area for {parent:?}");
            assert!(actual.x >= parent.x);
            assert!(actual.y >= parent.y);
            assert!(actual.right() <= parent.right());
            assert!(actual.bottom() <= parent.bottom());
        }
    }

    #[test]
    fn palette_viewport_offset_keeps_selection_visible() {
        let cases = [
            (0, 0, 0),
            (0, 1, 0),
            (1, 1, 1),
            (0, 3, 0),
            (2, 3, 0),
            (3, 3, 1),
            (7, 3, 5),
            (usize::from(u16::MAX), 3, u16::MAX - 2),
        ];

        for (selected, height, expected) in cases {
            let offset = viewport_offset(selected, height);
            assert_eq!(offset, expected, "selected={selected}, height={height}");

            if height > 0 {
                let offset = usize::from(offset);
                let height = usize::from(height);
                assert!(offset <= selected);
                assert!(selected < offset + height);
                assert_eq!(offset, selected.saturating_sub(height - 1));
            }
        }

        assert_eq!(
            viewport_offset(usize::from(u16::MAX) + 42, 1),
            u16::MAX,
            "scroll saturates when the selected index cannot fit in u16"
        );
    }

    #[test]
    fn palette_render_clears_underlying_styles() {
        let width = 80;
        let height = 24;
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        let seed_style = DEFAULT_DARK.ui_style(Tier::TrueColor, UiSlot::BadgeInsert);
        let seed_fg = seed_style.fg.expect("seed foreground");
        let seed_bg = seed_style.bg.expect("seed background");
        let seed_modifier = seed_style.add_modifier;
        let palette = PaletteState::default();

        terminal
            .draw(|frame| {
                let seed_lines = (0..height)
                    .map(|_| Line::raw("X".repeat(usize::from(width))))
                    .collect::<Vec<_>>();
                frame.render_widget(Paragraph::new(seed_lines).style(seed_style), frame.area());
                palette.render(frame, &DEFAULT_DARK, Tier::TrueColor);
            })
            .unwrap();

        let modal = palette_area(Rect::new(0, 0, width, height));
        let buffer = terminal.backend().buffer();
        for y in modal.y..modal.bottom() {
            for x in modal.x..modal.right() {
                let cell = buffer.cell((x, y)).unwrap();
                assert_ne!(cell.symbol(), "X", "seed glyph survived at ({x}, {y})");
                assert_ne!(cell.fg, seed_fg, "seed foreground survived at ({x}, {y})");
                assert_ne!(cell.bg, seed_bg, "seed background survived at ({x}, {y})");
                assert!(
                    !cell.modifier.contains(seed_modifier),
                    "seed modifier survived at ({x}, {y})"
                );
            }
        }
    }

    #[test]
    fn test_palette_down_clamps_at_last_row() {
        let mut palette = PaletteState {
            filter: "R-N4".to_string(),
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
            ..PaletteState::new(Contexts::ALL)
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
    fn palette_disables_commands_outside_the_opening_context() {
        let normal = PaletteState::new(Contexts::NORMAL);
        let normal_rows = normal.build_rows(normal.context, &Keymap::default());
        assert!(normal_rows.iter().any(|row| matches!(
            row,
            PaletteRow::Command {
                id: Command::EnterCharacterSelect,
                disabled: false,
                ..
            }
        )));
        assert!(normal_rows.iter().any(|row| matches!(
            row,
            PaletteRow::Command {
                id: Command::SelectYank,
                disabled: true,
                ..
            }
        )));

        let select = PaletteState::new(Contexts::SELECT);
        let select_rows = select.build_rows(select.context, &Keymap::default());
        assert!(select_rows.iter().any(|row| matches!(
            row,
            PaletteRow::Command {
                id: Command::EnterCharacterSelect,
                disabled: true,
                ..
            }
        )));
        assert!(select_rows.iter().any(|row| matches!(
            row,
            PaletteRow::Command {
                id: Command::SelectYank,
                disabled: false,
                ..
            }
        )));
    }

    #[test]
    fn selected_disabled_palette_command_is_not_executable() {
        let palette = PaletteState {
            filter: "select-yank".to_string(),
            ..PaletteState::new(Contexts::NORMAL)
        };
        assert_eq!(palette.selected_command(), None);
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
        let sections = ["R-N", "R-I", "R-E", "V-X"];
        for section in sections {
            assert!(
                row_ids.iter().any(|id| id.starts_with(section)),
                "VIM_REFERENCE should cover section {}",
                section
            );
        }
    }
}
