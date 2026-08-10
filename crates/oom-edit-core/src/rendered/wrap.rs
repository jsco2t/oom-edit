//! Width-aware styled-text wrapping.
//!
//! `wrap_lines` takes a `StyledLine` and a target width, producing multiple
//! `StyledLine`s that fit within the width. It is unicode-width aware
//! (via the `unicode-width` crate), breaks at spaces when possible,
//! hard-breaks overlong tokens, and preserves span styles across wrap points.
//!
//! A `hanging_indent` parameter lets callers control the indentation of
//! continuation lines (e.g., for list item wrap-arounds).

use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::style::{Span, StyledLine};

/// Wrap a styled line into multiple lines that fit within `width` columns.
///
/// `hanging_indent` is the column offset for continuation lines (line 0 uses
/// no indent; lines 1+ use `hanging_indent` spaces of padding). A value of 0
/// means no hanging indent.
///
/// Styles survive wrap points: if a span is split across a wrap boundary,
/// both resulting lines carry the same `SemanticStyle` for the split portion.
pub fn wrap_lines(input: &StyledLine, width: u16, hanging_indent: u16) -> Vec<StyledLine> {
    if width == 0 {
        return vec![StyledLine {
            text: String::new(),
            spans: Vec::new(),
        }];
    }

    let w = width as usize;
    let hi = hanging_indent as usize;

    // If the text fits on one line (accounting for hanging indent on non-first
    // lines — but for the first line, we just check against width), return it.
    if text_width(&input.text) <= w {
        return vec![input.clone()];
    }

    // We need to wrap. Work at the character level, tracking which spans
    // cover which character positions.
    let chars: Vec<char> = input.text.chars().collect();
    let char_count = chars.len();

    if char_count == 0 {
        return vec![StyledLine {
            text: String::new(),
            spans: Vec::new(),
        }];
    }

    // Build a mapping from character index to span index.
    // Each character position maps to the span that covers it (if any).
    let mut char_to_span: Vec<Option<usize>> = vec![None; char_count];
    for (si, span) in input.spans.iter().enumerate() {
        for item in char_to_span
            .iter_mut()
            .take(span.end_col.min(char_count))
            .skip(span.start_col)
        {
            *item = Some(si);
        }
    }

    let mut result: Vec<StyledLine> = Vec::new();
    let mut pos = 0;

    while pos < char_count {
        let is_first = result.is_empty();
        let available = if is_first { w } else { w.saturating_sub(hi) };

        // Skip leading whitespace on continuation lines
        if !is_first {
            while pos < char_count && chars[pos].is_whitespace() {
                pos += 1;
            }
            if pos >= char_count {
                break;
            }
        }

        if available == 0 {
            // Width too small — hard-break the next character
            let ch = chars[pos];
            let style = char_to_span[pos].map(|si| input.spans[si].style);
            let style_spans = style.map(|s| {
                vec![Span {
                    start_col: hi,
                    end_col: hi + 1,
                    style: s,
                }]
            });

            let mut text = String::new();
            if !is_first {
                text.push_str(&" ".repeat(hi));
            }
            text.push(ch);

            result.push(StyledLine {
                text,
                spans: style_spans.unwrap_or_default(),
            });
            pos += 1;
            continue;
        }

        // Find the end of this wrapped segment.
        // Try to break at a space; otherwise hard-break at `available`.
        let end = find_wrap_point(&chars, pos, available);

        // Extract text for this line
        let line_chars = &chars[pos..end];
        let mut text: String = line_chars.iter().collect();
        // Prepend hanging indent for continuation lines
        if !is_first {
            text = format!("{}{}", " ".repeat(hi), text);
        }

        // Build spans for this line, adjusting column offsets
        let _line_text_width = text_width(&text);
        let col_offset = if is_first { 0 } else { hi };

        let spans: Vec<Span> = {
            // Determine which character indices in the original text are on this line
            let line_char_start = pos;
            let line_char_end = pos + line_chars.len();

            // Collect the unique span indices used on this line
            let mut span_indices: Vec<usize> = Vec::new();
            for ci in line_char_start..line_char_end {
                if let Some(si) = char_to_span.get(ci).copied().flatten() {
                    if !span_indices.contains(&si) {
                        span_indices.push(si);
                    }
                }
            }

            // For each span, compute its portion on this line
            let mut line_spans = Vec::new();
            for &si in &span_indices {
                let src_span = &input.spans[si];
                // Intersection of this span with the current line's char range
                let span_start = src_span.start_col.max(line_char_start);
                let span_end = src_span.end_col.min(line_char_end);

                if span_start < span_end {
                    // Map char indices to column positions within this line
                    let col_start = col_offset + (span_start - line_char_start);
                    let col_end = col_offset + (span_end - line_char_start);
                    line_spans.push(Span {
                        start_col: col_start,
                        end_col: col_end,
                        style: src_span.style,
                    });
                }
            }

            line_spans
        };

        result.push(StyledLine { text, spans });

        pos = end;
    }

    result
}

/// Hard-wrap a source line without dropping or inserting any source character.
///
/// Unlike [`wrap_lines`], this function does not prefer word boundaries or
/// trim continuation whitespace. Source mode must display raw Markdown
/// exactly, so concatenating the returned line texts always reproduces the
/// input text. Zero-width suffixes remain attached to the preceding scalar,
/// and an over-wide scalar is emitted whole so wrapping always makes progress.
pub(crate) fn wrap_source_line(input: &StyledLine, width: u16) -> Vec<StyledLine> {
    if width == 0 {
        return vec![StyledLine {
            text: String::new(),
            spans: Vec::new(),
        }];
    }

    let max_width = usize::from(width);
    if text_width(&input.text) <= max_width {
        return vec![input.clone()];
    }

    let chars: Vec<char> = input.text.chars().collect();
    if chars.is_empty() {
        return vec![input.clone()];
    }

    let mut result = Vec::new();
    let mut start = 0;
    while start < chars.len() {
        let mut end = start;
        let mut display_width = 0;
        while end < chars.len() {
            let character_width = chars[end].width().unwrap_or(0);
            if end > start && character_width > 0 && display_width + character_width > max_width {
                break;
            }

            display_width += character_width;
            end += 1;

            if display_width >= max_width {
                while end < chars.len() && chars[end].width().unwrap_or(0) == 0 {
                    end += 1;
                }
                break;
            }
        }

        let text = chars[start..end].iter().collect();
        let spans = input
            .spans
            .iter()
            .filter_map(|span| {
                let span_start = span.start_col.max(start);
                let span_end = span.end_col.min(end);
                (span_start < span_end).then(|| Span {
                    start_col: span_start - start,
                    end_col: span_end - start,
                    style: span.style,
                })
            })
            .collect();
        result.push(StyledLine { text, spans });
        start = end;
    }

    result
}

/// Find the best wrap point starting from `start` in `chars`, such that the
/// text from `start` to the returned index fits within `max_cols` display
/// columns.
///
/// Prefers breaking at a space character. If no space fits, returns
/// `start + max_cols` (hard break, clamped to char count).
fn find_wrap_point(chars: &[char], start: usize, max_cols: usize) -> usize {
    if start >= chars.len() {
        return start;
    }

    let mut cols = 0;
    let mut last_space = start;

    for (i, ch) in chars.iter().enumerate().skip(start) {
        let w = ch.width().unwrap_or(0);

        // If adding this character exceeds the width
        if cols + w > max_cols {
            // Break at the last space we saw, if any
            if last_space > start {
                return last_space;
            }
            // No space found — hard break here. An over-wide character at
            // the start still has to be consumed so the wrapper makes
            // progress, together with any zero-width suffix it owns.
            if i == start {
                let mut end = i + 1;
                while end < chars.len() && chars[end].width().unwrap_or(0) == 0 {
                    end += 1;
                }
                return end;
            }
            return i;
        }

        cols += w;

        if chars[i] == ' ' {
            last_space = i; // break before the space (exclude space from current line)
        }

        // If we've filled exactly to max_cols, check if we should break
        // at the last space instead of including trailing non-space chars
        if cols == max_cols {
            // If the current character is a space, break before it
            if chars[i] == ' ' {
                return i;
            }
            // If the word ends at the boundary, include it and any zero-width
            // suffix characters before wrapping.
            let mut word_end = i + 1;
            while word_end < chars.len() && chars[word_end].width().unwrap_or(0) == 0 {
                word_end += 1;
            }
            let candidate: String = chars[start..word_end].iter().collect();
            if candidate.width() > max_cols {
                if last_space > start && chars[last_space] == ' ' {
                    return last_space;
                }
                return if i == start { word_end } else { i };
            }
            if word_end >= chars.len() || chars[word_end] == ' ' {
                return word_end;
            }
            // If the last recorded space is strictly before the current position
            // and is actually a space character, prefer breaking at the space
            if last_space > start && last_space < i && chars[last_space] == ' ' {
                return last_space;
            }
            return word_end;
        }
    }

    chars.len()
}

/// Compute the display width of a string in terminal columns.
pub fn text_width(s: &str) -> usize {
    s.width()
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::style::SemanticStyle;

    #[test]
    fn wrap_simple_text() {
        let input = StyledLine {
            text: "hello world".to_string(),
            spans: Vec::new(),
        };
        let result = wrap_lines(&input, 8, 0);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].text, "hello");
        assert_eq!(result[1].text, "world");
    }

    #[test]
    fn wrap_word_ends_exactly_at_width() {
        let input = StyledLine {
            text: "abcde fghi x".to_string(),
            spans: Vec::new(),
        };

        let result = wrap_lines(&input, 10, 0);

        assert_eq!(result.len(), 2);
        assert_eq!(result[0].text, "abcde fghi");
        assert_eq!(result[1].text, "x");
    }

    #[test]
    fn wrap_word_ends_at_width_eol() {
        let input = StyledLine {
            text: "abcde fghi".to_string(),
            spans: Vec::new(),
        };
        let chars: Vec<char> = input.text.chars().collect();

        assert_eq!(find_wrap_point(&chars, 0, 10), chars.len());
        assert_eq!(wrap_lines(&input, 10, 0), vec![input]);
    }

    #[test]
    fn wrap_word_mid_boundary_still_backtracks() {
        let input = StyledLine {
            text: "abcde fghij x".to_string(),
            spans: Vec::new(),
        };

        let result = wrap_lines(&input, 10, 0);

        assert_eq!(result.len(), 2);
        assert_eq!(result[0].text, "abcde");
        assert_eq!(result[1].text, "fghij x");
    }

    #[test]
    fn wrap_word_with_zero_width_suffix_ends_at_width() {
        let input = StyledLine {
            text: "abcde fghi\u{301} x".to_string(),
            spans: Vec::new(),
        };

        let result = wrap_lines(&input, 10, 0);

        assert_eq!(result.len(), 2);
        assert_eq!(result[0].text, "abcde fghi\u{301}");
        assert_eq!(text_width(&result[0].text), 10);
        assert_eq!(result[1].text, "x");
    }

    #[test]
    fn wrap_overlong_word_keeps_zero_width_suffix_with_base() {
        let input = StyledLine {
            text: "abcdefghij\u{301}k".to_string(),
            spans: Vec::new(),
        };

        let result = wrap_lines(&input, 10, 0);

        assert_eq!(result.len(), 2);
        assert_eq!(result[0].text, "abcdefghij\u{301}");
        assert_eq!(text_width(&result[0].text), 10);
        assert_eq!(result[1].text, "k");
    }

    #[test]
    fn wrap_variation_selector_respects_display_width() {
        let input = StyledLine {
            text: "abcdefghi*\u{fe0f} x".to_string(),
            spans: Vec::new(),
        };

        let result = wrap_lines(&input, 10, 0);

        assert_eq!(result.len(), 2);
        assert_eq!(result[0].text, "abcdefghi");
        assert_eq!(result[1].text, "*\u{fe0f} x");
        assert!(result.iter().all(|line| text_width(&line.text) <= 10));
    }

    #[test]
    fn wrap_overwide_variation_sequence_makes_progress() {
        let input = StyledLine {
            text: "*\u{fe0f}x".to_string(),
            spans: Vec::new(),
        };
        let chars: Vec<char> = input.text.chars().collect();

        assert!(find_wrap_point(&chars, 0, 1) > 0);
        let result = wrap_lines(&input, 1, 0);

        assert_eq!(result.len(), 2);
        assert_eq!(result[0].text, "*\u{fe0f}");
        assert_eq!(result[1].text, "x");
    }

    #[test]
    fn wrap_overwide_glyph_at_width_one_makes_progress() {
        let input = StyledLine {
            text: "甲x".to_string(),
            spans: Vec::new(),
        };

        let result = wrap_lines(&input, 1, 0);

        assert_eq!(result.len(), 2);
        assert_eq!(result[0].text, "甲");
        assert_eq!(result[1].text, "x");
    }

    #[test]
    fn wrap_preserves_style_across_wrap() {
        let input = StyledLine {
            text: "hello world foo bar".to_string(),
            spans: vec![Span {
                start_col: 0,
                end_col: 17,
                style: SemanticStyle::Emphasis,
            }],
        };
        let result = wrap_lines(&input, 8, 0);
        assert_eq!(result.len(), 3);

        // Each line should carry the Emphasis style
        for line in &result {
            assert!(
                line.spans
                    .iter()
                    .any(|s| s.style == SemanticStyle::Emphasis),
                "line '{}' should have Emphasis style",
                line.text
            );
        }
    }

    #[test]
    fn wrap_with_hanging_indent() {
        let input = StyledLine {
            text: "first second third fourth".to_string(),
            spans: Vec::new(),
        };
        let result = wrap_lines(&input, 10, 4);
        assert!(result.len() >= 2);
        // First line has no indent
        assert_eq!(result[0].text, "first");
        // Continuation lines have 4-space indent
        for line in result.iter().skip(1) {
            assert!(
                line.text.starts_with("    "),
                "continuation line should have hanging indent: {:?}",
                line.text
            );
        }
    }

    #[test]
    fn wrap_cjk_characters() {
        // CJK characters are 2 columns wide
        let input = StyledLine {
            text: "你好世界hello".to_string(),
            spans: Vec::new(),
        };
        let result = wrap_lines(&input, 8, 0);
        // "你好" = 4 cols, "世界" = 4 cols, total 8 — fits
        // "hello" = 5 cols — needs second line
        assert!(!result.is_empty(), "should produce at least one line");
        // First line should fit within 8 cols
        for line in &result {
            assert!(
                text_width(&line.text) <= 8,
                "line '{}' width {} exceeds 8",
                line.text,
                text_width(&line.text)
            );
        }
    }

    #[test]
    fn wrap_overlong_token_hard_break() {
        let input = StyledLine {
            text: "superlongwordthatdoesnotfit at all".to_string(),
            spans: Vec::new(),
        };
        let result = wrap_lines(&input, 10, 0);
        // The overlong token should be hard-broken
        for line in &result {
            assert!(
                text_width(&line.text) <= 10,
                "line '{}' width {} exceeds 10",
                line.text,
                text_width(&line.text)
            );
        }
    }

    #[test]
    fn unicode_hard_break_span_uses_character_indices() {
        let input = StyledLine {
            text: "é🙂".to_string(),
            spans: vec![Span {
                start_col: 0,
                end_col: 2,
                style: SemanticStyle::Emphasis,
            }],
        };

        let result = wrap_lines(&input, 1, 1);
        let continuation = &result[1];
        let span = continuation
            .spans
            .first()
            .expect("hard-broken Unicode character should retain its style");

        assert_eq!(continuation.text, " 🙂");
        assert_eq!((span.start_col, span.end_col), (1, 2));
        assert_eq!(
            continuation
                .text
                .chars()
                .skip(span.start_col)
                .take(span.end_col - span.start_col)
                .collect::<String>(),
            "🙂"
        );
    }

    #[test]
    fn wrap_empty_text() {
        let input = StyledLine {
            text: String::new(),
            spans: Vec::new(),
        };
        let result = wrap_lines(&input, 80, 0);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].text, "");
    }

    #[test]
    fn wrap_text_fits_single_line() {
        let input = StyledLine {
            text: "short".to_string(),
            spans: Vec::new(),
        };
        let result = wrap_lines(&input, 80, 0);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].text, "short");
    }

    #[test]
    fn wrap_multiple_spans() {
        let input = StyledLine {
            text: "hello world foo".to_string(),
            spans: vec![
                Span {
                    start_col: 0,
                    end_col: 5,
                    style: SemanticStyle::Emphasis,
                },
                Span {
                    start_col: 6,
                    end_col: 11,
                    style: SemanticStyle::Strong,
                },
            ],
        };
        let result = wrap_lines(&input, 8, 0);
        assert!(result.len() >= 2);
        // First line "hello" should have Emphasis
        assert!(result[0]
            .spans
            .iter()
            .any(|s| s.style == SemanticStyle::Emphasis));
        // Second line "world" should have Strong
        assert!(result[1]
            .spans
            .iter()
            .any(|s| s.style == SemanticStyle::Strong));
    }

    #[test]
    fn source_wrap_preserves_whitespace_and_styles() {
        let input = StyledLine {
            text: "# xxxxxxxxxxxx".to_string(),
            spans: vec![Span {
                start_col: 0,
                end_col: 14,
                style: SemanticStyle::Heading1,
            }],
        };

        let result = wrap_source_line(&input, 5);

        assert_eq!(
            result
                .iter()
                .map(|line| line.text.as_str())
                .collect::<Vec<_>>(),
            ["# xxx", "xxxxx", "xxxx"]
        );
        assert_eq!(
            result
                .iter()
                .map(|line| line.text.as_str())
                .collect::<String>(),
            input.text
        );
        assert!(result.iter().all(|line| line.spans
            == vec![Span {
                start_col: 0,
                end_col: line.text.chars().count(),
                style: SemanticStyle::Heading1,
            }]));
    }

    #[test]
    fn source_wrap_keeps_zero_width_suffixes_with_wide_atoms() {
        let input = StyledLine {
            text: "a\u{301}甲x".to_string(),
            spans: Vec::new(),
        };

        let result = wrap_source_line(&input, 2);

        assert_eq!(
            result
                .iter()
                .map(|line| line.text.as_str())
                .collect::<Vec<_>>(),
            ["a\u{301}", "甲", "x"]
        );
        assert_eq!(
            result
                .iter()
                .map(|line| line.text.as_str())
                .collect::<String>(),
            input.text
        );
    }
}
