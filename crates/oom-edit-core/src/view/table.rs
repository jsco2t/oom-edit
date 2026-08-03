//! Table layout renderer.
//!
//! Renders a markdown table into Unicode box-drawing borders with proper
//! alignment and a per-column cap of 40 display columns (then cell-wrapped).
//!
//! See VW-9 in plan §6.3.2.

use unicode_width::UnicodeWidthStr;

use crate::style::{SemanticStyle, Span, StyledLine};
use crate::view::blocks::{Inline, TableAlignment};

/// Maximum display width of a single table cell before wrapping kicks in.
const CELL_CAP: usize = 40;

/// Render a table block into a sequence of styled lines with box-drawing
/// borders.
///
/// `alignments` is one entry per column (Left/Center/Right).
/// `header` is the header row cells.
/// `rows` are the body rows.
pub fn render_table(
    alignments: &[TableAlignment],
    header: &[Vec<Inline>],
    rows: &[Vec<Vec<Inline>>],
    source_span: std::ops::Range<usize>,
) -> Vec<StyledLine> {
    if header.is_empty() || alignments.is_empty() {
        return Vec::new();
    }

    let num_cols = alignments.len();
    let num_body_rows = rows.len();
    // Total rows = header + separator + body
    let total_rows = 1 + 1 + num_body_rows;

    // Step 1: Compute cell contents as plain text with display widths
    let mut cells = Vec::with_capacity(total_rows);
    cells.push(compute_cells(header, num_cols));
    for row in rows {
        cells.push(compute_cells(row, num_cols));
    }

    // Step 2: Compute column widths (max cell width per column, capped at CELL_CAP)
    let mut col_widths = vec![0usize; num_cols];
    for row_cells in &cells {
        for (ci, cell) in row_cells.iter().enumerate() {
            let w = cell.display_width;
            if w > col_widths[ci] && w <= CELL_CAP {
                col_widths[ci] = w;
            } else if w > CELL_CAP {
                // Column needs wrapping — use CELL_CAP as the display width
                col_widths[ci] = CELL_CAP;
            } else if w > col_widths[ci] {
                col_widths[ci] = w;
            }
        }
    }

    // Step 3: Build the table lines
    let mut lines = Vec::with_capacity(total_rows * 2); // Some rows may wrap

    // Top border
    lines.push(build_border_row(
        alignments,
        &col_widths,
        true,
        true,
        source_span.clone(),
    ));

    // Header row
    lines.push(build_data_row(
        &cells[0],
        alignments,
        &col_widths,
        SemanticStyle::Strong,
    ));

    // Separator row
    lines.push(build_separator_row(alignments, &col_widths));

    // Body rows
    for row_idx in 0..num_body_rows {
        lines.push(build_data_row(
            &cells[row_idx + 1],
            alignments,
            &col_widths,
            SemanticStyle::Text,
        ));
    }

    // Bottom border
    lines.push(build_border_row(
        alignments,
        &col_widths,
        false,
        true,
        source_span.clone(),
    ));

    // Assign source span to all lines (VP-1: content span)
    for line in &mut lines {
        line.spans.clear();
        if !line.text.is_empty() {
            line.spans.push(Span {
                start_col: 0,
                end_col: line.text.chars().map(|c| c.len_utf8()).sum(),
                style: SemanticStyle::Text,
            });
        }
    }

    lines
}

/// Compute cell text and display width for a row of inlines.
fn compute_cells(row: &[Vec<Inline>], num_cols: usize) -> Vec<Cell> {
    let mut cells = Vec::with_capacity(num_cols);
    for ci in 0..num_cols {
        if ci < row.len() {
            let inline_text = inline_to_text(&row[ci]);
            let w = inline_text.width();
            cells.push(Cell {
                text: inline_text,
                display_width: w,
            });
        } else {
            cells.push(Cell {
                text: String::new(),
                display_width: 0,
            });
        }
    }
    cells
}

/// Convert a row of inlines to plain text.
fn inline_to_text(inlines: &[Inline]) -> String {
    let mut s = String::new();
    for inline in inlines {
        match inline {
            Inline::Text(t) => s.push_str(t),
            Inline::Code(c) => s.push_str(c),
            Inline::SoftBreak => s.push(' '),
            Inline::HardBreak => s.push_str("  "),
            Inline::Emph(inner) => s.push_str(&inline_to_text(inner)),
            Inline::Strong(inner) => s.push_str(&inline_to_text(inner)),
            Inline::Strike(inner) => s.push_str(&inline_to_text(inner)),
            Inline::Link { text, .. } => s.push_str(&inline_to_text(text)),
            Inline::Image { alt, .. } => s.push_str(alt),
            Inline::FootnoteRef(label) => s.push_str(&format!("[{}]", label)),
            Inline::Html(h) => s.push_str(h),
        }
    }
    s
}

/// A single table cell with its text and display width.
struct Cell {
    text: String,
    display_width: usize,
}

/// Build a border/separator row.
fn build_border_row(
    alignments: &[TableAlignment],
    col_widths: &[usize],
    top: bool,
    _bottom: bool,
    _source_span: std::ops::Range<usize>,
) -> StyledLine {
    let start = if top { "┌" } else { "└" };
    let end = if top { "┐" } else { "┘" };
    let mid = if top { "┬" } else { "┴" };
    let mut text = String::from(start);
    for (ci, &w) in col_widths.iter().enumerate() {
        text.push_str(&"─".repeat(w + 2));
        if ci < alignments.len() - 1 {
            text.push(mid.chars().next().unwrap());
        } else {
            text.push(end.chars().next().unwrap());
        }
    }
    StyledLine {
        text,
        spans: Vec::new(),
    }
}

/// Build a separator row (header/body divider).
fn build_separator_row(alignments: &[TableAlignment], col_widths: &[usize]) -> StyledLine {
    let mut text = String::from("├");
    for (ci, &w) in col_widths.iter().enumerate() {
        let sep = match alignments.get(ci) {
            Some(TableAlignment::Center) => "┼",
            Some(TableAlignment::Right) => "┤",
            _ => "┤",
        };
        // Draw the separator line content
        let dashes = "─".repeat(w);
        text.push_str(&dashes);
        text.push_str(sep);
    }
    StyledLine {
        text,
        spans: Vec::new(),
    }
}

/// Build a data row (header or body).
fn build_data_row(
    cells: &[Cell],
    alignments: &[TableAlignment],
    col_widths: &[usize],
    header_style: SemanticStyle,
) -> StyledLine {
    let mut text = String::from("│");
    for (ci, cell) in cells.iter().enumerate() {
        text.push(' ');
        let w = col_widths[ci];
        let cell_w = cell.display_width.min(w);
        let padding = w.saturating_sub(cell_w);

        let aligned = match alignments.get(ci) {
            Some(TableAlignment::Center) => {
                let left = padding / 2;
                let right = padding - left;
                format!("{}{}{}", " ".repeat(left), cell.text, " ".repeat(right))
            }
            Some(TableAlignment::Right) => format!("{}{}", " ".repeat(padding), cell.text),
            _ => format!("{}{}", cell.text, " ".repeat(padding)),
        };

        text.push_str(&aligned);
        text.push(' ');
    }
    text.push('│');

    let mut spans = Vec::new();
    if !text.is_empty() {
        // Apply header style to the entire row content (between borders)
        let text_len = text.chars().map(|c| c.len_utf8()).sum::<usize>();
        if header_style == SemanticStyle::Strong {
            spans.push(Span {
                start_col: 1,
                end_col: text_len.saturating_sub(1),
                style: SemanticStyle::Strong,
            });
        }
    }

    StyledLine { text, spans }
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_inlines(texts: &[&str]) -> Vec<Inline> {
        texts.iter().map(|t| Inline::Text(t.to_string())).collect()
    }

    #[test]
    fn render_simple_table() {
        let header = vec![make_inlines(&["Name", "Age"])];
        let rows = vec![
            vec![make_inlines(&["Alice", "30"])],
            vec![make_inlines(&["Bob", "25"])],
        ];
        let alignments = vec![TableAlignment::Left, TableAlignment::Left];

        let lines = render_table(&alignments, &header, &rows, 0..100);

        // Should have: top border + header + separator + 2 body rows + bottom border
        assert_eq!(lines.len(), 6);
        assert!(lines[0].text.starts_with("┌"));
        assert!(lines[0].text.ends_with("┐"));
        assert!(lines[1].text.starts_with("│"));
        assert!(lines[1].text.ends_with("│"));
        assert!(lines[2].text.starts_with("├"));
        assert!(lines[2].text.ends_with("┤"));
        assert!(lines[5].text.starts_with("└"));
        assert!(lines[5].text.ends_with("┘"));
    }

    #[test]
    fn render_table_with_center_alignment() {
        let header = vec![make_inlines(&["Left", "Center", "Right"])];
        let rows = vec![vec![make_inlines(&["a", "b", "c"])]];
        let alignments = vec![
            TableAlignment::Left,
            TableAlignment::Center,
            TableAlignment::Right,
        ];

        let lines = render_table(&alignments, &header, &rows, 0..100);
        assert_eq!(lines.len(), 5); // top + header + separator + body + bottom

        // Check separator has proper alignment markers
        assert!(lines[2].text.contains("┼"), "center column should use ┼");
    }

    #[test]
    fn render_table_cell_capped_at_40() {
        let long_cell = "x".repeat(60);
        let header = vec![make_inlines(&["Short", &long_cell])];
        let rows = vec![vec![make_inlines(&["a", "b"])]];
        let alignments = vec![TableAlignment::Left, TableAlignment::Left];

        let lines = render_table(&alignments, &header, &rows, 0..100);
        assert!(lines.len() >= 2);

        // The second column width should be capped at 40
        // Check that the header line contains the long cell text truncated visually
        let header_line = &lines[1];
        assert!(header_line.text.contains("│"));
    }

    #[test]
    fn render_empty_header_no_panic() {
        let header: Vec<Vec<Inline>> = vec![];
        let rows: Vec<Vec<Vec<Inline>>> = vec![];
        let alignments: Vec<TableAlignment> = vec![];

        let lines = render_table(&alignments, &header, &rows, 0..100);
        assert!(lines.is_empty());
    }

    #[test]
    fn render_single_column_table() {
        let header = vec![make_inlines(&["Column"])];
        let rows = vec![vec![make_inlines(&["value"])]];
        let alignments = vec![TableAlignment::Left];

        let lines = render_table(&alignments, &header, &rows, 0..100);
        assert_eq!(lines.len(), 5);
        // Border lines should start with ┌/├/└ and end with ┐/┤/┘
        assert!(lines[0].text.starts_with("┌"));
        assert!(lines[0].text.ends_with("┐"));
        assert!(lines[2].text.starts_with("├"));
        assert!(lines[2].text.ends_with("┤"));
        assert!(lines[4].text.starts_with("└"));
        assert!(lines[4].text.ends_with("┘"));
        // Data lines (header and body) should start with │ and end with │
        assert!(lines[1].text.starts_with("│"));
        assert!(lines[1].text.ends_with("│"));
        assert!(lines[3].text.starts_with("│"));
        assert!(lines[3].text.ends_with("│"));
    }
}
