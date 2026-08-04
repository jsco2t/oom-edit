//! Editor screen — the source-view rendering.
//!
//! T11 shipped a minimal body-only layout (status row is a single line).
//! T13 adds gutter, hint bar, proper status bar, selections, and match rendering.

use oom_edit_core::session::EditorSession;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::command::registry::Contexts;
use crate::command::Keymap;
use crate::widgets::hint_bar;
use crate::widgets::spans;
use crate::widgets::status_bar;

/// Render the editor screen into the given frame area.
///
/// The `area` is the full terminal rect; the body occupies the top portion
/// and the status row occupies the bottom line.
pub fn render_editor(
    frame: &mut Frame<'_>,
    session: &mut EditorSession,
    top_line: usize,
    status_msg: &str,
    transient: Option<&status_bar::Transient>,
    area: Rect,
) {
    let status_height: u16 = 1;
    let body_height = area.height.saturating_sub(status_height);

    let body_area = Rect {
        x: area.x,
        y: area.y,
        width: area.width,
        height: body_height,
    };

    let status_area = Rect {
        x: area.x,
        y: area.y + body_height,
        width: area.width,
        height: status_height,
    };

    render_body(frame, session, top_line, body_area);
    render_status_row(frame, session, status_msg, transient, status_area);
}

/// Render the editor body (gutter + source lines + cursor + selections).
fn render_body(frame: &mut Frame<'_>, session: &mut EditorSession, top_line: usize, area: Rect) {
    let height = area.height.max(1) as usize;

    let vp = oom_edit_core::session::Viewport {
        top_line,
        height: area.height,
        width: area.width,
    };

    let frame_data = session.render_source(vp);
    let mode = session.mode();
    let cursor = session.cursor();
    let line_count = session.line_count();

    // Compute gutter width.
    let gutter_w = status_bar::gutter_width(line_count) as u16;
    let gutter_w = gutter_w.max(4);
    let gutter_area_width = gutter_w.min(area.width);

    // Render gutter.
    if gutter_area_width > 0 && gutter_area_width < area.width {
        let gutter_area = Rect {
            x: area.x,
            y: area.y,
            width: gutter_area_width,
            height: area.height,
        };
        render_gutter(
            frame,
            mode,
            top_line,
            cursor.0,
            height,
            line_count,
            gutter_area,
        );
    }

    // Text area starts after gutter.
    let text_area = Rect {
        x: area.x + gutter_area_width,
        y: area.y,
        width: area.width.saturating_sub(gutter_area_width),
        height: area.height,
    };

    // Build lines for ratatui from the source frame.
    let mut lines = Vec::with_capacity(height);
    for styled_line in &frame_data.lines {
        let spans = spans::build_spans(&styled_line.text, &styled_line.spans);
        lines.push(Line::from(spans));
    }

    // Fill remaining lines with empty ones.
    while lines.len() < height {
        lines.push(Line::from(""));
    }

    let paragraph = Paragraph::new(lines).block(Block::default().borders(Borders::NONE));
    frame.render_widget(paragraph, text_area);

    // Draw cursor line highlight in Normal/Visual modes (not Insert).
    let cursor_row = frame_data.cursor.0 as usize;
    if matches!(
        mode,
        oom_edit_core::session::Mode::Normal
            | oom_edit_core::session::Mode::Visual
            | oom_edit_core::session::Mode::VisualLine
            | oom_edit_core::session::Mode::VisualBlock
    ) && cursor_row < height
    {
        render_cursor_line_highlight(frame, text_area, cursor_row);
    }

    // Draw cursor.
    let (cursor_row, cursor_col) = frame_data.cursor;
    let row = (text_area.y + cursor_row).min(text_area.y + text_area.height - 1);
    let col = text_area.x + cursor_col.min(text_area.width.saturating_sub(1));
    frame.set_cursor_position(ratatui::layout::Position::new(col, row));

    // Render visual selections.
    for sel in &frame_data.selections {
        render_selection(frame, text_area, sel);
    }
}

/// Render the line-number gutter.
fn render_gutter(
    frame: &mut Frame<'_>,
    mode: oom_edit_core::session::Mode,
    top_line: usize,
    cursor_line: usize,
    height: usize,
    line_count: usize,
    area: Rect,
) {
    let gutter_lines = status_bar::build_gutter(
        mode,
        top_line,
        cursor_line,
        height,
        line_count,
        area.width as usize,
    );

    let mut text_lines = Vec::new();
    for gutter_line in &gutter_lines {
        text_lines.push(Line::styled(
            gutter_line.clone(),
            Style::default().add_modifier(Modifier::DIM),
        ));
    }

    // Pad if needed.
    while text_lines.len() < area.height as usize {
        text_lines.push(Line::from(""));
    }

    let paragraph = Paragraph::new(text_lines).block(Block::default().borders(Borders::NONE));
    frame.render_widget(paragraph, area);
}

/// Render cursor line highlight in Normal/Visual modes.
fn render_cursor_line_highlight(frame: &mut Frame<'_>, area: Rect, cursor_row: usize) {
    let row = area.y + cursor_row as u16;
    if row < area.y + area.height {
        let highlight_area = Rect {
            x: area.x,
            y: row,
            width: area.width,
            height: 1,
        };
        let style = Style::default().bg(ratatui::style::Color::DarkGray);
        // Render a blank rectangle with the cursor line background.
        let paragraph = Paragraph::new("").style(style);
        frame.render_widget(paragraph, highlight_area);
    }
}

/// Render a visual selection as reversed text.
///
/// Full implementation would map byte ranges to viewport coordinates
/// and render a highlighted overlay. Placeholder for now.
fn render_selection(_frame: &mut Frame<'_>, _area: Rect, _sel: &std::ops::Range<usize>) {
    // Selection highlighting would map byte ranges to viewport coordinates
    // and render a highlighted overlay here.
}

/// Render the full status row: hint bar (left) + status bar (right).
fn render_status_row(
    frame: &mut Frame<'_>,
    session: &EditorSession,
    _status_msg: &str,
    transient: Option<&status_bar::Transient>,
    area: Rect,
) {
    let mode = session.mode();
    let ctx = mode_to_context(mode);

    // Build status bar state.
    let path = session
        .document_ref()
        .path()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| "(new)".to_string());
    let dirty = session.is_dirty();
    let cursor = session.cursor();

    // Check if command line is active.
    let command_line = session.command_line();

    let status = status_bar::StatusBar {
        mode,
        path,
        dirty,
        cursor_line: cursor.0 + 1, // 1-based for display
        line_count: session.line_count(),
        command_line: command_line.clone(),
    };

    let status_text = status.build(transient);

    // Split area: hint bar on left (when no transient and no command line),
    // status bar on right (or full width when transient/command line active).
    let has_transient = transient.is_some() && command_line.is_none();
    let cmdline_active = command_line.is_some();

    if cmdline_active || has_transient {
        // Status bar takes full width.
        status_bar::render(frame, area, &status_text);
    } else {
        // Hint bar on left, status bar on right.
        // Reserve RULER_COLS for the ruler on the right.
        let ruler_width = status_bar::RULER_COLS;
        let hint_width = area.width.saturating_sub(ruler_width);

        if hint_width > 0 {
            // Build hint bar text.
            let km = Keymap::default();
            let cells = hint_bar::build_hints(ctx, &km);
            let hint_text = hint_bar::format_hints(&cells, hint_width);

            let hint_area = Rect {
                x: area.x,
                y: area.y,
                width: hint_width,
                height: 1,
            };
            hint_bar::render(frame, hint_area, &hint_text, "");
        }

        // Status bar on right.
        let status_area = Rect {
            x: area.x + hint_width,
            y: area.y,
            width: area.width.saturating_sub(hint_width),
            height: 1,
        };
        status_bar::render(frame, status_area, &status_text);
    }
}

/// Convert editor mode to UI context.
fn mode_to_context(mode: oom_edit_core::session::Mode) -> Contexts {
    match mode {
        oom_edit_core::session::Mode::Normal => Contexts::NORMAL,
        oom_edit_core::session::Mode::Insert => Contexts::INSERT,
        oom_edit_core::session::Mode::Visual
        | oom_edit_core::session::Mode::VisualLine
        | oom_edit_core::session::Mode::VisualBlock => Contexts::VISUAL,
        oom_edit_core::session::Mode::Command => Contexts::COMMAND,
        oom_edit_core::session::Mode::View => Contexts::VIEW,
    }
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode_to_context_normal() {
        assert_eq!(
            mode_to_context(oom_edit_core::session::Mode::Normal),
            Contexts::NORMAL
        );
    }

    #[test]
    fn mode_to_context_insert() {
        assert_eq!(
            mode_to_context(oom_edit_core::session::Mode::Insert),
            Contexts::INSERT
        );
    }

    #[test]
    fn mode_to_context_visual() {
        assert_eq!(
            mode_to_context(oom_edit_core::session::Mode::Visual),
            Contexts::VISUAL
        );
    }

    #[test]
    fn mode_to_context_visual_line() {
        assert_eq!(
            mode_to_context(oom_edit_core::session::Mode::VisualLine),
            Contexts::VISUAL
        );
    }

    #[test]
    fn mode_to_context_visual_block() {
        assert_eq!(
            mode_to_context(oom_edit_core::session::Mode::VisualBlock),
            Contexts::VISUAL
        );
    }

    #[test]
    fn mode_to_context_command() {
        assert_eq!(
            mode_to_context(oom_edit_core::session::Mode::Command),
            Contexts::COMMAND
        );
    }

    #[test]
    fn mode_to_context_view() {
        assert_eq!(
            mode_to_context(oom_edit_core::session::Mode::View),
            Contexts::VIEW
        );
    }
}
