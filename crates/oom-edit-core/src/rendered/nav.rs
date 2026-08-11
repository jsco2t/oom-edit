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

    if let Some(idx) = layout.lines.iter().position(|rendered_line| {
        rendered_line.kind == LineKind::Content
            && (rendered_line.source.contains(&edit_offset)
                || rendered_line.source.start == edit_offset)
    }) {
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
    let atoms: Vec<_> = layout
        .lines
        .get(cursor.line)?
        .atoms
        .iter()
        .filter(|atom| atom.source.is_some())
        .collect();
    if atoms.is_empty() {
        return None;
    }
    let current = atoms
        .iter()
        .position(|atom| {
            atom.columns.contains(&cursor.column) || atom.columns.start == cursor.column
        })
        .unwrap_or(0);
    let target = if forward {
        current.saturating_add(count).min(atoms.len() - 1)
    } else {
        current.saturating_sub(count)
    };
    Some(RenderedPoint {
        row: cursor.line,
        column: atoms[target].columns.start,
    })
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
    let atoms: Vec<_> = layout
        .lines
        .iter()
        .enumerate()
        .flat_map(|(row, line)| {
            line.atoms.iter().filter_map(move |atom| {
                atom.source.as_ref().map(|_| {
                    (
                        RenderedPoint {
                            row,
                            column: atom.columns.start,
                        },
                        atom,
                    )
                })
            })
        })
        .collect();
    if atoms.is_empty() {
        return None;
    }
    let mut index = atoms
        .iter()
        .position(|(point, _)| {
            point.row == cursor.line
                && (point.column == cursor.column
                    || layout.lines[point.row]
                        .atoms
                        .iter()
                        .find(|atom| atom.columns.start == point.column)
                        .is_some_and(|atom| atom.columns.contains(&cursor.column)))
        })
        .unwrap_or(0);
    let big = motion.is_ascii_uppercase();
    for _ in 0..count.max(1) {
        match motion.to_ascii_lowercase() {
            'w' => {
                let class = atom_class(atoms[index].1, text, big);
                while index + 1 < atoms.len() && atom_class(atoms[index + 1].1, text, big) == class
                {
                    index += 1;
                }
                if index + 1 < atoms.len() {
                    index += 1;
                }
                while index + 1 < atoms.len()
                    && atom_class(atoms[index].1, text, big) == WordClass::Whitespace
                {
                    index += 1;
                }
            }
            'e' => {
                if index + 1 < atoms.len() {
                    index += 1;
                }
                while index + 1 < atoms.len()
                    && atom_class(atoms[index].1, text, big) == WordClass::Whitespace
                {
                    index += 1;
                }
                let class = atom_class(atoms[index].1, text, big);
                while index + 1 < atoms.len() && atom_class(atoms[index + 1].1, text, big) == class
                {
                    index += 1;
                }
            }
            'b' => {
                index = index.saturating_sub(1);
                while index > 0 && atom_class(atoms[index].1, text, big) == WordClass::Whitespace {
                    index -= 1;
                }
                let class = atom_class(atoms[index].1, text, big);
                while index > 0 && atom_class(atoms[index - 1].1, text, big) == class {
                    index -= 1;
                }
            }
            _ => return None,
        }
    }
    atoms.get(index).map(|(point, _)| *point)
}

fn edge_point(row: usize, layout: &RenderedLayout, end: bool) -> Option<RenderedPoint> {
    let line = layout.lines.get(row)?;
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

    let rows: Vec<RenderedSelectionRow> = match shape {
        SelectionShape::Character => (first.row..=last.row)
            .map(|row| {
                let start = if row == first.row { first.column } else { 0 };
                let end = if row == last.row {
                    last.column
                } else {
                    usize::MAX
                };
                selection_row(row, start, end, layout)
            })
            .collect(),
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
                    selected.columns = left..right;
                    selected
                })
                .collect()
        }
    };

    let source_ranges = normalize_ranges(
        rows.iter()
            .flat_map(|row| row.source_ranges.iter().cloned())
            .collect(),
    );
    let block_width = (shape == SelectionShape::Block).then(|| {
        rows.first()
            .map_or(0, |row| row.columns.end.saturating_sub(row.columns.start))
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
        SelectionShape::Character => {
            let anchor_range = source_for_point(anchor, layout);
            let active_range = source_for_point(active, layout);
            if let (Some(anchor_range), Some(active_range)) = (anchor_range, active_range) {
                let start = anchor_range.start.min(active_range.start);
                let end = anchor_range.end.max(active_range.end);
                selection.source_ranges = normalize_ranges(
                    layout
                        .lines
                        .iter()
                        .flat_map(|line| &line.atoms)
                        .filter_map(|atom| atom.source.clone())
                        .filter(|source| source.start >= start && source.end <= end)
                        .collect(),
                );
            }
        }
        SelectionShape::Line => {
            let anchor_offset = doc_position_to_byte_offset(anchor_source.0, anchor_source.1, text);
            let active_offset = doc_position_to_byte_offset(active_source.0, active_source.1, text);
            let first = anchor_offset.min(active_offset);
            let last = anchor_offset.max(active_offset);
            let selected = expand_source_points_to_physical_lines(first, last, text);
            selection.rows = line_selection_rows(&selected, layout, text);
            selection.source_ranges = vec![selected];
        }
        SelectionShape::Block => {}
    }
    selection
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

fn line_selection_rows(
    selected: &Range<usize>,
    layout: &RenderedLayout,
    text: &str,
) -> Vec<RenderedSelectionRow> {
    layout
        .lines
        .iter()
        .enumerate()
        .filter_map(|(row, line)| {
            let source = line
                .atoms
                .iter()
                .filter_map(|atom| atom.source.as_ref())
                .next()
                .cloned()
                .unwrap_or_else(|| line.source.clone());
            let physical = expand_physical_line(&source, text);
            (physical.start < selected.end && selected.start < physical.end).then(|| {
                let mut projected = selection_row(row, 0, usize::MAX, layout);
                projected.columns = 0..line.atoms.last().map_or(0, |atom| atom.columns.end);
                projected.source_ranges = vec![physical];
                projected
            })
        })
        .collect()
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
            columns: 0..0,
            source_ranges: Vec::new(),
        };
    };
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
        columns,
        source_ranges,
    }
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

    match key.code.kind {
        // Esc: no-op in rendered mode (handled by session layer)
        crate::input::KeyCodeKind::Esc => {}

        // j / Down: move down one line
        crate::input::KeyCodeKind::Char('j') | crate::input::KeyCodeKind::Down
            if !key.mods.ctrl && !key.mods.alt && !key.mods.shift =>
        {
            let new_line = cursor
                .line
                .saturating_add(step)
                .min(max_rendered_lines.saturating_sub(1));
            result.cursor_moved = true;
            result.new_cursor = Some(cursor_for_row(new_line, cursor.desired_column, layout));
        }

        // k / Up: move up one line
        crate::input::KeyCodeKind::Char('k') | crate::input::KeyCodeKind::Up
            if !key.mods.ctrl && !key.mods.alt && !key.mods.shift =>
        {
            let new_line = cursor.line.saturating_sub(step);
            result.cursor_moved = true;
            result.new_cursor = Some(cursor_for_row(new_line, cursor.desired_column, layout));
        }

        // h / Left: previous source-backed display atom.
        crate::input::KeyCodeKind::Char('h') | crate::input::KeyCodeKind::Left
            if !key.mods.ctrl && !key.mods.alt && !key.mods.shift =>
        {
            if let Some(point) = horizontal_point(cursor, layout, false, step) {
                result.cursor_moved = true;
                result.new_cursor = Some(RenderedCursor::at(point));
            }
        }

        // l / Right: next source-backed display atom.
        crate::input::KeyCodeKind::Char('l') | crate::input::KeyCodeKind::Right
            if !key.mods.ctrl && !key.mods.alt && !key.mods.shift =>
        {
            if let Some(point) = horizontal_point(cursor, layout, true, step) {
                result.cursor_moved = true;
                result.new_cursor = Some(RenderedCursor::at(point));
            }
        }

        // Rendered word motions traverse source-backed display atoms. Hidden
        // Markdown delimiters and synthetic cells are never cursor stops.
        crate::input::KeyCodeKind::Char('w')
        | crate::input::KeyCodeKind::Char('W')
        | crate::input::KeyCodeKind::Char('e')
        | crate::input::KeyCodeKind::Char('E')
            if !key.mods.ctrl && !key.mods.alt && !key.mods.shift =>
        {
            if let crate::input::KeyCodeKind::Char(motion) = key.code.kind {
                if let Some(point) = word_point(cursor, layout, text, motion, step) {
                    result.cursor_moved = true;
                    result.new_cursor = Some(RenderedCursor::at(point));
                }
            }
        }

        crate::input::KeyCodeKind::Char('b') | crate::input::KeyCodeKind::Char('B')
            if !key.mods.ctrl && !key.mods.alt && !key.mods.shift =>
        {
            if let crate::input::KeyCodeKind::Char(motion) = key.code.kind {
                if let Some(point) = word_point(cursor, layout, text, motion, step) {
                    result.cursor_moved = true;
                    result.new_cursor = Some(RenderedCursor::at(point));
                }
            }
        }

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

        // g: go to first line, or the counted rendered line.
        crate::input::KeyCodeKind::Char('g')
            if !key.mods.ctrl && !key.mods.alt && !key.mods.shift =>
        {
            let new_line = count
                .saturating_sub(1)
                .min(max_rendered_lines.saturating_sub(1));
            result.cursor_moved = true;
            result.new_cursor = Some(cursor_for_row(new_line, cursor.desired_column, layout));
        }

        // G: go to last line (or to line N if count > 0)
        crate::input::KeyCodeKind::Char('G')
            if !key.mods.ctrl && !key.mods.alt && !key.mods.shift =>
        {
            if count > 1 {
                let new_line = count
                    .saturating_sub(1)
                    .min(max_rendered_lines.saturating_sub(1));
                result.cursor_moved = true;
                result.new_cursor = Some(cursor_for_row(new_line, cursor.desired_column, layout));
            } else {
                result.cursor_moved = true;
                result.new_cursor = Some(cursor_for_row(
                    max_rendered_lines.saturating_sub(1),
                    cursor.desired_column,
                    layout,
                ));
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

        // Tab: jump to next target
        crate::input::KeyCodeKind::Tab if !key.mods.ctrl && !key.mods.alt && !key.mods.shift => {
            if let Some(target) = next_jump_target(cursor, jump_targets, false) {
                result.cursor_moved = true;
                result.new_cursor = Some(cursor_for_row(target, cursor.desired_column, layout));
            }
        }

        // Shift-Tab: jump to previous target
        crate::input::KeyCodeKind::BackTab
            if !key.mods.ctrl && !key.mods.alt && !key.mods.shift =>
        {
            if let Some(target) = next_jump_target(cursor, jump_targets, true) {
                result.cursor_moved = true;
                result.new_cursor = Some(cursor_for_row(target, cursor.desired_column, layout));
            }
        }

        // Ctrl-d: scroll down half viewport
        crate::input::KeyCodeKind::Char('d') if key.mods.ctrl => {
            let page = if count > 1 {
                count
            } else {
                max_rendered_lines / 2
            };
            let new_line =
                (cursor.line.saturating_add(page)).min(max_rendered_lines.saturating_sub(1));
            result.cursor_moved = true;
            result.new_cursor = Some(cursor_for_row(new_line, cursor.desired_column, layout));
        }

        // Ctrl-u: scroll up half viewport
        crate::input::KeyCodeKind::Char('u') if key.mods.ctrl => {
            let page = if count > 1 {
                count
            } else {
                max_rendered_lines / 2
            };
            let new_line = cursor.line.saturating_sub(page);
            result.cursor_moved = true;
            result.new_cursor = Some(cursor_for_row(new_line, cursor.desired_column, layout));
        }

        // Ctrl-f: scroll down full viewport
        crate::input::KeyCodeKind::Char('f') if key.mods.ctrl => {
            let page = if count > 1 { count } else { max_rendered_lines };
            let new_line =
                (cursor.line.saturating_add(page)).min(max_rendered_lines.saturating_sub(1));
            result.cursor_moved = true;
            result.new_cursor = Some(cursor_for_row(new_line, cursor.desired_column, layout));
        }

        // Ctrl-b: scroll up full viewport
        crate::input::KeyCodeKind::Char('b') if key.mods.ctrl => {
            let page = if count > 1 { count } else { max_rendered_lines };
            let new_line = cursor.line.saturating_sub(page);
            result.cursor_moved = true;
            result.new_cursor = Some(cursor_for_row(new_line, cursor.desired_column, layout));
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

        // [[: jump to previous heading
        crate::input::KeyCodeKind::Char('[')
            if !key.mods.ctrl && !key.mods.alt && !key.mods.shift && count > 1 =>
        {
            let targets: Vec<&JumpTarget> = layout
                .jump_targets
                .iter()
                .filter(|t| matches!(t.kind, TargetKind::Heading(_)))
                .collect();
            if let Some(target) = find_prev_by_kind(cursor, &targets, count) {
                result.cursor_moved = true;
                result.new_cursor =
                    Some(cursor_for_row(target.line, cursor.desired_column, layout));
            }
        }

        // ]]: jump to next heading
        crate::input::KeyCodeKind::Char(']')
            if !key.mods.ctrl && !key.mods.alt && !key.mods.shift && count > 1 =>
        {
            let targets: Vec<&JumpTarget> = layout
                .jump_targets
                .iter()
                .filter(|t| matches!(t.kind, TargetKind::Heading(_)))
                .collect();
            if let Some(target) = find_next_by_kind(cursor, &targets, count) {
                result.cursor_moved = true;
                result.new_cursor =
                    Some(cursor_for_row(target.line, cursor.desired_column, layout));
            }
        }

        // Enter: on a link-target line, show destination
        crate::input::KeyCodeKind::Enter if !key.mods.ctrl && !key.mods.alt && !key.mods.shift => {
            if let Some(target) = layout.jump_targets.iter().find(|t| t.line == cursor.line) {
                match &target.kind {
                    TargetKind::Heading(_) => {
                        result.message = Some(format!("Heading at line {}", target.line + 1));
                    }
                    TargetKind::Link(idx) => {
                        if let Some((_, url)) = layout.link_index.get(*idx) {
                            result.message = Some(format!("Link: {}", url));
                        }
                    }
                    TargetKind::Footnote => {
                        result.message = Some(format!("Footnote at line {}", target.line + 1));
                    }
                }
            }
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
    let mut rendered_lines_with_matches = layout
        .lines
        .iter()
        .enumerate()
        .filter_map(|(rendered_line, line)| {
            (line.kind == LineKind::Content && search.matches(&line.styled.text))
                .then_some(rendered_line)
        })
        .collect::<Vec<_>>();

    if rendered_lines_with_matches.is_empty() {
        return None;
    }

    // Deduplicate and sort
    rendered_lines_with_matches.sort();
    rendered_lines_with_matches.dedup();

    if direction == SearchDirection::Forward {
        // Find first match after cursor
        rendered_lines_with_matches
            .iter()
            .find_map(|vl| if *vl > cursor.line { Some(*vl) } else { None })
            .or_else(|| rendered_lines_with_matches.first().copied())
    } else {
        // Find last match before cursor
        rendered_lines_with_matches
            .iter()
            .rev()
            .find_map(|vl| if *vl < cursor.line { Some(*vl) } else { None })
            .or_else(|| rendered_lines_with_matches.last().copied())
    }
}

// ── Block boundary jumping ({/}) ──────────────────────────────────────────

/// Find the previous synthetic boundary line before the cursor.
fn find_prev_boundary(cursor: &RenderedCursor, layout: &RenderedLayout) -> Option<usize> {
    let cursor_line = cursor.line;
    (0..cursor_line)
        .rev()
        .find(|&i| layout.lines[i].kind == LineKind::Synthetic)
}

/// Find the next synthetic boundary line after the cursor.
fn find_next_boundary(cursor: &RenderedCursor, layout: &RenderedLayout) -> Option<usize> {
    let cursor_line = cursor.line;
    (cursor_line + 1..layout.lines.len()).find(|&i| layout.lines[i].kind == LineKind::Synthetic)
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
    use crate::style::{RenderedLine, StyledLine};

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
}
