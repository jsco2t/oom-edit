//! Table layout renderer.
//!
//! Renders a markdown table into Unicode box-drawing borders with proper
//! alignment and a per-column cap of 40 display columns (then cell-wrapped).
//!
//! See VW-9 in plan §6.3.2.

use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

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
    lines.extend(build_data_row(
        &cells[0],
        alignments,
        &col_widths,
        SemanticStyle::Strong,
    ));

    // Separator row
    lines.push(build_separator_row(&col_widths));

    // Body rows
    for row_idx in 0..num_body_rows {
        lines.extend(build_data_row(
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
fn build_separator_row(col_widths: &[usize]) -> StyledLine {
    let mut text = String::from("├");
    let last = col_widths.len() - 1;
    for (ci, &w) in col_widths.iter().enumerate() {
        text.push_str(&"─".repeat(w + 2));
        text.push(if ci < last { '┼' } else { '┤' });
    }
    StyledLine {
        text,
        spans: Vec::new(),
    }
}

/// Split cell text into fixed-width chunks without splitting a character.
fn split_cell_text(text: &str, max_width: usize) -> Vec<String> {
    if max_width == 0 {
        return vec![String::new()];
    }

    let mut lines = Vec::new();
    let mut chunk_start = 0;

    while chunk_start < text.len() {
        let mut chunk_end = text.len();
        let mut last_fitting_end = None;
        let mut last_base_start = chunk_start;
        let mut consuming_overwide_sequence = false;

        for (relative_start, ch) in text[chunk_start..].char_indices() {
            let char_start = chunk_start + relative_start;
            if consuming_overwide_sequence && ch.width().unwrap_or(0) > 0 {
                chunk_end = char_start;
                break;
            }

            let candidate_end = char_start + ch.len_utf8();
            let candidate_width = text[chunk_start..candidate_end].width();
            if candidate_width <= max_width {
                last_fitting_end = Some(candidate_end);
                if ch.width().unwrap_or(0) > 0 {
                    last_base_start = char_start;
                }
                continue;
            }

            if last_fitting_end.is_none() {
                // An indivisible wide character must still make progress even
                // when the requested width is narrower than the character.
                last_fitting_end = Some(candidate_end);
                last_base_start = char_start;
                consuming_overwide_sequence = true;
                continue;
            }

            if ch.width().unwrap_or(0) == 0 {
                // Sequence-aware width can change when a zero-width suffix is
                // added (for example, U+FE0F emoji presentation). Keep that
                // suffix with its base instead of leaving it on the next line.
                if last_base_start > chunk_start {
                    chunk_end = last_base_start;
                    break;
                }
                last_fitting_end = Some(candidate_end);
                consuming_overwide_sequence = true;
                continue;
            }

            chunk_end = last_fitting_end.expect("a fitting boundary was checked above");
            break;
        }

        lines.push(text[chunk_start..chunk_end].to_string());
        chunk_start = chunk_end;
    }

    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

/// Build a data row (header or body).
fn build_data_row(
    cells: &[Cell],
    alignments: &[TableAlignment],
    col_widths: &[usize],
    header_style: SemanticStyle,
) -> Vec<StyledLine> {
    let wrapped_cells: Vec<Vec<String>> = cells
        .iter()
        .enumerate()
        .map(|(ci, cell)| {
            let width = col_widths[ci];
            if cell.display_width > width {
                split_cell_text(&cell.text, width)
            } else {
                vec![cell.text.clone()]
            }
        })
        .collect();
    let max_lines = wrapped_cells.iter().map(Vec::len).max().unwrap_or(1);
    let mut lines = Vec::with_capacity(max_lines);

    for line_index in 0..max_lines {
        let mut text = String::from("│");
        for (ci, cell_lines) in wrapped_cells.iter().enumerate() {
            let chunk = cell_lines.get(line_index).map_or("", String::as_str);
            let width = col_widths[ci];
            let padding = width.saturating_sub(chunk.width());

            text.push(' ');
            if line_index == 0 {
                match alignments.get(ci) {
                    Some(TableAlignment::Center) => {
                        let left = padding / 2;
                        let right = padding - left;
                        text.push_str(&" ".repeat(left));
                        text.push_str(chunk);
                        text.push_str(&" ".repeat(right));
                    }
                    Some(TableAlignment::Right) => {
                        text.push_str(&" ".repeat(padding));
                        text.push_str(chunk);
                    }
                    _ => {
                        text.push_str(chunk);
                        text.push_str(&" ".repeat(padding));
                    }
                }
            } else {
                text.push_str(chunk);
                text.push_str(&" ".repeat(padding));
            }
            text.push(' ');
            text.push('│');
        }

        let mut spans = Vec::new();
        if header_style == SemanticStyle::Strong {
            let text_len = text.chars().count();
            spans.push(Span {
                start_col: 1,
                end_col: text_len.saturating_sub(1),
                style: SemanticStyle::Strong,
            });
        }
        lines.push(StyledLine { text, spans });
    }

    lines
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_row(texts: &[&str]) -> Vec<Vec<Inline>> {
        texts
            .iter()
            .map(|text| vec![Inline::Text((*text).to_string())])
            .collect()
    }

    #[test]
    fn render_simple_table() {
        let header = make_row(&["Name", "Age"]);
        let rows = vec![make_row(&["Alice", "30"]), make_row(&["Bob", "25"])];
        let alignments = vec![TableAlignment::Left, TableAlignment::Left];

        let lines = render_table(&alignments, &header, &rows, 0..100);

        // Should have: top border + header + separator + 2 body rows + bottom border
        assert_eq!(lines.len(), 6);
        assert_eq!(lines[0].text, "┌───────┬─────┐");
        assert_eq!(lines[1].text, "│ Name  │ Age │");
        assert_eq!(lines[2].text, "├───────┼─────┤");
        assert_eq!(lines[3].text, "│ Alice │ 30  │");
        assert_eq!(lines[4].text, "│ Bob   │ 25  │");
        assert_eq!(lines[5].text, "└───────┴─────┘");
        assert_eq!(lines[1].text.matches('│').count(), 3);
        assert_eq!(lines[3].text.matches('│').count(), 3);
    }

    #[test]
    fn render_table_with_center_alignment() {
        let header = make_row(&["Left", "Center", "Right"]);
        let rows = vec![make_row(&["a", "b", "c"])];
        let alignments = vec![
            TableAlignment::Left,
            TableAlignment::Center,
            TableAlignment::Right,
        ];

        let lines = render_table(&alignments, &header, &rows, 0..100);
        assert_eq!(lines.len(), 5); // top + header + separator + body + bottom

        assert_eq!(lines[2].text, "├──────┼────────┼───────┤");
    }

    #[test]
    fn all_rows_equal_width() {
        let cases = [
            (
                vec![TableAlignment::Left, TableAlignment::Left],
                make_row(&["First", "Second"]),
                vec![make_row(&["a", "longer"])],
            ),
            (
                vec![
                    TableAlignment::Left,
                    TableAlignment::Center,
                    TableAlignment::Right,
                ],
                make_row(&["Left", "Center", "Right"]),
                vec![make_row(&["a", "b", "c"])],
            ),
            (
                vec![TableAlignment::Left],
                make_row(&["Only"]),
                vec![make_row(&["value"])],
            ),
        ];

        for (alignments, header, rows) in cases {
            let lines = render_table(&alignments, &header, &rows, 0..100);
            let expected_width = lines[0].text.chars().count();
            assert!(
                lines
                    .iter()
                    .all(|line| line.text.chars().count() == expected_width),
                "table rows differ in width: {:?}",
                lines.iter().map(|line| &line.text).collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn data_row_inner_separators() {
        let header = make_row(&["One", "Two", "Three"]);
        let rows = vec![make_row(&["a", "b", "c"])];
        let alignments = vec![
            TableAlignment::Left,
            TableAlignment::Left,
            TableAlignment::Left,
        ];

        let lines = render_table(&alignments, &header, &rows, 0..100);

        assert_eq!(lines[1].text.matches('│').count(), 4);
        assert_eq!(lines[3].text.matches('│').count(), 4);
    }

    #[test]
    fn separator_interior_junctions_uniform() {
        let header = make_row(&["A", "B", "C"]);
        let rows = vec![make_row(&["1", "2", "3"])];
        let alignment_sets = [
            vec![
                TableAlignment::Left,
                TableAlignment::Left,
                TableAlignment::Left,
            ],
            vec![
                TableAlignment::Left,
                TableAlignment::Center,
                TableAlignment::Right,
            ],
        ];

        for alignments in alignment_sets {
            let lines = render_table(&alignments, &header, &rows, 0..100);
            assert_eq!(lines[2].text, "├───┼───┼───┤");
        }
    }

    #[test]
    fn render_table_cell_capped_at_40() {
        let long_cell = "x".repeat(60);
        let header = make_row(&["Short", "Value"]);
        let rows = vec![make_row(&["a", &long_cell])];
        let alignments = vec![TableAlignment::Left, TableAlignment::Left];

        let lines = render_table(&alignments, &header, &rows, 0..100);
        assert_eq!(lines.len(), 6);
        let border_width = lines[0].text.width();
        assert!(lines.iter().all(|line| line.text.width() == border_width));
        assert!(lines[3].text.contains(&"x".repeat(40)));
        assert!(lines[4].text.contains(&"x".repeat(20)));
    }

    #[test]
    fn cell_overflow_wraps() {
        let long_cell = "x".repeat(60);
        let header = make_row(&["Content", "State"]);
        let rows = vec![make_row(&[&long_cell, "ok"])];
        let alignments = vec![TableAlignment::Left, TableAlignment::Left];

        let lines = render_table(&alignments, &header, &rows, 0..100);
        let border_width = lines[0].text.chars().count();

        assert_eq!(lines.len(), 6);
        assert!(lines
            .iter()
            .all(|line| line.text.chars().count() == border_width));
        assert_eq!(lines[3].text.matches('x').count(), 40);
        assert_eq!(lines[4].text.matches('x').count(), 20);
        assert_eq!(lines[3].text, format!("│ {} │ ok    │", "x".repeat(40)));
        assert_eq!(
            lines[4].text,
            format!("│ {}{} │       │", "x".repeat(20), " ".repeat(20))
        );
    }

    #[test]
    fn cell_overflow_cjk_wraps() {
        let long_cell = "東".repeat(25);
        let header = make_row(&["Content"]);
        let rows = vec![make_row(&[&long_cell])];
        let alignments = vec![TableAlignment::Left];

        let lines = render_table(&alignments, &header, &rows, 0..100);
        let border_width = lines[0].text.width();

        assert_eq!(lines.len(), 6);
        assert!(lines.iter().all(|line| line.text.width() == border_width));
        assert_eq!(lines[3].text.matches('東').count(), 20);
        assert_eq!(lines[4].text.matches('東').count(), 5);
    }

    #[test]
    fn cell_overflow_emoji_presentation_sequences_wrap() {
        let sequence = "*\u{fe0f}";
        let long_cell = sequence.repeat(25);
        let header = make_row(&["Content"]);
        let rows = vec![make_row(&[&long_cell])];
        let alignments = vec![TableAlignment::Left];

        let lines = render_table(&alignments, &header, &rows, 0..100);
        let border_width = lines[0].text.width();

        assert_eq!(lines.len(), 6);
        assert!(lines.iter().all(|line| line.text.width() == border_width));
        assert_eq!(lines[3].text.matches(sequence).count(), 20);
        assert_eq!(lines[4].text.matches(sequence).count(), 5);
    }

    #[test]
    fn cell_wrap_height_alignment() {
        let long_cell = "x".repeat(90);
        let header = make_row(&["Content", "Other"]);
        let rows = vec![make_row(&[&long_cell, "ok"])];
        let alignments = vec![TableAlignment::Left, TableAlignment::Left];

        let lines = render_table(&alignments, &header, &rows, 0..100);
        let wrapped_rows = &lines[3..6];

        assert_eq!(lines.len(), 7);
        assert!(wrapped_rows
            .iter()
            .all(|line| line.text.matches('│').count() == 3));
        assert_eq!(wrapped_rows[0].text.split('│').nth(2), Some(" ok    "));
        assert_eq!(wrapped_rows[1].text.split('│').nth(2), Some("       "));
        assert_eq!(wrapped_rows[2].text.split('│').nth(2), Some("       "));
    }

    #[test]
    fn split_cell_text_unit_tests() {
        assert_eq!(split_cell_text("", 4), vec![""]);
        assert_eq!(split_cell_text("abc", 4), vec!["abc"]);
        assert_eq!(split_cell_text("abcd", 4), vec!["abcd"]);
        assert_eq!(split_cell_text("abcdef", 4), vec!["abcd", "ef"]);
        assert_eq!(split_cell_text("abc", 0), vec![""]);
        assert_eq!(split_cell_text("abc", 1), vec!["a", "b", "c"]);
        assert_eq!(split_cell_text("abc東", 4), vec!["abc", "東"]);
        assert_eq!(split_cell_text("東西", 1), vec!["東", "西"]);
        assert_eq!(
            split_cell_text(&format!("{}*\u{fe0f}", "a".repeat(39)), 40),
            vec!["a".repeat(39), "*\u{fe0f}".to_string()]
        );
    }

    #[test]
    fn wrapped_header_lines_have_strong_style() {
        let long_header = "x".repeat(60);
        let header = make_row(&[&long_header]);
        let alignments = vec![TableAlignment::Left];

        let lines = render_table(&alignments, &header, &[], 0..100);

        assert_eq!(lines.len(), 5);
        for line in &lines[1..3] {
            assert_eq!(line.spans.len(), 1);
            assert_eq!(line.spans[0].style, SemanticStyle::Strong);
            assert_eq!(line.spans[0].start_col, 1);
            assert_eq!(line.spans[0].end_col, line.text.chars().count() - 1);
        }
    }

    #[test]
    fn separator_width_matches_border() {
        let header = make_row(&["Name", "Age"]);
        let rows = vec![make_row(&["Alice", "30"])];
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
        let header = make_row(&["Name", "Age"]);
        let rows = vec![make_row(&["Alice", "30"])];
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
        let header = make_row(&["Name", "Age"]);
        let rows = vec![make_row(&["Alice", "30"])];
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
        let header = make_row(&[&long]);
        let rows = vec![make_row(&["short"])];
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
        let header = make_row(&["café", "東京🙂"]);
        let rows = vec![make_row(&["résumé", "大阪🚀"])];
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
        let cells = compute_cells(&make_row(&["café東京🙂"]), 1);
        let lines = build_data_row(
            &cells,
            &[TableAlignment::Left],
            &[cells[0].display_width],
            SemanticStyle::Strong,
        );
        let line = &lines[0];
        let span = line.spans.first().expect("header row should be styled");

        assert_eq!(span.start_col, 1);
        assert_eq!(span.end_col, line.text.chars().count() - 1);
        assert_eq!(span_text(line, span), " café東京🙂 ");
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
        let header = make_row(&["Column"]);
        let rows = vec![make_row(&["value"])];
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
