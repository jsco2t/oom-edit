//! Cached rendered occurrence counts with parser-owned source positions.

use crate::style::{RenderedLine, RenderedSearch};

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
    offsets: Vec<usize>,
}

impl SearchIndex {
    pub(super) fn build<'a>(
        search: &RenderedSearch,
        width: u16,
        rows: impl IntoIterator<Item = &'a RenderedLine>,
    ) -> Self {
        Self::from_offsets(
            search,
            width,
            rows.into_iter()
                .flat_map(|row| search.source_match_offsets(row))
                .collect(),
        )
    }

    pub(super) fn from_offsets(
        search: &RenderedSearch,
        width: u16,
        mut offsets: Vec<usize>,
    ) -> Self {
        offsets.sort_unstable();
        Self {
            pattern: search.pattern.clone(),
            width,
            offsets,
        }
    }

    pub(super) fn status(&self, line: usize, source_lines: &[usize]) -> SearchStatus {
        let end = source_lines.get(line + 1).copied().unwrap_or(usize::MAX);
        SearchStatus {
            current: self.offsets.partition_point(|found| *found < end),
            total: self.offsets.len(),
        }
    }
}
