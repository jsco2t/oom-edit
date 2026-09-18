//! Pure rope helpers — no editor, host, or vim state involved.
//!
//! These lived in `vim.rs` for historical reasons, which made the
//! mode-agnostic engine core (`substitute.rs`, `editor.rs`) appear to depend
//! on the vim discipline when it only ever wanted rope utilities. Hoisting
//! them here removes the last real engine-core → `vim::` call so `vim.rs` can
//! relocate into `hjkl-vim` (#267 / #265).

/// Return row `r` from a rope as an owned `String`, stripping its line break.
pub fn rope_line_to_str(rope: &ropey::Rope, r: usize) -> String {
    let s = rope.line(r).to_string();
    strip_line_break(&s).to_string()
}

fn strip_line_break(text: &str) -> &str {
    if let Some(stripped) = text.strip_suffix("\r\n") {
        return stripped;
    }
    match text.chars().last() {
        Some(last)
            if matches!(
                last,
                '\n' | '\r' | '\u{000B}' | '\u{000C}' | '\u{0085}' | '\u{2028}' | '\u{2029}'
            ) =>
        {
            &text[..text.len() - last.len_utf8()]
        }
        _ => text,
    }
}

/// Join rows `lo..=hi` from a rope, preserving internal line breaks and
/// stripping the trailing break. Callers must ensure `lo <= hi < rope.len_lines()`.
pub fn rope_row_range_str(rope: &ropey::Rope, lo: usize, hi: usize) -> String {
    let n = rope.len_lines();
    let lo = lo.min(n.saturating_sub(1));
    let hi = hi.min(n.saturating_sub(1));
    if lo > hi {
        return String::new();
    }
    // Use byte-slice to grab the full range in one rope walk.
    let start_byte = rope.line_to_byte(lo);
    // Ropey counts Unicode line breaks and CRLF as one separator. Convert
    // from character boundaries so a multibyte separator stays intact.
    let end_byte = if hi + 1 < n {
        let next_line_char = rope.line_to_char(hi + 1);
        let crlf = next_line_char >= 2
            && rope.char(next_line_char - 1) == '\n'
            && rope.char(next_line_char - 2) == '\r';
        rope.char_to_byte(next_line_char - if crlf { 2 } else { 1 })
    } else {
        rope.len_bytes()
    };
    rope.byte_slice(start_byte..end_byte).to_string()
}

/// Snapshot all rows from a rope as `Vec<String>` (no trailing `\n`).
/// Use only when the caller truly needs mutable per-row access; prefer
/// rope iterators otherwise.
pub fn rope_to_lines_vec(rope: &ropey::Rope) -> Vec<String> {
    let n = rope.len_lines();
    (0..n).map(|r| rope_line_to_str(rope, r)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ropey::Rope;

    #[test]
    fn line_to_str_strips_newline() {
        let rope = Rope::from_str("abc\ndef\n");
        assert_eq!(rope_line_to_str(&rope, 0), "abc");
        assert_eq!(rope_line_to_str(&rope, 1), "def");
    }

    #[test]
    fn line_to_str_final_line_without_newline() {
        let rope = Rope::from_str("abc\ndef");
        assert_eq!(rope_line_to_str(&rope, 1), "def");
    }

    #[test]
    fn line_to_str_strips_multibyte_line_breaks() {
        let rope = Rope::from_str("a\u{85}b\u{2028}c\r\nd");
        assert_eq!(rope_line_to_str(&rope, 0), "a");
        assert_eq!(rope_line_to_str(&rope, 1), "b");
        assert_eq!(rope_line_to_str(&rope, 2), "c");
        assert_eq!(rope_line_to_str(&rope, 3), "d");
    }

    #[test]
    fn row_range_str_joins_inclusive() {
        let rope = Rope::from_str("a\nb\nc\n");
        assert_eq!(rope_row_range_str(&rope, 0, 1), "a\nb");
        assert_eq!(rope_row_range_str(&rope, 1, 2), "b\nc");
    }

    #[test]
    fn row_range_str_single_row() {
        let rope = Rope::from_str("a\nb\nc");
        assert_eq!(rope_row_range_str(&rope, 1, 1), "b");
    }

    #[test]
    fn row_range_str_preserves_boundaries_around_unicode_line_breaks() {
        let rope = Rope::from_str("a\u{85}b\u{2028}c\r\nd");
        assert_eq!(rope_row_range_str(&rope, 0, 0), "a");
        assert_eq!(rope_row_range_str(&rope, 0, 1), "a\u{85}b");
        assert_eq!(rope_row_range_str(&rope, 1, 2), "b\u{2028}c");
        assert_eq!(rope_row_range_str(&rope, 2, 3), "c\r\nd");
    }

    #[test]
    fn row_range_str_clamps_out_of_bounds() {
        let rope = Rope::from_str("a\nb");
        // hi past the end clamps to the last row rather than panicking.
        assert_eq!(rope_row_range_str(&rope, 0, 99), "a\nb");
    }

    #[test]
    fn row_range_str_empty_when_lo_gt_hi() {
        let rope = Rope::from_str("a\nb\nc");
        assert_eq!(rope_row_range_str(&rope, 2, 1), "");
    }

    #[test]
    fn to_lines_vec_snapshots_all_rows() {
        let rope = Rope::from_str("a\nb\nc");
        assert_eq!(rope_to_lines_vec(&rope), vec!["a", "b", "c"]);
    }
}
