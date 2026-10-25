//! Cached rendered occurrence counts with parser-owned source positions.

use unicode_width::UnicodeWidthStr;

use crate::style::{LineKind, RenderedLine, RenderedSearch};

/// Occurrence counts for the active nonempty rendered search.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SearchStatus {
    /// Last occurrence through the cursor's logical source line, or zero before the first.
    pub current: usize,
    /// Total nonoverlapping occurrences in searchable rendered content rows.
    pub total: usize,
}

pub(super) struct SearchIndex {
    pub(super) pattern: String,
    pub(super) width: u16,
    lines: Vec<usize>,
}

impl SearchIndex {
    pub(super) fn build<'a>(
        search: &RenderedSearch,
        width: u16,
        rows: impl IntoIterator<Item = &'a RenderedLine>,
        source_lines: &[usize],
    ) -> Self {
        let mut lines = Vec::new();
        for row in rows {
            if row.kind != LineKind::Content {
                continue;
            }
            for start in search.find_matches(&row.styled.text) {
                let column = row.styled.text[..start].width();
                let offset = row
                    .atoms
                    .iter()
                    .find(|atom| atom.columns.contains(&column) || atom.columns.start == column)
                    .and_then(|atom| atom.source.as_ref())
                    .map_or(row.source.start, |source| source.start);
                lines.push(
                    source_lines
                        .partition_point(|line| *line <= offset)
                        .saturating_sub(1),
                );
            }
        }
        lines.sort_unstable();
        Self {
            pattern: search.pattern.clone(),
            width,
            lines,
        }
    }

    pub(super) fn status(&self, line: usize) -> SearchStatus {
        SearchStatus {
            current: self.lines.partition_point(|found| *found <= line),
            total: self.lines.len(),
        }
    }
}
