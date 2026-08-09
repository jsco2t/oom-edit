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
    /// The document line number displayed for each visual row. The first
    /// visual row of a document line is `Some(1-based line number)`; wrapped
    /// continuation rows and padding rows are `None`.
    pub line_numbers: Vec<Option<usize>>,
    /// 1-based document line number selected by `viewport.top_line`.
    pub first_line_number: usize,
    /// Cursor position in viewport-relative screen coordinates: `(row, col)`
    /// where `row` is 0-based within `lines` and `col` is the character column
    /// within that visual row.
    pub cursor: (u16, u16),
    /// Visual-mode selection ranges, viewport-relative, expressed as byte
    /// ranges into the full document text.
    pub selections: Vec<std::ops::Range<usize>>,
}

// ── View layout types ──────────────────────────────────────────────────────

/// The kind of a rendered view line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineKind {
    /// A line derived from actual document content.
    Content,
    /// A synthetic line (blank separator, border, links index, footnote
    /// separator). Per VP-1: synthetic lines carry the source span of the
    /// nearest preceding content line.
    Synthetic,
}

/// A single rendered line in the View layout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ViewLine {
    /// The styled text for this line.
    pub styled: StyledLine,
    /// Source byte range into the original markdown text. For content lines
    /// this covers the actual source; for synthetic lines it carries the
    /// nearest preceding content line's span (VP-1).
    pub source: std::ops::Range<usize>,
    /// Whether this line is content or synthetic.
    pub kind: LineKind,
}

/// The kind of jump target in the View layout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TargetKind {
    /// A heading jump target, carrying the heading level (1–6).
    Heading(u8),
    /// A link jump target, carrying the index into `link_index`.
    Link(usize),
    /// A footnote jump target.
    Footnote,
}

/// A jump target within the View layout (for navigation).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JumpTarget {
    /// 0-based line number in `ViewLayout.lines` where this target begins.
    pub line: usize,
    /// The kind of target.
    pub kind: TargetKind,
}

/// The full rendered layout for View mode.
///
/// Produced by `ViewLayout::build()` from a `BlockModel`, wrap width, and
/// highlighter. The layout is a sequence of styled, wrapped lines with
/// source-mapping and jump targets for navigation.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ViewLayout {
    /// Rendered lines, in display order.
    pub lines: Vec<ViewLine>,
    /// Jump targets sorted by line number.
    pub jump_targets: Vec<JumpTarget>,
    /// Link destinations: `(marker_index, url)` pairs. Marker `[n]` refers
    /// to `link_index[n]`.
    pub link_index: Vec<(usize, String)>,
}

/// Cursor position in View mode (0-based line number).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ViewCursor {
    /// 0-based line number in the view layout.
    pub line: usize,
}

/// Search state for View mode navigation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ViewSearch {
    /// The search pattern.
    pub pattern: String,
    /// Whether the pattern is a regex.
    pub is_regex: bool,
    /// The last search direction.
    pub last_direction: SearchDirection,
}

/// Direction of a search operation in View mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchDirection {
    /// Search forward (next match).
    Forward,
    /// Search backward (previous match).
    Backward,
}
