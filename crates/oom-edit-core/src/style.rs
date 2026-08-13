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
    /// Select highlight highlight.
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

/// A single styled source line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StyledLine {
    /// The line text (no trailing newline).
    pub text: String,
    /// Zero or more spans covering portions of `text`. Sorted by `start_col`,
    /// non-overlapping.
    pub spans: Vec<Span>,
}

/// The full rendered source frame for the Insert-mode viewport.
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
}

// ── Rendered layout types ──────────────────────────────────────────────────

/// The kind of a rendered Markdown line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineKind {
    /// A line derived from actual document content.
    Content,
    /// A synthetic line (blank separator, border, links index, footnote
    /// separator). Per VP-1: synthetic lines carry the source span of the
    /// nearest preceding content line.
    Synthetic,
}

/// Renderer-neutral presentation role for a rendered line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RenderedLineRole {
    /// Ordinary rendered document content.
    #[default]
    Document,
    /// YAML/TOML metadata panel content.
    Metadata,
    /// Rendered fenced-code surface, including header, body, and closing rows.
    CodeFence,
}

/// A single rendered Markdown line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderedLine {
    /// The styled text for this line.
    pub styled: StyledLine,
    /// Source byte range into the original markdown text. For content lines
    /// this covers the actual source; for synthetic lines it carries the
    /// nearest preceding content line's span (VP-1).
    pub source: std::ops::Range<usize>,
    /// Whether this line is content or synthetic.
    pub kind: LineKind,
    /// Renderer-neutral surface identity.
    pub role: RenderedLineRole,
    /// Ordered display-cell atoms. Source-derived atoms own exact raw byte
    /// ranges; generated presentation cells carry `None`.
    pub atoms: Vec<RenderedSourceAtom>,
}

/// Smallest selectable rendered unit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderedSourceAtom {
    /// Inclusive/exclusive display-cell columns occupied by the whole atom.
    pub columns: std::ops::Range<usize>,
    /// Raw Markdown ownership, or `None` for a synthetic glyph/cell.
    pub source: Option<std::ops::Range<usize>>,
}

/// The kind of jump target in the rendered layout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TargetKind {
    /// A heading jump target, carrying the heading level (1–6).
    Heading(u8),
    /// A link jump target, carrying the index into `link_index`.
    Link(usize),
    /// A footnote jump target.
    Footnote,
}

/// A jump target within the rendered layout (for navigation).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JumpTarget {
    /// 0-based line number in `RenderedLayout.lines` where this target begins.
    pub line: usize,
    /// The kind of target.
    pub kind: TargetKind,
}

/// The full rendered layout used by Normal and Select.
///
/// Produced by `RenderedLayout::build()` from a `BlockModel`, wrap width, and
/// highlighter. The layout is a sequence of styled, wrapped lines with
/// source-mapping and jump targets for navigation.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RenderedLayout {
    /// Rendered lines, in display order.
    pub lines: Vec<RenderedLine>,
    /// Optional 1-based source line number for each rendered row. Only the
    /// first content row for a distinct source line is numbered; wrapped,
    /// repeated, and synthetic rows are `None`.
    pub line_numbers: Vec<Option<usize>>,
    /// Jump targets sorted by line number.
    pub jump_targets: Vec<JumpTarget>,
    /// Link destinations: `(marker_index, url)` pairs. Marker `[n]` refers
    /// to `link_index[n]`.
    pub link_index: Vec<(usize, String)>,
}

/// A point in final rendered display-cell coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, PartialOrd, Ord)]
pub struct RenderedPoint {
    /// 0-based rendered row.
    pub row: usize,
    /// 0-based display-cell column.
    pub column: usize,
}

/// Cursor position in rendered Normal or Select.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct RenderedCursor {
    /// 0-based line number in the rendered layout.
    pub line: usize,
    /// 0-based display-cell column in the rendered row.
    pub column: usize,
    /// Preferred display column retained across vertical movement.
    pub desired_column: usize,
}

/// Shape of a rendered Select region.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SelectionShape {
    /// Inclusive source-backed atoms between two display points.
    #[default]
    Character,
    /// Exact physical source lines intersected by the endpoints.
    Line,
    /// Inclusive display-column rectangle across rendered rows.
    Block,
}

/// One selected rendered row, including display geometry and raw-source ranges.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderedSelectionRow {
    /// 0-based rendered row.
    pub row: usize,
    /// Selected display-cell interval for painting.
    pub columns: std::ops::Range<usize>,
    /// Ordered raw-source ranges contributed by this row.
    pub source_ranges: Vec<std::ops::Range<usize>>,
}

/// Renderer-neutral metadata for a character, line, or block selection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderedSelection {
    /// Display point where Select began.
    pub anchor: RenderedPoint,
    /// Active display endpoint.
    pub active: RenderedPoint,
    /// Active selection shape.
    pub shape: SelectionShape,
    /// Ordered, normalized, UTF-8-safe raw-source ranges.
    pub source_ranges: Vec<std::ops::Range<usize>>,
    /// Per-row display/source projection used by block edits and overlays.
    pub rows: Vec<RenderedSelectionRow>,
    /// Inclusive block rectangle width, only for [`SelectionShape::Block`].
    pub block_width: Option<usize>,
}

/// Search state for rendered-mode navigation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderedSearch {
    /// The search pattern.
    pub pattern: String,
    /// Whether the pattern is a regex.
    pub is_regex: bool,
    /// The last search direction.
    pub last_direction: SearchDirection,
}

/// Direction of a search operation in rendered mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchDirection {
    /// Search forward (next match).
    Forward,
    /// Search backward (previous match).
    Backward,
}
