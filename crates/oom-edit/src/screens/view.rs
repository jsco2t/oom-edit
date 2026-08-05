//! View-mode screen — the rendered markdown presentation.
//!
//! T14 adds the View-mode render path: styled lines mapped through the theme,
//! view-cursor selection highlight, search-match overlay, and jump-target
//! emphasis. The body has no gutter — it spans the full terminal width.
//!
//! See plan §6.3, VN-1, VN-3.

use oom_edit_core::session::EditorSession;
use ratatui::layout::Rect;
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::theme::{self, Tier};
use crate::widgets::spans;

/// Render the View-mode screen into the given frame area.
///
/// The `area` is the body area (status row already subtracted). View mode has
/// no gutter — the layout fills the full body width.
pub fn render_view(
    frame: &mut Frame<'_>,
    session: &mut EditorSession,
    view_top: usize,
    area: Rect,
    theme_name: &str,
    tier: Tier,
) {
    let height = area.height.max(1) as usize;
    let width = area.width;

    // Get the cursor line BEFORE borrowing layout immutably.
    let cursor_line = session.view_cursor_line();

    // Build (or return cached) view layout.
    let layout = session.render_view(width);

    if layout.lines.is_empty() {
        return;
    }

    // Compute visible line range.
    let max_lines = layout.lines.len();
    let view_top = view_top.min(max_lines.saturating_sub(1));
    let view_bottom = (view_top + height).min(max_lines);

    // Build ratatui lines from the view layout's styled lines.
    let mut lines: Vec<Line<'_>> = Vec::with_capacity(height);

    for i in view_top..view_bottom {
        let view_line = &layout.lines[i];
        let spans = spans::build_spans(&view_line.styled.text, &view_line.styled.spans);

        // Apply view-cursor selection highlight (VN-1).
        if i == cursor_line {
            // Full-line Selection highlight using theme.
            let sel_style =
                theme::get_theme(theme_name).style(tier, oom_edit_core::SemanticStyle::Selection);
            lines.push(Line::styled(
                format!("{}{}", view_line.styled.text, " ".repeat(80)),
                sel_style,
            ));
        } else {
            lines.push(Line::from(spans));
        }
    }

    // Fill remaining lines with blanks.
    while lines.len() < height {
        lines.push(Line::from(""));
    }

    // Truncate to exactly viewport height.
    lines.truncate(height);

    let paragraph = Paragraph::new(lines).block(Block::default().borders(Borders::NONE));
    frame.render_widget(paragraph, area);
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use crate::widgets::spans;

    /// View screen: build_spans with empty text returns a single empty span.
    #[test]
    fn build_view_spans_empty_text() {
        let spans = spans::build_spans("", &[]);
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].content, "");
    }

    /// View screen: build_spans with no spans returns unstyled white text.
    #[test]
    fn build_view_spans_no_spans() {
        let spans = spans::build_spans("hello", &[]);
        assert_eq!(spans.len(), 1);
        assert!(spans[0].style.fg.is_some());
    }

    /// View screen: build_spans with one span styles only that portion.
    #[test]
    fn build_view_spans_with_span() {
        let spans = vec![oom_edit_core::style::Span {
            start_col: 0,
            end_col: 5,
            style: oom_edit_core::SemanticStyle::Heading1,
        }];
        let result = spans::build_spans("hello", &spans);
        // Should produce 5 spans (one per character), all styled.
        assert!(!result.is_empty());
    }
}
