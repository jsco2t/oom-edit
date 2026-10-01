//! Retained block-row storage partitioned from the canonical complete builder.

#[cfg(test)]
use std::cell::Cell;
use std::ops::Range;

use crate::style::{
    JumpTarget, LineKind, RenderedCursor, RenderedLayout, RenderedLine, RenderedPoint,
    RenderedSearch, SearchDirection,
};
use crate::syntax::Highlighter;

use super::{
    link_footer_row, render_block_chunk, rendered_line_numbers, BlockModel, LayoutBoundary,
    RenderedCodeFenceRegion,
};

struct RowChunk {
    rows: Vec<RenderedLine>,
    line_numbers: Vec<Option<usize>>,
    jump_targets: Vec<JumpTarget>,
    links: Vec<(usize, String)>,
    fences: Vec<RenderedCodeFenceRegion>,
    source_delta: isize,
    line_delta: isize,
    marker_delta: isize,
    offsets: Vec<RowOffset>,
}

#[derive(Clone, Copy)]
struct RowOffset {
    start: usize,
    source: isize,
    lines: isize,
}

impl RowChunk {
    fn offset_at(&self, local: usize) -> RowOffset {
        let index = self.offsets.partition_point(|offset| offset.start <= local);
        self.offsets[index.saturating_sub(1)]
    }

    fn shift_sources_after(&mut self, old_end: usize, shift: RowShift) {
        for local in 0..self.rows.len() {
            let offset = self.source_delta + self.offset_at(local).source;
            let current_start = self.rows[local]
                .source
                .start
                .checked_add_signed(offset)
                .unwrap();
            if current_start >= old_end {
                shift_line(&mut self.rows[local], shift.source);
                if let Some(number) = &mut self.line_numbers[local] {
                    *number = number.checked_add_signed(shift.lines).unwrap();
                }
            }
        }
        for fence in &mut self.fences {
            if fence.source.start >= old_end {
                shift_range(&mut fence.source, shift.source);
            }
        }
    }
}

/// One copy of the canonical builder's rows, grouped by semantic block.
pub(crate) struct RetainedRows {
    chunks: Vec<RowChunk>,
    row_prefix: Vec<usize>,
    has_link_footer: bool,
    #[cfg(test)]
    work: RowWork,
    #[cfg(test)]
    selection_reads: Cell<usize>,
}

/// Source and physical-line displacement of blocks after one local edit.
#[derive(Clone, Copy)]
pub(crate) struct RowShift {
    pub(crate) source: isize,
    pub(crate) lines: isize,
}

/// Final-text block replacement and suffix rebasing for one model window.
pub(crate) struct RowWindowChange {
    pub(crate) old_blocks: Range<usize>,
    pub(crate) new_blocks: Range<usize>,
    pub(crate) old_source_end: usize,
    pub(crate) shift: RowShift,
    pub(crate) old_links: usize,
    pub(crate) new_links: usize,
}

#[cfg(test)]
#[derive(Debug, Clone, Copy, Default)]
struct RowWork {
    rebuilt_rows: usize,
    visible_reads: usize,
    retained_body_rows: usize,
}

impl RetainedRows {
    #[cfg(test)]
    pub(crate) fn work(&self) -> (usize, usize, usize) {
        (
            self.work.rebuilt_rows,
            self.work.visible_reads,
            self.work.retained_body_rows,
        )
    }

    #[cfg(test)]
    pub(crate) fn selection_reads(&self) -> usize {
        self.selection_reads.get()
    }

    pub(crate) fn build(
        model: &BlockModel,
        width: u16,
        highlighter: &Highlighter,
        front_matter_collapsed: bool,
    ) -> Self {
        let (layout, fences, boundaries) = RenderedLayout::build_with_boundaries(
            model,
            width,
            highlighter,
            front_matter_collapsed,
        );
        Self::from_complete(layout, fences, &boundaries)
    }

    pub(crate) fn from_complete(
        layout: RenderedLayout,
        fences: Vec<RenderedCodeFenceRegion>,
        boundaries: &[LayoutBoundary],
    ) -> Self {
        let rows = partition(layout.lines, boundaries, |part| &part.rows);
        let line_numbers = partition(layout.line_numbers, boundaries, |part| &part.rows);
        let targets = partition(layout.jump_targets, boundaries, |part| &part.targets);
        let links = partition(layout.link_index, boundaries, |part| &part.links);
        let fences = partition(fences, boundaries, |part| &part.fences);
        let mut row_prefix = vec![0];
        let mut chunks: Vec<RowChunk> = rows
            .into_iter()
            .zip(line_numbers)
            .zip(targets)
            .zip(links)
            .zip(fences)
            .zip(boundaries)
            .map(
                |(((((rows, line_numbers), mut jump_targets), links), mut fences), boundary)| {
                    for target in &mut jump_targets {
                        target.line -= boundary.rows.start;
                    }
                    for fence in &mut fences {
                        fence.rows.start -= boundary.rows.start;
                        fence.rows.end -= boundary.rows.start;
                    }
                    row_prefix.push(row_prefix.last().unwrap() + rows.len());
                    RowChunk {
                        rows,
                        line_numbers,
                        jump_targets,
                        links,
                        fences,
                        source_delta: 0,
                        line_delta: 0,
                        marker_delta: 0,
                        offsets: vec![RowOffset {
                            start: 0,
                            source: 0,
                            lines: 0,
                        }],
                    }
                },
            )
            .collect();
        let footer = chunks
            .pop()
            .expect("builder includes a link-appendix boundary");
        let has_link_footer = !footer.rows.is_empty();
        let link_count = chunks.iter().map(|chunk| chunk.links.len()).sum::<usize>();
        assert_eq!(
            footer.rows.len(),
            if has_link_footer { link_count + 2 } else { 0 }
        );
        assert!(footer.links.is_empty() && footer.fences.is_empty());
        row_prefix.pop();
        Self {
            chunks,
            row_prefix,
            has_link_footer,
            #[cfg(test)]
            work: RowWork {
                rebuilt_rows: 0,
                visible_reads: 0,
                retained_body_rows: 0,
            },
            #[cfg(test)]
            selection_reads: Cell::new(0),
        }
    }

    pub(crate) fn row_count(&self) -> usize {
        self.row_prefix.last().copied().unwrap_or(0)
            + if self.has_link_footer {
                self.link_count() + 2
            } else {
                0
            }
    }

    pub(crate) fn rows_near_source_range(
        &self,
        selected: &Range<usize>,
        model: &BlockModel,
        highlighter: &Highlighter,
        width: u16,
    ) -> Vec<(usize, RenderedLine)> {
        if model.blocks.is_empty() {
            return (0..self.row_count())
                .filter_map(|row| self.row(row).map(|line| (row, line)))
                .collect();
        }
        let first = model
            .blocks
            .partition_point(|block| block.span.end <= selected.start);
        let last = model
            .blocks
            .partition_point(|block| block.span.start < selected.end);
        let start_chunk = first.saturating_sub(1);
        let end_chunk = last.saturating_add(1).min(self.chunks.len());
        let mut lines = Vec::new();
        for chunk in start_chunk..end_chunk {
            let start = self.row_prefix[chunk];
            if self.chunks[chunk].marker_delta == 0 {
                lines.extend(
                    (start..self.row_prefix[chunk + 1])
                        .filter_map(|row| self.row(row).map(|line| (row, line))),
                );
            } else {
                let replacement = render_block_chunk(
                    model,
                    chunk,
                    width,
                    highlighter,
                    self.preceding_content_source(chunk),
                    chunk > 0,
                    self.chunks[..chunk]
                        .iter()
                        .map(|part| part.links.len())
                        .sum(),
                );
                assert_eq!(replacement.lines.len(), self.chunks[chunk].rows.len());
                lines.extend(
                    replacement
                        .lines
                        .into_iter()
                        .enumerate()
                        .map(|(local, line)| (start + local, line)),
                );
            }
        }
        if end_chunk == self.chunks.len() {
            let end_row = self.row_prefix[end_chunk];
            lines.extend(
                (end_row..self.row_count()).filter_map(|row| self.row(row).map(|line| (row, line))),
            );
        }
        lines
    }

    fn link_count(&self) -> usize {
        self.chunks.iter().map(|chunk| chunk.links.len()).sum()
    }

    pub(crate) fn block_link_count(&self, index: usize) -> Option<usize> {
        self.chunks.get(index).map(|chunk| chunk.links.len())
    }

    /// Splice complete code-source lines while retaining every untouched row.
    /// Existing suffix atoms are rebased only when read.
    pub(crate) fn splice_fence_interior(
        &mut self,
        index: usize,
        edit: &crate::vim::TextEdit,
        model: &BlockModel,
        highlighter: &Highlighter,
        shift: RowShift,
    ) -> bool {
        if !highlighter.fence_edit_is_incremental() {
            return false;
        }
        let Some(super::BlockKind::CodeFence {
            lang,
            content_span,
            indented: false,
            ..
        }) = model.blocks.get(index).map(|block| &block.kind)
        else {
            return false;
        };
        let Some(chunk) = self.chunks.get(index) else {
            return false;
        };
        if !chunk.links.is_empty() || !chunk.jump_targets.is_empty() || chunk.marker_delta != 0 {
            return false;
        }
        let current_start = |local: usize| {
            chunk.rows[local]
                .source
                .start
                .checked_add_signed(chunk.source_delta + chunk.offset_at(local).source)
        };
        let current_end = |local: usize| {
            chunk.rows[local]
                .source
                .end
                .checked_add_signed(chunk.source_delta + chunk.offset_at(local).source)
        };
        let source_delta =
            edit.new_text_len as isize - (edit.range.end - edit.range.start) as isize;
        let changed = highlighter.fence_changed_source_range();
        let affected_start =
            changed.map_or(edit.range.start, |range| range.start.min(edit.range.start));
        let changed_end = changed
            .map_or(edit.range.start + edit.new_text_len, |range| {
                range.end.max(edit.range.start + edit.new_text_len)
            })
            .min(content_span.end);
        let Some(affected_old_end) = changed_end.checked_add_signed(-source_delta) else {
            return false;
        };
        let affected_old_end = affected_old_end.max(edit.range.end);
        let start_local = (0..chunk.rows.len()).find(|&local| {
            chunk.rows[local].kind == LineKind::Content
                && current_start(local).is_some_and(|start| start <= affected_start)
                && current_end(local).is_some_and(|end| affected_start < end)
        });
        let Some(start_local) = start_local else {
            return false;
        };
        let after_local = (start_local + 1..chunk.rows.len())
            .find(|&local| {
                chunk.rows[local].kind != LineKind::Content
                    || current_start(local).is_some_and(|start| start >= affected_old_end)
            })
            .unwrap_or(chunk.rows.len());
        if chunk.rows[start_local..after_local]
            .iter()
            .any(|row| row.kind != LineKind::Content)
        {
            return false;
        }
        let envelope_start = current_start(start_local).unwrap();
        let old_envelope_end = if after_local == chunk.rows.len()
            || chunk.rows[after_local].kind != LineKind::Content
        {
            current_end(after_local.saturating_sub(1)).unwrap()
        } else {
            current_start(after_local).unwrap()
        };
        if affected_old_end > old_envelope_end {
            return false;
        }
        let old_suffix = chunk.offset_at(after_local);
        let Some(new_envelope_end) = old_envelope_end.checked_add_signed(source_delta) else {
            return false;
        };
        let Some(envelope) = highlighter.text().get(envelope_start..new_envelope_end) else {
            return false;
        };
        let mut sources = Vec::new();
        let mut source_start = envelope_start;
        for line in envelope.split_inclusive('\n') {
            let source_end = source_start + line.len();
            sources.push(source_start..source_end);
            source_start = source_end;
        }
        let added = if sources.is_empty() {
            Vec::new()
        } else {
            let Some(added) = super::code_fence_lines_from_source(
                highlighter,
                lang.as_deref().unwrap_or(""),
                &sources,
            ) else {
                return false;
            };
            added
        };
        let added_count = added.len();
        let removed_count = after_local - start_local;
        let line_delta = added_count as isize - removed_count as isize;
        if shift.source != source_delta || shift.lines != line_delta {
            return false;
        }
        let added_numbers = added
            .iter()
            .map(|row| {
                Some(
                    highlighter
                        .line_starts()
                        .partition_point(|start| *start <= row.source.start),
                )
            })
            .collect::<Vec<_>>();
        let chunk = &mut self.chunks[index];
        let mut offsets = chunk
            .offsets
            .iter()
            .copied()
            .filter(|offset| offset.start < start_local)
            .collect::<Vec<_>>();
        if added_count > 0 {
            offsets.push(RowOffset {
                start: start_local,
                source: -chunk.source_delta,
                lines: -chunk.line_delta,
            });
        }
        offsets.push(RowOffset {
            start: start_local + added_count,
            source: old_suffix.source + source_delta,
            lines: old_suffix.lines + line_delta,
        });
        offsets.extend(
            chunk
                .offsets
                .iter()
                .copied()
                .filter(|offset| offset.start > after_local)
                .map(|mut offset| {
                    offset.start = offset.start - removed_count + added_count;
                    offset.source += source_delta;
                    offset.lines += line_delta;
                    offset
                }),
        );
        offsets.sort_by_key(|offset| offset.start);
        offsets.dedup_by(|right, left| {
            if right.start == left.start {
                *left = *right;
                true
            } else {
                right.source == left.source && right.lines == left.lines
            }
        });
        chunk.offsets = offsets;
        chunk.rows.splice(start_local..after_local, added);
        chunk
            .line_numbers
            .splice(start_local..after_local, added_numbers);
        for fence in &mut chunk.fences {
            fence.rows.end = fence.rows.end - removed_count + added_count;
            fence.source.end = fence.source.end.checked_add_signed(source_delta).unwrap();
        }
        for closing in chunk
            .fences
            .iter()
            .filter_map(|fence| fence.rows.end.checked_sub(1))
        {
            if closing == 0
                || chunk.rows[closing].kind != LineKind::Synthetic
                || chunk.rows[closing - 1].kind != LineKind::Content
            {
                continue;
            }
            let previous_offset = chunk.source_delta + chunk.offset_at(closing - 1).source;
            let closing_offset = chunk.source_delta + chunk.offset_at(closing).source;
            // The closing gutter inherits the last body row's source range.
            let mut source = chunk.rows[closing - 1].source.clone();
            shift_range(&mut source, previous_offset - closing_offset);
            chunk.rows[closing].source = source;
        }
        let changed_end = model.blocks[index].span.end;
        let old_end = changed_end.checked_add_signed(-source_delta).unwrap();
        for (part, later) in self.chunks.iter_mut().enumerate().skip(index + 1) {
            if part == model.blocks.len() {
                later.shift_sources_after(
                    old_end,
                    RowShift {
                        source: source_delta,
                        lines: line_delta,
                    },
                );
            } else if model.blocks[part].span.start >= changed_end {
                later.source_delta += source_delta;
                later.line_delta += line_delta;
            }
        }
        for part in index..self.chunks.len() {
            self.row_prefix[part + 1] = self.row_prefix[part] + self.chunks[part].rows.len();
        }
        #[cfg(test)]
        {
            self.work.rebuilt_rows = added_count;
            self.work.visible_reads = 0;
            self.work.retained_body_rows =
                self.row_prefix.last().copied().unwrap_or(0) - added_count;
        }
        true
    }

    pub(crate) fn link_destination_at_row(&self, row: usize) -> Option<&str> {
        let body = *self.row_prefix.last()?;
        let marker = row.checked_sub(body + 2)?;
        self.has_link_footer
            .then(|| self.link_destination(marker))
            .flatten()
    }

    pub(crate) fn fence_regions(&self) -> Vec<RenderedCodeFenceRegion> {
        let mut fences = Vec::new();
        for (chunk, base_row) in self.chunks.iter().zip(&self.row_prefix) {
            fences.extend(chunk.fences.iter().cloned().map(|mut fence| {
                fence.rows.start += *base_row;
                fence.rows.end += *base_row;
                shift_range(&mut fence.source, chunk.source_delta);
                fence
            }));
        }
        fences
    }

    pub(crate) fn jump_targets(&self) -> Vec<JumpTarget> {
        let mut targets = Vec::new();
        let mut prior_links = 0;
        for (chunk, base_row) in self.chunks.iter().zip(&self.row_prefix) {
            let old_first_link = chunk.links.first().map(|(marker, _)| *marker);
            targets.extend(chunk.jump_targets.iter().map(|target| {
                let mut target = target.clone();
                target.line += *base_row;
                if let (crate::style::TargetKind::Link(index), Some(old_first)) =
                    (&mut target.kind, old_first_link)
                {
                    *index = prior_links + index.saturating_sub(old_first);
                }
                target
            }));
            prior_links += chunk.links.len();
        }
        if self.has_link_footer {
            let first = *self.row_prefix.last().unwrap();
            for index in 0..prior_links {
                targets.push(JumpTarget {
                    line: first + index + 2,
                    kind: crate::style::TargetKind::Link(index),
                });
            }
        }
        targets
    }

    pub(crate) fn find_next_match(
        &mut self,
        search: &RenderedSearch,
        cursor: &RenderedCursor,
        direction: SearchDirection,
        model: &BlockModel,
        highlighter: &Highlighter,
        width: u16,
    ) -> Option<usize> {
        for index in 0..model.blocks.len() {
            self.refresh_marker_chunk(index, model, highlighter.text(), highlighter, width);
        }
        super::nav::matching_row_for_cursor(
            cursor,
            direction,
            self.chunks
                .iter()
                .zip(&self.row_prefix)
                .flat_map(|(chunk, base)| {
                    chunk
                        .rows
                        .iter()
                        .enumerate()
                        .filter_map(move |(local, line)| {
                            (line.kind == LineKind::Content && search.matches(&line.styled.text))
                                .then_some(base + local)
                        })
                }),
        )
    }

    fn stored_line(&self, row: usize) -> Option<&RenderedLine> {
        let (chunk, local) = self.chunk_for_row(row)?;
        self.chunks.get(chunk)?.rows.get(local)
    }

    pub(crate) fn boundary_row(&self, cursor: usize, forward: bool) -> Option<usize> {
        if forward {
            (cursor.saturating_add(1)..self.row_count()).find(|&row| self.is_boundary(row))
        } else {
            (0..cursor).rev().find(|&row| self.is_boundary(row))
        }
    }

    fn is_boundary(&self, row: usize) -> bool {
        let body = *self.row_prefix.last().unwrap_or(&0);
        if row >= body {
            return row < self.row_count();
        }
        self.stored_line(row).is_some_and(|line| {
            line.kind == LineKind::Synthetic
                || (line.kind == LineKind::Content
                    && line.styled.text.is_empty()
                    && line.atoms.is_empty())
        })
    }

    pub(crate) fn edge_content_row(&self, last: bool) -> Option<usize> {
        if last {
            (0..self.row_count()).rev().find(|&row| {
                self.stored_line(row)
                    .is_some_and(|line| line.kind == LineKind::Content)
            })
        } else {
            (0..self.row_count()).find(|&row| {
                self.stored_line(row)
                    .is_some_and(|line| line.kind == LineKind::Content)
            })
        }
    }

    pub(crate) fn target_message_at_row(&self, row: usize) -> Option<String> {
        super::nav::target_message_at_row(row, &self.jump_targets(), |index| {
            self.link_destination(index).map(ToOwned::to_owned)
        })
    }

    fn link_destination(&self, mut index: usize) -> Option<&str> {
        for chunk in &self.chunks {
            if index < chunk.links.len() {
                return Some(&chunk.links[index].1);
            }
            index -= chunk.links.len();
        }
        None
    }

    fn chunk_for_row(&self, index: usize) -> Option<(usize, usize)> {
        let chunk = self.row_prefix.partition_point(|start| *start <= index);
        let chunk = chunk.checked_sub(1)?;
        self.chunks
            .get(chunk)?
            .rows
            .get(index - self.row_prefix[chunk])?;
        Some((chunk, index - self.row_prefix[chunk]))
    }

    pub(crate) fn row(&self, index: usize) -> Option<RenderedLine> {
        let body_rows = *self.row_prefix.last()?;
        if index >= body_rows {
            let local = index - body_rows;
            if !self.has_link_footer || index >= self.row_count() {
                return None;
            }
            let link = local
                .checked_sub(2)
                .map(|marker| (marker, self.link_destination(marker).unwrap()));
            return Some(link_footer_row(
                local,
                link,
                self.preceding_content_source(self.chunks.len())
                    .unwrap_or(0..0),
            ));
        }
        let (chunk, local) = self.chunk_for_row(index)?;
        assert_eq!(self.chunks[chunk].marker_delta, 0);
        let mut line = self.chunks[chunk].rows[local].clone();
        let offset = self.chunks[chunk].offset_at(local);
        shift_line(&mut line, self.chunks[chunk].source_delta + offset.source);
        if local == 0
            && chunk > 0
            && line.kind == LineKind::Synthetic
            && line.styled.text.is_empty()
        {
            if let Some(previous) = self.preceding_content_source(chunk) {
                line.source = previous;
            }
        }
        Some(line)
    }

    /// Read a bounded row range without changing the retained store.
    /// Marker changes are rebuilt once for each touched block.
    pub(crate) fn rows_current_range(
        &self,
        range: Range<usize>,
        model: &BlockModel,
        highlighter: &Highlighter,
        width: u16,
    ) -> Vec<(usize, RenderedLine)> {
        let end = range.end.min(self.row_count());
        let mut result = Vec::with_capacity(end.saturating_sub(range.start));
        let mut row = range.start;
        while row < end {
            let Some((chunk, local)) = self.chunk_for_row(row) else {
                if let Some(line) = self.row(row) {
                    result.push((row, line));
                }
                row += 1;
                continue;
            };
            let chunk_end = end.min(self.row_prefix[chunk + 1]);
            if self.chunks[chunk].marker_delta == 0 {
                result.extend(
                    (row..chunk_end).filter_map(|index| self.row(index).map(|line| (index, line))),
                );
            } else {
                let replacement = render_block_chunk(
                    model,
                    chunk,
                    width,
                    highlighter,
                    self.preceding_content_source(chunk),
                    chunk > 0,
                    self.chunks[..chunk]
                        .iter()
                        .map(|part| part.links.len())
                        .sum(),
                );
                #[cfg(test)]
                self.selection_reads
                    .set(self.selection_reads.get() + replacement.lines.len());
                assert_eq!(replacement.lines.len(), self.chunks[chunk].rows.len());
                result.extend(
                    replacement
                        .lines
                        .into_iter()
                        .skip(local)
                        .take(chunk_end - row)
                        .enumerate()
                        .map(|(offset, line)| (row + offset, line)),
                );
            }
            row = chunk_end;
        }
        #[cfg(test)]
        self.selection_reads
            .set(self.selection_reads.get() + result.len());
        result
    }

    /// Identify a row within the contiguous run sharing its source line.
    pub(crate) fn line_identity_at(
        &self,
        row: usize,
        model: &BlockModel,
        highlighter: &Highlighter,
        width: u16,
    ) -> Option<(Range<usize>, usize)> {
        let source = self
            .rows_current_range(row..row + 1, model, highlighter, width)
            .pop()?
            .1
            .source;
        let mut first = row;
        while first > 0 {
            let previous = self
                .rows_current_range(first - 1..first, model, highlighter, width)
                .pop()
                .map(|(_, line)| line.source);
            if previous.as_ref() != Some(&source) {
                break;
            }
            first -= 1;
        }
        Some((source, row - first))
    }

    pub(crate) fn line_number(&self, index: usize) -> Option<usize> {
        let (chunk, local) = self.chunk_for_row(index)?;
        self.chunks[chunk].line_numbers[local].map(|number| {
            number
                .checked_add_signed(
                    self.chunks[chunk].line_delta + self.chunks[chunk].offset_at(local).lines,
                )
                .unwrap()
        })
    }

    pub(crate) fn point_for_offset_in_block(
        &mut self,
        index: usize,
        offset: usize,
        model: &BlockModel,
        text: &str,
        highlighter: &Highlighter,
        width: u16,
    ) -> Option<RenderedCursor> {
        let rows = self.row_prefix.get(index).copied()?..self.row_prefix.get(index + 1).copied()?;
        if matches!(
            model.blocks.get(index).map(|block| &block.kind),
            Some(super::BlockKind::CodeFence { .. })
        ) {
            let chunk = self.chunks.get(index)?;
            let (mut lower, mut upper) = (0, chunk.rows.len());
            while lower < upper {
                let middle = lower + (upper - lower) / 2;
                let before_or_at = chunk.rows[middle]
                    .source
                    .start
                    .checked_add_signed(chunk.source_delta + chunk.offset_at(middle).source)
                    .is_some_and(|source| source <= offset);
                if before_or_at {
                    lower = middle + 1;
                } else {
                    upper = middle;
                }
            }
            let local = lower.saturating_sub(1);
            let row = rows.start + local;
            let line = self.row_current(row, model, text, highlighter, width)?;
            if let Some(atom) = line.atoms.iter().find(|atom| {
                atom.source
                    .as_ref()
                    .is_some_and(|source| source.contains(&offset) || source.start == offset)
            }) {
                return Some(RenderedCursor::at(RenderedPoint {
                    row,
                    column: atom.columns.start,
                }));
            }
            if line.kind == LineKind::Content
                && (line.source.contains(&offset) || line.source.start == offset)
            {
                let column = line
                    .atoms
                    .iter()
                    .find(|atom| atom.source.is_some())
                    .map_or(0, |atom| atom.columns.start);
                return Some(RenderedCursor {
                    line: row,
                    column,
                    desired_column: 0,
                });
            }
        }
        let mut content = None;
        for row in rows {
            let line = self.row_current(row, model, text, highlighter, width)?;
            if let Some(atom) = line.atoms.iter().find(|atom| {
                atom.source
                    .as_ref()
                    .is_some_and(|source| source.contains(&offset) || source.start == offset)
            }) {
                return Some(RenderedCursor::at(RenderedPoint {
                    row,
                    column: atom.columns.start,
                }));
            }
            if line.kind == LineKind::Content
                && (line.source.contains(&offset) || line.source.start == offset)
            {
                let column = line
                    .atoms
                    .iter()
                    .find(|atom| atom.source.is_some())
                    .map_or(0, |atom| atom.columns.start);
                content.get_or_insert(RenderedCursor {
                    line: row,
                    column,
                    desired_column: 0,
                });
            }
        }
        content
    }

    pub(crate) fn row_current(
        &mut self,
        index: usize,
        model: &BlockModel,
        text: &str,
        highlighter: &Highlighter,
        width: u16,
    ) -> Option<RenderedLine> {
        #[cfg(test)]
        {
            self.work.visible_reads += 1;
        }
        if let Some((chunk, _)) = self.chunk_for_row(index) {
            self.refresh_marker_chunk(chunk, model, text, highlighter, width);
        }
        self.row(index)
    }

    pub(crate) fn source_backed_point(
        &mut self,
        point: RenderedPoint,
        model: &BlockModel,
        text: &str,
        highlighter: &Highlighter,
        width: u16,
    ) -> Option<(RenderedPoint, Range<usize>)> {
        let total = self.row_count();
        let center = point.row.min(total.checked_sub(1)?);
        for distance in 0..total {
            for row in [center.checked_sub(distance), center.checked_add(distance)] {
                let Some(row) = row.filter(|row| *row < total) else {
                    continue;
                };
                let line = self.row_current(row, model, text, highlighter, width)?;
                let nearest = line
                    .atoms
                    .iter()
                    .filter(|atom| atom.source.is_some())
                    .min_by_key(|atom| {
                        if point.column < atom.columns.start {
                            atom.columns.start - point.column
                        } else if point.column >= atom.columns.end {
                            point.column - atom.columns.end + 1
                        } else {
                            0
                        }
                    });
                if let Some(atom) = nearest {
                    return Some((
                        RenderedPoint {
                            row,
                            column: atom.columns.start,
                        },
                        atom.source
                            .clone()
                            .expect("source-backed atom was selected"),
                    ));
                }
            }
        }
        None
    }

    pub(crate) fn cursor_for_row(
        &mut self,
        row: usize,
        desired_column: usize,
        model: &BlockModel,
        text: &str,
        highlighter: &Highlighter,
        width: u16,
    ) -> RenderedCursor {
        let row = row.min(self.row_count().saturating_sub(1));
        let column = self
            .row_current(row, model, text, highlighter, width)
            .and_then(|line| {
                line.atoms
                    .into_iter()
                    .filter(|atom| atom.source.is_some())
                    .min_by_key(|atom| atom.columns.start.abs_diff(desired_column))
            })
            .map_or(0, |atom| atom.columns.start);
        RenderedCursor {
            line: row,
            column,
            desired_column,
        }
    }

    fn preceding_content_source(&self, chunk: usize) -> Option<Range<usize>> {
        self.chunks[..chunk].iter().rev().find_map(|part| {
            part.rows
                .iter()
                .enumerate()
                .rev()
                .find_map(|(local, line)| {
                    (line.kind == LineKind::Content).then(|| {
                        let mut source = line.source.clone();
                        shift_range(
                            &mut source,
                            part.source_delta + part.offset_at(local).source,
                        );
                        source
                    })
                })
        })
    }

    pub(crate) fn replace_window(
        &mut self,
        change: RowWindowChange,
        model: &BlockModel,
        highlighter: &Highlighter,
        width: u16,
    ) -> bool {
        let RowWindowChange {
            old_blocks,
            new_blocks,
            old_source_end,
            shift,
            old_links,
            new_links,
        } = change;
        if old_blocks.start >= old_blocks.end
            || new_blocks.start != old_blocks.start
            || new_blocks.start >= new_blocks.end
            || old_blocks.end > self.chunks.len()
            || new_blocks.end > model.blocks.len()
            || self.chunks.len() - old_blocks.len() + new_blocks.len() != model.blocks.len() + 1
            || self.chunks[old_blocks.clone()]
                .iter()
                .map(|chunk| chunk.links.len())
                .sum::<usize>()
                != old_links
        {
            return false;
        }
        let mut preceding = self.preceding_content_source(old_blocks.start);
        let mut marker = self.chunks[..old_blocks.start]
            .iter()
            .map(|chunk| chunk.links.len())
            .sum::<usize>();
        let mut replacements = Vec::with_capacity(new_blocks.len());
        for index in new_blocks.clone() {
            let rendered = render_block_chunk(
                model,
                index,
                width,
                highlighter,
                preceding.clone(),
                index > 0,
                marker,
            );
            marker += rendered.link_index.len();
            preceding = rendered
                .lines
                .iter()
                .rev()
                .find(|line| line.kind == LineKind::Content)
                .map(|line| line.source.clone())
                .or(preceding);
            replacements.push(RowChunk {
                line_numbers: rendered_line_numbers_indexed(
                    &rendered.lines,
                    highlighter.line_starts(),
                ),
                rows: rendered.lines,
                jump_targets: rendered.jump_targets,
                links: rendered.link_index,
                fences: rendered.code_fence_regions,
                source_delta: 0,
                line_delta: 0,
                marker_delta: 0,
                offsets: vec![RowOffset {
                    start: 0,
                    source: 0,
                    lines: 0,
                }],
            });
        }
        if replacements
            .iter()
            .map(|chunk| chunk.links.len())
            .sum::<usize>()
            != new_links
        {
            return false;
        }
        #[cfg(test)]
        let rebuilt_rows: usize = replacements.iter().map(|chunk| chunk.rows.len()).sum();
        let link_delta = new_links as isize - old_links as isize;
        if link_delta != 0
            && self
                .chunks
                .last()
                .is_some_and(|chunk| !chunk.links.is_empty())
        {
            return false;
        }
        self.chunks.splice(old_blocks.clone(), replacements);
        for (index, chunk) in self.chunks.iter_mut().enumerate().skip(new_blocks.end) {
            if index == model.blocks.len() {
                chunk.shift_sources_after(old_source_end, shift);
            } else {
                chunk.source_delta += shift.source;
                chunk.line_delta += shift.lines;
            }
            if !chunk.links.is_empty() {
                chunk.marker_delta += link_delta;
            }
        }
        self.row_prefix.truncate(old_blocks.start + 1);
        for chunk in &self.chunks[old_blocks.start..] {
            self.row_prefix
                .push(self.row_prefix.last().unwrap() + chunk.rows.len());
        }
        self.has_link_footer = self.link_count() > 0;
        if link_delta != 0 {
            let changing_digits = (new_blocks.end..model.blocks.len())
                .filter(|&index| {
                    self.chunks[index].links.iter().any(|(marker, _)| {
                        let old = marker
                            .checked_add_signed(self.chunks[index].marker_delta - link_delta)
                            .unwrap();
                        let new = marker
                            .checked_add_signed(self.chunks[index].marker_delta)
                            .unwrap();
                        decimal_digits(old) != decimal_digits(new)
                    })
                })
                .collect::<Vec<_>>();
            for index in changing_digits {
                self.refresh_marker_chunk(index, model, highlighter.text(), highlighter, width);
            }
        }
        #[cfg(test)]
        {
            self.work = RowWork {
                rebuilt_rows,
                visible_reads: 0,
                retained_body_rows: self
                    .row_prefix
                    .last()
                    .copied()
                    .unwrap_or(0)
                    .saturating_sub(rebuilt_rows),
            };
        }
        true
    }

    pub(crate) fn replace_local_block(
        &mut self,
        index: usize,
        model: &BlockModel,
        text: &str,
        highlighter: &Highlighter,
        width: u16,
        shift: RowShift,
    ) -> bool {
        let preceding_source = self.preceding_content_source(index);
        let first_marker = self.chunks[..index]
            .iter()
            .map(|chunk| chunk.links.len())
            .sum();
        let replacement = render_block_chunk(
            model,
            index,
            width,
            highlighter,
            preceding_source,
            index > 0,
            first_marker,
        );
        let link_delta =
            replacement.link_index.len() as isize - self.chunks[index].links.len() as isize;
        if link_delta != 0
            && self
                .chunks
                .last()
                .is_some_and(|chunk| !chunk.links.is_empty())
        {
            return false;
        }
        #[cfg(test)]
        {
            self.work = RowWork {
                rebuilt_rows: replacement.lines.len(),
                visible_reads: 0,
                retained_body_rows: self
                    .row_prefix
                    .last()
                    .copied()
                    .unwrap_or(0)
                    .saturating_sub(self.chunks[index].rows.len()),
            };
        }
        self.chunks[index] = RowChunk {
            line_numbers: rendered_line_numbers(
                &replacement.lines,
                text.len(),
                highlighter.line_starts(),
            ),
            rows: replacement.lines,
            jump_targets: replacement.jump_targets,
            links: replacement.link_index,
            fences: replacement.code_fence_regions,
            source_delta: 0,
            line_delta: 0,
            marker_delta: 0,
            offsets: vec![RowOffset {
                start: 0,
                source: 0,
                lines: 0,
            }],
        };
        let changed_end = model.blocks[index].span.end;
        let old_end = changed_end.checked_add_signed(-shift.source).unwrap();
        for (part, chunk) in self.chunks.iter_mut().enumerate().skip(index + 1) {
            if part == model.blocks.len() {
                chunk.shift_sources_after(old_end, shift);
            } else if model.blocks[part].span.start >= changed_end {
                chunk.source_delta += shift.source;
                chunk.line_delta += shift.lines;
            }
            if !chunk.links.is_empty() {
                chunk.marker_delta += link_delta;
            }
        }
        self.has_link_footer = self.link_count() > 0;
        for chunk in index..self.chunks.len() {
            self.row_prefix[chunk + 1] = self.row_prefix[chunk] + self.chunks[chunk].rows.len();
        }
        if link_delta != 0 {
            let changing_digits = (index + 1..model.blocks.len())
                .filter(|&chunk| {
                    self.chunks[chunk].links.iter().any(|(marker, _)| {
                        let old = marker
                            .checked_add_signed(self.chunks[chunk].marker_delta - link_delta)
                            .unwrap();
                        let new = marker
                            .checked_add_signed(self.chunks[chunk].marker_delta)
                            .unwrap();
                        decimal_digits(old) != decimal_digits(new)
                    })
                })
                .collect::<Vec<_>>();
            for chunk in changing_digits {
                self.refresh_marker_chunk(chunk, model, text, highlighter, width);
            }
        }
        true
    }

    fn refresh_marker_chunk(
        &mut self,
        index: usize,
        model: &BlockModel,
        text: &str,
        highlighter: &Highlighter,
        width: u16,
    ) {
        if self.chunks[index].marker_delta == 0 {
            return;
        }
        assert!(
            index < model.blocks.len(),
            "footnote links need a wider rebuild"
        );
        let old_rows = self.chunks[index].rows.len();
        let preceding_source = self.preceding_content_source(index);
        let first_marker = self.chunks[..index]
            .iter()
            .map(|chunk| chunk.links.len())
            .sum();
        let replacement = render_block_chunk(
            model,
            index,
            width,
            highlighter,
            preceding_source,
            index > 0,
            first_marker,
        );
        self.chunks[index] = RowChunk {
            line_numbers: rendered_line_numbers(
                &replacement.lines,
                text.len(),
                highlighter.line_starts(),
            ),
            rows: replacement.lines,
            jump_targets: replacement.jump_targets,
            links: replacement.link_index,
            fences: replacement.code_fence_regions,
            source_delta: 0,
            line_delta: 0,
            marker_delta: 0,
            offsets: vec![RowOffset {
                start: 0,
                source: 0,
                lines: 0,
            }],
        };
        #[cfg(test)]
        {
            self.work.rebuilt_rows += self.chunks[index].rows.len();
            self.work.retained_body_rows = self.work.retained_body_rows.saturating_sub(old_rows);
        }
        if self.chunks[index].rows.len() != old_rows {
            for chunk in index..self.chunks.len() {
                self.row_prefix[chunk + 1] = self.row_prefix[chunk] + self.chunks[chunk].rows.len();
            }
        }
    }

    pub(crate) fn into_complete_current(
        mut self,
        model: &BlockModel,
        text: &str,
        highlighter: &Highlighter,
        width: u16,
    ) -> (
        RenderedLayout,
        Vec<RenderedCodeFenceRegion>,
        Vec<LayoutBoundary>,
    ) {
        for index in 0..model.blocks.len() {
            self.refresh_marker_chunk(index, model, text, highlighter, width);
        }
        let boundaries = self.boundaries();
        let (layout, fences) = self.into_complete();
        (layout, fences, boundaries)
    }

    fn boundaries(&self) -> Vec<LayoutBoundary> {
        let mut boundaries = Vec::with_capacity(self.chunks.len() + 1);
        let (mut row, mut target, mut link, mut fence) = (0, 0, 0, 0);
        for chunk in &self.chunks {
            let end_row = row + chunk.rows.len();
            let end_target = target + chunk.jump_targets.len();
            let end_link = link + chunk.links.len();
            let end_fence = fence + chunk.fences.len();
            boundaries.push(LayoutBoundary {
                rows: row..end_row,
                targets: target..end_target,
                links: link..end_link,
                fences: fence..end_fence,
            });
            (row, target, link, fence) = (end_row, end_target, end_link, end_fence);
        }
        boundaries.push(LayoutBoundary {
            rows: row..self.row_count(),
            targets: target..target + if self.has_link_footer { link } else { 0 },
            links: link..link,
            fences: fence..fence,
        });
        boundaries
    }

    pub(crate) fn into_complete(self) -> (RenderedLayout, Vec<RenderedCodeFenceRegion>) {
        assert!(self.chunks.iter().all(|chunk| chunk.marker_delta == 0));
        let mut layout = RenderedLayout::default();
        let mut fences = Vec::new();
        let mut previous_content: Option<Range<usize>> = None;
        for (mut chunk, base_row) in self.chunks.into_iter().zip(self.row_prefix) {
            let offsets = chunk.offsets.clone();
            for (local, line) in chunk.rows.iter_mut().enumerate() {
                let offset = offsets[offsets.partition_point(|offset| offset.start <= local) - 1];
                shift_line(line, chunk.source_delta + offset.source);
                if local == 0
                    && base_row > 0
                    && line.kind == LineKind::Synthetic
                    && line.styled.text.is_empty()
                {
                    if let Some(source) = &previous_content {
                        line.source = source.clone();
                    }
                }
                if line.kind == LineKind::Content {
                    previous_content = Some(line.source.clone());
                }
            }
            layout.lines.extend(chunk.rows);
            layout
                .line_numbers
                .extend(
                    chunk
                        .line_numbers
                        .into_iter()
                        .enumerate()
                        .map(|(local, number)| {
                            let offset = offsets
                                [offsets.partition_point(|offset| offset.start <= local) - 1];
                            number.map(|number| {
                                number
                                    .checked_add_signed(chunk.line_delta + offset.lines)
                                    .unwrap()
                            })
                        }),
                );
            layout
                .jump_targets
                .extend(chunk.jump_targets.into_iter().map(|mut target| {
                    target.line += base_row;
                    target
                }));
            layout.link_index.extend(chunk.links);
            fences.extend(chunk.fences.into_iter().map(|mut fence| {
                fence.rows.start += base_row;
                fence.rows.end += base_row;
                shift_range(&mut fence.source, chunk.source_delta);
                fence
            }));
        }
        if self.has_link_footer {
            let source = previous_content.unwrap_or(0..0);
            let first = layout.lines.len();
            layout.lines.push(link_footer_row(0, None, source.clone()));
            layout.lines.push(link_footer_row(1, None, source.clone()));
            layout.line_numbers.extend([None, None]);
            for (index, (_, destination)) in layout.link_index.iter().enumerate() {
                layout.lines.push(link_footer_row(
                    index + 2,
                    Some((index, destination)),
                    source.clone(),
                ));
                layout.line_numbers.push(None);
                layout.jump_targets.push(JumpTarget {
                    line: first + index + 2,
                    kind: crate::style::TargetKind::Link(index),
                });
            }
        }
        (layout, fences)
    }
}

fn decimal_digits(mut number: usize) -> usize {
    let mut digits = 1;
    while number >= 10 {
        number /= 10;
        digits += 1;
    }
    digits
}

fn rendered_line_numbers_indexed(
    lines: &[RenderedLine],
    line_starts: &[usize],
) -> Vec<Option<usize>> {
    let mut previous_content_source: Option<Range<usize>> = None;
    lines
        .iter()
        .map(|line| {
            if line.kind != LineKind::Content
                || previous_content_source.as_ref() == Some(&line.source)
            {
                return None;
            }
            previous_content_source = Some(line.source.clone());
            Some(line_starts.partition_point(|start| *start <= line.source.start))
        })
        .collect()
}

fn shift_range(range: &mut Range<usize>, delta: isize) {
    range.start = range.start.checked_add_signed(delta).unwrap();
    range.end = range.end.checked_add_signed(delta).unwrap();
}

fn shift_line(line: &mut RenderedLine, delta: isize) {
    shift_range(&mut line.source, delta);
    for atom in &mut line.atoms {
        if let Some(source) = &mut atom.source {
            shift_range(source, delta);
        }
    }
}

fn partition<T>(
    mut values: Vec<T>,
    boundaries: &[LayoutBoundary],
    range: impl Fn(&LayoutBoundary) -> &Range<usize>,
) -> Vec<Vec<T>> {
    let mut chunks = Vec::with_capacity(boundaries.len());
    for boundary in boundaries.iter().rev() {
        let span = range(boundary);
        let chunk = values.split_off(span.start);
        assert_eq!(chunk.len(), span.end - span.start);
        chunks.push(chunk);
    }
    assert!(values.is_empty());
    chunks.reverse();
    chunks
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frontmatter::front_matter_span;
    use crate::rendered::retained::{ModelEditScope, RetainedBlockModel};
    use crate::rendered::BlockModel;
    use crate::syntax::Highlighter;
    use crate::vim::TextEdit;
    use proptest::prelude::*;

    #[test]
    #[ignore = "exact 1 MiB affected-block row feasibility diagnostic"]
    fn acceptance_1mb_local_block_rows_match_complete_builder() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/kitchen-sink-1mb.md");
        let original = std::fs::read_to_string(path).unwrap();
        assert_eq!(original.len(), 1_048_722);
        for (first_line, ranges) in [
            (600, vec![18003..18518, 18524..18551, 18556..18557]),
            (130, std::iter::once(3948..4243).collect()),
        ] {
            let old_model = BlockModel::build(&original, front_matter_span(&original));
            let old_highlighter = Highlighter::new(&original);
            let old_rows = RetainedRows::build(&old_model, 100, &old_highlighter, false);
            let window_start = original[..ranges[0].start]
                .rfind("\n## ")
                .map_or(0, |at| at + 1);
            let old_window_end = original[ranges.last().unwrap().end..]
                .find("\n## ")
                .map_or(original.len(), |at| ranges.last().unwrap().end + at + 1);
            let mut current = original.clone();
            for range in ranges.iter().rev() {
                current.replace_range(range.clone(), "");
            }
            let removed_bytes = ranges
                .iter()
                .map(|range| range.end - range.start)
                .sum::<usize>();
            let window_end = old_window_end - removed_bytes;
            let new_model = BlockModel::build(&current, front_matter_span(&current));
            let new_highlighter = Highlighter::new(&current);
            let (complete, _, boundaries) =
                RenderedLayout::build_with_boundaries(&new_model, 100, &new_highlighter, false);
            let first = new_model
                .blocks
                .partition_point(|block| block.span.end <= window_start);
            let last = new_model
                .blocks
                .partition_point(|block| block.span.start < window_end);
            assert!(first < last);
            let mut preceding = old_rows.preceding_content_source(first);
            let mut marker = old_rows.chunks[..first]
                .iter()
                .map(|chunk| chunk.links.len())
                .sum::<usize>();
            let old_first = old_model
                .blocks
                .partition_point(|block| block.span.end <= window_start);
            let old_last = old_model
                .blocks
                .partition_point(|block| block.span.start < old_window_end);
            assert_eq!(old_first, first);
            let started = std::time::Instant::now();
            let mut rendered_rows = 0;
            let mut replacements = Vec::new();
            for (index, boundary) in boundaries.iter().enumerate().take(last).skip(first) {
                let chunk = render_block_chunk(
                    &new_model,
                    index,
                    100,
                    &new_highlighter,
                    preceding.clone(),
                    index > 0,
                    marker,
                );
                assert_eq!(chunk.lines, complete.lines[boundary.rows.clone()]);
                let numbers = rendered_line_numbers(
                    &chunk.lines,
                    current.len(),
                    new_highlighter.line_starts(),
                );
                assert_eq!(numbers, complete.line_numbers[boundary.rows.clone()]);
                marker += chunk.link_index.len();
                preceding = chunk
                    .lines
                    .iter()
                    .rev()
                    .find(|line| line.kind == LineKind::Content)
                    .map(|line| line.source.clone())
                    .or(preceding);
                rendered_rows += chunk.lines.len();
                replacements.push(RowChunk {
                    line_numbers: numbers,
                    rows: chunk.lines,
                    jump_targets: chunk.jump_targets,
                    links: chunk.link_index,
                    fences: chunk.code_fence_regions,
                    source_delta: 0,
                    line_delta: 0,
                    marker_delta: 0,
                    offsets: vec![RowOffset {
                        start: 0,
                        source: 0,
                        lines: 0,
                    }],
                });
            }
            let local_ns = started.elapsed().as_nanos();
            assert!(rendered_rows < 100);
            let old_link_count = old_rows.chunks[old_first..old_last]
                .iter()
                .map(|chunk| chunk.links.len())
                .sum::<usize>();
            let new_link_count = replacements
                .iter()
                .map(|chunk| chunk.links.len())
                .sum::<usize>();
            let link_delta = new_link_count as isize - old_link_count as isize;
            let line_delta = -(ranges
                .iter()
                .map(|range| {
                    original[range.clone()]
                        .bytes()
                        .filter(|byte| *byte == b'\n')
                        .count()
                })
                .sum::<usize>() as isize);
            let source_delta = -(removed_bytes as isize);
            let started = std::time::Instant::now();
            let mut projected = old_rows;
            projected.chunks.splice(old_first..old_last, replacements);
            for chunk in projected.chunks.iter_mut().skip(last) {
                chunk.source_delta += source_delta;
                chunk.line_delta += line_delta;
                if !chunk.links.is_empty() {
                    chunk.marker_delta += link_delta;
                }
            }
            projected.row_prefix.clear();
            projected.row_prefix.push(0);
            for chunk in &projected.chunks {
                projected
                    .row_prefix
                    .push(projected.row_prefix.last().unwrap() + chunk.rows.len());
            }
            projected.has_link_footer = projected.link_count() > 0;
            let splice_ns = started.elapsed().as_nanos();
            let (published, published_fences, published_boundaries) =
                projected.into_complete_current(&new_model, &current, &new_highlighter, 100);
            let full_materialization_ns = started.elapsed().as_nanos() - splice_ns;
            let (expected, expected_fences, expected_boundaries) =
                RenderedLayout::build_with_boundaries(&new_model, 100, &new_highlighter, false);
            assert_eq!(published, expected);
            assert_eq!(published_fences, expected_fences);
            assert_eq!(published_boundaries, expected_boundaries);
            println!(
                "PROSE-ROW-WINDOW\t{first_line}\t{}\t{rendered_rows}\t{local_ns}\t{splice_ns}\t{full_materialization_ns}",
                last - first,
            );
        }
    }

    #[test]
    fn exact_prose_window_rows_match_complete_layout_and_indices() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/kitchen-sink-1mb.md");
        let original = std::fs::read_to_string(path).unwrap();
        for ranges in [
            vec![18003..18518, 18524..18551, 18556..18557],
            std::iter::once(3948..4243).collect(),
        ] {
            let edits = ranges
                .into_iter()
                .rev()
                .map(|range| crate::vim::TextEdit {
                    range,
                    new_text_len: 0,
                    new_text: String::new(),
                })
                .collect::<Vec<_>>();
            let edit = crate::vim::collapsed_descending_edit(&original, &edits).unwrap();
            let old_source = Highlighter::new(&original);
            let mut retained_model = super::super::RetainedBlockModel::new(&original);
            let mut rows = RetainedRows::build(retained_model.model(), 100, &old_source, false);
            let window = old_source.local_section_window(&edit).unwrap();
            let scope = retained_model
                .prepare_window_edit(&original, &edit, window)
                .unwrap();
            let mut current = original.clone();
            current.replace_range(edit.range.clone(), &edit.new_text);
            let mut new_source = old_source;
            new_source.apply_edit(&edits);
            let super::super::ModelChange::Window {
                old_blocks,
                new_blocks,
                old_source_end,
                source_delta,
                line_delta,
                old_links,
                new_links,
            } = retained_model.apply_edit(&current, &edit, scope)
            else {
                panic!("exact prose edit must retain a bounded model window");
            };
            assert!(rows.replace_window(
                RowWindowChange {
                    old_blocks,
                    new_blocks,
                    old_source_end,
                    shift: RowShift {
                        source: source_delta,
                        lines: line_delta,
                    },
                    old_links,
                    new_links,
                },
                retained_model.model(),
                &new_source,
                100,
            ));
            let (published, fences, boundaries) =
                rows.into_complete_current(retained_model.model(), &current, &new_source, 100);
            let (complete, expected_fences, expected_boundaries) =
                RenderedLayout::build_with_boundaries(
                    retained_model.model(),
                    100,
                    &new_source,
                    false,
                );
            assert_eq!(published, complete);
            assert_eq!(fences, expected_fences);
            assert_eq!(boundaries, expected_boundaries);
        }
    }

    proptest! {
        #![proptest_config(ProptestConfig {
            cases: 64,
            rng_seed: proptest::test_runner::RngSeed::Fixed(0x0220_2026),
            failure_persistence: None,
            ..ProptestConfig::default()
        })]

        #[test]
        fn disjoint_section_row_splice_matches_complete_build(
            word in "[a-z]{1,12}",
            width in 16u16..80,
            bullet in prop::sample::select(vec!["-", "+", "*"]),
            unicode in any::<bool>(),
            link in any::<bool>(),
        ) {
            let suffix = if unicode { " é λ" } else { " alpha" };
            let link_text = if link { " [linked](/target)" } else { " plain" };
            let first_line = format!("First {word}{suffix}{link_text}\n");
            let second_line = format!("{bullet} second {word}{suffix}\n");
            let original = format!(
                "# Root\n\n## A\n\n{first_line}\n{second_line}{bullet} third item\n\n| Name | Value |\n| --- | --- |\n| {word} | `code` |\n\n## B\n\nTail [link](/tail).\n"
            );
            let first = original.find(&first_line).unwrap();
            let second = original.find(&second_line).unwrap();
            let edits = vec![
                TextEdit { range: second..second + second_line.len(), new_text_len: 0, new_text: String::new() },
                TextEdit { range: first..first + first_line.len(), new_text_len: 0, new_text: String::new() },
            ];
            let edit = crate::vim::collapsed_descending_edit(&original, &edits).unwrap();
            let window_start = original.find("## A").unwrap();
            let window_end = original.find("## B").unwrap();
            let old_source = Highlighter::new(&original);
            let mut retained = RetainedBlockModel::new(&original);
            let mut rows = RetainedRows::build(retained.model(), width, &old_source, false);
            let scope = retained.prepare_window_edit(&original, &edit, window_start..window_end).unwrap();
            let mut current = original.clone();
            current.replace_range(edit.range.clone(), &edit.new_text);
            let mut highlighter = old_source;
            highlighter.apply_edit(&edits);
            let crate::rendered::ModelChange::Window {
                old_blocks, new_blocks, old_source_end, source_delta, line_delta, old_links, new_links,
            } = retained.apply_edit(&current, &edit, scope) else {
                prop_assert!(false, "window unexpectedly widened");
                return Ok(());
            };
            let published = rows.replace_window(
                RowWindowChange {
                    old_blocks, new_blocks, old_source_end,
                    shift: RowShift { source: source_delta, lines: line_delta },
                    old_links, new_links,
                },
                retained.model(), &highlighter, width,
            );
            prop_assert!(published);
            let (actual, fences, boundaries) = rows.into_complete_current(retained.model(), &current, &highlighter, width);
            let fresh_model = BlockModel::build(&current, front_matter_span(&current));
            prop_assert_eq!(retained.model(), &fresh_model);
            let fresh_source = Highlighter::new(&current);
            let (expected, expected_fences, expected_boundaries) =
                RenderedLayout::build_with_boundaries(&fresh_model, width, &fresh_source, false);
            prop_assert_eq!(actual, expected);
            prop_assert_eq!(fences, expected_fences);
            prop_assert_eq!(boundaries, expected_boundaries);
        }
    }

    #[test]
    fn isolated_fence_rows_match_complete_cells_styles_and_atoms() {
        for text in [
            "```rust\nlet s = r###\"open\ninside\n\"###;\n```\n",
            "# Heading\n\n```go\n/* open\ninside\n*/\nfunc main() {}\n```\n",
            "```rust\r\nfn café() { let π = 3; }\r\n```\r\n",
        ] {
            let model = BlockModel::build(text, front_matter_span(text));
            let highlighter = Highlighter::new(text);
            let layout = RenderedLayout::build(&model, 80, &highlighter);
            let lang = model
                .blocks
                .iter()
                .find_map(|block| {
                    if let crate::rendered::BlockKind::CodeFence { lang, .. } = &block.kind {
                        lang.as_deref()
                    } else {
                        None
                    }
                })
                .unwrap_or("");
            let mut checked = 0;
            for line in &layout.lines {
                if line.role == crate::style::RenderedLineRole::CodeFence
                    && line.kind == LineKind::Content
                {
                    let isolated = super::super::code_fence_line_from_source(
                        &highlighter,
                        lang,
                        line.source.clone(),
                    );
                    assert_eq!(isolated.as_ref(), Some(line));
                    checked += 1;
                }
            }
            assert!(checked > 0);
        }
    }

    #[test]
    fn interior_fence_row_splice_matches_complete_after_delete_and_restore() {
        for language in ["rust", "go"] {
            let mut text = format!("# Heading\n\n```{language}\n");
            for index in 0..1200 {
                text.push_str(&format!("fn item_{index}() {{ let value = {index}; }}\n"));
            }
            text.push_str("```\n\nTail.\n");
            let original = text.clone();
            let mut model = RetainedBlockModel::new(&text);
            let mut highlighter = Highlighter::new(&text);
            let (layout, fences, boundaries) =
                RenderedLayout::build_with_boundaries(model.model(), 100, &highlighter, false);
            let mut rows = RetainedRows::from_complete(layout, fences, &boundaries);
            for (target, count) in [(5, 15), (600, 15), (1190, 5)] {
                let start = text.find(&format!("fn item_{target}(")).unwrap();
                let end = start
                    + text[start..]
                        .split_inclusive('\n')
                        .take(count)
                        .map(str::len)
                        .sum::<usize>();
                let removed = text[start..end].to_string();
                for edit in [
                    TextEdit {
                        range: start..end,
                        new_text_len: 0,
                        new_text: String::new(),
                    },
                    TextEdit {
                        range: start..start,
                        new_text_len: removed.len(),
                        new_text: removed.clone(),
                    },
                ] {
                    let scope = model.prepare_edit(&text, &edit);
                    let ModelEditScope::FenceInterior { index, .. } = scope else {
                        panic!("known fence edit should stay local");
                    };
                    highlighter.apply_edit(std::slice::from_ref(&edit));
                    text.replace_range(edit.range.clone(), &edit.new_text);
                    let change = model.apply_edit(&text, &edit, scope);
                    let super::super::ModelChange::FenceInterior {
                        source_delta,
                        line_delta,
                        ..
                    } = change
                    else {
                        panic!("fence edit must stay local");
                    };
                    assert!(
                        rows.splice_fence_interior(
                            index,
                            &edit,
                            model.model(),
                            &highlighter,
                            RowShift {
                                source: source_delta,
                                lines: line_delta
                            },
                        ),
                        "{language} target {target} edit {:?}",
                        edit.range
                    );
                    let fresh_model = BlockModel::build(&text, front_matter_span(&text));
                    let fresh_highlighter = Highlighter::new(&text);
                    let (expected, expected_fences) = RenderedLayout::build_with_fence_regions(
                        &fresh_model,
                        100,
                        &fresh_highlighter,
                        false,
                    );
                    assert_eq!(rows.row_count(), expected.lines.len());
                    for (row, line) in expected.lines.iter().enumerate() {
                        assert_eq!(rows.row(row).as_ref(), Some(line), "{language} row {row}");
                        assert_eq!(rows.line_number(row), expected.line_numbers[row]);
                    }
                    assert_eq!(rows.fence_regions(), expected_fences);
                }
            }
            assert_eq!(text, original);
        }
    }

    #[test]
    fn fence_style_propagation_matches_complete_rows() {
        let mut text = String::from("```rust\nlet value = r#\"start\n");
        for index in 0..120 {
            text.push_str(&format!("content_{index} = 7;\n"));
        }
        text.push_str("\"#;\nlet tail = 9;\n```\n\nAfter.\n");
        let mut model = RetainedBlockModel::new(&text);
        let mut highlighter = Highlighter::new(&text);
        let (layout, fences, boundaries) =
            RenderedLayout::build_with_boundaries(model.model(), 100, &highlighter, false);
        let mut rows = RetainedRows::from_complete(layout, fences, &boundaries);
        let at = text.find("r#\"").unwrap() + 2;
        let edit = TextEdit {
            range: at..at,
            new_text_len: 1,
            new_text: "#".to_string(),
        };
        let scope = model.prepare_edit(&text, &edit);
        let ModelEditScope::FenceInterior { index, .. } = scope else {
            panic!("raw-string delimiter is inside the fence");
        };
        highlighter.apply_edit(std::slice::from_ref(&edit));
        assert!(
            highlighter
                .fence_changed_source_range()
                .is_some_and(|range| range.end > edit.range.end + 100),
            "raw-string delimiter changes highlighting far beyond the edit"
        );
        text.replace_range(edit.range.clone(), &edit.new_text);
        let change = model.apply_edit(&text, &edit, scope);
        let super::super::ModelChange::FenceInterior {
            source_delta,
            line_delta,
            ..
        } = change
        else {
            panic!("raw-string delimiter edit remains local to the fence");
        };
        assert!(rows.splice_fence_interior(
            index,
            &edit,
            model.model(),
            &highlighter,
            RowShift {
                source: source_delta,
                lines: line_delta
            },
        ));
        assert!(rows.work().0 > 100);
        let fresh_model = BlockModel::build(&text, front_matter_span(&text));
        let fresh_highlighter = Highlighter::new(&text);
        assert_eq!(
            highlighter.highlight_lines(0..highlighter.line_starts().len()),
            fresh_highlighter.highlight_lines(0..fresh_highlighter.line_starts().len()),
            "source highlighter must match before comparing rendered rows"
        );
        let (expected, _) =
            RenderedLayout::build_with_fence_regions(&fresh_model, 100, &fresh_highlighter, false);
        for (row, line) in expected.lines.iter().enumerate() {
            assert_eq!(rows.row(row).as_ref(), Some(line), "row {row}");
        }
    }

    #[test]
    fn go_comment_propagation_matches_complete_rows() {
        let mut text = String::from("```go\nvar x = 0; /* begin\n");
        for index in 0..120 {
            text.push_str(&format!("value_{index} := 7\n"));
        }
        text.push_str("*/\nfunc tail() {}\n```\n\nAfter.\n");
        let mut model = RetainedBlockModel::new(&text);
        let mut highlighter = Highlighter::new(&text);
        let (layout, fences, boundaries) =
            RenderedLayout::build_with_boundaries(model.model(), 100, &highlighter, false);
        let mut rows = RetainedRows::from_complete(layout, fences, &boundaries);
        let at = text.find("/* begin").unwrap();
        let edit = TextEdit {
            range: at..at + 2,
            new_text_len: 2,
            new_text: "  ".to_string(),
        };
        let scope = model.prepare_edit(&text, &edit);
        let ModelEditScope::FenceInterior { index, .. } = scope else {
            panic!("comment edit is inside the fence");
        };
        highlighter.apply_edit(std::slice::from_ref(&edit));
        assert!(
            highlighter
                .fence_changed_source_range()
                .is_some_and(|range| range.end > edit.range.end + 100),
            "comment opener changes highlighting far beyond the edit"
        );
        text.replace_range(edit.range.clone(), &edit.new_text);
        let change = model.apply_edit(&text, &edit, scope);
        let super::super::ModelChange::FenceInterior {
            source_delta,
            line_delta,
            ..
        } = change
        else {
            panic!("comment edit remains local to the fence");
        };
        assert!(rows.splice_fence_interior(
            index,
            &edit,
            model.model(),
            &highlighter,
            RowShift {
                source: source_delta,
                lines: line_delta
            },
        ));
        assert!(rows.work().0 > 100);
        let fresh_model = BlockModel::build(&text, front_matter_span(&text));
        let fresh_highlighter = Highlighter::new(&text);
        let (expected, _) =
            RenderedLayout::build_with_fence_regions(&fresh_model, 100, &fresh_highlighter, false);
        for (row, line) in expected.lines.iter().enumerate() {
            assert_eq!(rows.row(row).as_ref(), Some(line), "row {row}");
        }
    }

    #[test]
    fn cold_source_injection_does_not_publish_partially_restyled_rows() {
        let mut text = String::from("```rust\nlet value = r#\"start\n");
        for index in 0..120 {
            text.push_str(&format!("content_{index} = 7;\n"));
        }
        text.push_str("\"#;\n```\n");
        let mut model = RetainedBlockModel::new(&text);
        let warmed = Highlighter::new(&text);
        let (layout, fences, boundaries) =
            RenderedLayout::build_with_boundaries(model.model(), 100, &warmed, false);
        let mut rows = RetainedRows::from_complete(layout, fences, &boundaries);
        let mut cold = Highlighter::new(&text);
        let at = text.find("r#\"").unwrap() + 2;
        let edit = TextEdit {
            range: at..at,
            new_text_len: 1,
            new_text: "#".to_string(),
        };
        let scope = model.prepare_edit(&text, &edit);
        let ModelEditScope::FenceInterior { index, .. } = scope else {
            panic!("code edit remains inside the fence");
        };
        cold.apply_edit(std::slice::from_ref(&edit));
        text.insert(at, '#');
        let change = model.apply_edit(&text, &edit, scope);
        let super::super::ModelChange::FenceInterior {
            source_delta,
            line_delta,
            ..
        } = change
        else {
            panic!("model edit remains inside the fence");
        };
        assert!(!rows.splice_fence_interior(
            index,
            &edit,
            model.model(),
            &cold,
            RowShift {
                source: source_delta,
                lines: line_delta,
            },
        ));
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(32))]
        #[test]
        fn local_unicode_fence_edits_keep_full_source_and_rendered_fidelity(
            language in prop_oneof![Just("rust"), Just("go")],
            crlf in any::<bool>(),
            index in 0usize..64,
            replacement in prop_oneof![Just("é"), Just("🙂"), Just("e"), Just(""), Just("x\n")],
        ) {
            let eol = if crlf { "\r\n" } else { "\n" };
            let mut text = format!("# Title{eol}{eol}```{language}{eol}");
            for line in 0..64 {
                text.push_str(&format!("let value_{line:03} = \"café\";{eol}"));
            }
            text.push_str(&format!("```{eol}{eol}Tail.{eol}"));
            let mut model = RetainedBlockModel::new(&text);
            let mut highlighter = Highlighter::new(&text);
            let (layout, fences, boundaries) =
                RenderedLayout::build_with_boundaries(model.model(), 100, &highlighter, false);
            let mut rows = RetainedRows::from_complete(layout, fences, &boundaries);
            let target = format!("value_{index:03} = \"caf");
            let at = text.find(&target).unwrap() + target.len();
            let edit = TextEdit {
                range: at..at + "é".len(),
                new_text_len: replacement.len(),
                new_text: replacement.to_string(),
            };
            let scope = model.prepare_edit(&text, &edit);
            let ModelEditScope::FenceInterior { index: block_index, .. } = scope else {
                prop_assert!(false, "ordinary code edit must stay inside its fence");
                unreachable!()
            };
            highlighter.apply_edit(std::slice::from_ref(&edit));
            text.replace_range(edit.range.clone(), replacement);
            let change = model.apply_edit(&text, &edit, scope);
            let super::super::ModelChange::FenceInterior {
                source_delta,
                line_delta,
                ..
            } = change else {
                prop_assert!(false, "model edit should stay inside its fence");
                unreachable!()
            };
            let spliced = rows.splice_fence_interior(
                block_index,
                &edit,
                model.model(),
                &highlighter,
                RowShift { source: source_delta, lines: line_delta },
            );
            if !spliced {
                prop_assert!(crlf, "LF code rows must splice locally");
                rows = RetainedRows::build(model.model(), 100, &highlighter, false);
            }
            let fresh_model = BlockModel::build(&text, front_matter_span(&text));
            let fresh_highlighter = Highlighter::new(&text);
            prop_assert_eq!(model.model(), &fresh_model);
            prop_assert_eq!(
                highlighter.highlight_lines(0..highlighter.line_starts().len()),
                fresh_highlighter.highlight_lines(0..fresh_highlighter.line_starts().len()),
            );
            let (expected, expected_fences) = RenderedLayout::build_with_fence_regions(
                &fresh_model,
                100,
                &fresh_highlighter,
                false,
            );
            prop_assert_eq!(rows.row_count(), expected.lines.len());
            for (row, line) in expected.lines.iter().enumerate() {
                let actual = rows.row(row);
                prop_assert_eq!(actual.as_ref(), Some(line), "row {}", row);
                prop_assert_eq!(rows.line_number(row), expected.line_numbers[row]);
            }
            prop_assert_eq!(rows.fence_regions(), expected_fences);
        }
    }

    #[test]
    #[ignore = "exact 1 MiB code-row splice diagnostic is run by its benchmark target"]
    fn acceptance_1mb_fence_row_splice_matches_complete_layout() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/kitchen-sink-1mb.md");
        let original = std::fs::read_to_string(path).unwrap();
        let old_highlighter = Highlighter::new(&original);
        let old_model = BlockModel::build(&original, front_matter_span(&original));
        let old = RenderedLayout::build(&old_model, 100, &old_highlighter);
        let rss_before = std::fs::read_to_string("/proc/self/status")
            .ok()
            .and_then(|status| {
                status.lines().find_map(|line| {
                    line.strip_prefix("VmRSS:")
                        .and_then(|value| value.split_whitespace().next())
                        .and_then(|value| value.parse::<usize>().ok())
                        .map(|kib| kib * 1024)
                })
            })
            .unwrap_or(0);
        let (copy, fences, boundaries) =
            RenderedLayout::build_with_boundaries(&old_model, 100, &old_highlighter, false);
        let retained = RetainedRows::from_complete(copy, fences, &boundaries);
        let row_heap = retained.row_prefix.capacity() * std::mem::size_of::<usize>()
            + retained.chunks.capacity() * std::mem::size_of::<RowChunk>()
            + retained
                .chunks
                .iter()
                .map(|chunk| {
                    chunk.rows.capacity() * std::mem::size_of::<RenderedLine>()
                        + chunk
                            .rows
                            .iter()
                            .map(|line| {
                                line.styled.text.capacity()
                                    + line.styled.spans.capacity()
                                        * std::mem::size_of::<crate::style::Span>()
                                    + line.atoms.capacity()
                                        * std::mem::size_of::<crate::style::RenderedSourceAtom>()
                            })
                            .sum::<usize>()
                        + chunk.line_numbers.capacity() * std::mem::size_of::<Option<usize>>()
                        + chunk.jump_targets.capacity()
                            * std::mem::size_of::<crate::style::JumpTarget>()
                        + chunk.links.capacity() * std::mem::size_of::<(usize, String)>()
                        + chunk
                            .links
                            .iter()
                            .map(|(_, url)| url.capacity())
                            .sum::<usize>()
                        + chunk.fences.capacity()
                            * std::mem::size_of::<crate::rendered::RenderedCodeFenceRegion>()
                })
                .sum::<usize>();
        let rss_after = std::fs::read_to_string("/proc/self/status")
            .ok()
            .and_then(|status| {
                status.lines().find_map(|line| {
                    line.strip_prefix("VmRSS:")
                        .and_then(|value| value.split_whitespace().next())
                        .and_then(|value| value.parse::<usize>().ok())
                        .map(|kib| kib * 1024)
                })
            })
            .unwrap_or(0);
        assert_eq!(retained.row_count(), old.lines.len());
        println!(
            "ROW_HEAP\t{row_heap}\t{}\t{}",
            rss_after.saturating_sub(rss_before),
            retained.row_count()
        );
        for (language, first_line) in [("rust", 3000), ("go", 20000)] {
            let start = original
                .split_inclusive('\n')
                .take(first_line)
                .map(str::len)
                .sum::<usize>();
            let end = start
                + original[start..]
                    .split_inclusive('\n')
                    .take(15)
                    .map(str::len)
                    .sum::<usize>();
            let first_row = old
                .lines
                .iter()
                .position(|line| {
                    line.role == crate::style::RenderedLineRole::CodeFence
                        && line.source.start == start
                        && line.kind == LineKind::Content
                })
                .unwrap();
            let after_row = old
                .lines
                .iter()
                .position(|line| {
                    line.role == crate::style::RenderedLineRole::CodeFence
                        && line.source.start == end
                        && line.kind == LineKind::Content
                })
                .unwrap();
            assert_eq!(after_row - first_row, 15);
            let delta = -((end - start) as isize);
            let edited = format!("{}{}", &original[..start], &original[end..]);
            let fresh_highlighter = Highlighter::new(&edited);
            let fresh_model = BlockModel::build(&edited, front_matter_span(&edited));
            let expected = RenderedLayout::build(&fresh_model, 100, &fresh_highlighter);
            assert_eq!(old.lines.len() - 15, expected.lines.len());
            let read = |row: usize| {
                let prior = if row < first_row { row } else { row + 15 };
                let mut line = old.lines[prior].clone();
                if prior >= after_row && line.source.start >= end {
                    shift_line(&mut line, delta);
                }
                line
            };
            let started = std::time::Instant::now();
            let viewport = (first_row..first_row + 41).map(&read).collect::<Vec<_>>();
            let viewport_ns = started.elapsed().as_nanos();
            assert_eq!(viewport, expected.lines[first_row..first_row + 41]);
            for (row, expected_line) in expected.lines.iter().enumerate() {
                assert_eq!(read(row), *expected_line, "{language} rendered row {row}");
            }
            println!(
                "FENCE_ROWS\t{language}\t{viewport_ns}\t41\t{}",
                expected.lines.len()
            );
        }
    }

    #[test]
    fn partition_and_reassemble_are_cell_and_metadata_exact() {
        let documents = [
            "---\ntitle: Demo\n---\n\n# Heading [link](https://example.invalid)\n\n```rust\nfn main() {}\n```\n",
            "> - nested item\n> - next item\n\n| A | B |\n|---|---|\n| α | `β` |\n",
            "A [^note] and [second](https://example.invalid/second).\n\n[^note]: Footnote body.\n",
        ];
        for text in documents {
            let model = BlockModel::build(text, front_matter_span(text));
            let highlighter = Highlighter::new(text);
            for width in [20, 80] {
                let (layout, fences, boundaries) =
                    RenderedLayout::build_with_boundaries(&model, width, &highlighter, false);
                let expected = (layout.clone(), fences.clone());
                let indexed = RetainedRows::from_complete(layout, fences, &boundaries);
                assert_eq!(indexed.row_count(), expected.0.lines.len());
                for (row, line) in expected.0.lines.iter().enumerate() {
                    assert_eq!(indexed.row(row).as_ref(), Some(line));
                }
                assert_eq!(indexed.row(expected.0.lines.len()), None);
                assert_eq!(indexed.into_complete(), expected);
            }
        }
    }

    #[test]
    fn indexed_pointer_mapping_matches_complete_layout_on_synthetic_and_content_rows() {
        let documents = [
            "# Heading\n\nA [link](https://example.invalid) with a long wrapped line and café.\n\nTail.\n",
            "| A | B |\n|---|---|\n| α | `β` |\n\n```rust\nlet x = 1;\n```\n",
        ];
        for text in documents {
            let model = BlockModel::build(text, front_matter_span(text));
            let highlighter = Highlighter::new(text);
            for width in [12, 40] {
                let (layout, fences, boundaries) =
                    RenderedLayout::build_with_boundaries(&model, width, &highlighter, false);
                let mut indexed = RetainedRows::from_complete(layout.clone(), fences, &boundaries);
                for row in 0..layout.lines.len() + 2 {
                    for column in [0, 1, 5, 40] {
                        let point = RenderedPoint { row, column };
                        let expected =
                            super::super::nav::source_backed_point(point, &layout).map(|point| {
                                let source =
                                    super::super::nav::source_for_point(point, &layout).unwrap();
                                (point, source)
                            });
                        assert_eq!(
                            indexed.source_backed_point(point, &model, text, &highlighter, width,),
                            expected,
                            "width {width}, point {point:?}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn one_local_line_delete_retains_suffix_rows_with_lazy_source_rebase() {
        let before =
            "# Heading\n\nA first line with α.\nA second line with β.\n\nTail paragraph.\n";
        let mut text = before.to_string();
        let mut retained = RetainedBlockModel::new(before);
        let old_highlighter = Highlighter::new(before);
        let (layout, fences, boundaries) =
            RenderedLayout::build_with_boundaries(retained.model(), 20, &old_highlighter, false);
        let mut indexed = RetainedRows::from_complete(layout, fences, &boundaries);
        let start = before.find("A first").unwrap();
        let end = start + before[start..].find('\n').unwrap() + 1;
        let edit = TextEdit {
            range: start..end,
            new_text_len: 0,
            new_text: String::new(),
        };
        let scope = retained.prepare_edit(before, &edit);
        let ModelEditScope::Local { index, .. } = scope else {
            panic!("ordinary paragraph line deletion must retain unrelated blocks");
        };
        text.replace_range(start..end, "");
        retained.apply_edit(&text, &edit, scope);
        let fresh_highlighter = Highlighter::new(&text);
        indexed.replace_local_block(
            index,
            retained.model(),
            &text,
            &fresh_highlighter,
            20,
            RowShift {
                source: -(end as isize - start as isize),
                lines: -1,
            },
        );
        let expected = RenderedLayout::build_with_fence_regions(
            retained.model(),
            20,
            &fresh_highlighter,
            false,
        );
        for (row, line) in expected.0.lines.iter().enumerate() {
            assert_eq!(indexed.row(row).as_ref(), Some(line), "row {row}");
        }
        let (layout, fences, boundaries) =
            indexed.into_complete_current(retained.model(), &text, &fresh_highlighter, 20);
        assert_eq!((layout.clone(), fences.clone()), expected);
        assert_eq!(boundaries.len(), retained.model().blocks.len() + 2);
        let repartitioned = RetainedRows::from_complete(layout, fences, &boundaries);
        assert_eq!(repartitioned.into_complete(), expected);
    }

    #[test]
    fn deleting_one_link_rebases_footer_and_digit_boundary_rows_exactly() {
        let mut text = String::from(
            "# Heading\n\nFirst [link](https://example.invalid/first).\nSecond [link](https://example.invalid/removed).\nThird line.\n\n",
        );
        for index in 0..12 {
            text.push_str(&format!(
                "Paragraph {index} [link](https://example.invalid/{index}).\n\n"
            ));
        }
        let before = text.clone();
        let mut retained = RetainedBlockModel::new(&before);
        let old_highlighter = Highlighter::new(&before);
        let width = 24;
        let (layout, fences, boundaries) =
            RenderedLayout::build_with_boundaries(retained.model(), width, &old_highlighter, false);
        let mut indexed = RetainedRows::from_complete(layout, fences, &boundaries);
        let start = before.find("Second [link]").unwrap();
        let end = start + before[start..].find('\n').unwrap() + 1;
        let edit = TextEdit {
            range: start..end,
            new_text_len: 0,
            new_text: String::new(),
        };
        let scope = retained.prepare_edit(&before, &edit);
        let ModelEditScope::Local { index, .. } = scope else {
            panic!("removing one paragraph line should retain unrelated blocks");
        };
        text.replace_range(start..end, "");
        retained.apply_edit(&text, &edit, scope);
        let highlighter = Highlighter::new(&text);
        indexed.replace_local_block(
            index,
            retained.model(),
            &text,
            &highlighter,
            width,
            RowShift {
                source: -((end - start) as isize),
                lines: -1,
            },
        );
        let expected =
            RenderedLayout::build_with_fence_regions(retained.model(), width, &highlighter, false);
        assert_eq!(indexed.row_count(), expected.0.lines.len());
        assert_eq!(indexed.jump_targets(), expected.0.jump_targets);
        let selected_start = text.find("Paragraph 5").unwrap();
        let selected =
            selected_start..selected_start + text[selected_start..].find('\n').unwrap() + 1;
        let selected_rows = super::super::nav::line_selection_rows_with(
            &selected,
            &text,
            indexed.rows_near_source_range(&selected, retained.model(), &highlighter, width),
        );
        let expected_rows = super::super::nav::line_selection_rows_with(
            &selected,
            &text,
            expected.0.lines.iter().cloned().enumerate(),
        );
        assert_eq!(selected_rows, expected_rows);
        for (row, line) in expected.0.lines.iter().enumerate() {
            assert_eq!(
                indexed
                    .row_current(row, retained.model(), &text, &highlighter, width)
                    .as_ref(),
                Some(line),
                "row {row}"
            );
        }
        let (layout, fences, boundaries) =
            indexed.into_complete_current(retained.model(), &text, &highlighter, width);
        assert_eq!((layout.clone(), fences.clone()), expected);
        let repartitioned = RetainedRows::from_complete(layout, fences, &boundaries);
        assert_eq!(repartitioned.into_complete(), expected);
    }

    #[test]
    fn one_megabyte_link_delete_reads_only_visible_rows_before_full_oracle_check() {
        let before = crate::realistic_fixtures::generate("mixed", 1024 * 1024);
        let mut text = before.clone();
        let mut retained = RetainedBlockModel::new(&before);
        let old_highlighter = Highlighter::new(&before);
        let width = 100;
        let (layout, fences, boundaries) =
            RenderedLayout::build_with_boundaries(retained.model(), width, &old_highlighter, false);
        let mut indexed = RetainedRows::from_complete(layout, fences, &boundaries);
        let start = before
            .split_inclusive('\n')
            .take(6)
            .map(str::len)
            .sum::<usize>();
        let end = start + before[start..].find('\n').unwrap() + 1;
        let edit = TextEdit {
            range: start..end,
            new_text_len: 0,
            new_text: String::new(),
        };
        let scope = retained.prepare_edit(&before, &edit);
        let ModelEditScope::Local { index, .. } = scope else {
            panic!("fixture's ordinary paragraph edit must be local");
        };
        text.replace_range(start..end, "");
        retained.apply_edit(&text, &edit, scope);
        let highlighter = Highlighter::new(&text);
        indexed.replace_local_block(
            index,
            retained.model(),
            &text,
            &highlighter,
            width,
            RowShift {
                source: -((end - start) as isize),
                lines: -1,
            },
        );
        assert!(indexed.work.retained_body_rows > 10_000);
        assert!(indexed.work.rebuilt_rows < 500);
        for row in 0..41 {
            assert!(indexed
                .row_current(row, retained.model(), &text, &highlighter, width)
                .is_some());
        }
        assert_eq!(indexed.work.visible_reads, 41);
        assert!(indexed.work.rebuilt_rows < 500);

        let expected =
            RenderedLayout::build_with_fence_regions(retained.model(), width, &highlighter, false);
        assert_eq!(indexed.row_count(), expected.0.lines.len());
        for (row, line) in expected.0.lines.iter().enumerate() {
            assert_eq!(
                indexed
                    .row_current(row, retained.model(), &text, &highlighter, width)
                    .as_ref(),
                Some(line),
                "row {row}"
            );
        }
        assert_eq!(indexed.into_complete(), expected);
    }
}
