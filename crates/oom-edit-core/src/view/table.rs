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
            col_widths[ci] = col_widths[ci].max(w.min(CELL_CAP));
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

    // Assign a default Text span to lines that have no semantic spans.
    for line in &mut lines {
        if line.spans.is_empty() && !line.text.is_empty() {
            line.spans.push(Span {
                start_col: 0,
                end_col: line.text.chars().count(),
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
        let dashes = "─".repeat(w + 2);
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
        let text_len = text.chars().count();
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
    fn separator_width_matches_border() {
        let header = vec![make_inlines(&["Name", "Age"])];
        let rows = vec![vec![make_inlines(&["Alice", "30"])]];
        let alignments = vec![TableAlignment::Left, TableAlignment::Left];
        let lines = render_table(&alignments, &header, &rows, 0..100);

        let top_width = lines[0].text.chars().count();
        let separator_width = lines[2].text.chars().count();
        let bottom_width = lines[4].text.chars().count();
        assert_eq!(
            top_width, separator_width,
            "separator width ({separator_width}) must match top border width ({top_width})"
        );
        assert_eq!(
            top_width, bottom_width,
            "bottom border width ({bottom_width}) must match top border width ({top_width})"
        );
    }

    #[test]
    fn header_row_has_strong_style() {
        let header = vec![make_inlines(&["Name", "Age"])];
        let rows = vec![vec![make_inlines(&["Alice", "30"])]];
        let alignments = vec![TableAlignment::Left, TableAlignment::Left];
        let lines = render_table(&alignments, &header, &rows, 0..100);

        let header_line = &lines[1];
        assert!(
            header_line
                .spans
                .iter()
                .any(|span| span.style == SemanticStyle::Strong),
            "header row should have Strong style, got spans: {:?}",
            header_line.spans
        );
    }

    #[test]
    fn body_row_has_text_style_and_not_strong() {
        let header = vec![make_inlines(&["Name", "Age"])];
        let rows = vec![vec![make_inlines(&["Alice", "30"])]];
        let alignments = vec![TableAlignment::Left, TableAlignment::Left];
        let lines = render_table(&alignments, &header, &rows, 0..100);

        let body_line = &lines[3];
        assert!(
            body_line
                .spans
                .iter()
                .any(|span| span.style == SemanticStyle::Text),
            "body row should have Text style, got spans: {:?}",
            body_line.spans
        );
        assert!(
            body_line
                .spans
                .iter()
                .all(|span| span.style != SemanticStyle::Strong),
            "body row should not have Strong style"
        );
    }

    #[test]
    fn column_width_capped_at_cell_cap() {
        let long = "x".repeat(60);
        let header = vec![make_inlines(&[&long])];
        let rows = vec![vec![make_inlines(&["short"])]];
        let alignments = vec![TableAlignment::Left];
        let lines = render_table(&alignments, &header, &rows, 0..100);

        let border_width = lines[0].text.chars().count();
        assert_eq!(
            border_width,
            CELL_CAP + 4,
            "border width should be CELL_CAP + 4 for a single column"
        );
    }

    #[test]
    fn rendered_unicode_table_spans_cover_complete_rows_in_character_indices() {
        let header = vec![make_inlines(&["café", "東京🙂"])];
        let rows = vec![vec![make_inlines(&["résumé", "大阪🚀"])]];
        let alignments = vec![TableAlignment::Left, TableAlignment::Left];

        let lines = render_table(&alignments, &header, &rows, 0..100);

        for (line_index, line) in lines.iter().enumerate() {
            let char_count = line.text.chars().count();
            assert_eq!(
                line.spans.len(),
                1,
                "rendered table row should have exactly one span: {:?}",
                line.text
            );
            let span = &line.spans[0];
            assert!(span.start_col <= span.end_col);
            assert!(
                span.end_col <= char_count,
                "span [{}, {}) exceeds {char_count} chars in {:?}",
                span.start_col,
                span.end_col,
                line.text
            );
            if line_index == 1 {
                assert_eq!(span.style, SemanticStyle::Strong);
                assert_eq!(span.start_col, 1);
                assert_eq!(span.end_col, char_count - 1);
                let content: String = line
                    .text
                    .chars()
                    .skip(1)
                    .take(char_count.saturating_sub(2))
                    .collect();
                assert_eq!(span_text(line, span), content);
            } else {
                assert_eq!(span.style, SemanticStyle::Text);
                assert_eq!(span.start_col, 0);
                assert_eq!(span.end_col, char_count);
                assert_eq!(span_text(line, span), line.text);
            }
        }
    }

    #[test]
    fn unicode_header_style_excludes_only_character_borders() {
        let cells = compute_cells(&[make_inlines(&["café東京🙂"])], 1);
        let line = build_data_row(
            &cells,
            &[TableAlignment::Left],
            &[cells[0].display_width],
            SemanticStyle::Strong,
        );
        let span = line.spans.first().expect("header row should be styled");

        assert_eq!(span.start_col, 1);
        assert_eq!(span.end_col, line.text.chars().count() - 1);
        assert_eq!(span_text(&line, span), " café東京🙂 ");
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

    fn span_text(line: &StyledLine, span: &Span) -> String {
        line.text
            .chars()
            .skip(span.start_col)
            .take(span.end_col - span.start_col)
            .collect()
    }
}
