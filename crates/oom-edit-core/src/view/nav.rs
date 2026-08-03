//! View navigation — cursor, search, and key handling for View mode.
//!
//! This module implements the View navigation layer: position mapping between
//! edit and view coordinates (`enter_view` / `leave_view`), scroll-top
//! calculation (`view_scroll_top`), and View-mode key handling
//! (`handle_key`). Together they satisfy VN-1 through VN-6 and VP-1 through
//! VP-4.
//!
//! See architecture §6.4 for the navigation contract.

use std::ops::Range;

use crate::style::{LineKind, SearchDirection, ViewCursor, ViewLayout, ViewSearch};

// ── ViewCursor ─────────────────────────────────────────────────────────────

impl ViewCursor {
    /// Create a new cursor at the given 0-based line.
    pub fn new(line: usize) -> Self {
        Self { line }
    }

    /// Clamp to the valid range `[0, max_lines)`.
    pub fn clamp(&mut self, max_lines: usize) {
        self.line = self.line.min(max_lines.saturating_sub(1));
    }
}

// ── ViewSearch ─────────────────────────────────────────────────────────────

impl ViewSearch {
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

// ── VP-1 / VP-2 / VP-3: enter_view — edit → view cursor mapping ───────────

/// Map an edit cursor (line, col) to a View cursor line.
///
/// VP-1: When entering View from Normal/Insert/Visual, the view cursor
/// snaps to the rendered line containing the edit cursor.
///
/// VP-2: If the edit cursor is on a line that wraps into multiple view
/// lines, the cursor maps to the first view line of that content line.
///
/// VP-3: If the edit cursor is beyond the last content line, the view
/// cursor clamps to the last view line.
pub fn enter_view(edit_line: usize, _edit_col: usize, layout: &ViewLayout) -> ViewCursor {
    // Count content lines up to and including edit_line
    let mut view_line_idx = 0;
    let mut doc_line = 0usize;

    for view_line in &layout.lines {
        if view_line.kind == LineKind::Content {
            if doc_line == edit_line {
                return ViewCursor::new(view_line_idx);
            }
            doc_line += 1;
            view_line_idx += 1;
        }
    }

    // edit_line is beyond the document — clamp to last view line
    if layout.lines.is_empty() {
        ViewCursor::new(0)
    } else {
        ViewCursor::new(layout.lines.len().saturating_sub(1))
    }
}

// ── VP-1 / VP-2 / VP-3: leave_view — view → edit cursor mapping ───────────

/// Map a View cursor line back to an edit cursor (line, col).
///
/// VP-1: Synthetic view lines map to the source span of the nearest
/// preceding content line.
///
/// VP-2: The edit cursor column is set to 0 (start of line).
///
/// VP-3: If the view cursor is beyond the last content line, it clamps
/// to the last document line.
///
/// `text` is the full document text, needed to convert source byte offsets
/// to document line numbers.
pub fn leave_view(view_cursor: &ViewCursor, layout: &ViewLayout, text: &str) -> (usize, usize) {
    for (view_line_idx, view_line) in layout.lines.iter().enumerate() {
        if view_line_idx == view_cursor.line {
            // Extract document line from source span
            let doc_line = source_to_doc_line(&view_line.source, text);
            return (doc_line, 0);
        }
    }

    // Beyond last view line: clamp to last doc line
    let last_content_source = layout.lines.iter().rev().find_map(|l| {
        if l.kind == LineKind::Content {
            Some(&l.source)
        } else {
            None
        }
    });

    match last_content_source {
        Some(source) => (source_to_doc_line(source, text), 0),
        None => (0, 0),
    }
}

/// Convert a source byte range to a 0-based document line number.
fn source_to_doc_line(source: &Range<usize>, text: &str) -> usize {
    let start = source.start.min(text.len());
    text[..start].chars().filter(|&c| c == '\n').count()
}

// ── VN-2: view_scroll_top — pure scroll position calculation ──────────────

/// Compute the scroll-top index for a given cursor position and viewport height.
///
/// Keeps the cursor centered when possible, with hysteresis: once the cursor
/// is within the center third of the viewport, scrolling doesn't change.
///
/// `layout_height` is the total number of view lines.
/// `cursor` is the 0-based view line of the cursor.
/// `viewport_height` is the number of visible lines.
/// `current_top` is the current scroll position (for hysteresis).
pub fn view_scroll_top(
    cursor: usize,
    viewport_height: usize,
    layout_height: usize,
    current_top: usize,
) -> usize {
    if viewport_height == 0 || layout_height == 0 {
        return 0;
    }

    let cursor = cursor.min(layout_height.saturating_sub(1));
    let max_top = layout_height.saturating_sub(viewport_height);

    // Center window: cursor should be in the middle third of viewport
    let center_start = viewport_height / 3;
    let center_end = (2 * viewport_height) / 3;

    if cursor < center_start {
        // Cursor in top third: scroll up to show it
        0
    } else if cursor >= max_top + center_end {
        // Cursor in bottom third near end: scroll to bottom
        max_top
    } else if cursor < current_top + center_start {
        // Cursor moved above center window: scroll up
        cursor.saturating_sub(center_start)
    } else if cursor >= current_top + center_end {
        // Cursor moved below center window: scroll down
        cursor.saturating_sub(center_end)
    } else {
        // Cursor within center window: keep current scroll
        current_top
    }
}

// ── VN-1 / VN-3 / VN-4 / VN-5 / VN-6: View mode key handling ──────────────

/// The result of handling a View mode key.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ViewKeyResult {
    /// Whether the cursor moved.
    pub cursor_moved: bool,
    /// New cursor position, if the cursor moved.
    pub new_cursor: Option<ViewCursor>,
    /// Whether the search state changed.
    pub search_changed: bool,
    /// New search state, if search was activated or modified.
    pub new_search: Option<ViewSearch>,
    /// Whether the layout should be recomputed.
    pub layout_dirty: bool,
}

/// Handle a key in View mode. Returns the effect of the keypress.
///
/// Implements:
/// - VN-1: j/k/arrows for line-by-line navigation
/// - VN-3: gg/G/counts for document navigation
/// - VN-4: Tab/Shift-Tab for jump targets
/// - VN-5: search with / and ?
/// - VN-6: n/N for repeat search
pub fn handle_key(
    key: crate::session::KeyInput,
    cursor: &ViewCursor,
    search: Option<&ViewSearch>,
    max_view_lines: usize,
    jump_targets: &[crate::style::JumpTarget],
    layout: &ViewLayout,
    count: usize,
) -> ViewKeyResult {
    let mut result = ViewKeyResult::default();
    let step = if count > 1 { count } else { 1 };

    match key.code.kind {
        // Esc: no-op in View mode (handled by session layer)
        crate::session::KeyCodeKind::Esc => {}

        // j / Down: move down one line
        crate::session::KeyCodeKind::Char('j') | crate::session::KeyCodeKind::Down
            if !key.mods.ctrl && !key.mods.alt && !key.mods.shift =>
        {
            let new_line = cursor
                .line
                .saturating_add(step)
                .min(max_view_lines.saturating_sub(1));
            result.cursor_moved = true;
            result.new_cursor = Some(ViewCursor::new(new_line));
        }

        // k / Up: move up one line
        crate::session::KeyCodeKind::Char('k') | crate::session::KeyCodeKind::Up
            if !key.mods.ctrl && !key.mods.alt && !key.mods.shift =>
        {
            let new_line = cursor.line.saturating_sub(step);
            result.cursor_moved = true;
            result.new_cursor = Some(ViewCursor::new(new_line));
        }

        // h / Left: no-op (horizontal scroll is TUI concern)
        crate::session::KeyCodeKind::Char('h') | crate::session::KeyCodeKind::Left
            if !key.mods.ctrl && !key.mods.alt && !key.mods.shift => {}

        // l / Right: no-op (horizontal scroll is TUI concern)
        crate::session::KeyCodeKind::Char('l') | crate::session::KeyCodeKind::Right
            if !key.mods.ctrl && !key.mods.alt && !key.mods.shift => {}

        // g: go to first line (or start of gg)
        crate::session::KeyCodeKind::Char('g')
            if !key.mods.ctrl && !key.mods.alt && !key.mods.shift =>
        {
            result.cursor_moved = true;
            result.new_cursor = Some(ViewCursor::new(0));
        }

        // G: go to last line (or to line N if count > 0)
        crate::session::KeyCodeKind::Char('G')
            if !key.mods.ctrl && !key.mods.alt && !key.mods.shift =>
        {
            if count > 1 {
                let new_line =
                    (cursor.line.saturating_add(step - 1)).min(max_view_lines.saturating_sub(1));
                result.cursor_moved = true;
                result.new_cursor = Some(ViewCursor::new(new_line));
            } else {
                result.cursor_moved = true;
                result.new_cursor = Some(ViewCursor::new(max_view_lines.saturating_sub(1)));
            }
        }

        // /: start forward search
        crate::session::KeyCodeKind::Char('/')
            if !key.mods.ctrl && !key.mods.alt && !key.mods.shift =>
        {
            let mut new_search = ViewSearch::new("");
            new_search.set_direction(SearchDirection::Forward);
            result.search_changed = true;
            result.new_search = Some(new_search);
        }

        // ?: start backward search
        crate::session::KeyCodeKind::Char('?')
            if !key.mods.ctrl && !key.mods.alt && !key.mods.shift =>
        {
            let mut new_search = ViewSearch::new("");
            new_search.set_direction(SearchDirection::Backward);
            result.search_changed = true;
            result.new_search = Some(new_search);
        }

        // n: repeat search in same direction
        crate::session::KeyCodeKind::Char('n')
            if !key.mods.ctrl && !key.mods.alt && !key.mods.shift =>
        {
            if let Some(current_search) = search {
                result.search_changed = true;
                result.new_search = Some(current_search.clone());
            }
        }

        // N: repeat search in reverse direction
        crate::session::KeyCodeKind::Char('N')
            if !key.mods.ctrl && !key.mods.alt && !key.mods.shift =>
        {
            if let Some(current_search) = search {
                let mut new_search = current_search.clone();
                new_search.set_direction(match new_search.direction() {
                    SearchDirection::Forward => SearchDirection::Backward,
                    SearchDirection::Backward => SearchDirection::Forward,
                });
                result.search_changed = true;
                result.new_search = Some(new_search);
            }
        }

        // Tab: jump to next target
        crate::session::KeyCodeKind::Tab if !key.mods.ctrl && !key.mods.alt && !key.mods.shift => {
            if let Some(target) = next_jump_target(cursor, jump_targets, false) {
                result.cursor_moved = true;
                result.new_cursor = Some(ViewCursor::new(target));
            }
        }

        // Shift-Tab: jump to previous target
        crate::session::KeyCodeKind::BackTab
            if !key.mods.ctrl && !key.mods.alt && !key.mods.shift =>
        {
            if let Some(target) = next_jump_target(cursor, jump_targets, true) {
                result.cursor_moved = true;
                result.new_cursor = Some(ViewCursor::new(target));
            }
        }

        // Page Up / Page Down: handled by session layer with viewport info
        crate::session::KeyCodeKind::PageUp | crate::session::KeyCodeKind::PageDown => {}

        // Home: go to first content line
        crate::session::KeyCodeKind::Home if !key.mods.ctrl && !key.mods.alt && !key.mods.shift => {
            if let Some(first_content) = layout
                .lines
                .iter()
                .position(|l| l.kind == LineKind::Content)
            {
                result.cursor_moved = true;
                result.new_cursor = Some(ViewCursor::new(first_content));
            }
        }

        // End: go to last content line
        crate::session::KeyCodeKind::End if !key.mods.ctrl && !key.mods.alt && !key.mods.shift => {
            let last_content = layout
                .lines
                .iter()
                .rposition(|l| l.kind == LineKind::Content);
            if let Some(last) = last_content {
                result.cursor_moved = true;
                result.new_cursor = Some(ViewCursor::new(last));
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
    cursor: &ViewCursor,
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

/// Find the view line containing the next search match.
///
/// Returns the view line index of the next match, or `None` if no match.
pub fn find_next_match(
    search: &ViewSearch,
    cursor: &ViewCursor,
    layout: &ViewLayout,
    text: &str,
    direction: SearchDirection,
) -> Option<usize> {
    let matches = search.find_matches(text);
    if matches.is_empty() {
        return None;
    }

    // Map byte offsets to view lines
    // For each match, find which view line it falls on
    let mut view_lines_with_matches: Vec<usize> = Vec::new();

    for match_offset in &matches {
        if *match_offset >= text.len() {
            continue;
        }

        // Find the document line containing this match
        let text_prefix = &text[..*match_offset];
        let doc_line = text_prefix.chars().filter(|&c| c == '\n').count();

        // Find the view line for this document line
        let mut doc_line_idx = 0usize;
        for (view_line, view_line_info) in layout.lines.iter().enumerate() {
            if view_line_info.kind == LineKind::Content {
                if doc_line_idx == doc_line {
                    view_lines_with_matches.push(view_line);
                    break;
                }
                doc_line_idx += 1;
            }
        }
    }

    if view_lines_with_matches.is_empty() {
        return None;
    }

    // Deduplicate and sort
    view_lines_with_matches.sort();
    view_lines_with_matches.dedup();

    if direction == SearchDirection::Forward {
        // Find first match after cursor
        view_lines_with_matches
            .iter()
            .find_map(|vl| if *vl > cursor.line { Some(*vl) } else { None })
            .or_else(|| view_lines_with_matches.first().copied())
    } else {
        // Find last match before cursor
        view_lines_with_matches
            .iter()
            .rev()
            .find_map(|vl| if *vl < cursor.line { Some(*vl) } else { None })
            .or_else(|| view_lines_with_matches.last().copied())
    }
}
