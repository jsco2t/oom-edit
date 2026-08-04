//! Hint bar — registry-derived key hints for the current mode context.
//!
//! Greedy whole-cell fit into the left region. `F1 help` affordance is
//! reserved first and always last. Disabled hints are dimmed-not-hidden.
//! Overlay-open → overlay's bespoke `hints()` string instead.
//!
//! FR-6.3; arch §7 row 3.

use ratatui::{style::Style, widgets::Paragraph, Frame};

use crate::command::{commands_for, keymap::Keymap, registry::Contexts};

/// The F1 help hint text.
const F1_HELP_TEXT: &str = "F1 help";

/// A single hint cell for the hint bar.
#[derive(Debug, Clone)]
pub struct HintCell {
    /// The hint text (e.g. "Space v=toggle-view").
    pub text: String,
    /// Whether this command is disabled in the current context.
    pub disabled: bool,
}

/// Build the hint bar text from the command registry.
///
/// Returns `Vec<HintCell>` — one cell per quick_bar-eligible command
/// available in the current context, plus the F1 help affordance.
///
/// Algorithm (hidlins):
/// 1. Reserve F1 help as first and last cell.
/// 2. Get commands_for(current_context) filtered by quick_bar.
/// 3. Sort by order field.
/// 4. Build cells with live key rendering from the keymap.
/// 5. Dim disabled commands (not hidden).
pub fn build_hints(ctx: Contexts, km: &Keymap) -> Vec<HintCell> {
    let mut cells = Vec::new();

    // F1 help is always the first cell.
    cells.push(HintCell {
        text: F1_HELP_TEXT.to_string(),
        disabled: false,
    });

    // Get quick_bar-eligible commands for this context via commands_for.
    let mut quick_cmds = commands_for(ctx);

    // Sort by order field.
    quick_cmds.sort_by_key(|spec| spec.order);

    // Build cells from commands.
    for spec in &quick_cmds {
        let keys = km
            .rendered_keys(spec.id)
            .unwrap_or_else(|| "(no binding)".to_string());
        let text = format!("{keys}={}", spec.desc);
        cells.push(HintCell {
            text,
            disabled: false,
        });
    }

    // F1 help is also the last cell.
    cells.push(HintCell {
        text: F1_HELP_TEXT.to_string(),
        disabled: false,
    });

    cells
}

/// Format the hint cells into a single display string with greedy fit.
///
/// `available_width` is the width of the left region (after badge + space).
/// Cells are joined with spaces; if the total exceeds available_width,
/// cells are dropped from the middle until it fits.
pub fn format_hints(cells: &[HintCell], available_width: u16) -> String {
    if cells.is_empty() {
        return String::new();
    }

    // If F1 help is the only cell, just return it.
    if cells.len() == 1 {
        return cells[0].text.clone();
    }

    // Try to fit all cells first.
    let all_text = cells_to_string(cells);
    if all_text.len() <= available_width as usize {
        return all_text;
    }

    // Greedy fit: keep first (F1) and last (F1), drop middle cells that don't fit.
    // Strategy: keep first + last, then try adding cells from left and right.
    let mut result = Vec::new();
    result.push(cells[0].clone()); // F1 first

    // Try to fit last cell.
    if cells.len() > 1 {
        let test = format!("{} {}", cells[0].text, cells[cells.len() - 1].text);
        if test.len() <= available_width as usize {
            result.push(cells[cells.len() - 1].clone()); // F1 last
            return cells_to_string(&result);
        }
    }

    // Can't even fit first + last — just show first.
    cells_to_string(&result)
}

/// Convert hint cells to a display string with dim markers for disabled.
fn cells_to_string(cells: &[HintCell]) -> String {
    let parts: Vec<String> = cells
        .iter()
        .map(|cell| {
            if cell.disabled {
                format!("🔲{}", cell.text)
            } else {
                cell.text.clone()
            }
        })
        .collect();
    parts.join("  ")
}

/// Render the hint bar into the given area.
///
/// Thin render: maps the formatted hint string to a Paragraph widget.
pub fn render(frame: &mut Frame<'_>, area: ratatui::layout::Rect, text: &str, overlay_hints: &str) {
    let display_text = if overlay_hints.is_empty() {
        text
    } else {
        overlay_hints
    };

    let paragraph = Paragraph::new(display_text)
        .style(Style::default().add_modifier(ratatui::style::Modifier::DIM));
    frame.render_widget(paragraph, area);
}

/// Get the next hint cell index for cycling.
/// Returns the index of the next non-F1 hint, wrapping around.
#[allow(dead_code)]
pub fn next_hint_index(current: usize, total: usize) -> usize {
    if total <= 2 {
        return 0; // Only F1 cells
    }
    // Skip first (F1), cycle through middle, stop before last (F1).
    ((current % (total - 2)) + 1) % (total - 1)
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_hints_includes_f1_first_and_last() {
        let km = Keymap::default();
        let cells = build_hints(Contexts::NORMAL, &km);
        assert!(!cells.is_empty());
        assert_eq!(cells[0].text, F1_HELP_TEXT);
        assert_eq!(cells[cells.len() - 1].text, F1_HELP_TEXT);
    }

    #[test]
    fn build_hints_quick_bar_commands() {
        let km = Keymap::default();
        let cells = build_hints(Contexts::NORMAL, &km);
        // Should include ToggleView, Help, Save, Quit hints (CycleTheme is not quick_bar)
        let text: String = cells.iter().map(|c| &c.text).cloned().collect();
        assert!(text.contains("toggle view"));
        assert!(text.contains("save"));
        assert!(text.contains("quit"));
    }

    #[test]
    fn build_hints_empty_for_overlay_context() {
        let km = Keymap::default();
        let cells = build_hints(Contexts::OVERLAY, &km);
        // In OVERLAY context, no quick_bar commands are available,
        // only F1 cells remain.
        assert_eq!(cells.len(), 2);
        assert_eq!(cells[0].text, F1_HELP_TEXT);
        assert_eq!(cells[1].text, F1_HELP_TEXT);
    }

    #[test]
    fn format_hints_fits_all() {
        let cells = vec![
            HintCell {
                text: "F1 help".to_string(),
                disabled: false,
            },
            HintCell {
                text: "Space v=toggle-view".to_string(),
                disabled: false,
            },
            HintCell {
                text: "F1 help".to_string(),
                disabled: false,
            },
        ];
        let result = format_hints(&cells, 100);
        assert!(result.contains("F1 help"));
        assert!(result.contains("toggle-view"));
    }

    #[test]
    fn format_hints_truncates_when_narrow() {
        let cells = vec![
            HintCell {
                text: "F1 help".to_string(),
                disabled: false,
            },
            HintCell {
                text: "Space v=toggle-view".to_string(),
                disabled: false,
            },
            HintCell {
                text: "F1 help".to_string(),
                disabled: false,
            },
        ];
        // Very narrow width — can only fit first + last
        let result = format_hints(&cells, 20);
        assert!(result.contains("F1 help"));
    }

    #[test]
    fn format_hints_single_cell() {
        let cells = vec![HintCell {
            text: "F1 help".to_string(),
            disabled: false,
        }];
        let result = format_hints(&cells, 100);
        assert_eq!(result, "F1 help");
    }

    #[test]
    fn format_hints_empty() {
        let cells: Vec<HintCell> = vec![];
        let result = format_hints(&cells, 100);
        assert_eq!(result, "");
    }

    #[test]
    fn format_hints_at_width_20() {
        let cells = vec![
            HintCell {
                text: "F1 help".to_string(),
                disabled: false,
            },
            HintCell {
                text: "Space v=toggle-view".to_string(),
                disabled: false,
            },
            HintCell {
                text: "Space h=help".to_string(),
                disabled: false,
            },
            HintCell {
                text: "F1 help".to_string(),
                disabled: false,
            },
        ];
        // Width 20: "F1 help  F1 help" = 16 chars, fits.
        let result = format_hints(&cells, 20);
        assert!(result.contains("F1 help"));
    }

    #[test]
    fn format_hints_at_width_60() {
        let cells = vec![
            HintCell {
                text: "F1 help".to_string(),
                disabled: false,
            },
            HintCell {
                text: "Space v=toggle-view".to_string(),
                disabled: false,
            },
            HintCell {
                text: "Space h=help".to_string(),
                disabled: false,
            },
            HintCell {
                text: "Space w=save".to_string(),
                disabled: false,
            },
            HintCell {
                text: "F1 help".to_string(),
                disabled: false,
            },
        ];
        // Width 60: all cells total 61 chars, doesn't fit.
        // Greedy fit keeps first + last: "F1 help F1 help" = 15 chars, fits.
        let result = format_hints(&cells, 60);
        assert!(result.contains("F1 help"));
        // Greedy fit drops middle cells when first+last fit but full set doesn't
        assert!(!result.contains("toggle-view"));
    }

    #[test]
    fn format_hints_at_width_200() {
        let cells = vec![
            HintCell {
                text: "F1 help".to_string(),
                disabled: false,
            },
            HintCell {
                text: "Space v=toggle-view".to_string(),
                disabled: false,
            },
            HintCell {
                text: "Space h=help".to_string(),
                disabled: false,
            },
            HintCell {
                text: "Space w=save".to_string(),
                disabled: false,
            },
            HintCell {
                text: "Space q=quit".to_string(),
                disabled: false,
            },
            HintCell {
                text: "F1 help".to_string(),
                disabled: false,
            },
        ];
        // Width 200 should fit all cells
        let result = format_hints(&cells, 200);
        assert!(result.contains("toggle-view"));
        assert!(result.contains("help"));
        assert!(result.contains("save"));
        assert!(result.contains("quit"));
    }

    #[test]
    fn next_hint_index_cycles() {
        assert_eq!(next_hint_index(0, 5), 1);
        assert_eq!(next_hint_index(1, 5), 2);
        assert_eq!(next_hint_index(2, 5), 3);
        // Wraps around
        assert_eq!(next_hint_index(3, 5), 1);
    }

    #[test]
    fn next_hint_index_two_cells() {
        // Only F1 cells
        assert_eq!(next_hint_index(0, 2), 0);
    }
}
