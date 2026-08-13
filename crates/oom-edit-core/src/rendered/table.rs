//! Table layout renderer.
//!
//! Renders a markdown table into Unicode box-drawing borders with proper
//! alignment and a per-column cap of 40 display columns (then cell-wrapped).
//!
//! See VW-9 in plan §6.3.2.

use unicode_width::UnicodeWidthStr;

use crate::rendered::blocks::{Inline, InlineLeaf, TableAlignment};
use crate::rendered::wrap::MappedLine;
#[cfg(test)]
use crate::style::Span;
use crate::style::{SemanticStyle, StyledLine};

/// Maximum display width of a single table cell before wrapping kicks in.
const CELL_CAP: usize = 40;

/// Render a table block into a sequence of styled lines with box-drawing
/// borders.
///
/// `alignments` is one entry per column (Left/Center/Right).
/// `header` is the header row cells.
/// `rows` are the body rows.
#[cfg(test)]
pub fn render_table(
    alignments: &[TableAlignment],
    header: &[Vec<Inline>],
    rows: &[Vec<Vec<Inline>>],
    source_span: std::ops::Range<usize>,
) -> Vec<StyledLine> {
    render_table_with_rows(alignments, header, rows, source_span)
        .into_iter()
        .map(|line| line.into_parts().0)
        .collect()
}

/// One rendered table line plus its logical Markdown row (header is zero).
pub(super) struct RenderedTableLine {
    pub(super) mapped: MappedLine,
    pub(super) logical_row: Option<usize>,
}

impl RenderedTableLine {
    pub(super) fn into_parts(self) -> (StyledLine, Vec<crate::style::RenderedSourceAtom>) {
        self.mapped.into_parts()
    }
}

/// Render table lines while retaining row identity for source mapping.
pub(super) fn render_table_with_rows(
    alignments: &[TableAlignment],
    header: &[Vec<Inline>],
    rows: &[Vec<Vec<Inline>>],
    source_span: std::ops::Range<usize>,
) -> Vec<RenderedTableLine> {
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
    lines.push(RenderedTableLine {
        mapped: build_border_row(alignments, &col_widths, true, true, source_span.clone()),
        logical_row: None,
    });

    // Header row
    lines.extend(
        build_data_row(&cells[0], alignments, &col_widths, SemanticStyle::Strong)
            .into_iter()
            .map(|line| RenderedTableLine {
                mapped: line,
                logical_row: Some(0),
            }),
    );

    // Separator row
    lines.push(RenderedTableLine {
        mapped: build_separator_row(&col_widths),
        logical_row: None,
    });

    // Body rows
    for row_idx in 0..num_body_rows {
        lines.extend(
            build_data_row(
                &cells[row_idx + 1],
                alignments,
                &col_widths,
                SemanticStyle::Text,
            )
            .into_iter()
            .map(|line| RenderedTableLine {
                mapped: line,
                logical_row: Some(row_idx + 1),
            }),
        );
    }

    // Bottom border
    lines.push(RenderedTableLine {
        mapped: build_border_row(alignments, &col_widths, false, true, source_span.clone()),
        logical_row: None,
    });

    lines
}

/// Compute cell text and display width for a row of inlines.
fn compute_cells(row: &[Vec<Inline>], num_cols: usize) -> Vec<Cell> {
    let mut cells = Vec::with_capacity(num_cols);
    for ci in 0..num_cols {
        if ci < row.len() {
            let mapped = inline_to_mapped(&row[ci], SemanticStyle::Text);
            let w = mapped.width();
            cells.push(Cell {
                mapped,
                display_width: w,
            });
        } else {
            cells.push(Cell {
                mapped: MappedLine::default(),
                display_width: 0,
            });
        }
    }
    cells
}

/// Convert a row of inlines while retaining each leaf's source ownership.
fn inline_to_mapped(inlines: &[Inline], style: SemanticStyle) -> MappedLine {
    let mut line = MappedLine::default();
    for inline in inlines {
        match inline {
            Inline::Text(leaf) | Inline::SoftBreak(leaf) | Inline::HardBreak(leaf) => {
                append_leaf(&mut line, leaf, style);
            }
            Inline::Code(leaf) => append_leaf(&mut line, leaf, SemanticStyle::CodeSpan),
            Inline::Emph(inner) => {
                line.append(inline_to_mapped(inner, SemanticStyle::Emphasis));
            }
            Inline::Strong(inner) => {
                line.append(inline_to_mapped(inner, SemanticStyle::Strong));
            }
            Inline::Strike(inner) => {
                line.append(inline_to_mapped(inner, SemanticStyle::Strikethrough));
            }
            Inline::Link { text, .. } => {
                line.append(inline_to_mapped(text, SemanticStyle::Link));
            }
            Inline::Image { alt, .. } => {
                line.append(inline_to_mapped(alt, SemanticStyle::Link));
            }
            Inline::FootnoteRef(leaf) => append_leaf(&mut line, leaf, SemanticStyle::Link),
            Inline::Html(leaf) => append_leaf(&mut line, leaf, SemanticStyle::HtmlRaw),
        }
    }
    line
}

fn append_leaf(line: &mut MappedLine, leaf: &InlineLeaf, style: SemanticStyle) {
    for atom in &leaf.atoms {
        line.push(atom.text.clone(), style, Some(atom.source.clone()));
    }
}

/// A single table cell with its text and display width.
struct Cell {
    mapped: MappedLine,
    display_width: usize,
}

/// Build a border/separator row.
fn build_border_row(
    alignments: &[TableAlignment],
    col_widths: &[usize],
    top: bool,
    _bottom: bool,
    _source_span: std::ops::Range<usize>,
) -> MappedLine {
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
    let mut line = MappedLine::default();
    line.push_generated(&text, SemanticStyle::Text);
    line
}

/// Build a separator row (header/body divider).
fn build_separator_row(col_widths: &[usize]) -> MappedLine {
    let mut text = String::from("├");
    let last = col_widths.len() - 1;
    for (ci, &w) in col_widths.iter().enumerate() {
        text.push_str(&"─".repeat(w + 2));
        text.push(if ci < last { '┼' } else { '┤' });
    }
    let mut line = MappedLine::default();
    line.push_generated(&text, SemanticStyle::Text);
    line
}

/// Split cell text into fixed-width chunks without splitting a character.
fn split_cell_text(line: &MappedLine, max_width: usize) -> Vec<MappedLine> {
    if max_width == 0 {
        return vec![MappedLine::default()];
    }

    let mut lines = Vec::new();
    let mut chunk_start = 0;
    while chunk_start < line.fragments.len() {
        let mut chunk_end = chunk_start;
        let mut used = 0;
        while chunk_end < line.fragments.len() {
            let width = line.fragments[chunk_end].text.width();
            if chunk_end > chunk_start && width > 0 && used + width > max_width {
                break;
            }
            used += width;
            chunk_end += 1;
            if used >= max_width {
                break;
            }
        }
        if chunk_end == chunk_start {
            chunk_end += 1;
        }
        lines.push(MappedLine {
            fragments: line.fragments[chunk_start..chunk_end].to_vec(),
        });
        chunk_start = chunk_end;
    }

    if lines.is_empty() {
        lines.push(MappedLine::default());
    }
    lines
}

/// Build a data row (header or body).
fn build_data_row(
    cells: &[Cell],
    alignments: &[TableAlignment],
    col_widths: &[usize],
    header_style: SemanticStyle,
) -> Vec<MappedLine> {
    let wrapped_cells: Vec<Vec<MappedLine>> = cells
        .iter()
        .enumerate()
        .map(|(ci, cell)| {
            let width = col_widths[ci];
            if cell.display_width > width {
                split_cell_text(&cell.mapped, width)
            } else {
                vec![cell.mapped.clone()]
            }
        })
        .collect();
    let max_lines = wrapped_cells.iter().map(Vec::len).max().unwrap_or(1);
    let mut lines = Vec::with_capacity(max_lines);

    for line_index in 0..max_lines {
        let mut line = MappedLine::default();
        line.push_generated("│", SemanticStyle::Text);
        for (ci, cell_lines) in wrapped_cells.iter().enumerate() {
            let mut chunk = cell_lines.get(line_index).cloned().unwrap_or_default();
            for fragment in &mut chunk.fragments {
                if fragment.style == SemanticStyle::Text {
                    fragment.style = header_style;
                }
            }
            let width = col_widths[ci];
            let padding = width.saturating_sub(chunk.width());

            line.push_generated(" ", SemanticStyle::Text);
            if line_index == 0 {
                match alignments.get(ci) {
                    Some(TableAlignment::Center) => {
                        let left = padding / 2;
                        let right = padding - left;
                        line.push_generated(&" ".repeat(left), SemanticStyle::Text);
                        line.append(chunk);
                        line.push_generated(&" ".repeat(right), SemanticStyle::Text);
                    }
                    Some(TableAlignment::Right) => {
                        line.push_generated(&" ".repeat(padding), SemanticStyle::Text);
                        line.append(chunk);
                    }
                    _ => {
                        line.append(chunk);
                        line.push_generated(&" ".repeat(padding), SemanticStyle::Text);
                    }
                }
            } else {
                line.append(chunk);
                line.push_generated(&" ".repeat(padding), SemanticStyle::Text);
            }
            line.push_generated(" │", SemanticStyle::Text);
        }
        if header_style == SemanticStyle::Strong && line.fragments.len() > 2 {
            let last = line.fragments.len() - 1;
            for fragment in &mut line.fragments[1..last] {
                if fragment.style == SemanticStyle::Text {
                    fragment.style = SemanticStyle::Strong;
                }
            }
        }
        lines.push(line);
    }

    lines
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_leaf(text: &str) -> InlineLeaf {
        let mut mapped = MappedLine::default();
        mapped.push_generated(text, SemanticStyle::Text);
        InlineLeaf {
            text: text.to_string(),
            atoms: mapped
                .fragments
                .into_iter()
                .map(|fragment| crate::rendered::blocks::InlineAtom {
                    text: fragment.text,
                    source: 0..0,
                })
                .collect(),
        }
    }

    fn make_mapped(text: &str) -> MappedLine {
        let mut mapped = MappedLine::default();
        mapped.push_generated(text, SemanticStyle::Text);
        mapped
    }

    fn make_row(texts: &[&str]) -> Vec<Vec<Inline>> {
        texts
            .iter()
            .map(|text| vec![Inline::Text(make_leaf(text))])
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
        let split = |text: &str, width| {
            split_cell_text(&make_mapped(text), width)
                .into_iter()
                .map(|line| line.text())
                .collect::<Vec<_>>()
        };
        assert_eq!(split("", 4), vec![""]);
        assert_eq!(split("abc", 4), vec!["abc"]);
        assert_eq!(split("abcd", 4), vec!["abcd"]);
        assert_eq!(split("abcdef", 4), vec!["abcd", "ef"]);
        assert_eq!(split("abc", 0), vec![""]);
        assert_eq!(split("abc", 1), vec!["a", "b", "c"]);
        assert_eq!(split("abc東", 4), vec!["abc", "東"]);
        assert_eq!(split("東西", 1), vec!["東", "西"]);
        assert_eq!(
            split(&format!("{}*\u{fe0f}", "a".repeat(39)), 40),
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
            assert_eq!(line.spans.len(), 3);
            let strong = &line.spans[1];
            assert_eq!(strong.style, SemanticStyle::Strong);
            assert_eq!(strong.start_col, 1);
            assert_eq!(strong.end_col, line.text.chars().count() - 1);
            assert_eq!(line.spans[0].style, SemanticStyle::Text);
            assert_eq!(line.spans[2].style, SemanticStyle::Text);
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
            if line_index == 1 {
                assert_eq!(line.spans.len(), 3);
                assert_eq!(line.spans[0], span(0, 1, SemanticStyle::Text));
                assert_eq!(
                    line.spans[1],
                    span(1, char_count - 1, SemanticStyle::Strong)
                );
                assert_eq!(
                    line.spans[2],
                    span(char_count - 1, char_count, SemanticStyle::Text)
                );
                let content: String = line
                    .text
                    .chars()
                    .skip(1)
                    .take(char_count.saturating_sub(2))
                    .collect();
                assert_eq!(span_text(line, &line.spans[1]), content);
            } else {
                assert_eq!(line.spans.len(), 1);
                let span = &line.spans[0];
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
        let line = lines[0].clone().into_parts().0;
        let span = line
            .spans
            .iter()
            .find(|span| span.style == SemanticStyle::Strong)
            .expect("header payload should be strong");

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

    fn span(start_col: usize, end_col: usize, style: SemanticStyle) -> Span {
        Span {
            start_col,
            end_col,
            style,
        }
    }
}
