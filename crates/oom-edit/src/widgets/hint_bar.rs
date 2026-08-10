//! Hint bar — registry-derived key hints for the current mode context.
//!
//! Greedy whole-cell fit into the flexible status region. Each eligible
//! registry command appears once in registry priority order. Disabled hints
//! are dimmed-not-hidden.
//! Overlay-open → overlay's bespoke `hints()` string instead.
//!
//! FR-6.3; arch §7 row 3.

use ratatui::text::Line;

use crate::command::{commands_for, keymap::Keymap, registry::Contexts};

/// A single hint cell for the hint bar.
#[derive(Debug, Clone)]
pub struct HintCell {
    /// The hint text (for example, `v=character-wise selection`).
    pub text: String,
    /// Whether this command is disabled in the current context.
    pub disabled: bool,
}

/// Build the hint bar text from the command registry.
///
/// Returns `Vec<HintCell>` — one cell per quick_bar-eligible command
/// available in the current context.
///
/// Commands are sorted by registry `order` and use live key rendering from
/// the keymap. The registry is the only source of hint cells.
pub fn build_hints(ctx: Contexts, km: &Keymap) -> Vec<HintCell> {
    let mut quick_cmds = commands_for(ctx);
    quick_cmds.sort_by_key(|spec| spec.order);

    quick_cmds
        .into_iter()
        .map(|spec| {
            let keys = km
                .rendered_keys(spec.id)
                .unwrap_or_else(|| "(no binding)".to_string());
            HintCell {
                text: format!("{keys}={}", spec.desc),
                disabled: false,
            }
        })
        .collect()
}

/// Format the hint cells into a single display string with greedy fit.
///
/// `available_width` is the width of the flexible region after the badge.
/// In registry priority order, each whole cell is retained when it fits; a
/// cell that does not fit is skipped so a later, shorter cell can still be
/// useful. Cells are never truncated or duplicated.
pub fn format_hints(cells: &[HintCell], available_width: u16) -> String {
    let mut result = Vec::new();
    let mut used = 0;

    for cell in cells {
        let cell_text = display_cell_text(cell);
        let separator_width = usize::from(!result.is_empty()) * 2;
        let needed = separator_width + Line::from(cell_text).width();
        if used + needed <= available_width as usize {
            used += needed;
            result.push(cell.clone());
        }
    }

    cells_to_string(&result)
}

fn display_cell_text(cell: &HintCell) -> String {
    if cell.disabled {
        format!("🔲{}", cell.text)
    } else {
        cell.text.clone()
    }
}

/// Convert hint cells to a display string with dim markers for disabled.
fn cells_to_string(cells: &[HintCell]) -> String {
    let parts: Vec<String> = cells.iter().map(display_cell_text).collect();
    parts.join("  ")
}

/// Get the next hint cell index for cycling, wrapping around all cells.
#[allow(dead_code)]
pub fn next_hint_index(current: usize, total: usize) -> usize {
    if total == 0 {
        return 0;
    }
    (current + 1) % total
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_hints_contains_each_quick_command_once() {
        let km = Keymap::default();
        let cells = build_hints(Contexts::NORMAL, &km);
        let expected = commands_for(Contexts::NORMAL);

        assert_eq!(cells.len(), expected.len());
        for spec in expected {
            let suffix = format!("={}", spec.desc);
            assert_eq!(
                cells
                    .iter()
                    .filter(|cell| cell.text.ends_with(&suffix))
                    .count(),
                1,
                "quick-bar command {:?} must appear exactly once",
                spec.id
            );
        }
    }

    #[test]
    fn build_hints_quick_bar_commands() {
        let km = Keymap::default();
        let cells = build_hints(Contexts::NORMAL, &km);
        // Normal exposes Select, Help, Save, and Quit in priority order.
        let text: String = cells.iter().map(|c| &c.text).cloned().collect();
        assert!(text.contains("character-wise selection"));
        assert!(text.contains("save"));
        assert!(text.contains("quit"));
    }

    #[test]
    fn build_hints_empty_for_overlay_context() {
        let km = Keymap::default();
        let cells = build_hints(Contexts::OVERLAY, &km);
        assert!(cells.is_empty());
    }

    #[test]
    fn format_hints_greedily_keeps_priority_order() {
        let cells = vec![
            HintCell {
                text: "first".to_string(),
                disabled: false,
            },
            HintCell {
                text: "second-long".to_string(),
                disabled: false,
            },
            HintCell {
                text: "third".to_string(),
                disabled: false,
            },
        ];
        assert_eq!(format_hints(&cells, 4), "");
        assert_eq!(format_hints(&cells, 5), "first");
        assert_eq!(format_hints(&cells, 12), "first  third");
        assert_eq!(format_hints(&cells, 25), "first  second-long  third");
    }

    #[test]
    fn format_hints_single_cell() {
        let cells = vec![HintCell {
            text: "Space h=help".to_string(),
            disabled: false,
        }];
        let result = format_hints(&cells, 100);
        assert_eq!(result, "Space h=help");
    }

    #[test]
    fn format_hints_empty() {
        let cells: Vec<HintCell> = vec![];
        let result = format_hints(&cells, 100);
        assert_eq!(result, "");
    }

    #[test]
    fn format_hints_never_partially_renders_a_cell() {
        let cells = vec![
            HintCell {
                text: "v=select-lines".to_string(),
                disabled: false,
            },
            HintCell {
                text: "Space h=help".to_string(),
                disabled: false,
            },
        ];
        assert_eq!(format_hints(&cells, 19), "v=select-lines");
        assert_eq!(format_hints(&cells, 13), "Space h=help");
        assert_eq!(format_hints(&cells, 11), "");
    }

    #[test]
    fn format_hints_uses_display_width_for_disabled_marker() {
        let cells = vec![HintCell {
            text: "help".to_string(),
            disabled: true,
        }];
        assert_eq!(format_hints(&cells, 5), "");
        assert_eq!(format_hints(&cells, 6), "🔲help");
    }

    #[test]
    fn next_hint_index_cycles() {
        assert_eq!(next_hint_index(0, 3), 1);
        assert_eq!(next_hint_index(1, 3), 2);
        assert_eq!(next_hint_index(2, 3), 0);
    }

    #[test]
    fn next_hint_index_empty() {
        assert_eq!(next_hint_index(0, 0), 0);
    }
}
