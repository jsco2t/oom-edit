//! Shared span-building utilities for editor and rendered Markdown renderers.

use oom_edit_core::SemanticStyle;
use ratatui::style::Style;
use ratatui::text::{Line, Span};

use crate::theme::{Theme, Tier};

/// Build ratatui spans from a styled line's text and semantic spans.
pub fn build_spans<'a>(
    text: &'a str,
    spans: &'a [oom_edit_core::Span],
    theme: &Theme,
    tier: Tier,
) -> Vec<Span<'a>> {
    if text.is_empty() {
        return vec![Span::raw("")];
    }

    let fallback_style = theme.style(tier, SemanticStyle::Text);

    if spans.is_empty() {
        return vec![Span::styled(text.to_string(), fallback_style)];
    }

    // Build a list of (start, end, style) ranges.
    let mut ranges: Vec<(usize, usize, Style)> = Vec::new();
    let chars: Vec<char> = text.chars().collect();
    let total_len = chars.len();

    for span in spans {
        let start = span.start_col.min(total_len);
        let end = span.end_col.min(total_len);
        if start < end {
            let style = resolve_style(theme, tier, span.style);
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
                result.push(Span::styled(chars[pos].to_string(), fallback_style));
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
        result.push(Span::styled(chars[pos].to_string(), fallback_style));
        pos += 1;
    }

    if result.is_empty() {
        result.push(Span::raw(""));
    }

    result
}

/// Resolve a core [`SemanticStyle`] to a ratatui [`Style`].
pub fn resolve_style(theme: &Theme, tier: Tier, style: SemanticStyle) -> Style {
    theme.style(tier, style)
}

/// Compose one style layer over a display-cell interval without replacing the
/// semantic spans that produced the base styles. `higher_priority` protects a
/// later layer from an earlier layer's foreground while retaining modifiers.
pub fn apply_interval_style<'a>(
    spans: Vec<Span<'a>>,
    columns: std::ops::Range<usize>,
    decoration: Style,
    higher_priority: Option<Style>,
) -> Line<'a> {
    let mut result = Vec::new();
    let mut display_column = 0usize;
    for span in spans {
        let mut groups: Vec<String> = Vec::new();
        for character in span.content.chars() {
            if Span::raw(character.to_string()).width() == 0 {
                if let Some(previous) = groups.last_mut() {
                    previous.push(character);
                } else {
                    groups.push(character.to_string());
                }
            } else {
                groups.push(character.to_string());
            }
        }
        for group in groups {
            let width = Span::raw(group.as_str()).width();
            let group_columns = display_column..display_column + width;
            display_column += width;
            let decorated = group_columns.end > columns.start && group_columns.start < columns.end;
            let mut style = span.style;
            if decorated {
                style = style.add_modifier(decoration.add_modifier);
                if higher_priority != Some(span.style) {
                    if let Some(foreground) = decoration.fg {
                        style = style.fg(foreground);
                    }
                }
                if let Some(background) = decoration.bg {
                    style = style.bg(background);
                }
            }
            result.push(Span::styled(group, style));
        }
    }
    Line::from(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::{DEFAULT_DARK, DEFAULT_LIGHT};

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

        let rendered = build_spans(&line.text, &line.spans, &DEFAULT_DARK, Tier::TrueColor);
        let expected_style = resolve_style(&DEFAULT_DARK, Tier::TrueColor, SemanticStyle::Emphasis);
        let rendered_text: String = rendered.iter().map(|span| span.content.as_ref()).collect();
        let styled_text: String = rendered
            .iter()
            .filter(|span| span.style == expected_style)
            .map(|span| span.content.as_ref())
            .collect();

        assert_eq!(rendered_text, "café");
        assert_eq!(styled_text, "café");
    }

    #[test]
    fn fallback_text_uses_selected_theme_and_tier() {
        let dark = build_spans("plain", &[], &DEFAULT_DARK, Tier::TrueColor);
        let light = build_spans("plain", &[], &DEFAULT_LIGHT, Tier::TrueColor);
        let monochrome = build_spans("plain", &[], &DEFAULT_DARK, Tier::Monochrome);

        assert_eq!(
            dark[0].style,
            DEFAULT_DARK.style(Tier::TrueColor, SemanticStyle::Text)
        );
        assert_eq!(
            light[0].style,
            DEFAULT_LIGHT.style(Tier::TrueColor, SemanticStyle::Text)
        );
        assert_eq!(
            monochrome[0].style,
            DEFAULT_DARK.style(Tier::Monochrome, SemanticStyle::Text)
        );
        assert_ne!(dark[0].style, light[0].style);
    }

    #[test]
    fn semantic_spans_use_selected_theme_and_tier() {
        let source_spans = [oom_edit_core::Span {
            start_col: 0,
            end_col: 4,
            style: SemanticStyle::Heading1,
        }];

        let light = build_spans("text", &source_spans, &DEFAULT_LIGHT, Tier::TrueColor);
        let monochrome = build_spans("text", &source_spans, &DEFAULT_DARK, Tier::Monochrome);

        assert!(light.iter().all(|span| {
            span.style == DEFAULT_LIGHT.style(Tier::TrueColor, SemanticStyle::Heading1)
        }));
        assert!(monochrome.iter().all(|span| {
            span.style == DEFAULT_DARK.style(Tier::Monochrome, SemanticStyle::Heading1)
        }));
    }
}
