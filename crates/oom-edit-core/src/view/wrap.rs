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
                    end_col: hi + ch.len_utf8(),
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
            // No space found — hard break here
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
            // If the last recorded space is strictly before the current position
            // and is actually a space character, prefer breaking at the space
            if last_space > start && last_space < i && chars[last_space] == ' ' {
                return last_space;
            }
            return i + 1;
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
}
