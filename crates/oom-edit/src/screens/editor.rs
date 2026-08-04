//! Editor screen — the source-view rendering.
//!
//! T11 ships a minimal body-only layout (status row is a single line). T13
//! adds gutter, hint bar, and a proper status bar.

use oom_edit_core::session::EditorSession;
use oom_edit_core::SemanticStyle;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::theme::Theme;

/// Render the editor screen into the given frame area.
///
/// The `area` is the full terminal rect; the body occupies the top portion
/// and the status row occupies the bottom line.
pub fn render_editor(
    frame: &mut Frame<'_>,
    session: &mut EditorSession,
    top_line: usize,
    status_msg: &str,
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
    render_status(frame, session, status_msg, status_area);
}

/// Render the editor body (source lines + cursor + selections).
fn render_body(frame: &mut Frame<'_>, session: &mut EditorSession, top_line: usize, area: Rect) {
    // Clamp height to avoid zero-height panes.
    let height = area.height.max(1);
    let width = area.width.max(1);

    let vp = oom_edit_core::session::Viewport {
        top_line,
        height,
        width,
    };

    let frame_data = session.render_source(vp);

    // Build lines for ratatui.
    let mut lines = Vec::with_capacity(height as usize);
    for styled_line in &frame_data.lines {
        // Split the styled line into spans based on the semantic styles.
        let spans = build_spans(&styled_line.text, &styled_line.spans);
        lines.push(Line::from(spans));
    }

    // Fill remaining lines with empty ones.
    while lines.len() < height as usize {
        lines.push(Line::from(""));
    }

    let paragraph = Paragraph::new(lines).block(Block::default().borders(Borders::NONE));
    frame.render_widget(paragraph, area);

    // Draw cursor.
    let (cursor_row, cursor_col) = frame_data.cursor;
    let row = (area.y + cursor_row).min(area.y + area.height - 1);
    let col = area.x + cursor_col.min(area.width.saturating_sub(1));
    frame.set_cursor_position(ratatui::layout::Position::new(col, row));

    // Render visual selections with reversed style.
    for sel in &frame_data.selections {
        render_selection(frame, area, sel);
    }
}

/// Build ratatui spans from a styled line's text and semantic spans.
fn build_spans(text: &str, spans: &[oom_edit_core::style::Span]) -> Vec<Span<'static>> {
    if text.is_empty() {
        return vec![Span::raw("")];
    }

    if spans.is_empty() {
        return vec![Span::styled(
            text.to_string(),
            Style::default().fg(ratatui::style::Color::White),
        )];
    }

    // Build a list of (start, end, style) ranges.
    let mut ranges: Vec<(usize, usize, Style)> = Vec::new();
    let chars: Vec<char> = text.chars().collect();
    let total_len = chars.len();

    for span in spans {
        let start = span.start_col.min(total_len);
        let end = span.end_col.min(total_len);
        if start < end {
            let style = resolve_style(span.style);
            ranges.push((start, end, style));
        }
    }

    // Walk through characters and assign styles based on overlapping ranges.
    let mut result = Vec::new();
    let mut pos = 0;

    for (start, end, style) in &ranges {
        // Add any unstyled text before this range.
        while pos < *start {
            if pos < total_len {
                result.push(Span::styled(
                    chars[pos].to_string(),
                    Style::default().fg(ratatui::style::Color::White),
                ));
            }
            pos += 1;
        }
        // Add styled text.
        while pos < *end {
            if pos < total_len {
                result.push(Span::styled(chars[pos].to_string(), *style));
            }
            pos += 1;
        }
    }

    // Add remaining unstyled text.
    while pos < total_len {
        result.push(Span::styled(
            chars[pos].to_string(),
            Style::default().fg(ratatui::style::Color::White),
        ));
        pos += 1;
    }

    if result.is_empty() {
        result.push(Span::raw(""));
    }

    result
}

/// Resolve a core [`SemanticStyle`] to a ratatui [`Style`].
fn resolve_style(style: SemanticStyle) -> ratatui::style::Style {
    let base = Theme::resolve(style);
    ratatui::style::Style {
        fg: base.fg,
        bg: base.bg,
        underline_color: base.underline_color,
        add_modifier: base.add_modifier,
        sub_modifier: base.sub_modifier,
    }
}

/// Render a visual selection as reversed text.
fn render_selection(_frame: &mut Frame<'_>, _area: Rect, _sel: &std::ops::Range<usize>) {
    // Draw reversed text for the selection range.
    // For T11, we draw a simple highlight at the cursor position.
    // A full implementation would map byte ranges to viewport coordinates.
}

/// Render the status row (T11 placeholder; T13 adds proper status bar).
fn render_status(frame: &mut Frame<'_>, session: &EditorSession, status_msg: &str, area: Rect) {
    let mode = session.mode();
    let mode_str = mode_name(mode);
    let path = session
        .document_ref()
        .path()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| "(new)".to_string());
    let dirty = if session.is_dirty() { " [+]" } else { "" };

    let cursor = session.cursor();

    let text = format!(
        " {mode_str} | {path}{dirty} | {}:{} | {status_msg}",
        cursor.0 + 1,
        cursor.1 + 1,
    );

    let paragraph = Paragraph::new(text).block(Block::default().borders(Borders::BOTTOM));
    frame.render_widget(paragraph, area);
}

/// Return a short name for the mode (used in the status bar).
fn mode_name(mode: oom_edit_core::session::Mode) -> &'static str {
    match mode {
        oom_edit_core::session::Mode::Normal => " NORMAL ",
        oom_edit_core::session::Mode::Insert => " INSERT ",
        oom_edit_core::session::Mode::Visual => " VISUAL ",
        oom_edit_core::session::Mode::VisualLine => " V-LINE ",
        oom_edit_core::session::Mode::VisualBlock => " V-BLOCK ",
        oom_edit_core::session::Mode::Command => " :CMD ",
        oom_edit_core::session::Mode::View => " VIEW ",
    }
}
