//! Renderer-agnostic style model.
//!
//! The core emits semantic style slots (`SemanticStyle`); colors exist only in
//! the TUI's theme module. No signal is ever color-only — a modifier or text
//! glyph always carries it (accessibility is a requirement, not a theme).
//!
//! See architecture §5 for the full type contract.

// ── SemanticStyle ──────────────────────────────────────────────────────────

/// Semantic display slots. The ONLY style vocabulary the core speaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SemanticStyle {
    /// Plain text.
    Text,
    /// Heading level 1.
    Heading1,
    /// Heading level 2.
    Heading2,
    /// Heading level 3.
    Heading3,
    /// Heading level 4.
    Heading4,
    /// Heading level 5.
    Heading5,
    /// Heading level 6.
    Heading6,
    /// Emphasized (italic) text.
    Emphasis,
    /// Strong (bold) text.
    Strong,
    /// Strikethrough text.
    Strikethrough,
    /// Inline code span.
    CodeSpan,
    /// Fenced code block.
    CodeBlock,
    /// Blockquote.
    Quote,
    /// List marker (bullet, number, etc.).
    ListMarker,
    /// Link text.
    Link,
    /// Link URL.
    LinkUrl,
    /// Horizontal rule.
    Rule,
    /// Raw HTML.
    HtmlRaw,
    /// Front matter delimiter (`---`).
    FmDelimiter,
    /// Front matter key.
    FmKey,
    /// Front matter value.
    FmValue,
    /// Keyword (from code blocks).
    Keyword,
    /// Function name (from code blocks).
    Function,
    /// Type name (from code blocks).
    TypeName,
    /// String literal (from code blocks).
    StringLit,
    /// Number literal (from code blocks).
    NumberLit,
    /// Comment (from code blocks).
    Comment,
    /// Operator (from code blocks).
    Operator,
    /// Variable name (from code blocks).
    Variable,
    /// Punctuation (from code blocks).
    Punct,
    /// Visual selection highlight.
    Selection,
    /// Search match highlight.
    Match,
    /// Cursor line highlight.
    CursorLine,
    /// Muted / dimmed text.
    Muted,
}

// ── Span / StyledLine ──────────────────────────────────────────────────────

/// A styled span within a single line of text. `start_col` is inclusive,
/// `end_col` is exclusive. Spans are sorted by `start_col` and non-overlapping.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Span {
    /// Inclusive start column (0-based character index).
    pub start_col: usize,
    /// Exclusive end column (0-based character index).
    pub end_col: usize,
    /// The semantic style for this span.
    pub style: SemanticStyle,
}

/// A single line of styled text for the source editor view.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StyledLine {
    /// The line text (no trailing newline).
    pub text: String,
    /// Zero or more spans covering portions of `text`. Sorted by `start_col`,
    /// non-overlapping.
    pub spans: Vec<Span>,
}

/// The full rendered source frame for the editor viewport.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceFrame {
    /// Exactly `viewport.height` lines (padded with empty StyledLines if the
    /// buffer has fewer lines).
    pub lines: Vec<StyledLine>,
    /// 1-based line number of the first line in `lines`.
    pub first_line_number: usize,
    /// Cursor position in viewport-relative coordinates: `(row, col)` where
    /// `row` is 0-based within `lines` and `col` is the character column.
    pub cursor: (u16, u16),
    /// Visual-mode selection ranges, viewport-relative, expressed as byte
    /// ranges into the full document text.
    pub selections: Vec<std::ops::Range<usize>>,
}
