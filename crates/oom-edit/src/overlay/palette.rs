//! Command palette (FR-6.6/6.7).
//!
//! A fuzzy-filtered, executable command palette with two sections:
//! - **App commands** — from the registry, dimmed when context-disabled
//! - **Vim reference** — static, non-executable, rendered muted
//!
//! Floor geometry: 40×12. Close with Esc; execute with Enter.

use ratatui::{
    layout::{Constraint, Layout, Rect},
    text::Line,
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

#[cfg(test)]
use crate::command::rendered_binding;
use crate::command::{rendered_binding_for, AppCommand, BindingRole, Contexts};
use crate::theme::{Theme, Tier, UiSlot};

// ── Vim reference table ─────────────────────────────────────────────────────

/// Static reference entries for the supported four-mode interaction model.
/// Format: `(keys, description, row-id, editable ex prefill)`.
pub static VIM_REFERENCE: &[(&str, &str, &str, Option<&str>)] = &[
    ("j/k, ↑/↓", "Move by rendered row.", "R-N1", None),
    (
        "gg / G",
        "Jump to the first / last rendered row.",
        "R-N2",
        None,
    ),
    (
        "Tab / S-Tab",
        "Move between rendered jump targets.",
        "R-N3",
        None,
    ),
    ("/pattern⏎", "Search rendered text forward.", "R-N4", None),
    ("n / N", "Repeat the rendered search.", "R-N4", None),
    ("i/a/I/A/o/O", "Enter source Insert mode.", "R-I1", None),
    (
        "Esc",
        "Return from Insert to rendered Normal.",
        "R-I2",
        None,
    ),
    ("u / <C-r>", "Undo / redo.", "R-E1", None),
    (":w", "Save (atomic).", "V-X1", Some("w")),
    (
        ":w {path}",
        "Save a copy to path without retargeting buffer.",
        "V-X1",
        Some("w "),
    ),
    (":q", "Quit; refuses if dirty.", "V-X2", Some("q")),
    (":q!", "Quit; discards changes.", "V-X2", Some("q!")),
    (":wq", "Save then quit.", "V-X3", Some("wq")),
    (":x", "Save then quit (if changed).", "V-X3", Some("x")),
    (
        ":e {path}",
        "Open file; refuses if dirty without !.",
        "V-X4",
        Some("e "),
    ),
    (":e!", "Reload current file from disk.", "V-X4", Some("e!")),
    (
        ":reload",
        "Reload current file from disk.",
        "V-X4",
        Some("reload"),
    ),
    (
        ":reload-all",
        "Reload every open tab from disk.",
        "V-X4",
        Some("reload-all"),
    ),
    (
        ":saveas {path}",
        "Save to path and retarget buffer.",
        "V-X5",
        Some("saveas "),
    ),
    (":{number}", "Jump to line.", "V-X6", None),
    (
        ":s/pat/rep/",
        "Substitute on current line.",
        "V-X7",
        Some("s/"),
    ),
    (
        ":s/pat/rep/g",
        "Substitute all on current line.",
        "V-X7",
        Some("s/"),
    ),
    (
        ":%s/pat/rep/g",
        "Substitute all in document.",
        "V-X7",
        Some("%s/"),
    ),
    (
        ":noh",
        "Clear search-match highlighting.",
        "V-X8",
        Some("noh"),
    ),
    (":help", "Open the command palette.", "V-X8", Some("help")),
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
        let mut state = Self {
            filter: String::new(),
            selected: 0,
            context,
            last_was_reference: false,
        };
        state.select_first_action();
        state
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
        id: AppCommand,
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
        prefill: Option<&'static str>,
    },
}

/// What Enter can do with the focused palette row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PaletteAction {
    App(AppCommand),
    ExPrefill(&'static str),
}

impl PaletteState {
    /// Build the full row list from the registry and Vim reference table.
    pub(crate) fn build_rows(&self, ctx: Contexts) -> Vec<PaletteRow> {
        let mut rows = Vec::new();

        // App commands section.
        for spec in crate::command::COMMANDS {
            let enabled = spec.contexts.contains(ctx);
            let keys = rendered_binding_for(spec, ctx);
            match spec.binding {
                BindingRole::AppChord { command, .. } => rows.push(PaletteRow::Command {
                    id: command,
                    name: spec.name.to_string(),
                    desc: spec.desc.to_string(),
                    keys,
                    disabled: !enabled,
                }),
                BindingRole::AppSpaceDigit | BindingRole::CoreKey { .. } => {
                    rows.push(PaletteRow::Reference {
                        keys,
                        desc: spec.desc.to_string(),
                        row_id: spec.conformance_id.unwrap_or(spec.name).to_string(),
                        prefill: None,
                    });
                }
                BindingRole::CoreEx { prefill, .. } => rows.push(PaletteRow::Reference {
                    keys,
                    desc: spec.desc.to_string(),
                    row_id: spec.conformance_id.unwrap_or(spec.name).to_string(),
                    prefill,
                }),
            }
        }

        // Vim reference section.
        for (keys, desc, row_id, prefill) in VIM_REFERENCE {
            rows.push(PaletteRow::Reference {
                keys: keys.to_string(),
                desc: desc.to_string(),
                row_id: row_id.to_string(),
                prefill: *prefill,
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
                PaletteRow::Reference {
                    keys, desc, row_id, ..
                } => format!("{} {} {}", keys, desc, row_id).to_lowercase(),
            };

            if fuzzy_match(&lower, &searchable) {
                indices.push(i);
            }
        }

        indices
    }

    fn action_positions(&self, rows: &[PaletteRow]) -> Vec<usize> {
        self.filter_rows(rows)
            .into_iter()
            .enumerate()
            .filter_map(|(position, row_index)| {
                matches!(
                    rows[row_index],
                    PaletteRow::Command {
                        disabled: false,
                        ..
                    }
                )
                .then_some(position)
            })
            .collect()
    }

    fn select_first_action(&mut self) {
        let rows = self.build_rows(self.context);
        self.selected = self.action_positions(&rows).first().copied().unwrap_or(0);
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
    pub fn handle_key(&mut self, key: &oom_edit_core::KeyInput) -> bool {
        use oom_edit_core::KeyCodeKind;

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
                let rows = self.build_rows(self.context);
                let visible_count = self.filter_rows(&rows).len();
                self.selected = self
                    .selected
                    .saturating_add(1)
                    .min(visible_count.saturating_sub(1));
                return true;
            }
            KeyCodeKind::Char(c) if !key.mods.ctrl && !key.mods.alt => {
                // Append to filter.
                self.filter.push(*c);
                self.select_first_action();
                return true;
            }
            KeyCodeKind::Backspace => {
                self.filter.pop();
                self.select_first_action();
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

        let surface = theme.ui_style(tier, UiSlot::PaletteSurface);
        let block = Block::default()
            .borders(Borders::ALL)
            .title(" Command Palette ")
            .style(surface);
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

        frame.render_widget(
            Paragraph::new(format!("> {}", self.filter))
                .style(theme.ui_style(tier, UiSlot::PaletteText)),
            filter_area,
        );

        // Build rows.
        let all_rows = self.build_rows(self.context);
        let visible_indices = self.filter_rows(&all_rows);

        // Build display lines.
        let mut lines: Vec<Line<'_>> = Vec::new();

        for (idx, &row_idx) in visible_indices.iter().enumerate() {
            let row = &all_rows[row_idx];
            let selected = idx == self.selected;
            let style = if selected {
                theme.ui_style(tier, UiSlot::PaletteSelected)
            } else if matches!(row, PaletteRow::Reference { .. })
                || matches!(row, PaletteRow::Command { disabled: true, .. })
            {
                theme.ui_style(tier, UiSlot::PaletteSecondary)
            } else {
                theme.ui_style(tier, UiSlot::PaletteText)
            };
            let line = Line::styled(palette_line(row, selected, list_area.width), style);
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
        "↑↓ navigate · type to filter · Enter run/open ex · Esc close"
    }

    pub(crate) fn selected_action(&self) -> Option<PaletteAction> {
        let all_rows = self.build_rows(self.context);
        let visible_indices = self.filter_rows(&all_rows);
        match visible_indices
            .get(self.selected)
            .and_then(|&row_idx| all_rows.get(row_idx))?
        {
            PaletteRow::Command {
                id,
                disabled: false,
                ..
            } => Some(PaletteAction::App(*id)),
            PaletteRow::Reference {
                prefill: Some(text),
                ..
            } => Some(PaletteAction::ExPrefill(text)),
            _ => None,
        }
    }

    /// Get the command to execute (if the selected row is a Command).
    #[cfg(test)]
    pub fn selected_command(&self) -> Option<AppCommand> {
        let all_rows = self.build_rows(self.context);
        let visible_indices = self.filter_rows(&all_rows);
        if let Some(&row_idx) = visible_indices.get(self.selected) {
            if let PaletteRow::Command { id, disabled, .. } = &all_rows[row_idx] {
                return (!disabled).then_some(*id);
            }
        }
        None
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PaletteColumns {
    primary: usize,
    description: usize,
    tail: usize,
}

fn palette_columns(width: u16) -> PaletteColumns {
    let available = usize::from(width).saturating_sub(2);
    if available < 12 {
        return PaletteColumns {
            primary: available,
            description: 0,
            tail: 0,
        };
    }
    let tail = (available.saturating_sub(26)).min(14);
    let description = (available.saturating_sub(tail + 14)).min(30);
    let separators = usize::from(description > 0) + usize::from(tail > 0);
    let content = available.saturating_sub(separators);
    PaletteColumns {
        primary: content.saturating_sub(description + tail),
        description,
        tail,
    }
}

fn clip(text: &str, width: usize) -> String {
    if Line::from(text).width() <= width {
        return text.to_string();
    }
    if width <= 1 {
        return "…".chars().take(width).collect();
    }
    let mut result = String::new();
    for ch in text.chars() {
        let next = format!("{result}{ch}");
        if Line::from(format!("{next}…")).width() > width {
            break;
        }
        result.push(ch);
    }
    format!("{result}…")
}

fn pad(text: &str, width: usize) -> String {
    let clipped = clip(text, width);
    format!(
        "{clipped}{}",
        " ".repeat(width.saturating_sub(Line::from(clipped.as_str()).width()))
    )
}

/// One display-width-aware layout shared by executable and reference rows.
fn palette_line(row: &PaletteRow, selected: bool, width: u16) -> String {
    let (marker, primary, description, tail) = match row {
        PaletteRow::Command {
            name,
            desc,
            keys,
            disabled,
            ..
        } => (
            if *disabled {
                if selected {
                    "×›"
                } else {
                    "× "
                }
            } else if selected {
                "▸ "
            } else {
                "  "
            },
            name.as_str(),
            desc.as_str(),
            keys.as_str(),
        ),
        PaletteRow::Reference {
            keys,
            desc,
            row_id,
            prefill,
        } => (
            match (selected, prefill.is_some()) {
                (true, true) => "↳ ",
                (false, true) => "◦ ",
                (true, false) => "› ",
                (false, false) => "· ",
            },
            keys.as_str(),
            desc.as_str(),
            row_id.as_str(),
        ),
    };
    let columns = palette_columns(width);
    let mut result = format!("{marker}{}", pad(primary, columns.primary));
    if columns.description > 0 {
        result.push(' ');
        result.push_str(&pad(description, columns.description));
    }
    if columns.tail > 0 {
        result.push(' ');
        result.push_str(&pad(tail, columns.tail));
    }
    result
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
    use oom_edit_core::{KeyCode, KeyCodeKind, KeyInput, Modifiers};
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
    fn palette_rows_share_exact_display_columns_at_wide_and_floor_widths() {
        let command = PaletteRow::Command {
            id: AppCommand::Help,
            name: "help".to_string(),
            desc: "command palette".to_string(),
            keys: "Space h".to_string(),
            disabled: false,
        };
        let reference = PaletteRow::Reference {
            keys: "/pattern⏎".to_string(),
            desc: "Search rendered text forward.".to_string(),
            row_id: "R-N4".to_string(),
            prefill: None,
        };
        for width in [40, 80] {
            let command = palette_line(&command, false, width);
            let reference = palette_line(&reference, false, width);
            assert_eq!(Line::from(command.as_str()).width(), usize::from(width));
            assert_eq!(Line::from(reference.as_str()).width(), usize::from(width));

            let command_description = command.find("command").expect("command description");
            let reference_description = reference.find("Search").expect("reference description");
            assert_eq!(
                Line::from(&command[..command_description]).width(),
                Line::from(&reference[..reference_description]).width()
            );

            let command_tail = command.find("Space h").expect("command binding");
            let reference_tail = reference.find("R-N4").expect("reference row id");
            assert_eq!(
                Line::from(&command[..command_tail]).width(),
                Line::from(&reference[..reference_tail]).width()
            );
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
        let rows = palette.build_rows(Contexts::ALL);
        assert!(palette.filter_rows(&rows).len() >= 2);

        let down = key(KeyCodeKind::Down);
        for _ in 0..5 {
            assert!(palette.handle_key(&down));
        }
        assert_eq!(palette.selected_command(), None);

        assert!(palette.handle_key(&key(KeyCodeKind::Tab)));
        assert_eq!(palette.selected_command(), None);
    }

    #[test]
    fn palette_selects_an_executable_app_command() {
        let mut palette = PaletteState::new(Contexts::ALL);
        for character in "help".chars() {
            palette.handle_key(&key(KeyCodeKind::Char(character)));
        }
        let rows = palette.build_rows(Contexts::ALL);
        let visible_rows = palette.filter_rows(&rows);
        assert_eq!(visible_rows.len(), 3);
        assert_eq!(palette.selected_command(), Some(AppCommand::Help));
    }

    #[test]
    fn palette_focus_traverses_actions_references_and_disabled_rows_in_each_mode() {
        for context in [Contexts::NORMAL, Contexts::SELECT] {
            let mut palette = PaletteState::new(context);
            assert_eq!(palette.selected_command(), Some(AppCommand::Help));
            let rows = palette.build_rows(context);
            let focused = &rows[palette.selected];
            assert!(palette_line(focused, true, 60).starts_with("▸ "));

            let help_index = palette.selected;
            for _ in 0..help_index {
                palette.handle_key(&key(KeyCodeKind::Up));
            }
            assert_eq!(palette.selected, 0);
            assert_eq!(palette.selected_command(), None);
            assert!(palette_line(&rows[0], true, 60).starts_with("› "));
            palette.handle_key(&key(KeyCodeKind::BackTab));
            assert_eq!(palette.selected, 0);
            for _ in 0..help_index {
                palette.handle_key(&key(KeyCodeKind::Tab));
            }
            assert_eq!(palette.selected_command(), Some(AppCommand::Help));
            palette.handle_key(&key(KeyCodeKind::Down));
            assert_eq!(palette.selected_command(), Some(AppCommand::Save));
            palette.handle_key(&key(KeyCodeKind::Up));
            assert_eq!(palette.selected_command(), Some(AppCommand::Help));
            for _ in 0..rows.len() + 5 {
                palette.handle_key(&key(KeyCodeKind::Down));
            }
            assert_eq!(palette.selected, rows.len() - 1);
            assert_eq!(palette.selected_command(), None);
            assert!(palette_line(&rows[palette.selected], true, 60).starts_with("↳ "));
            palette.handle_key(&key(KeyCodeKind::Tab));
            assert_eq!(palette.selected, rows.len() - 1);
            palette.handle_key(&key(KeyCodeKind::BackTab));
            assert_eq!(palette.selected, rows.len() - 2);
        }

        let mut command_context = PaletteState::new(Contexts::COMMAND);
        let rows = command_context.build_rows(Contexts::COMMAND);
        let disabled_index = rows
            .iter()
            .position(|row| matches!(row, PaletteRow::Command { disabled: true, .. }))
            .unwrap();
        for _ in 0..disabled_index {
            command_context.handle_key(&key(KeyCodeKind::Down));
        }
        assert_eq!(command_context.selected, disabled_index);
        assert_eq!(command_context.selected_command(), None);
        assert!(palette_line(&rows[disabled_index], true, 60).starts_with("×›"));
    }

    #[test]
    fn palette_scrolls_to_focused_row_below_the_visible_list() {
        for (width, height) in [(40, 12), (80, 24)] {
            let mut palette = PaletteState::new(Contexts::NORMAL);
            let count = palette.build_rows(Contexts::NORMAL).len();
            for _ in 0..count {
                palette.handle_key(&key(KeyCodeKind::Down));
            }
            assert_eq!(palette.selected, count - 1);
            let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
            terminal
                .draw(|frame| palette.render(frame, &DEFAULT_DARK, Tier::TrueColor))
                .unwrap();
            let area = palette_area(Rect::new(0, 0, width, height));
            let buffer = terminal.backend().buffer();
            let visible: String = (area.y..area.bottom())
                .flat_map(|y| {
                    (area.x..area.right())
                        .map(move |x| buffer.cell((x, y)).unwrap().symbol().to_string())
                })
                .collect();
            assert!(
                visible.contains(":help"),
                "last row is not visible at {width}×{height}"
            );
            assert!(
                visible.contains('↳'),
                "focused ex row has no visible marker"
            );
        }
    }

    #[test]
    fn palette_keeps_core_bindings_as_non_executable_references() {
        let normal = PaletteState::new(Contexts::NORMAL);
        let normal_rows = normal.build_rows(normal.context);
        assert!(normal_rows.iter().any(|row| matches!(
            row,
            PaletteRow::Reference { row_id, .. } if row_id == "select-character"
        )));
        assert!(normal_rows.iter().any(|row| matches!(
            row,
            PaletteRow::Command {
                id: AppCommand::Help,
                disabled: false,
                ..
            }
        )));
        assert!(normal_rows.iter().any(|row| matches!(
            row,
            PaletteRow::Reference { row_id, .. } if row_id == "select-yank"
        )));
        assert!(normal_rows.iter().any(|row| matches!(
            row,
            PaletteRow::Reference { row_id, .. } if row_id == "select-yank-plain-text"
        )));
    }

    #[test]
    fn palette_rows_match_registry_commands_and_live_keys() {
        let palette = PaletteState::new(Contexts::ALL);
        let rows = palette.build_rows(Contexts::ALL);
        assert_eq!(
            rows.len(),
            crate::command::COMMANDS.len() + VIM_REFERENCE.len()
        );

        for spec in crate::command::COMMANDS {
            let binding = rendered_binding(spec);
            assert!(rows.iter().any(|row| match row {
                PaletteRow::Command { name, keys, .. } => name == spec.name && keys == &binding,
                PaletteRow::Reference { row_id, keys, .. } => {
                    row_id == spec.conformance_id.unwrap_or(spec.name) && keys == &binding
                }
            }));
        }
    }

    #[test]
    fn palette_help_row_lists_only_contextual_question_shortcuts() {
        let palette = PaletteState::new(Contexts::NORMAL);
        let help_keys = |context| {
            palette
                .build_rows(context)
                .into_iter()
                .find_map(|row| match row {
                    PaletteRow::Command {
                        id: AppCommand::Help,
                        keys,
                        ..
                    } => Some(keys),
                    _ => None,
                })
                .unwrap()
        };
        assert_eq!(help_keys(Contexts::NORMAL), "? / Space h/?");
        assert_eq!(help_keys(Contexts::SELECT), "? / Space h/?");
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
        let rows = palette.build_rows(Contexts::ALL);
        assert!(palette.filter_rows(&rows).is_empty());

        assert!(palette.handle_key(&key(KeyCodeKind::Down)));
        assert_eq!(palette.selected_command(), None);
    }

    #[test]
    fn vim_reference_has_entries() {
        assert!(!VIM_REFERENCE.is_empty());
        // Check that every entry has its required display fields.
        for (keys, desc, row_id, prefill) in VIM_REFERENCE {
            assert!(!keys.is_empty(), "keys should not be empty");
            assert!(!desc.is_empty(), "desc should not be empty");
            assert!(!row_id.is_empty(), "row_id should not be empty");
            if let Some(text) = prefill {
                assert!(
                    keys.starts_with(':'),
                    "ex prefill requires an ex row: {keys}"
                );
                assert!(!text.is_empty());
                assert!(!text.starts_with(':'));
                assert!(!text.chars().any(char::is_control));
                assert!(
                    !text.contains('{') && !text.contains('}'),
                    "placeholder leaked into {keys}"
                );
            }
        }
    }

    #[test]
    fn palette_ex_rows_have_explicit_prefills_and_ambiguous_rows_stay_read_only() {
        let mut palette = PaletteState::new(Contexts::NORMAL);
        let rows = palette.build_rows(Contexts::NORMAL);
        for (keys, expected) in [
            (":wq", Some("wq")),
            (":q!", Some("q!")),
            (":e {path}", Some("e ")),
            (":tabnew {path}", Some("tabnew ")),
            (":tabclose", Some("tabclose")),
            (":set spell / :set nospell", None),
            (":{number}", None),
        ] {
            palette.selected = rows
                .iter()
                .position(|row| matches!(row, PaletteRow::Reference { keys: row_keys, .. } if row_keys == keys))
                .unwrap();
            assert_eq!(
                palette.selected_action(),
                expected.map(PaletteAction::ExPrefill),
                "{keys}"
            );
        }
        palette.selected = rows
            .iter()
            .position(|row| matches!(row, PaletteRow::Reference { keys, .. } if keys == "j/k, ↑/↓"))
            .unwrap();
        assert_eq!(palette.selected_action(), None);
    }

    #[test]
    fn vim_reference_covers_all_sections() {
        let row_ids: Vec<&str> = VIM_REFERENCE.iter().map(|(_, _, id, _)| *id).collect();
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
