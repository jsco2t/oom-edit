//! Shared span-building utilities for editor and view renderers.

use oom_edit_core::SemanticStyle;
use ratatui::style::Style;
use ratatui::text::Span;

use crate::theme;

/// Build ratatui spans from a styled line's text and semantic spans.
pub fn build_spans<'a>(text: &'a str, spans: &'a [oom_edit_core::style::Span]) -> Vec<Span<'a>> {
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
pub fn resolve_style(style: SemanticStyle) -> ratatui::style::Style {
    #[allow(deprecated)]
    let base = theme::resolve(style);
    ratatui::style::Style {
        fg: base.fg,
        bg: base.bg,
        underline_color: base.underline_color,
        add_modifier: base.add_modifier,
        sub_modifier: base.sub_modifier,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unicode_character_indexed_span_styles_complete_text() {
        let line = oom_edit_core::StyledLine {
            text: "café".to_string(),
            spans: vec![oom_edit_core::Span {
                start_col: 0,
                end_col: 4,
                style: SemanticStyle::Emphasis,
            }],
        };

        let rendered = build_spans(&line.text, &line.spans);
        let expected_style = resolve_style(SemanticStyle::Emphasis);
        let rendered_text: String = rendered.iter().map(|span| span.content.as_ref()).collect();
        let styled_text: String = rendered
            .iter()
            .filter(|span| span.style == expected_style)
            .map(|span| span.content.as_ref())
            .collect();

        assert_eq!(rendered_text, "café");
        assert_eq!(styled_text, "café");
    }
}
