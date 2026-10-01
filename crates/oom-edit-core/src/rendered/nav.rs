//! Rendered navigation, source mapping, search, and Select range handling.
//!
//! This module implements rendered navigation: position mapping between
//! source and rendered coordinates (`enter_rendered` / canonical source offsets), scroll-top
//! calculation (`rendered_scroll_top`), and Rendered-mode key handling
//! (`handle_key`). Together they satisfy VN-1 through VN-6 and VP-1 through
//! VP-4.
//!
//! See architecture §6.4 for the navigation contract.

use std::ops::Range;

use crate::style::{
    JumpTarget, LineKind, RenderedCursor, RenderedLayout, RenderedPoint, RenderedSearch,
    RenderedSelection, RenderedSelectionRow, SearchDirection, SelectionShape, TargetKind,
};

// ── RenderedCursor ─────────────────────────────────────────────────────────────

impl RenderedCursor {
    /// Create a new cursor at the given 0-based line.
    pub fn new(line: usize) -> Self {
        Self {
            line,
            column: 0,
            desired_column: 0,
        }
    }

    /// Create a cursor at a 2D display point.
    pub fn at(point: RenderedPoint) -> Self {
        Self {
            line: point.row,
            column: point.column,
            desired_column: point.column,
        }
    }

    /// Return the renderer-neutral display point.
    pub fn point(self) -> RenderedPoint {
        RenderedPoint {
            row: self.line,
            column: self.column,
        }
    }
}

// ── RenderedSearch ─────────────────────────────────────────────────────────────

impl RenderedSearch {
    /// Create a new search with the given pattern.
    pub fn new(pattern: &str) -> Self {
        Self {
            pattern: pattern.to_string(),
            is_regex: false,
            last_direction: SearchDirection::Forward,
        }
    }

    /// Check if the pattern matches the given text (substring match).
    pub fn matches(&self, text: &str) -> bool {
        text.contains(&self.pattern)
    }

    /// Find all matches in the given text, returning byte offsets.
    pub fn find_matches(&self, text: &str) -> Vec<usize> {
        let mut matches = Vec::new();
        if self.pattern.is_empty() {
            return matches;
        }
        let pattern = &self.pattern;
        let mut search_start = 0;
        while let Some(pos) = text[search_start..].find(pattern) {
            let abs_pos = search_start + pos;
            matches.push(abs_pos);
            search_start = abs_pos + pattern.len();
        }
        matches
    }

    /// Set the search direction.
    pub fn set_direction(&mut self, direction: SearchDirection) {
        self.last_direction = direction;
    }

    /// Get the current search direction.
    pub fn direction(&self) -> SearchDirection {
        self.last_direction
    }

    /// Toggle between regex and literal search.
    pub fn toggle_regex(&mut self) {
        self.is_regex = !self.is_regex;
    }

    /// Check if regex mode is enabled.
    pub fn is_regex(&self) -> bool {
        self.is_regex
    }
}

// ── VP-1 / VP-2 / VP-3: enter_rendered — edit → rendered cursor mapping ───────────

/// Map an edit cursor (line, col) to a rendered cursor line.
///
/// VP-1: The rendered cursor snaps to the rendered row containing the source cursor.
///
/// VP-2: If the source cursor is on a line that wraps, it maps to the first
/// rendered row of that content line.
///
/// VP-3: A source cursor beyond the last content line clamps to the last
/// rendered row.
///
/// `text` is the full document text, needed to convert source byte offsets
/// to document line numbers.
#[cfg(test)]
pub fn enter_rendered(
    edit_line: usize,
    edit_col: usize,
    layout: &RenderedLayout,
    text: &str,
) -> RenderedCursor {
    let last_doc_line = text.bytes().filter(|byte| *byte == b'\n').count();
    if edit_line > last_doc_line {
        return RenderedCursor::new(layout.lines.len().saturating_sub(1));
    }

    let edit_offset = doc_position_to_byte_offset(edit_line, edit_col, text);
    enter_rendered_at_offset(
        edit_line,
        edit_offset,
        layout,
        |offset| {
            text[..offset.min(text.len())]
                .bytes()
                .filter(|byte| *byte == b'\n')
                .count()
        },
        |offset| {
            offset
                .checked_sub(1)
                .and_then(|previous| text.as_bytes().get(previous))
                .is_some_and(|byte| *byte == b'\n')
        },
    )
}

/// Map a source position using the highlighter's maintained line starts.
/// Unlike `enter_rendered`, this does not rescan source prefixes per row.
pub(crate) fn enter_rendered_indexed(
    edit_line: usize,
    edit_col: usize,
    layout: &RenderedLayout,
    text: &str,
    line_starts: &[usize],
) -> RenderedCursor {
    if edit_line >= line_starts.len() {
        return RenderedCursor::new(layout.lines.len().saturating_sub(1));
    }

    let line_start = line_starts[edit_line];
    let line_text = text[line_start..]
        .split_once('\n')
        .map_or(&text[line_start..], |(line, _)| line);
    let col_offset = line_text
        .char_indices()
        .nth(edit_col)
        .map_or(line_text.len(), |(offset, _)| offset);
    let edit_offset = line_start + col_offset;
    enter_rendered_at_offset(
        edit_line,
        edit_offset,
        layout,
        |offset| source_line_for_offset(line_starts, offset),
        |offset| {
            offset
                .checked_sub(1)
                .and_then(|previous| text.as_bytes().get(previous))
                .is_some_and(|byte| *byte == b'\n')
        },
    )
}

/// The largest line start at or before a byte offset.
pub(crate) fn source_line_for_offset(line_starts: &[usize], offset: usize) -> usize {
    line_starts
        .partition_point(|line_start| *line_start <= offset)
        .saturating_sub(1)
}

/// Map a canonical source byte offset to the nearest source-backed rendered
/// atom. The caller owns byte/line conversion so ordinary remaps can stay
/// rope-backed without allocating the complete document.
pub(crate) fn enter_rendered_at_offset(
    edit_line: usize,
    edit_offset: usize,
    layout: &RenderedLayout,
    source_line_for_offset: impl Fn(usize) -> usize,
    byte_before_is_newline: impl Fn(usize) -> bool,
) -> RenderedCursor {
    if let Some((row, atom)) = layout
        .lines
        .iter()
        .enumerate()
        .find_map(|(row, rendered_line)| {
            rendered_line
                .atoms
                .iter()
                .find(|atom| {
                    atom.source.as_ref().is_some_and(|source| {
                        source.contains(&edit_offset) || source.start == edit_offset
                    })
                })
                .map(|atom| (row, atom))
        })
    {
        return RenderedCursor::at(RenderedPoint {
            row,
            column: atom.columns.start,
        });
    }

    if let Some(idx) = layout
        .lines
        .iter()
        .enumerate()
        .filter(|(_, rendered_line)| {
            rendered_line.kind == LineKind::Content
                && (rendered_line.source.contains(&edit_offset)
                    || rendered_line.source.start == edit_offset)
        })
        .min_by_key(|(idx, rendered_line)| {
            (
                rendered_line.source.start != edit_offset,
                rendered_line
                    .source
                    .end
                    .saturating_sub(rendered_line.source.start),
                *idx,
            )
        })
        .map(|(idx, _)| idx)
    {
        return cursor_for_row(idx, 0, layout);
    }

    if let Some(idx) = layout.lines.iter().position(|rendered_line| {
        rendered_line.kind == LineKind::Content
            && rendered_line.source.end == edit_offset
            && !byte_before_is_newline(rendered_line.source.end)
    }) {
        return cursor_for_row(idx, 0, layout);
    }

    let nearest_after = layout
        .lines
        .iter()
        .enumerate()
        .filter_map(|(idx, rendered_line)| {
            if rendered_line.kind != LineKind::Content {
                return None;
            }

            let source_line = source_line_for_offset(rendered_line.source.start);
            (source_line > edit_line
                || (source_line == edit_line && rendered_line.source.start >= edit_offset))
                .then_some((idx, rendered_line.source.start))
        })
        .min_by_key(|(idx, source_start)| (*source_start, *idx));

    if let Some((idx, _)) = nearest_after {
        return cursor_for_row(idx, 0, layout);
    }

    // edit_line is beyond the document — clamp to last rendered row
    if layout.lines.is_empty() {
        RenderedCursor::new(0)
    } else {
        RenderedCursor::new(layout.lines.len().saturating_sub(1))
    }
}

/// Return the canonical source byte offset for a rendered cursor. Preserve
/// the current offset when it still belongs to the cursor's exact atom;
/// otherwise snap to that atom (or the row's nearest source span).
pub(crate) fn canonical_source_offset_for_row(
    cursor: &RenderedCursor,
    current_offset: usize,
    layout: &RenderedLayout,
) -> usize {
    let Some(line) = layout.lines.get(cursor.line) else {
        return layout
            .lines
            .iter()
            .rev()
            .find(|line| line.kind == LineKind::Content)
            .map_or(0, |line| line.source.start);
    };
    canonical_source_offset_for_line(cursor, current_offset, line)
}

pub(crate) fn canonical_source_offset_for_line(
    cursor: &RenderedCursor,
    current_offset: usize,
    line: &crate::style::RenderedLine,
) -> usize {
    let selected_source = line
        .atoms
        .iter()
        .find(|atom| atom.columns.contains(&cursor.column) || atom.columns.start == cursor.column)
        .and_then(|atom| atom.source.as_ref())
        .or_else(|| {
            line.atoms
                .iter()
                .filter_map(|atom| atom.source.as_ref())
                .min_by_key(|source| source.start)
        });
    selected_source.map_or(line.source.start, |source| {
        if source.contains(&current_offset) || source.start == current_offset {
            current_offset
        } else {
            source.start
        }
    })
}

pub(crate) fn cursor_for_row(
    row: usize,
    desired_column: usize,
    layout: &RenderedLayout,
) -> RenderedCursor {
    let row = row.min(layout.lines.len().saturating_sub(1));
    let column = layout
        .lines
        .get(row)
        .and_then(|line| {
            line.atoms
                .iter()
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

fn horizontal_point(
    cursor: &RenderedCursor,
    layout: &RenderedLayout,
    forward: bool,
    count: usize,
) -> Option<RenderedPoint> {
    horizontal_point_with(cursor, layout.lines.len(), forward, count, |row| {
        layout.lines.get(row).cloned()
    })
}

/// The same atom motion over either a complete or retained row source.
pub(crate) fn horizontal_point_with(
    cursor: &RenderedCursor,
    total: usize,
    forward: bool,
    count: usize,
    mut line_at: impl FnMut(usize) -> Option<crate::style::RenderedLine>,
) -> Option<RenderedPoint> {
    let row = cursor.line.min(total.checked_sub(1)?);
    let line = line_at(row)?;
    let source_atoms = line.atoms.iter().filter(|atom| atom.source.is_some());
    let nearest = source_atoms.min_by_key(|atom| {
        if cursor.column < atom.columns.start {
            atom.columns.start - cursor.column
        } else if cursor.column >= atom.columns.end {
            cursor.column - atom.columns.end + 1
        } else {
            0
        }
    });
    let (mut point, remaining) = if let Some(atom) = nearest {
        (
            RenderedPoint {
                row,
                column: atom.columns.start,
            },
            count,
        )
    } else {
        let candidates: Box<dyn Iterator<Item = usize>> = if forward {
            Box::new(row + 1..total)
        } else {
            Box::new((0..row).rev())
        };
        let point = candidates
            .filter_map(|candidate| {
                let line = line_at(candidate)?;
                let atom = if forward {
                    line.atoms.iter().find(|atom| atom.source.is_some())
                } else {
                    line.atoms.iter().rev().find(|atom| atom.source.is_some())
                }?;
                Some(RenderedPoint {
                    row: candidate,
                    column: atom.columns.start,
                })
            })
            .next()?;
        (point, count.saturating_sub(1))
    };

    for _ in 0..remaining {
        let line = line_at(point.row)?;
        let current = line.atoms.iter().position(|atom| {
            atom.source.is_some()
                && (atom.columns.contains(&point.column) || atom.columns.start == point.column)
        })?;
        let adjacent = if forward {
            line.atoms
                .iter()
                .skip(current + 1)
                .find(|atom| atom.source.is_some())
        } else {
            line.atoms[..current]
                .iter()
                .rev()
                .find(|atom| atom.source.is_some())
        };
        if let Some(atom) = adjacent {
            point.column = atom.columns.start;
            continue;
        }
        let candidates: Box<dyn Iterator<Item = usize>> = if forward {
            Box::new(point.row + 1..total)
        } else {
            Box::new((0..point.row).rev())
        };
        let next = candidates
            .filter_map(|candidate| {
                let line = line_at(candidate)?;
                let atom = if forward {
                    line.atoms.iter().find(|atom| atom.source.is_some())
                } else {
                    line.atoms.iter().rev().find(|atom| atom.source.is_some())
                }?;
                Some(RenderedPoint {
                    row: candidate,
                    column: atom.columns.start,
                })
            })
            .next();
        let Some(next) = next else { break };
        point = next;
    }
    Some(point)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum WordClass {
    Whitespace,
    Word,
    Punctuation,
}

fn atom_class(atom: &crate::style::RenderedSourceAtom, text: &str, big: bool) -> WordClass {
    let raw = atom
        .source
        .as_ref()
        .and_then(|source| text.get(source.clone()))
        .unwrap_or_default();
    if raw.chars().all(char::is_whitespace) {
        WordClass::Whitespace
    } else if big
        || raw
            .chars()
            .all(|character| character.is_alphanumeric() || character == '_')
    {
        WordClass::Word
    } else {
        WordClass::Punctuation
    }
}

fn word_point(
    cursor: &RenderedCursor,
    layout: &RenderedLayout,
    text: &str,
    motion: char,
    count: usize,
) -> Option<RenderedPoint> {
    word_point_with(cursor, layout.lines.len(), text, motion, count, |row| {
        layout.lines.get(row).cloned()
    })
}

#[derive(Clone)]
struct AtomPosition {
    point: RenderedPoint,
    atom: crate::style::RenderedSourceAtom,
}

fn atom_position(row: usize, atom: &crate::style::RenderedSourceAtom) -> AtomPosition {
    AtomPosition {
        point: RenderedPoint {
            row,
            column: atom.columns.start,
        },
        atom: atom.clone(),
    }
}

fn adjacent_source_atom(
    current: &AtomPosition,
    forward: bool,
    total: usize,
    line_at: &mut impl FnMut(usize) -> Option<crate::style::RenderedLine>,
) -> Option<AtomPosition> {
    let line = line_at(current.point.row)?;
    let index = line
        .atoms
        .iter()
        .position(|atom| atom.source.is_some() && atom.columns.start == current.point.column)?;
    let within = if forward {
        line.atoms
            .iter()
            .skip(index + 1)
            .find(|atom| atom.source.is_some())
    } else {
        line.atoms[..index]
            .iter()
            .rev()
            .find(|atom| atom.source.is_some())
    };
    if let Some(atom) = within {
        return Some(atom_position(current.point.row, atom));
    }
    if forward {
        for row in current.point.row + 1..total {
            let line = line_at(row)?;
            if let Some(atom) = line.atoms.iter().find(|atom| atom.source.is_some()) {
                return Some(atom_position(row, atom));
            }
        }
    } else {
        for row in (0..current.point.row).rev() {
            let line = line_at(row)?;
            if let Some(atom) = line.atoms.iter().rev().find(|atom| atom.source.is_some()) {
                return Some(atom_position(row, atom));
            }
        }
    }
    None
}

/// Word motion over an indexed source without collecting every document atom.
pub(crate) fn word_point_with(
    cursor: &RenderedCursor,
    total: usize,
    text: &str,
    motion: char,
    count: usize,
    mut line_at: impl FnMut(usize) -> Option<crate::style::RenderedLine>,
) -> Option<RenderedPoint> {
    let current = line_at(cursor.line).and_then(|line| {
        line.atoms
            .iter()
            .find(|atom| {
                atom.source.is_some()
                    && (atom.columns.start == cursor.column
                        || atom.columns.contains(&cursor.column))
            })
            .map(|atom| atom_position(cursor.line, atom))
    });
    let mut current = if let Some(current) = current {
        current
    } else {
        (0..total).find_map(|row| {
            line_at(row).and_then(|line| {
                line.atoms
                    .iter()
                    .find(|atom| atom.source.is_some())
                    .map(|atom| atom_position(row, atom))
            })
        })?
    };
    let big = motion.is_ascii_uppercase();
    for _ in 0..count.max(1) {
        match motion.to_ascii_lowercase() {
            'w' => {
                let class = atom_class(&current.atom, text, big);
                while let Some(next) = adjacent_source_atom(&current, true, total, &mut line_at) {
                    if atom_class(&next.atom, text, big) != class {
                        break;
                    }
                    current = next;
                }
                if let Some(next) = adjacent_source_atom(&current, true, total, &mut line_at) {
                    current = next;
                }
                while atom_class(&current.atom, text, big) == WordClass::Whitespace {
                    let Some(next) = adjacent_source_atom(&current, true, total, &mut line_at)
                    else {
                        break;
                    };
                    current = next;
                }
            }
            'e' => {
                if let Some(next) = adjacent_source_atom(&current, true, total, &mut line_at) {
                    current = next;
                }
                while atom_class(&current.atom, text, big) == WordClass::Whitespace {
                    let Some(next) = adjacent_source_atom(&current, true, total, &mut line_at)
                    else {
                        break;
                    };
                    current = next;
                }
                let class = atom_class(&current.atom, text, big);
                while let Some(next) = adjacent_source_atom(&current, true, total, &mut line_at) {
                    if atom_class(&next.atom, text, big) != class {
                        break;
                    }
                    current = next;
                }
            }
            'b' => {
                if let Some(previous) = adjacent_source_atom(&current, false, total, &mut line_at) {
                    current = previous;
                }
                while atom_class(&current.atom, text, big) == WordClass::Whitespace {
                    let Some(previous) = adjacent_source_atom(&current, false, total, &mut line_at)
                    else {
                        break;
                    };
                    current = previous;
                }
                let class = atom_class(&current.atom, text, big);
                while let Some(previous) =
                    adjacent_source_atom(&current, false, total, &mut line_at)
                {
                    if atom_class(&previous.atom, text, big) != class {
                        break;
                    }
                    current = previous;
                }
            }
            _ => return None,
        }
    }
    Some(current.point)
}

pub(crate) fn edge_point_with(
    row: usize,
    end: bool,
    line: &crate::style::RenderedLine,
) -> Option<RenderedPoint> {
    let atom = if end {
        line.atoms.iter().rev().find(|atom| atom.source.is_some())
    } else {
        line.atoms.iter().find(|atom| atom.source.is_some())
    }?;
    Some(RenderedPoint {
        row,
        column: atom.columns.start,
    })
}

fn edge_point(row: usize, layout: &RenderedLayout, end: bool) -> Option<RenderedPoint> {
    edge_point_with(row, end, layout.lines.get(row)?)
}

pub(crate) fn target_message_at_row(
    row: usize,
    targets: &[JumpTarget],
    mut link_at: impl FnMut(usize) -> Option<String>,
) -> Option<String> {
    let target = targets.iter().find(|target| target.line == row)?;
    match &target.kind {
        TargetKind::Heading(_) => Some(format!("Heading at line {}", target.line + 1)),
        TargetKind::Link(index) => link_at(*index).map(|url| format!("Link: {url}")),
        TargetKind::Footnote => Some(format!("Footnote at line {}", target.line + 1)),
    }
}

/// Project a 2D rendered selection into display intervals and raw-source ranges.
pub fn project_selection(
    anchor: RenderedPoint,
    active: RenderedPoint,
    shape: SelectionShape,
    layout: &RenderedLayout,
    text: &str,
) -> RenderedSelection {
    if layout.lines.is_empty() {
        return RenderedSelection {
            anchor,
            active,
            shape,
            source_ranges: Vec::new(),
            rows: Vec::new(),
            block_width: (shape == SelectionShape::Block).then_some(0),
        };
    }

    let anchor = clamp_point(anchor, layout);
    let active = clamp_point(active, layout);
    let (first, last) = if anchor <= active {
        (anchor, active)
    } else {
        (active, anchor)
    };

    let character_ranges = (shape == SelectionShape::Character)
        .then(|| character_source_ranges(anchor, active, layout))
        .flatten();
    let rows: Vec<RenderedSelectionRow> = match shape {
        SelectionShape::Character => character_ranges.as_ref().map_or_else(
            || {
                (first.row..=last.row)
                    .map(|row| {
                        let start = if row == first.row { first.column } else { 0 };
                        let end = if row == last.row {
                            last.column
                        } else {
                            usize::MAX
                        };
                        selection_row(row, start, end, layout)
                    })
                    .collect()
            },
            |ranges| character_selection_rows(ranges, layout),
        ),
        SelectionShape::Line => {
            let first_source = source_for_point(first, layout)
                .unwrap_or_else(|| layout.lines[first.row].source.clone());
            let last_source = source_for_point(last, layout)
                .unwrap_or_else(|| layout.lines[last.row].source.clone());
            let selected = expand_source_points_to_physical_lines(
                first_source.start.min(last_source.start),
                first_source.end.max(last_source.end),
                text,
            );
            line_selection_rows(&selected, layout, text)
        }
        SelectionShape::Block => {
            let left = anchor.column.min(active.column);
            let right = [anchor, active]
                .into_iter()
                .filter_map(|point| source_atom_for_point(point, layout))
                .map(|atom| atom.columns.end)
                .max()
                .unwrap_or_else(|| anchor.column.max(active.column).saturating_add(1));
            (anchor.row.min(active.row)..=anchor.row.max(active.row))
                .map(|row| {
                    let mut selected = selection_row(row, left, right.saturating_sub(1), layout);
                    selected.columns = single_interval(left..right);
                    selected
                })
                .collect()
        }
    };

    let source_ranges = character_ranges.unwrap_or_else(|| {
        normalize_ranges(
            rows.iter()
                .flat_map(|row| row.source_ranges.iter().cloned())
                .collect(),
        )
    });
    let block_width = (shape == SelectionShape::Block).then(|| {
        rows.first()
            .and_then(|row| row.columns.first())
            .map_or(0, |columns| columns.end.saturating_sub(columns.start))
    });

    RenderedSelection {
        anchor,
        active,
        shape,
        source_ranges,
        rows,
        block_width,
    }
}

/// Project a selection while using canonical source positions to keep
/// character- and line-wise operator ranges stable across layout rebuilds.
pub fn project_selection_from_source_positions(
    anchor: RenderedPoint,
    active: RenderedPoint,
    shape: SelectionShape,
    anchor_source: (usize, usize),
    active_source: (usize, usize),
    layout: &RenderedLayout,
    text: &str,
) -> RenderedSelection {
    let mut selection = project_selection(anchor, active, shape, layout, text);
    match shape {
        SelectionShape::Character => {}
        SelectionShape::Line => {
            let selected = physical_lines_for_source_positions(anchor_source, active_source, text);
            selection.rows = line_selection_rows(&selected, layout, text);
            selection.source_ranges = vec![selected];
        }
        SelectionShape::Block => {}
    }
    selection
}

/// Project only rows involved in a selection, keeping document row numbers.
/// Callers supply all rows that can contribute to the selected source span.
pub(crate) fn project_selection_from_rows(
    anchor: RenderedPoint,
    active: RenderedPoint,
    shape: SelectionShape,
    source_positions: ((usize, usize), (usize, usize)),
    lines: &[(usize, crate::style::RenderedLine)],
    character_ranges: Option<&[Range<usize>]>,
    text: &str,
) -> RenderedSelection {
    let (anchor_source, active_source) = source_positions;
    let empty = || RenderedSelection {
        anchor,
        active,
        shape,
        source_ranges: Vec::new(),
        rows: Vec::new(),
        block_width: (shape == SelectionShape::Block).then_some(0),
    };
    if lines.is_empty() {
        return empty();
    }
    let line_at = |point: RenderedPoint| {
        lines
            .iter()
            .find(|(row, _)| *row == point.row)
            .map(|(_, line)| line)
    };
    let source_at = |point: RenderedPoint| {
        line_at(point).and_then(|line| {
            line.atoms
                .iter()
                .find(|atom| {
                    atom.columns.contains(&point.column) || atom.columns.start == point.column
                })
                .and_then(|atom| atom.source.clone())
        })
    };
    let (first, last) = if anchor <= active {
        (anchor, active)
    } else {
        (active, anchor)
    };
    let selected_source = match (character_ranges, source_at(anchor), source_at(active)) {
        (Some(ranges), _, _) if shape == SelectionShape::Character => Some(ranges.to_vec()),
        (None, Some(anchor), Some(active)) if shape == SelectionShape::Character => {
            let start = anchor.start.min(active.start);
            let end = anchor.end.max(active.end);
            Some(normalize_ranges(
                lines
                    .iter()
                    .flat_map(|(_, line)| &line.atoms)
                    .filter_map(|atom| atom.source.clone())
                    .filter(|source| source.start >= start && source.end <= end)
                    .collect(),
            ))
        }
        _ => None,
    };
    let rows = match shape {
        SelectionShape::Character => selected_source.as_ref().map_or_else(
            || {
                lines
                    .iter()
                    .filter(|(row, _)| first.row <= *row && *row <= last.row)
                    .map(|(row, line)| {
                        let start = if *row == first.row { first.column } else { 0 };
                        let end = if *row == last.row {
                            last.column
                        } else {
                            usize::MAX
                        };
                        selection_row_for_line(*row, start, end, line)
                    })
                    .collect()
            },
            |ranges| {
                character_selection_rows_with(ranges, lines.iter().map(|(row, line)| (*row, line)))
            },
        ),
        SelectionShape::Line => {
            let selected = physical_lines_for_source_positions(anchor_source, active_source, text);
            line_selection_rows_with(&selected, text, lines.iter().cloned())
        }
        SelectionShape::Block => {
            let left = anchor.column.min(active.column);
            let right = [anchor, active]
                .into_iter()
                .filter_map(|point| {
                    line_at(point).and_then(|line| {
                        line.atoms.iter().find(|atom| {
                            atom.source.is_some()
                                && (atom.columns.contains(&point.column)
                                    || atom.columns.start == point.column)
                        })
                    })
                })
                .map(|atom| atom.columns.end)
                .max()
                .unwrap_or_else(|| anchor.column.max(active.column).saturating_add(1));
            lines
                .iter()
                .filter(|(row, _)| first.row <= *row && *row <= last.row)
                .map(|(row, line)| {
                    let mut selected =
                        selection_row_for_line(*row, left, right.saturating_sub(1), line);
                    selected.columns = single_interval(left..right);
                    selected
                })
                .collect()
        }
    };
    let source_ranges = match shape {
        SelectionShape::Line => vec![physical_lines_for_source_positions(
            anchor_source,
            active_source,
            text,
        )],
        _ => selected_source.unwrap_or_else(|| {
            normalize_ranges(
                rows.iter()
                    .flat_map(|row| row.source_ranges.iter().cloned())
                    .collect(),
            )
        }),
    };
    let block_width = (shape == SelectionShape::Block).then(|| {
        rows.first()
            .and_then(|row| row.columns.first())
            .map_or(0, |columns| columns.end.saturating_sub(columns.start))
    });
    RenderedSelection {
        anchor,
        active,
        shape,
        source_ranges,
        rows,
        block_width,
    }
}

/// Widen only mutation ranges whose entire physical line of visible text is
/// selected. Source-less presentation cells do not participate in coverage.
pub(crate) fn character_mutation_ranges(
    selected: &[Range<usize>],
    layout: &RenderedLayout,
    text: &str,
) -> Vec<Range<usize>> {
    character_mutation_ranges_with_lines(selected, layout.lines.iter(), text)
}

pub(crate) fn character_mutation_ranges_with_lines<'a>(
    selected: &[Range<usize>],
    rendered_lines: impl Iterator<Item = &'a crate::style::RenderedLine>,
    text: &str,
) -> Vec<Range<usize>> {
    struct PhysicalLineCoverage {
        start: usize,
        content_end: usize,
        end: usize,
        has_visible: bool,
        all_visible_selected: bool,
    }

    let (Some(first), Some(last)) = (selected.first(), selected.last()) else {
        return Vec::new();
    };
    let start = text[..first.start.min(text.len())]
        .rfind('\n')
        .map_or(0, |newline| newline + 1);
    let limit = last.end.min(text.len());
    let mut lines = Vec::new();
    let mut offset = start;
    while offset < text.len() && offset < limit {
        let end = text[offset..]
            .find('\n')
            .map_or(text.len(), |newline| offset + newline + 1);
        let content_end = if text.as_bytes().get(end.saturating_sub(1)) == Some(&b'\n') {
            end - 1 - usize::from(text.as_bytes().get(end.saturating_sub(2)) == Some(&b'\r'))
        } else {
            end
        };
        lines.push(PhysicalLineCoverage {
            start: offset,
            content_end,
            end,
            has_visible: false,
            all_visible_selected: true,
        });
        offset = end;
    }
    if lines.is_empty() {
        return selected.to_vec();
    }

    for atom in rendered_lines.flat_map(|line| &line.atoms) {
        let Some(source) = &atom.source else {
            continue;
        };
        let index = lines
            .partition_point(|line| line.start <= source.start)
            .saturating_sub(1);
        let Some(line) = lines.get_mut(index) else {
            continue;
        };
        if source.start < line.start || source.start >= line.content_end {
            continue;
        }
        line.has_visible = true;
        let selected_index = selected
            .partition_point(|range| range.start <= source.start)
            .saturating_sub(1);
        if source.end > line.content_end
            || !selected
                .get(selected_index)
                .is_some_and(|range| range.start <= source.start && source.end <= range.end)
        {
            line.all_visible_selected = false;
        }
    }

    let full: Vec<bool> = lines
        .iter()
        .map(|line| line.has_visible && line.all_visible_selected)
        .collect();
    let mut ranges = selected.to_vec();
    for (index, line) in lines.iter().enumerate() {
        let enclosed_blank = !full[index]
            && text[line.start..line.content_end].trim().is_empty()
            && (0..index)
                .rev()
                .find(|&previous| {
                    !text[lines[previous].start..lines[previous].content_end]
                        .trim()
                        .is_empty()
                })
                .is_some_and(|previous| full[previous])
            && (index + 1..lines.len())
                .find(|&next| {
                    !text[lines[next].start..lines[next].content_end]
                        .trim()
                        .is_empty()
                })
                .is_some_and(|next| full[next]);
        if !full[index] && !enclosed_blank {
            continue;
        }
        let mut range_start = line.start;
        if line.end == text.len() && line.content_end == line.end && line.start > 0 {
            range_start -= 1;
            if range_start > 0 && text.as_bytes()[range_start - 1] == b'\r' {
                range_start -= 1;
            }
        }
        ranges.push(range_start..line.end);
    }
    normalize_ranges(ranges)
}

pub(crate) fn source_for_point(
    point: RenderedPoint,
    layout: &RenderedLayout,
) -> Option<Range<usize>> {
    layout.lines.get(point.row).and_then(|line| {
        line.atoms
            .iter()
            .find(|atom| atom.columns.contains(&point.column) || atom.columns.start == point.column)
            .and_then(|atom| atom.source.clone())
    })
}

/// Resolve a display cell to the closest source-backed atom. Synthetic rows
/// use the nearest source-backed row, so decorations never acquire provenance.
pub(crate) fn source_backed_point(
    point: RenderedPoint,
    layout: &RenderedLayout,
) -> Option<RenderedPoint> {
    let center = point.row.min(layout.lines.len().checked_sub(1)?);
    for distance in 0..layout.lines.len() {
        for row in [center.checked_sub(distance), center.checked_add(distance)] {
            let Some(row) = row.filter(|row| *row < layout.lines.len()) else {
                continue;
            };
            let nearest = layout.lines[row]
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
                return Some(RenderedPoint {
                    row,
                    column: atom.columns.start,
                });
            }
        }
    }
    None
}

pub(crate) fn point_for_source_range(
    source_range: &Range<usize>,
    layout: &RenderedLayout,
) -> Option<RenderedPoint> {
    layout
        .lines
        .iter()
        .enumerate()
        .flat_map(|(row, line)| line.atoms.iter().map(move |atom| (row, atom)))
        .filter_map(|(row, atom)| atom.source.as_ref().map(|source| (row, atom, source)))
        .filter(|(_, _, source)| {
            source == &source_range
                || (source.start < source_range.end && source_range.start < source.end)
        })
        .min_by_key(|(row, atom, source)| {
            (
                usize::from(*source != source_range),
                source.start.abs_diff(source_range.start),
                *row,
                atom.columns.start,
            )
        })
        .map(|(row, atom, _)| RenderedPoint {
            row,
            column: atom.columns.start,
        })
}

pub(crate) fn line_identity_for_point(
    point: RenderedPoint,
    layout: &RenderedLayout,
) -> Option<(Range<usize>, usize)> {
    let line = layout.lines.get(point.row)?;
    let ordinal = layout.lines[..point.row]
        .iter()
        .filter(|candidate| candidate.source == line.source)
        .count();
    Some((line.source.clone(), ordinal))
}

pub(crate) fn point_for_line_identity(
    source: &Range<usize>,
    ordinal: usize,
    desired_column: usize,
    layout: &RenderedLayout,
) -> Option<RenderedPoint> {
    let rows: Vec<_> = layout
        .lines
        .iter()
        .enumerate()
        .filter(|(_, line)| &line.source == source)
        .collect();
    let (row, line) = rows.get(ordinal).or_else(|| rows.last()).copied()?;
    let column = line
        .atoms
        .iter()
        .filter(|atom| atom.source.is_some())
        .min_by_key(|atom| atom.columns.start.abs_diff(desired_column))
        .map_or(desired_column, |atom| atom.columns.start);
    Some(RenderedPoint { row, column })
}

fn source_atom_for_point(
    point: RenderedPoint,
    layout: &RenderedLayout,
) -> Option<&crate::style::RenderedSourceAtom> {
    layout.lines.get(point.row).and_then(|line| {
        line.atoms.iter().find(|atom| {
            atom.source.is_some()
                && (atom.columns.contains(&point.column) || atom.columns.start == point.column)
        })
    })
}

pub(crate) fn physical_lines_for_source_positions(
    anchor_source: (usize, usize),
    active_source: (usize, usize),
    text: &str,
) -> Range<usize> {
    let anchor_offset = doc_position_to_byte_offset(anchor_source.0, anchor_source.1, text);
    let active_offset = doc_position_to_byte_offset(active_source.0, active_source.1, text);
    expand_source_points_to_physical_lines(
        anchor_offset.min(active_offset),
        anchor_offset.max(active_offset),
        text,
    )
}

fn line_selection_rows(
    selected: &Range<usize>,
    layout: &RenderedLayout,
    text: &str,
) -> Vec<RenderedSelectionRow> {
    line_selection_rows_with(selected, text, layout.lines.iter().cloned().enumerate())
}

pub(crate) fn line_selection_rows_with(
    selected: &Range<usize>,
    text: &str,
    lines: impl IntoIterator<Item = (usize, crate::style::RenderedLine)>,
) -> Vec<RenderedSelectionRow> {
    lines
        .into_iter()
        .filter_map(|(row, line)| {
            line_row_intersects_source(&line, selected, text).then(|| {
                let source = line
                    .atoms
                    .iter()
                    .filter_map(|atom| atom.source.as_ref())
                    .next()
                    .cloned()
                    .unwrap_or_else(|| line.source.clone());
                let physical = expand_physical_line(&source, text);
                RenderedSelectionRow {
                    row,
                    columns: single_interval(
                        0..line.atoms.last().map_or(0, |atom| atom.columns.end),
                    ),
                    source_ranges: vec![physical],
                }
            })
        })
        .collect()
}

pub(crate) fn line_row_intersects_source(
    line: &crate::style::RenderedLine,
    selected: &Range<usize>,
    text: &str,
) -> bool {
    let source = line
        .atoms
        .iter()
        .filter_map(|atom| atom.source.as_ref())
        .next()
        .unwrap_or(&line.source);
    let physical = expand_physical_line(source, text);
    physical.start < selected.end && selected.start < physical.end
}

fn clamp_point(point: RenderedPoint, layout: &RenderedLayout) -> RenderedPoint {
    let row = point.row.min(layout.lines.len().saturating_sub(1));
    let column = layout.lines[row]
        .atoms
        .iter()
        .filter(|atom| atom.source.is_some())
        .min_by_key(|atom| atom.columns.start.abs_diff(point.column))
        .map_or(0, |atom| atom.columns.start);
    RenderedPoint { row, column }
}

fn selection_row(
    row: usize,
    start_column: usize,
    end_column: usize,
    layout: &RenderedLayout,
) -> RenderedSelectionRow {
    let Some(line) = layout.lines.get(row) else {
        return RenderedSelectionRow {
            row,
            columns: Vec::new(),
            source_ranges: Vec::new(),
        };
    };
    selection_row_for_line(row, start_column, end_column, line)
}

fn selection_row_for_line(
    row: usize,
    start_column: usize,
    end_column: usize,
    line: &crate::style::RenderedLine,
) -> RenderedSelectionRow {
    let selected: Vec<_> = line
        .atoms
        .iter()
        .filter(|atom| {
            atom.source.is_some()
                && atom.columns.end > start_column
                && (end_column == usize::MAX || atom.columns.start <= end_column)
        })
        .collect();
    let columns = selected
        .first()
        .zip(selected.last())
        .map_or(0..0, |(first, last)| first.columns.start..last.columns.end);
    let source_ranges = normalize_ranges(
        selected
            .into_iter()
            .filter_map(|atom| atom.source.clone())
            .collect(),
    );
    RenderedSelectionRow {
        row,
        columns: single_interval(columns),
        source_ranges,
    }
}

fn character_source_ranges(
    anchor: RenderedPoint,
    active: RenderedPoint,
    layout: &RenderedLayout,
) -> Option<Vec<Range<usize>>> {
    let anchor_range = source_for_point(anchor, layout)?;
    let active_range = source_for_point(active, layout)?;
    let start = anchor_range.start.min(active_range.start);
    let end = anchor_range.end.max(active_range.end);
    Some(normalize_ranges(
        layout
            .lines
            .iter()
            .flat_map(|line| &line.atoms)
            .filter_map(|atom| atom.source.clone())
            .filter(|source| source.start >= start && source.end <= end)
            .collect(),
    ))
}

fn single_interval(columns: Range<usize>) -> Vec<Range<usize>> {
    std::iter::once(columns).collect()
}

pub(crate) fn character_selection_rows(
    source_ranges: &[Range<usize>],
    layout: &RenderedLayout,
) -> Vec<RenderedSelectionRow> {
    character_selection_rows_with(source_ranges, layout.lines.iter().enumerate())
}

fn character_selection_rows_with<'a>(
    source_ranges: &[Range<usize>],
    lines: impl IntoIterator<Item = (usize, &'a crate::style::RenderedLine)>,
) -> Vec<RenderedSelectionRow> {
    lines
        .into_iter()
        .filter_map(|(row, line)| {
            let columns = normalize_ranges(
                source_ranges
                    .iter()
                    .flat_map(|range| project_atom_intervals(range, &line.atoms))
                    .collect(),
            );
            if columns.is_empty() {
                return None;
            }
            let row_source_ranges = normalize_ranges(
                line.atoms
                    .iter()
                    .filter_map(|atom| atom.source.clone())
                    .filter(|source| {
                        source_ranges.iter().any(|selected| {
                            source.start < selected.end && selected.start < source.end
                        })
                    })
                    .collect(),
            );
            Some(RenderedSelectionRow {
                row,
                columns,
                source_ranges: row_source_ranges,
            })
        })
        .collect()
}

pub(crate) fn project_atom_intervals(
    source_range: &Range<usize>,
    atoms: &[crate::style::RenderedSourceAtom],
) -> Vec<Range<usize>> {
    let mut projected = Vec::new();
    let mut current: Option<Range<usize>> = None;
    for atom in atoms {
        let intersects = atom.source.as_ref().is_some_and(|source| {
            source.start < source_range.end && source_range.start < source.end
        });
        if !intersects || atom.columns.start >= atom.columns.end {
            if let Some(columns) = current.take() {
                projected.push(columns);
            }
            continue;
        }

        if current
            .as_ref()
            .is_some_and(|columns| columns.end == atom.columns.start)
        {
            current.as_mut().unwrap().end = atom.columns.end;
        } else {
            if let Some(columns) = current.take() {
                projected.push(columns);
            }
            current = Some(atom.columns.clone());
        }
    }
    if let Some(columns) = current {
        projected.push(columns);
    }
    projected
}

fn expand_physical_line(source: &Range<usize>, text: &str) -> Range<usize> {
    let start = source.start.min(text.len());
    let end = source.end.min(text.len());
    let line_start = text[..start].rfind('\n').map_or(0, |newline| newline + 1);
    let line_end = if end > start && text.as_bytes().get(end - 1) == Some(&b'\n') {
        end
    } else {
        text[end..]
            .find('\n')
            .map_or(text.len(), |newline| end + newline + 1)
    };
    line_start..line_end
}

fn expand_source_points_to_physical_lines(first: usize, last: usize, text: &str) -> Range<usize> {
    let first = first.min(text.len());
    let last = last.min(text.len());
    let start = text[..first].rfind('\n').map_or(0, |newline| newline + 1);
    let end = text[last..]
        .find('\n')
        .map_or(text.len(), |newline| last + newline + 1);
    start..end
}

fn normalize_ranges(mut ranges: Vec<Range<usize>>) -> Vec<Range<usize>> {
    ranges.retain(|range| range.start < range.end);
    ranges.sort_by_key(|range| (range.start, range.end));
    let mut normalized: Vec<Range<usize>> = Vec::with_capacity(ranges.len());
    for range in ranges {
        if let Some(previous) = normalized.last_mut() {
            if range.start <= previous.end {
                previous.end = previous.end.max(range.end);
                continue;
            }
        }
        normalized.push(range);
    }
    normalized
}

/// Convert a 0-based document position to a byte offset, clamping positions
/// beyond the line or document to the nearest valid offset.
pub(crate) fn doc_position_to_byte_offset(line: usize, col: usize, text: &str) -> usize {
    let line_start = text
        .split_inclusive('\n')
        .take(line)
        .map(str::len)
        .sum::<usize>();
    let line_text = &text[line_start..];
    let line_text = line_text
        .split_once('\n')
        .map_or(line_text, |(line_text, _)| line_text);
    let col_offset = line_text
        .char_indices()
        .nth(col)
        .map_or(line_text.len(), |(offset, _)| offset);

    line_start + col_offset
}

// ── VN-1 / VN-3 / VN-4 / VN-5 / VN-6: rendered mode key handling ──────────────

/// The result of handling a rendered mode key.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RenderedKeyResult {
    /// Whether the cursor moved.
    pub cursor_moved: bool,
    /// New cursor position, if the cursor moved.
    pub new_cursor: Option<RenderedCursor>,
    /// Whether the search state changed.
    pub search_changed: bool,
    /// New search state, if search was activated or modified.
    pub new_search: Option<RenderedSearch>,
    /// Whether the layout should be recomputed.
    pub layout_dirty: bool,
    /// Whether the fm_collapsed state should be toggled.
    pub fm_collapsed_toggled: bool,
    /// Status message to display (e.g. "Search wrapped", "FM collapsed").
    pub message: Option<String>,
}

/// Whether a rendered command needs to inspect source characters. Ordinary
/// row, atom, edge, count, and jump navigation is entirely layout-backed.
pub(crate) fn key_inspects_source(key: crate::input::KeyInput) -> bool {
    matches!(
        key.code.kind,
        crate::input::KeyCodeKind::Char('w' | 'W' | 'e' | 'E' | 'b' | 'B' | 'n' | 'N')
    ) && !key.mods.ctrl
        && !key.mods.alt
        && !key.mods.shift
}

/// Row-only motions shared by complete and retained rendered projections.
pub(crate) fn vertical_target(
    key: crate::input::KeyInput,
    cursor: &RenderedCursor,
    total: usize,
    count: usize,
) -> Option<usize> {
    use crate::input::KeyCodeKind;

    let last = total.saturating_sub(1);
    let step = count.max(1);
    match key.code.kind {
        KeyCodeKind::Char('j') | KeyCodeKind::Down
            if !key.mods.ctrl && !key.mods.alt && !key.mods.shift =>
        {
            Some(cursor.line.saturating_add(step).min(last))
        }
        KeyCodeKind::Char('k') | KeyCodeKind::Up
            if !key.mods.ctrl && !key.mods.alt && !key.mods.shift =>
        {
            Some(cursor.line.saturating_sub(step))
        }
        KeyCodeKind::Char('g') if !key.mods.ctrl && !key.mods.alt && !key.mods.shift => {
            Some(count.saturating_sub(1).min(last))
        }
        KeyCodeKind::Char('G') if !key.mods.ctrl && !key.mods.alt && !key.mods.shift => {
            Some(if count > 1 {
                count.saturating_sub(1).min(last)
            } else {
                last
            })
        }
        KeyCodeKind::Char('d') if key.mods.ctrl => Some(
            cursor
                .line
                .saturating_add(if count > 1 { count } else { total / 2 })
                .min(last),
        ),
        KeyCodeKind::Char('u') if key.mods.ctrl => {
            Some(
                cursor
                    .line
                    .saturating_sub(if count > 1 { count } else { total / 2 }),
            )
        }
        KeyCodeKind::Char('f') if key.mods.ctrl => Some(
            cursor
                .line
                .saturating_add(if count > 1 { count } else { total })
                .min(last),
        ),
        KeyCodeKind::Char('b') if key.mods.ctrl => {
            Some(
                cursor
                    .line
                    .saturating_sub(if count > 1 { count } else { total }),
            )
        }
        _ => None,
    }
}

pub(crate) fn horizontal_direction(key: crate::input::KeyInput) -> Option<bool> {
    use crate::input::KeyCodeKind;
    if key.mods.ctrl || key.mods.alt || key.mods.shift {
        return None;
    }
    match key.code.kind {
        KeyCodeKind::Char('h') | KeyCodeKind::Left => Some(false),
        KeyCodeKind::Char('l') | KeyCodeKind::Right => Some(true),
        _ => None,
    }
}

pub(crate) fn word_motion(key: crate::input::KeyInput) -> Option<char> {
    if key.mods.ctrl || key.mods.alt || key.mods.shift {
        return None;
    }
    match key.code.kind {
        crate::input::KeyCodeKind::Char(motion @ ('w' | 'W' | 'e' | 'E' | 'b' | 'B')) => {
            Some(motion)
        }
        _ => None,
    }
}

pub(crate) fn jump_row_for_key(
    key: crate::input::KeyInput,
    cursor: &RenderedCursor,
    count: usize,
    targets: &[JumpTarget],
) -> Option<Option<usize>> {
    use crate::input::KeyCodeKind;
    if key.mods.ctrl || key.mods.alt || key.mods.shift {
        return None;
    }
    match key.code.kind {
        KeyCodeKind::Tab => Some(next_jump_target(cursor, targets, false)),
        KeyCodeKind::BackTab => Some(next_jump_target(cursor, targets, true)),
        KeyCodeKind::Char('[') if count > 1 => {
            let headings = targets
                .iter()
                .filter(|target| matches!(target.kind, TargetKind::Heading(_)))
                .collect::<Vec<_>>();
            Some(find_prev_by_kind(cursor, &headings, count).map(|target| target.line))
        }
        KeyCodeKind::Char(']') if count > 1 => {
            let headings = targets
                .iter()
                .filter(|target| matches!(target.kind, TargetKind::Heading(_)))
                .collect::<Vec<_>>();
            Some(find_next_by_kind(cursor, &headings, count).map(|target| target.line))
        }
        _ => None,
    }
}

/// Handle a key in rendered mode. Returns the effect of the keypress.
///
/// Implements:
/// - VN-1: j/k/arrows for line-by-line navigation
/// - VN-3: gg/G/counts for document navigation
/// - VN-4: Tab/Shift-Tab for jump targets
/// - VN-5: search with / and ?
/// - VN-6: n/N for repeat search
#[allow(clippy::too_many_arguments)]
pub fn handle_key(
    key: crate::input::KeyInput,
    cursor: &RenderedCursor,
    search: Option<&RenderedSearch>,
    max_rendered_lines: usize,
    jump_targets: &[crate::style::JumpTarget],
    layout: &RenderedLayout,
    count: usize,
    text: &str,
) -> RenderedKeyResult {
    let mut result = RenderedKeyResult::default();
    let step = if count > 1 { count } else { 1 };

    if let Some(row) = vertical_target(key, cursor, max_rendered_lines, count) {
        result.cursor_moved = true;
        result.new_cursor = Some(cursor_for_row(row, cursor.desired_column, layout));
        return result;
    }
    if let Some(forward) = horizontal_direction(key) {
        if let Some(point) = horizontal_point(cursor, layout, forward, step) {
            result.cursor_moved = true;
            result.new_cursor = Some(RenderedCursor::at(point));
        }
        return result;
    }
    if let Some(motion) = word_motion(key) {
        if let Some(point) = word_point(cursor, layout, text, motion, step) {
            result.cursor_moved = true;
            result.new_cursor = Some(RenderedCursor::at(point));
        }
        return result;
    }
    if let Some(row) = jump_row_for_key(key, cursor, count, jump_targets) {
        if let Some(row) = row {
            result.cursor_moved = true;
            result.new_cursor = Some(cursor_for_row(row, cursor.desired_column, layout));
        }
        return result;
    }

    match key.code.kind {
        // Esc: no-op in rendered mode (handled by session layer)
        crate::input::KeyCodeKind::Esc => {}

        crate::input::KeyCodeKind::Char('0') | crate::input::KeyCodeKind::Char('^')
            if !key.mods.ctrl && !key.mods.alt && !key.mods.shift =>
        {
            if let Some(point) = edge_point(cursor.line, layout, false) {
                result.cursor_moved = true;
                result.new_cursor = Some(RenderedCursor::at(point));
            }
        }

        crate::input::KeyCodeKind::Char('$')
            if !key.mods.ctrl && !key.mods.alt && !key.mods.shift =>
        {
            if let Some(point) = edge_point(cursor.line, layout, true) {
                result.cursor_moved = true;
                result.new_cursor = Some(RenderedCursor::at(point));
            }
        }

        // /: start forward search
        crate::input::KeyCodeKind::Char('/')
            if !key.mods.ctrl && !key.mods.alt && !key.mods.shift =>
        {
            let mut new_search = RenderedSearch::new("");
            new_search.set_direction(SearchDirection::Forward);
            result.search_changed = true;
            result.new_search = Some(new_search);
        }

        // ?: start backward search
        crate::input::KeyCodeKind::Char('?')
            if !key.mods.ctrl && !key.mods.alt && !key.mods.shift =>
        {
            let mut new_search = RenderedSearch::new("");
            new_search.set_direction(SearchDirection::Backward);
            result.search_changed = true;
            result.new_search = Some(new_search);
        }

        // n: repeat search in same direction (with cursor movement)
        crate::input::KeyCodeKind::Char('n')
            if !key.mods.ctrl && !key.mods.alt && !key.mods.shift =>
        {
            if let Some(current_search) = search {
                if !current_search.pattern.is_empty() {
                    if let Some(match_line) = find_next_match(
                        current_search,
                        cursor,
                        layout,
                        text,
                        current_search.direction(),
                    ) {
                        result.cursor_moved = true;
                        result.new_cursor =
                            Some(cursor_for_row(match_line, cursor.desired_column, layout));
                        // Check if we wrapped around the document
                        let wrapped = if current_search.direction() == SearchDirection::Forward {
                            match_line <= cursor.line
                        } else {
                            match_line >= cursor.line
                        };
                        if wrapped {
                            result
                                .message
                                .get_or_insert_with(String::new)
                                .push_str(" (wrapped)");
                        }
                    }
                }
                result.search_changed = true;
                result.new_search = Some(current_search.clone());
            }
        }

        // N: repeat search in reverse direction (with cursor movement)
        crate::input::KeyCodeKind::Char('N')
            if !key.mods.ctrl && !key.mods.alt && !key.mods.shift =>
        {
            if let Some(current_search) = search {
                if !current_search.pattern.is_empty() {
                    let mut reverse = current_search.clone();
                    reverse.set_direction(match reverse.direction() {
                        SearchDirection::Forward => SearchDirection::Backward,
                        SearchDirection::Backward => SearchDirection::Forward,
                    });
                    if let Some(match_line) =
                        find_next_match(&reverse, cursor, layout, text, reverse.direction())
                    {
                        result.cursor_moved = true;
                        result.new_cursor =
                            Some(cursor_for_row(match_line, cursor.desired_column, layout));
                        let wrapped = if reverse.direction() == SearchDirection::Forward {
                            match_line <= cursor.line
                        } else {
                            match_line >= cursor.line
                        };
                        if wrapped {
                            result.message = Some(" (wrapped)".to_string());
                        }
                    }
                }
                result.search_changed = true;
                // N reverses this invocation without changing the committed
                // direction, so repeated N presses keep moving oppositely.
                result.new_search = Some(current_search.clone());
            }
        }

        // {: jump to previous synthetic boundary line
        crate::input::KeyCodeKind::Char('{')
            if !key.mods.ctrl && !key.mods.alt && !key.mods.shift =>
        {
            if let Some(target) = find_prev_boundary(cursor, layout) {
                result.cursor_moved = true;
                result.new_cursor = Some(cursor_for_row(target, cursor.desired_column, layout));
            }
        }

        // }: jump to next synthetic boundary line
        crate::input::KeyCodeKind::Char('}')
            if !key.mods.ctrl && !key.mods.alt && !key.mods.shift =>
        {
            if let Some(target) = find_next_boundary(cursor, layout) {
                result.cursor_moved = true;
                result.new_cursor = Some(cursor_for_row(target, cursor.desired_column, layout));
            }
        }

        // Enter: on a link-target line, show destination
        crate::input::KeyCodeKind::Enter if !key.mods.ctrl && !key.mods.alt && !key.mods.shift => {
            result.message = target_message_at_row(cursor.line, &layout.jump_targets, |index| {
                layout.link_index.get(index).map(|(_, url)| url.clone())
            });
        }

        // z: toggle front-matter collapse
        crate::input::KeyCodeKind::Char('z')
            if !key.mods.ctrl && !key.mods.alt && !key.mods.shift =>
        {
            result.layout_dirty = true;
            result.fm_collapsed_toggled = true;
            result.message = Some("FM collapse toggled".to_string());
        }

        // Page Up / Page Down: handled by session layer with viewport info
        crate::input::KeyCodeKind::PageUp | crate::input::KeyCodeKind::PageDown => {}

        // Home: go to first content line
        crate::input::KeyCodeKind::Home if !key.mods.ctrl && !key.mods.alt && !key.mods.shift => {
            if let Some(first_content) = layout
                .lines
                .iter()
                .position(|l| l.kind == LineKind::Content)
            {
                result.cursor_moved = true;
                result.new_cursor =
                    Some(cursor_for_row(first_content, cursor.desired_column, layout));
            }
        }

        // End: go to last content line
        crate::input::KeyCodeKind::End if !key.mods.ctrl && !key.mods.alt && !key.mods.shift => {
            let last_content = layout
                .lines
                .iter()
                .rposition(|l| l.kind == LineKind::Content);
            if let Some(last) = last_content {
                result.cursor_moved = true;
                result.new_cursor = Some(cursor_for_row(last, cursor.desired_column, layout));
            }
        }

        // All other keys: no-op (handled by session layer for read-only message)
        _ => {}
    }

    result
}

/// Find the next jump target after the current cursor position.
///
/// `reverse: true` means find the previous target (for Shift-Tab).
fn next_jump_target(
    cursor: &RenderedCursor,
    jump_targets: &[crate::style::JumpTarget],
    reverse: bool,
) -> Option<usize> {
    if jump_targets.is_empty() {
        return None;
    }

    let cursor_line = cursor.line;

    if reverse {
        // Find the last target before cursor_line
        jump_targets
            .iter()
            .rev()
            .find_map(|t| {
                if t.line < cursor_line {
                    Some(t.line)
                } else {
                    None
                }
            })
            .or_else(|| {
                // Wrap to last target
                jump_targets.last().map(|t| t.line)
            })
    } else {
        // Find the first target after cursor_line
        jump_targets
            .iter()
            .find_map(|t| {
                if t.line > cursor_line {
                    Some(t.line)
                } else {
                    None
                }
            })
            .or_else(|| {
                // Wrap to first target
                jump_targets.first().map(|t| t.line)
            })
    }
}

// ── Search navigation ─────────────────────────────────────────────────────

/// Find the rendered row containing the next search match.
///
/// Returns the rendered row index of the next match, or `None` if no match.
pub fn find_next_match(
    search: &RenderedSearch,
    cursor: &RenderedCursor,
    layout: &RenderedLayout,
    _text: &str,
    direction: SearchDirection,
) -> Option<usize> {
    matching_row_for_cursor(
        cursor,
        direction,
        layout.lines.iter().enumerate().filter_map(|(row, line)| {
            (line.kind == LineKind::Content && search.matches(&line.styled.text)).then_some(row)
        }),
    )
}

/// Resolve a sorted stream of matching rendered rows with wrap-around.
pub(crate) fn matching_row_for_cursor(
    cursor: &RenderedCursor,
    direction: SearchDirection,
    matches: impl IntoIterator<Item = usize>,
) -> Option<usize> {
    let mut first = None;
    let mut last = None;
    let mut next = None;
    let mut previous = None;
    for row in matches {
        first.get_or_insert(row);
        last = Some(row);
        if row > cursor.line && next.is_none() {
            next = Some(row);
        }
        if row < cursor.line {
            previous = Some(row);
        }
    }
    if direction == SearchDirection::Forward {
        next.or(first)
    } else {
        previous.or(last)
    }
}

// ── Block boundary jumping ({/}) ──────────────────────────────────────────

fn is_block_boundary(line: &crate::style::RenderedLine) -> bool {
    line.kind == LineKind::Synthetic
        || (line.kind == LineKind::Content && line.styled.text.is_empty() && line.atoms.is_empty())
}

/// Find the previous rendered boundary line before the cursor.
fn find_prev_boundary(cursor: &RenderedCursor, layout: &RenderedLayout) -> Option<usize> {
    let cursor_line = cursor.line;
    (0..cursor_line)
        .rev()
        .find(|&i| is_block_boundary(&layout.lines[i]))
}

/// Find the next rendered boundary line after the cursor.
fn find_next_boundary(cursor: &RenderedCursor, layout: &RenderedLayout) -> Option<usize> {
    let cursor_line = cursor.line;
    (cursor_line + 1..layout.lines.len()).find(|&i| is_block_boundary(&layout.lines[i]))
}

// ── Heading jumping ([[ / ]]) ─────────────────────────────────────────────

/// Find the previous heading target before the cursor, with multiplicity.
fn find_prev_by_kind<'a>(
    cursor: &'a RenderedCursor,
    targets: &'a [&'a JumpTarget],
    count: usize,
) -> Option<&'a JumpTarget> {
    targets
        .iter()
        .rev()
        .filter(|t| t.line < cursor.line)
        .nth(count.saturating_sub(2))
        .copied()
}

/// Find the next heading target after the cursor, with multiplicity.
fn find_next_by_kind<'a>(
    cursor: &'a RenderedCursor,
    targets: &'a [&'a JumpTarget],
    count: usize,
) -> Option<&'a JumpTarget> {
    targets
        .iter()
        .filter(|t| t.line > cursor.line)
        .nth(count.saturating_sub(2))
        .copied()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rendered::{BlockModel, RetainedRows};
    use crate::style::{RenderedLine, RenderedSourceAtom, StyledLine};
    use crate::syntax::Highlighter;
    use proptest::prelude::*;

    fn visible_sources_on_physical_line(
        layout: &RenderedLayout,
        start: usize,
        end: usize,
    ) -> Vec<Range<usize>> {
        normalize_ranges(
            layout
                .lines
                .iter()
                .flat_map(|line| &line.atoms)
                .filter_map(|atom| atom.source.clone())
                .filter(|source| start <= source.start && source.end <= end)
                .collect(),
        )
    }

    #[test]
    fn character_mutation_covers_physical_lines_only_when_all_visible_atoms_are_selected() {
        let cases = [
            ("# Heading &amp; café\nnext\n", "# Heading", 12),
            (
                "- wrapped **item text** and more words\nnext\n",
                "- wrapped",
                12,
            ),
            ("| A | B |\n|---|---|\n| café | tea |\n", "| café", 30),
            (
                "```rust\nfn main() { println!(\"hi\"); }\n```\n",
                "fn main",
                18,
            ),
            (
                "```go\nfunc main() { println(\"hi\") }\n```\n",
                "func main",
                18,
            ),
            ("alpha\r\nbeta\r\n", "beta", 20),
        ];
        for (text, needle, width) in cases {
            let highlighter = Highlighter::new(text);
            let model = BlockModel::build(text, crate::frontmatter::front_matter_span(text));
            let layout = RenderedLayout::build(&model, width, &highlighter);
            let start = text.find(needle).unwrap();
            let start = text[..start].rfind('\n').map_or(0, |newline| newline + 1);
            let end = text[start..]
                .find('\n')
                .map_or(text.len(), |newline| start + newline + 1);
            let selected = visible_sources_on_physical_line(&layout, start, end);
            assert!(!selected.is_empty(), "{needle:?}");
            let widened = character_mutation_ranges(&selected, &layout, text);
            assert_eq!(widened, vec![start..end], "{needle:?}");

            if selected.len() > 1 {
                let partial = selected[..selected.len() - 1].to_vec();
                assert_eq!(
                    character_mutation_ranges(&partial, &layout, text),
                    partial,
                    "partial {needle:?}"
                );
            }
        }
    }

    #[test]
    fn character_mutation_includes_enclosed_blank_but_not_hidden_structural_lines() {
        let text = "alpha\n\n[ref]: /url\n\nbeta";
        let highlighter = Highlighter::new(text);
        let model = BlockModel::build(text, crate::frontmatter::front_matter_span(text));
        let layout = RenderedLayout::build(&model, 40, &highlighter);
        let mut selected = visible_sources_on_physical_line(&layout, 0, 6);
        let beta = text.find("beta").unwrap();
        selected.extend(visible_sources_on_physical_line(&layout, beta, text.len()));
        let actual = character_mutation_ranges(&normalize_ranges(selected), &layout, text);
        assert_eq!(actual, vec![0..6, beta - 1..text.len()]);

        let beta_only = visible_sources_on_physical_line(&layout, beta, text.len());
        assert_eq!(
            character_mutation_ranges(&beta_only, &layout, text),
            vec![beta - 1..text.len()]
        );
    }

    fn prototype_bounded_character_selection(
        rows: &RetainedRows,
        anchor: RenderedPoint,
        active: RenderedPoint,
    ) -> (RenderedSelection, usize) {
        let first = anchor.row.min(active.row);
        let last = anchor.row.max(active.row);
        let visited = (first..=last)
            .map(|row| (row, rows.row(row).expect("indexed row exists")))
            .collect::<Vec<_>>();
        let source_at = |point: RenderedPoint| {
            visited[point.row - first]
                .1
                .atoms
                .iter()
                .find(|atom| {
                    atom.columns.contains(&point.column) || atom.columns.start == point.column
                })
                .and_then(|atom| atom.source.clone())
                .expect("source-backed selection endpoint")
        };
        let anchor_source = source_at(anchor);
        let active_source = source_at(active);
        let start = anchor_source.start.min(active_source.start);
        let end = anchor_source.end.max(active_source.end);
        let source_ranges = normalize_ranges(
            visited
                .iter()
                .flat_map(|(_, line)| line.atoms.iter())
                .filter_map(|atom| atom.source.clone())
                .filter(|source| source.start >= start && source.end <= end)
                .collect(),
        );
        let visible = visited
            .into_iter()
            .filter_map(|(row, line)| {
                let columns = normalize_ranges(
                    source_ranges
                        .iter()
                        .flat_map(|source| project_atom_intervals(source, &line.atoms))
                        .collect(),
                );
                if columns.is_empty() {
                    return None;
                }
                let row_sources = normalize_ranges(
                    line.atoms
                        .iter()
                        .filter_map(|atom| atom.source.clone())
                        .filter(|source| {
                            source_ranges.iter().any(|selected| {
                                source.start < selected.end && selected.start < source.end
                            })
                        })
                        .collect(),
                );
                Some(RenderedSelectionRow {
                    row,
                    columns,
                    source_ranges: row_sources,
                })
            })
            .collect();
        (
            RenderedSelection {
                anchor,
                active,
                shape: SelectionShape::Character,
                source_ranges,
                rows: visible,
                block_width: None,
            },
            last - first + 1,
        )
    }

    #[test]
    fn bounded_character_projection_matches_small_markdown_shapes() {
        let cases = [
            (
                "wrapped",
                "# Café\n\nA long line of repeated words that wraps twice.\nMore prose.\n",
                18,
            ),
            (
                "list",
                "- first **bold** item\n- second `code` item\n\nTail\n",
                28,
            ),
            (
                "table",
                "| Key | Value |\n|---|---|\n| café | `λ` |\n| tea | &amp; |\n",
                36,
            ),
            (
                "fence",
                "```rust\nfn café() {\n  let value = 1;\n}\n```\n",
                30,
            ),
            (
                "footnote",
                "A [link](https://example.invalid) and note[^a].\n\n[^a]: footnote text\n",
                38,
            ),
        ];
        for (name, text, width) in cases {
            let highlighter = Highlighter::new(text);
            let model = BlockModel::build(text, crate::frontmatter::front_matter_span(text));
            let (layout, fences, boundaries) =
                RenderedLayout::build_with_boundaries(&model, width, &highlighter, false);
            let retained = RetainedRows::from_complete(layout.clone(), fences, &boundaries);
            let points = layout
                .lines
                .iter()
                .enumerate()
                .filter_map(|(row, line)| {
                    line.atoms
                        .iter()
                        .find(|atom| atom.source.is_some())
                        .map(|atom| RenderedPoint {
                            row,
                            column: atom.columns.start,
                        })
                })
                .collect::<Vec<_>>();
            for (index, &anchor) in points.iter().enumerate() {
                for &active in points.iter().skip(index).take(5) {
                    for (from, to) in [(anchor, active), (active, anchor)] {
                        let (bounded, _) =
                            prototype_bounded_character_selection(&retained, from, to);
                        let complete =
                            project_selection(from, to, SelectionShape::Character, &layout, text);
                        assert_eq!(bounded, complete, "{name} {from:?}..{to:?}");
                    }
                }
            }
        }
    }

    #[test]
    #[ignore = "exact 1 MiB selection feasibility diagnostic is run by its benchmark target"]
    fn acceptance_1mb_bounded_character_projection_matches_complete() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/kitchen-sink-1mb.md");
        let text = std::fs::read_to_string(path).unwrap();
        let highlighter = Highlighter::new(&text);
        let model = BlockModel::build(&text, crate::frontmatter::front_matter_span(&text));
        let (layout, fences, boundaries) =
            RenderedLayout::build_with_boundaries(&model, 100, &highlighter, false);
        let retained = RetainedRows::from_complete(layout.clone(), fences, &boundaries);
        for (region, line) in [("prose", 600), ("rust", 3000), ("go", 20000)] {
            let offset = text
                .split_inclusive('\n')
                .take(line)
                .map(str::len)
                .sum::<usize>();
            let anchor_row = layout
                .lines
                .iter()
                .position(|rendered| {
                    rendered.atoms.iter().any(|atom| {
                        atom.source
                            .as_ref()
                            .is_some_and(|source| source.start >= offset)
                    })
                })
                .unwrap();
            let point_at = |row: usize| {
                let atom = layout.lines[row]
                    .atoms
                    .iter()
                    .find(|atom| atom.source.is_some())
                    .unwrap();
                RenderedPoint {
                    row,
                    column: atom.columns.start,
                }
            };
            let anchor = point_at(anchor_row);
            let active_row = (anchor_row + 15..anchor_row + 30)
                .find(|&row| {
                    layout
                        .lines
                        .get(row)
                        .is_some_and(|line| line.atoms.iter().any(|atom| atom.source.is_some()))
                })
                .unwrap();
            let active = point_at(active_row);
            let started = std::time::Instant::now();
            let (bounded, visited_rows) =
                prototype_bounded_character_selection(&retained, anchor, active);
            let bounded_ns = started.elapsed().as_nanos();
            let started = std::time::Instant::now();
            let complete =
                project_selection(anchor, active, SelectionShape::Character, &layout, &text);
            let complete_ns = started.elapsed().as_nanos();
            assert_eq!(bounded, complete, "{region}");
            assert!(visited_rows <= 30);
            println!(
                "SELECT_BOUNDED\t{region}\t{bounded_ns}\t{complete_ns}\t{visited_rows}\t{}",
                layout.lines.len()
            );
        }
    }

    fn assert_indexed_mapping_matches_reference(text: &str, widths: &[u16]) {
        let highlighter = Highlighter::new(text);
        let model = BlockModel::build(text, crate::frontmatter::front_matter_span(text));
        for &width in widths {
            let layout = RenderedLayout::build(&model, width, &highlighter);
            for line in 0..=highlighter.line_starts().len() {
                for col in [0, 1, 2, 4, 8, 32] {
                    assert_eq!(
                        enter_rendered_indexed(line, col, &layout, text, highlighter.line_starts(),),
                        enter_rendered(line, col, &layout, text),
                        "mapping differs at line={line}, col={col}, width={width}, text={text:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn indexed_mapping_matches_reference_for_source_less_and_utf8_boundaries() {
        let cases = [
            "\n# café &amp; tea\n\n- item\\* one\n- item two\n\n| A | B |\n|---|---|\n| x | x |\n",
            "\r\n```rust\r\nfn main() { println!(\"界\"); }\r\n```\r\n\r\nlast\r\n",
            "---\ntitle: hello\n---\n\n> quoted &lt;word&gt;\n\n```unknown\nrepeated repeated\n```\n",
            "\n\n",
            "no final newline",
        ];
        for text in cases {
            assert_indexed_mapping_matches_reference(text, &[12, 40, 80]);
        }
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(64))]

        #[test]
        fn indexed_mapping_matches_reference_over_generated_markdown(
            blocks in prop::collection::vec(0usize..10, 0..16),
            width in 8u16..80,
        ) {
            let fragments = [
                "", "# heading", "café &amp; tea", "- item", "| x | x |",
                "|---|---|", "```rust", "fn main() {}", "```", "\\* escaped",
            ];
            let text = blocks.iter().map(|index| fragments[*index]).collect::<Vec<_>>().join("\n");
            assert_indexed_mapping_matches_reference(&text, &[width]);
        }
    }

    #[test]
    fn indexed_line_lookup_has_logarithmic_comparison_bound() {
        let text = "content\n".repeat(131_072);
        let highlighter = Highlighter::new(&text);
        let starts = highlighter.line_starts();
        let mut comparisons = 0usize;
        for sample in 0..4096 {
            let offset = sample * text.len() / 4096;
            let expected = starts
                .partition_point(|start| {
                    comparisons += 1;
                    *start <= offset
                })
                .saturating_sub(1);
            assert_eq!(source_line_for_offset(starts, offset), expected);
        }
        assert!(comparisons <= 4096 * 19, "{comparisons} comparisons");
    }

    #[test]
    fn character_projection_follows_source_through_non_linear_rows() {
        let layout = layout_with_lines(vec![
            rendered_line_with_atoms(vec![
                RenderedSourceAtom {
                    columns: 0..1,
                    source: Some(0..1),
                },
                RenderedSourceAtom {
                    columns: 1..2,
                    source: None,
                },
                RenderedSourceAtom {
                    columns: 2..3,
                    source: Some(10..11),
                },
            ]),
            rendered_line_with_atoms(vec![
                RenderedSourceAtom {
                    columns: 0..1,
                    source: Some(1..2),
                },
                RenderedSourceAtom {
                    columns: 1..2,
                    source: None,
                },
                RenderedSourceAtom {
                    columns: 2..3,
                    source: Some(11..12),
                },
            ]),
        ]);

        let selection = project_selection(
            RenderedPoint { row: 0, column: 0 },
            RenderedPoint { row: 1, column: 0 },
            SelectionShape::Character,
            &layout,
            "abcdefghijkl",
        );

        assert_eq!(selection.source_ranges, vec![0..2]);
        assert_eq!(selection.rows[0].columns, vec![0..1]);
        assert_eq!(selection.rows[1].columns, vec![0..1]);
    }

    #[test]
    fn character_projection_retains_independent_intervals_on_one_row() {
        let layout = layout_with_lines(vec![rendered_line_with_atoms(vec![
            RenderedSourceAtom {
                columns: 0..1,
                source: Some(0..1),
            },
            RenderedSourceAtom {
                columns: 1..2,
                source: None,
            },
            RenderedSourceAtom {
                columns: 2..4,
                source: Some(2..5),
            },
            RenderedSourceAtom {
                columns: 4..5,
                source: Some(8..9),
            },
        ])]);

        let rows = character_selection_rows(&[0..1, 2..5], &layout);

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].columns, vec![0..1, 2..4]);
        assert_eq!(rows[0].source_ranges, vec![0..1, 2..5]);
    }

    #[test]
    fn enter_rendered_skips_synthetic_lines() {
        let text = "hello\nworld";
        let layout = layout_with_lines(vec![
            rendered_line(LineKind::Content, 0..5),
            rendered_line(LineKind::Synthetic, 0..5),
            rendered_line(LineKind::Content, 6..11),
        ]);

        assert_eq!(enter_rendered(1, 0, &layout, text), RenderedCursor::new(2));
    }

    #[test]
    fn enter_rendered_wrapped_content_line() {
        let text = "hello\nworld";
        let layout = layout_with_lines(vec![
            rendered_line(LineKind::Content, 0..5),
            rendered_line(LineKind::Content, 0..5),
            rendered_line(LineKind::Content, 0..5),
            rendered_line(LineKind::Content, 6..11),
        ]);

        assert_eq!(enter_rendered(0, 0, &layout, text), RenderedCursor::new(0));
        assert_eq!(enter_rendered(1, 0, &layout, text), RenderedCursor::new(3));
    }

    #[test]
    fn enter_rendered_combined() {
        let text = "hello\nworld";
        let layout = layout_with_lines(vec![
            rendered_line(LineKind::Content, 0..5),
            rendered_line(LineKind::Content, 0..5),
            rendered_line(LineKind::Synthetic, 0..5),
            rendered_line(LineKind::Content, 6..11),
        ]);

        assert_eq!(enter_rendered(1, 0, &layout, text), RenderedCursor::new(3));
    }

    #[test]
    fn enter_rendered_clamp_beyond_end() {
        let text = "hello\nworld";
        let layout = layout_with_lines(vec![
            rendered_line(LineKind::Content, 0..5),
            rendered_line(LineKind::Synthetic, 0..5),
            rendered_line(LineKind::Content, 6..11),
            rendered_line(LineKind::Content, 6..11),
            rendered_line(LineKind::Synthetic, 6..11),
        ]);

        assert_eq!(enter_rendered(2, 0, &layout, text), RenderedCursor::new(4));
    }

    #[test]
    fn enter_rendered_matches_multiline_source_range() {
        let text = "first\nsecond\n\nthird";
        let layout = layout_with_lines(vec![
            rendered_line(LineKind::Content, 0..12),
            rendered_line(LineKind::Synthetic, 0..12),
            rendered_line(LineKind::Content, 14..19),
        ]);

        assert_eq!(enter_rendered(1, 0, &layout, text), RenderedCursor::new(0));
        assert_eq!(enter_rendered(1, 6, &layout, text), RenderedCursor::new(0));
    }

    #[test]
    fn enter_rendered_uses_nearest_content_after_blank_line() {
        let text = "first\n\nthird\n\nfifth";
        let layout = layout_with_lines(vec![
            rendered_line(LineKind::Content, 0..6),
            rendered_line(LineKind::Synthetic, 0..6),
            rendered_line(LineKind::Content, 7..13),
            rendered_line(LineKind::Synthetic, 7..13),
            rendered_line(LineKind::Content, 14..19),
        ]);

        assert_eq!(enter_rendered(1, 0, &layout, text), RenderedCursor::new(2));
    }

    #[test]
    fn enter_rendered_prefers_containing_range_in_deferred_content() {
        let text = "[^a]: definition\nlater paragraph";
        let layout = layout_with_lines(vec![
            rendered_line(LineKind::Content, 17..32),
            rendered_line(LineKind::Content, 0..16),
        ]);

        assert_eq!(enter_rendered(0, 5, &layout, text), RenderedCursor::new(1));
    }

    #[test]
    fn enter_rendered_prefers_range_start_at_adjacent_block_boundary() {
        let text = "# H\nparagraph";
        let layout = layout_with_lines(vec![
            rendered_line(LineKind::Content, 0..4),
            rendered_line(LineKind::Synthetic, 0..4),
            rendered_line(LineKind::Content, 4..13),
        ]);

        assert_eq!(enter_rendered(1, 0, &layout, text), RenderedCursor::new(2));
    }

    #[test]
    fn doc_position_to_byte_offset_handles_utf8_columns() {
        let text = "aéz\n日x";

        assert_eq!(doc_position_to_byte_offset(0, 2, text), 3);
        assert_eq!(doc_position_to_byte_offset(1, 1, text), 8);
    }

    #[test]
    fn enter_rendered_uses_nearest_source_when_layout_is_not_source_ordered() {
        let text = "first\n\nsecond\n\nthird";
        let layout = layout_with_lines(vec![
            rendered_line(LineKind::Content, 15..20),
            rendered_line(LineKind::Content, 7..13),
        ]);

        assert_eq!(enter_rendered(1, 0, &layout, text), RenderedCursor::new(1));
    }

    #[test]
    fn search_uses_rendered_text_across_synthetic_lines() {
        let text = "**hello**\n\nfoo\n\nhello";
        let layout = layout_with_lines(vec![
            rendered_line_with_text("hello", LineKind::Content, 0..9),
            rendered_line(LineKind::Synthetic, 0..9),
            rendered_line_with_text("foo", LineKind::Content, 11..14),
            rendered_line(LineKind::Synthetic, 11..14),
            rendered_line_with_text("hello", LineKind::Content, 16..21),
        ]);

        let forward = RenderedSearch::new("foo");
        assert_eq!(
            find_next_match(
                &forward,
                &RenderedCursor::new(0),
                &layout,
                text,
                SearchDirection::Forward,
            ),
            Some(2)
        );

        let mut backward = RenderedSearch::new("hello");
        backward.set_direction(SearchDirection::Backward);
        assert_eq!(
            find_next_match(
                &backward,
                &RenderedCursor::new(0),
                &layout,
                text,
                SearchDirection::Backward,
            ),
            Some(4)
        );

        let source_syntax = RenderedSearch::new("**");
        assert_eq!(
            find_next_match(
                &source_syntax,
                &RenderedCursor::new(0),
                &layout,
                text,
                SearchDirection::Forward,
            ),
            None
        );
    }

    fn layout_with_lines(lines: Vec<RenderedLine>) -> RenderedLayout {
        RenderedLayout {
            lines,
            ..RenderedLayout::default()
        }
    }

    fn rendered_line(kind: LineKind, source: Range<usize>) -> RenderedLine {
        rendered_line_with_text("", kind, source)
    }

    fn rendered_line_with_text(text: &str, kind: LineKind, source: Range<usize>) -> RenderedLine {
        RenderedLine {
            styled: StyledLine {
                text: text.to_string(),
                spans: Vec::new(),
            },
            source,
            kind,
            role: crate::style::RenderedLineRole::Document,
            atoms: Vec::new(),
        }
    }

    fn rendered_line_with_atoms(atoms: Vec<RenderedSourceAtom>) -> RenderedLine {
        RenderedLine {
            styled: StyledLine {
                text: String::new(),
                spans: Vec::new(),
            },
            source: 0..0,
            kind: LineKind::Content,
            role: crate::style::RenderedLineRole::Document,
            atoms,
        }
    }
}
