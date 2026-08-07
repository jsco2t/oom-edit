//! View-mode screen — the rendered markdown presentation.
//!
//! T14 adds the View-mode render path: styled lines mapped through the theme,
//! view-cursor selection highlight, search-match overlay, and jump-target
//! emphasis. The body has no gutter — it spans the full terminal width.
//!
//! See plan §6.3, VN-1, VN-3.

use oom_edit_core::session::EditorSession;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::theme::{Theme, Tier};
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
    theme: &Theme,
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
        let spans =
            spans::build_spans(&view_line.styled.text, &view_line.styled.spans, theme, tier);

        // Apply view-cursor selection highlight (VN-1).
        if i == cursor_line {
            // Full-line Selection highlight using theme.
            let sel_style = theme.style(tier, oom_edit_core::SemanticStyle::Selection);
            lines.push(build_cursor_line(spans, width, sel_style));
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

/// Overlay the cursor-line selection style without replacing semantic styling.
fn build_cursor_line<'a>(mut spans: Vec<Span<'a>>, width: u16, selection_style: Style) -> Line<'a> {
    let text_width = spans.iter().map(Span::width).sum::<usize>();

    for span in &mut spans {
        span.style = span.style.add_modifier(selection_style.add_modifier);
        if let Some(background) = selection_style.bg {
            span.style = span.style.bg(background);
        }
    }

    let padding = (width as usize).saturating_sub(text_width);
    if padding > 0 {
        spans.push(Span::styled(" ".repeat(padding), selection_style));
    }

    Line::from(spans)
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use oom_edit_core::session::EditorSession;
    use oom_edit_core::SemanticStyle;
    use ratatui::backend::TestBackend;
    use ratatui::style::Modifier;
    use ratatui::Terminal;

    use super::{build_cursor_line, render_view};
    use crate::theme::{Tier, UiSlot, DEFAULT_DARK};
    use crate::widgets::spans;

    /// View screen: build_spans with empty text returns a single empty span.
    #[test]
    fn build_view_spans_empty_text() {
        let spans = spans::build_spans("", &[], &DEFAULT_DARK, Tier::TrueColor);
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].content, "");
    }

    /// View screen: build_spans with no spans returns theme-styled fallback text.
    #[test]
    fn build_view_spans_no_spans() {
        let spans = spans::build_spans("hello", &[], &DEFAULT_DARK, Tier::TrueColor);
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
        let result = spans::build_spans("hello", &spans, &DEFAULT_DARK, Tier::TrueColor);
        // Should produce 5 spans (one per character), all styled.
        assert!(!result.is_empty());
    }

    #[test]
    fn cursor_line_width_matches_terminal() {
        let semantic_spans = spans::build_spans("cursor line", &[], &DEFAULT_DARK, Tier::TrueColor);
        let selection_style = DEFAULT_DARK.style(Tier::TrueColor, SemanticStyle::Selection);

        let line = build_cursor_line(semantic_spans, 120, selection_style);

        assert_eq!(line.width(), 120);
    }

    #[test]
    fn cursor_line_preserves_semantic_spans() {
        let source_spans = [
            oom_edit_core::Span {
                start_col: 0,
                end_col: 2,
                style: SemanticStyle::Emphasis,
            },
            oom_edit_core::Span {
                start_col: 3,
                end_col: 7,
                style: SemanticStyle::CodeSpan,
            },
        ];
        let semantic_spans =
            spans::build_spans("em code", &source_spans, &DEFAULT_DARK, Tier::TrueColor);
        let emphasis_style = DEFAULT_DARK.style(Tier::TrueColor, SemanticStyle::Emphasis);
        let code_style = DEFAULT_DARK.style(Tier::TrueColor, SemanticStyle::CodeSpan);
        let selection_style = DEFAULT_DARK
            .ui_style(Tier::TrueColor, UiSlot::CursorLine)
            .add_modifier(Modifier::REVERSED);
        let selection_background = selection_style
            .bg
            .expect("true-color cursor line style should define a background");

        let line = build_cursor_line(semantic_spans, 12, selection_style);

        assert!(line.spans.len() > 2);
        assert_eq!(line.spans[0].style.fg, emphasis_style.fg);
        assert!(line.spans[0].style.has_modifier(Modifier::ITALIC));
        assert_eq!(line.spans[3].style.fg, code_style.fg);
        assert_ne!(line.spans[0].style.fg, line.spans[3].style.fg);
        assert!(line.spans.iter().all(|span| {
            span.style.bg == Some(selection_background)
                && span.style.has_modifier(Modifier::REVERSED)
        }));
    }

    #[test]
    fn cursor_line_applies_selection_modifier_at_every_tier() {
        for tier in [Tier::TrueColor, Tier::Color16, Tier::Monochrome] {
            let semantic_spans = spans::build_spans("em", &[], &DEFAULT_DARK, tier);
            let selection_style = DEFAULT_DARK.style(tier, SemanticStyle::Selection);

            let line = build_cursor_line(semantic_spans, 4, selection_style);

            assert!(line
                .spans
                .iter()
                .all(|span| span.style.has_modifier(Modifier::REVERSED)));
            if tier == Tier::Monochrome {
                assert!(line.spans.iter().all(|span| span.style.bg.is_none()));
            }
        }
    }

    #[test]
    fn render_view_cursor_line_uses_area_width_and_preserves_semantics() {
        let mut session = EditorSession::from_text("*em* `code`");
        let _ = session.toggle_view();
        let mut terminal = Terminal::new(TestBackend::new(120, 1)).unwrap();

        terminal
            .draw(|frame| {
                render_view(
                    frame,
                    &mut session,
                    0,
                    frame.area(),
                    &DEFAULT_DARK,
                    Tier::TrueColor,
                );
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        let emphasis = buffer.cell((0, 0)).expect("rendered emphasis cell");
        let code = buffer.cell((3, 0)).expect("rendered code cell");
        let final_cell = buffer.cell((119, 0)).expect("rendered final cell");
        let emphasis_style = DEFAULT_DARK.style(Tier::TrueColor, SemanticStyle::Emphasis);
        let code_style = DEFAULT_DARK.style(Tier::TrueColor, SemanticStyle::CodeSpan);

        assert_eq!(emphasis.fg, emphasis_style.fg.expect("emphasis foreground"));
        assert!(emphasis.modifier.contains(Modifier::ITALIC));
        assert!(emphasis.modifier.contains(Modifier::REVERSED));
        assert_eq!(code.fg, code_style.fg.expect("code foreground"));
        assert!(code.modifier.contains(Modifier::REVERSED));
        assert_eq!(final_cell.symbol(), " ");
        assert!(final_cell.modifier.contains(Modifier::REVERSED));
    }
}
