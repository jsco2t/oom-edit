//! Hint bar — registry-derived key hints for the current mode context.
//!
//! Greedy whole-cell fit into the flexible status region. Each eligible
//! registry command appears once in registry priority order. Disabled hints
//! are dimmed-not-hidden.
//! Overlay-open → overlay's bespoke `hints()` string instead.
//!
//! FR-6.3; arch §7 row 3.

use ratatui::text::Line;

use crate::command::{
    commands_for, registry::Contexts, rendered_binding, BindingAlias, BindingRole,
};

/// A single hint cell for the hint bar.
#[derive(Debug, Clone)]
pub struct HintCell {
    /// The hint text (for example, `v=character-wise selection`).
    pub text: String,
    /// A shorter complete hint used when the primary text does not fit.
    pub compact_text: Option<String>,
    /// Whether this command is disabled in the current context.
    pub disabled: bool,
}

/// Build the hint bar text from the command registry.
///
/// Returns complete quick-hint units in registry priority order. Eligible
/// Space continuations share one prefix so the bar does not repeat it.
///
/// Commands are sorted by registry `order` and use live key rendering from
/// the keymap. The registry is the only source of hint cells.
pub fn build_hints(ctx: Contexts) -> Vec<HintCell> {
    let mut quick_cmds = commands_for(ctx);
    quick_cmds.sort_by_key(|spec| spec.quick_bar_order);

    let mut cells = Vec::new();
    let mut space_items = Vec::new();
    let mut compact_space_hint = None;
    let mut space_insert_at = None;
    for spec in quick_cmds {
        match spec.binding {
            BindingRole::AppChord { continuation, .. } => {
                for alias in spec.aliases {
                    if let BindingAlias::Direct { key, contexts } = alias {
                        if contexts.contains(ctx) {
                            cells.push(HintCell {
                                text: format!("{key}={}", spec.quick_label.unwrap_or(spec.desc)),
                                compact_text: None,
                                disabled: false,
                            });
                        }
                    }
                }
                space_insert_at.get_or_insert(cells.len());
                let mut keys = vec![continuation.to_string()];
                space_items.push(format!(
                    "{continuation}={}",
                    spec.quick_label.unwrap_or(spec.desc)
                ));
                for alias in spec.aliases {
                    if let BindingAlias::Space {
                        continuation,
                        contexts,
                    } = alias
                    {
                        if contexts.contains(ctx) {
                            keys.push(continuation.to_string());
                            space_items.push(format!(
                                "{continuation}={}",
                                spec.quick_label.unwrap_or(spec.desc)
                            ));
                        }
                    }
                }
                compact_space_hint
                    .get_or_insert_with(|| format!("Space+{}={}", keys.join("/"), spec.name));
            }
            _ => {
                let keys = rendered_binding(spec);
                cells.push(HintCell {
                    text: format!("{keys}={}", spec.quick_label.unwrap_or(spec.desc)),
                    compact_text: None,
                    disabled: false,
                });
            }
        }
    }
    if !space_items.is_empty() {
        cells.insert(
            space_insert_at.expect("a Space item records its insertion point"),
            HintCell {
                text: format!("Space [{}]", space_items.join(", ")),
                compact_text: compact_space_hint,
                disabled: false,
            },
        );
    }
    cells
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
        let separator_width = usize::from(!result.is_empty()) * 2;
        let full_text = display_cell_text(cell);
        let selected_text =
            if hint_fits(&full_text, separator_width, used, available_width as usize) {
                Some(full_text)
            } else {
                cell.compact_text
                    .as_deref()
                    .map(|text| display_text(text, cell.disabled))
                    .filter(|text| hint_fits(text, separator_width, used, available_width as usize))
            };
        if let Some(displayed) = selected_text {
            used += separator_width + Line::from(displayed.as_str()).width();
            result.push(displayed);
        }
    }

    result.join("  ")
}

fn hint_fits(text: &str, separator_width: usize, used: usize, available_width: usize) -> bool {
    used + separator_width + Line::from(text).width() <= available_width
}

fn display_cell_text(cell: &HintCell) -> String {
    display_text(&cell.text, cell.disabled)
}

fn display_text(text: &str, disabled: bool) -> String {
    if disabled {
        format!("🔲{text}")
    } else {
        text.to_string()
    }
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
    fn build_hints_groups_space_continuations_and_keeps_standalone_keys() {
        let cells = build_hints(Contexts::NORMAL);
        assert_eq!(cells.len(), 5);
        assert_eq!(cells[0].text, "v=select");
        assert_eq!(cells[1].text, "/=search");
        assert_eq!(cells[2].text, ":=command");
        assert_eq!(cells[3].text, "?=commands");
        assert_eq!(
            cells[4].text,
            "Space [h=commands, ?=commands, w=save, q=quit]"
        );
        assert_eq!(cells[4].compact_text.as_deref(), Some("Space+h/?=help"));
    }

    #[test]
    fn build_hints_quick_bar_commands() {
        let cells = build_hints(Contexts::NORMAL);
        // Normal exposes Select, Search, Command mode, and grouped Space hints.
        let text: String = cells.iter().map(|c| &c.text).cloned().collect();
        assert!(text.contains("v=select"));
        assert!(text.contains("/=search"));
        assert!(text.contains(":=command"));
        assert!(text.contains("?=commands"));
        assert!(text.contains("?=commands, w=save"));
        assert_eq!(text.matches("Space [").count(), 1);
        assert!(!text.contains("help / command palette"));
        assert!(text.contains("save"));
        assert!(text.contains("quit"));
    }

    #[test]
    fn build_hints_empty_for_overlay_context() {
        let cells = build_hints(Contexts::OVERLAY);
        assert!(cells.is_empty());
    }

    #[test]
    fn format_hints_greedily_keeps_priority_order() {
        let cells = vec![
            HintCell {
                text: "first".to_string(),
                compact_text: None,
                disabled: false,
            },
            HintCell {
                text: "second-long".to_string(),
                compact_text: None,
                disabled: false,
            },
            HintCell {
                text: "third".to_string(),
                compact_text: None,
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
            compact_text: None,
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
                compact_text: None,
                disabled: false,
            },
            HintCell {
                text: "Space h=help".to_string(),
                compact_text: None,
                disabled: false,
            },
        ];
        assert_eq!(format_hints(&cells, 19), "v=select-lines");
        assert_eq!(format_hints(&cells, 13), "Space h=help");
        assert_eq!(format_hints(&cells, 11), "");
    }

    #[test]
    fn format_hints_keeps_or_drops_complete_group_at_each_boundary() {
        let cells = vec![
            HintCell {
                text: "v=select".to_string(),
                compact_text: None,
                disabled: false,
            },
            HintCell {
                text: "Space [h=palette, w=save]".to_string(),
                compact_text: Some("Space+h=palette".to_string()),
                disabled: false,
            },
        ];
        let first = Line::from(cells[0].text.clone()).width();
        let compact = first + 2 + Line::from("Space+h=palette").width();
        let both = first + 2 + Line::from(cells[1].text.clone()).width();
        assert_eq!(format_hints(&cells, (compact - 1) as u16), "v=select");
        assert_eq!(
            format_hints(&cells, compact as u16),
            "v=select  Space+h=palette"
        );
        assert_eq!(
            format_hints(&cells, (both - 1) as u16),
            "v=select  Space+h=palette"
        );
        assert_eq!(
            format_hints(&cells, both as u16),
            "v=select  Space [h=palette, w=save]"
        );
        assert_eq!(
            format_hints(&cells, (both + 1) as u16),
            "v=select  Space [h=palette, w=save]"
        );
    }

    #[test]
    fn format_hints_uses_display_width_for_disabled_marker() {
        let cells = vec![HintCell {
            text: "help".to_string(),
            compact_text: None,
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
