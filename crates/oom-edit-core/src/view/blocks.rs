//! Typed block tree of a markdown document with byte-accurate source spans.
//!
//! `BlockModel::build(text, fm_span)` consumes markdown text and produces a
//! tree of `Block` nodes, each carrying a `Range<usize>` into the original
//! source. Front matter (if `fm_span` is `Some`) is emitted as a leading
//! `Block::FrontMatter` node; the remainder is parsed by pulldown-cmark.
//!
//! Inline text is owned (post-unescape). Inline-level spans are NOT required —
//! VP-1 maps at line granularity via blocks.

use pulldown_cmark::{Alignment, CodeBlockKind, Event, Options, Parser, Tag, TagEnd};

// ── BlockModel ─────────────────────────────────────────────────────────────

/// A typed block tree of a markdown document.
#[derive(Debug)]
pub struct BlockModel {
    /// Top-level blocks, sorted by start byte, non-overlapping.
    pub blocks: Vec<Block>,
}

// ── Block ──────────────────────────────────────────────────────────────────

/// A single block in the document tree.
#[derive(Debug, PartialEq, Eq)]
pub struct Block {
    /// Byte range into the source text (inclusive start, exclusive end).
    pub span: std::ops::Range<usize>,
    /// The block's kind and nested content.
    pub kind: BlockKind,
}

/// The kind of a markdown block.
#[derive(Debug, PartialEq, Eq)]
pub enum BlockKind {
    /// YAML/TOML front matter (passthrough from the caller-provided span).
    FrontMatter,
    /// A heading with its level and inline content.
    Heading {
        /// Heading level (1–6).
        level: u8,
        /// Inline content of the heading.
        inlines: Vec<Inline>,
    },
    /// A paragraph with inline content.
    Paragraph {
        /// Inline content of the paragraph.
        inlines: Vec<Inline>,
    },
    /// A fenced code block.
    CodeFence {
        /// Language identifier, if present (e.g. `rust`, `python`).
        lang: Option<String>,
        /// Byte range of the fence *content* (between the opening and closing
        /// fence delimiters, excluding newline delimiters).
        content_span: std::ops::Range<usize>,
        /// Whether this block is indented (3-4 spaces / 1-2 tabs) rather than
        /// fenced with ```.
        indented: bool,
    },
    /// A list (ordered or unordered) with its items.
    List {
        /// `Some(start)` for ordered lists, `None` for unordered.
        ordered: Option<u64>,
        /// Whether the list is tight (no blank lines between items).
        tight: bool,
        /// List items in document order.
        items: Vec<ListItem>,
    },
    /// A blockquote with nested child blocks.
    BlockQuote {
        /// Nested child blocks within the blockquote.
        children: Vec<Block>,
    },
    /// A table with alignments, header rows, and body rows.
    Table {
        /// Column alignments (one per column).
        alignments: Vec<TableAlignment>,
        /// Header row cell contents.
        header: Vec<Vec<Inline>>,
        /// Body row cell contents (each row is a list of cell inlines).
        rows: Vec<Vec<Vec<Inline>>>,
    },
    /// A thematic break (horizontal rule).
    Rule,
    /// An HTML block with its raw content span.
    HtmlBlock {
        /// Byte range of the HTML block content (same as the block span).
        content_span: std::ops::Range<usize>,
    },
    /// A footnote definition with its label and child blocks.
    FootnoteDef {
        /// Footnote label (e.g. `^my-footnote`).
        label: String,
        /// Definition body blocks.
        children: Vec<Block>,
    },
}

/// Table column alignment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TableAlignment {
    /// Left-aligned (default).
    Left,
    /// Center-aligned.
    Center,
    /// Right-aligned.
    Right,
}

impl TableAlignment {
    fn from_pulldown(align: Alignment) -> Self {
        match align {
            Alignment::None => TableAlignment::Left,
            Alignment::Left => TableAlignment::Left,
            Alignment::Center => TableAlignment::Center,
            Alignment::Right => TableAlignment::Right,
        }
    }
}

// ── ListItem ───────────────────────────────────────────────────────────────

/// A single item within a list.
#[derive(Debug, PartialEq, Eq)]
pub struct ListItem {
    /// Byte range into the source text.
    pub span: std::ops::Range<usize>,
    /// Task list checkbox state, if present (`Some(true)` for checked,
    /// `Some(false)` for unchecked, `None` for a regular list item).
    pub task: Option<bool>,
    /// Child blocks within this list item.
    pub children: Vec<Block>,
}

// ── Inline ─────────────────────────────────────────────────────────────────

/// Inline content within a block (heading, paragraph, table cell, etc.).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Inline {
    /// Plain text (post-unescape).
    Text(String),
    /// Inline code span (content without backticks).
    Code(String),
    /// Soft line break (two+ spaces at end of line in source).
    SoftBreak,
    /// Hard line break (backslash at end of line or two+ newlines).
    HardBreak,
    /// Emphasized text.
    Emph(Vec<Inline>),
    /// Strong (bold) text.
    Strong(Vec<Inline>),
    /// Strikethrough text.
    Strike(Vec<Inline>),
    /// A link with its rendered text and destination URL.
    Link {
        /// Rendered link text.
        text: Vec<Inline>,
        /// Destination URL.
        dest: String,
    },
    /// An image with alt text and destination URL.
    Image {
        /// Alternative text.
        alt: String,
        /// Destination URL.
        dest: String,
    },
    /// A footnote reference.
    FootnoteRef(String),
    /// Raw inline HTML.
    Html(String),
}

// ── BlockModel::build ──────────────────────────────────────────────────────

impl BlockModel {
    /// Build a block model from markdown text.
    ///
    /// `fm_span` is the byte range of front matter (YAML `---` or TOML `+++`
    /// delimiters + content). If provided, a `Block::FrontMatter` node is
    /// emitted and the remainder is parsed by pulldown-cmark.
    ///
    /// Uses exactly `ENABLE_TABLES | ENABLE_FOOTNOTES | ENABLE_TASKLISTS |
    /// ENABLE_STRIKETHROUGH` options.
    pub fn build(text: &str, fm_span: Option<std::ops::Range<usize>>) -> Self {
        let mut blocks = Vec::new();

        // Determine the text to parse (skip front matter if present)
        let parse_start = fm_span.as_ref().map(|s| s.end).unwrap_or(0);
        let parse_text = &text[parse_start..];

        // If nothing left to parse, return what we have
        if parse_text.trim().is_empty() {
            if let Some(span) = fm_span {
                blocks.push(Block {
                    span,
                    kind: BlockKind::FrontMatter,
                });
            }
            return Self { blocks };
        }

        // Set up pulldown-cmark with required extensions
        let mut opts = Options::empty();
        opts.insert(Options::ENABLE_TABLES);
        opts.insert(Options::ENABLE_FOOTNOTES);
        opts.insert(Options::ENABLE_TASKLISTS);
        opts.insert(Options::ENABLE_STRIKETHROUGH);

        // Build the block tree using a single-pass stack machine
        let mut builder = BlockBuilder::new(text, parse_start);
        let parser = Parser::new_ext(parse_text, opts).into_offset_iter();

        for (event, range) in parser {
            let global_range = range.start + parse_start..range.end + parse_start;
            builder.feed(event, global_range);
        }

        // Transfer blocks collected during event processing
        blocks.append(&mut builder.blocks);

        // Flush any remaining open blocks (unclosed blocks on the stack)
        builder.flush_remaining(&mut blocks);

        // Prepend front matter block if present
        if let Some(span) = fm_span {
            blocks.insert(
                0,
                Block {
                    span,
                    kind: BlockKind::FrontMatter,
                },
            );
        }

        Self { blocks }
    }
}

// ── BlockBuilder (stack machine) ───────────────────────────────────────────

/// Inline stack context for emphasis/strong/strikethrough/link/image nesting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InlineStackKind {
    Emph,
    Strong,
    Strike,
    Link,
    Image,
}

/// Single-pass stack machine that converts pulldown-cmark events into blocks.
struct BlockBuilder {
    /// The full source text (for extracting inline text).
    text: String,
    /// Byte offset of where parsing starts in the full text.
    _offset: usize,
    /// Stack of open block contexts.
    stack: Vec<BuildContext>,
    /// Collected top-level blocks.
    blocks: Vec<Block>,
}

/// Context for a block currently being built on the stack.
enum BuildContext {
    /// A paragraph being accumulated (also used for table cells).
    Paragraph { start: usize, inlines: Vec<Inline> },
    /// A heading being accumulated.
    Heading {
        level: u8,
        start: usize,
        inlines: Vec<Inline>,
    },
    /// A blockquote with child blocks being collected.
    BlockQuote { start: usize, children: Vec<Block> },
    /// A list with items being collected.
    List {
        start: usize,
        ordered: Option<u64>,
        tight: bool,
        items: Vec<ListItem>,
    },
    /// A list item with child blocks.
    ListItem { start: usize, children: Vec<Block> },
    /// A table being assembled.
    Table {
        start: usize,
        alignments: Vec<TableAlignment>,
        header: Vec<Vec<Inline>>,
        current_row_cells: Vec<Vec<Inline>>,
        rows: Vec<Vec<Vec<Inline>>>,
        is_header_row: bool,
    },
    /// A fenced code block.
    CodeFence {
        start: usize,
        lang: Option<String>,
        fence_len: usize,
        indented: bool,
    },
    /// An HTML block.
    HtmlBlock { start: usize },
    /// A footnote definition being accumulated.
    FootnoteDef {
        label: String,
        start: usize,
        children: Vec<Block>,
    },
    /// An inline stack (emphasis, strong, strikethrough, link, image).
    InlineStack {
        kind: InlineStackKind,
        _start: usize,
        _dest: String,
        inlines: Vec<Inline>,
    },
}

impl BlockBuilder {
    fn new(text: &str, offset: usize) -> Self {
        Self {
            text: text.to_string(),
            _offset: offset,
            stack: Vec::new(),
            blocks: Vec::new(),
        }
    }

    /// Feed a single event with its global byte range.
    fn feed(&mut self, event: Event<'_>, span: std::ops::Range<usize>) {
        match &event {
            // ── Block-level Start events ───────────────────────────────
            Event::Start(Tag::Paragraph) => {
                self.stack.push(BuildContext::Paragraph {
                    start: span.start,
                    inlines: Vec::new(),
                });
            }

            Event::Start(Tag::Heading { level, .. }) => {
                self.stack.push(BuildContext::Heading {
                    level: (*level as u8).min(6),
                    start: span.start,
                    inlines: Vec::new(),
                });
            }

            Event::Start(Tag::BlockQuote(_kind)) => {
                self.stack.push(BuildContext::BlockQuote {
                    start: span.start,
                    children: Vec::new(),
                });
            }

            Event::Start(Tag::List(ordered)) => {
                self.stack.push(BuildContext::List {
                    start: span.start,
                    ordered: ordered.and_then(Some),
                    tight: true,
                    items: Vec::new(),
                });
            }

            Event::Start(Tag::Item) => {
                self.stack.push(BuildContext::ListItem {
                    start: span.start,
                    children: Vec::new(),
                });
            }

            Event::Start(Tag::TableCell) => {
                // Table cells accumulate inlines via a Paragraph context.
                // pulldown-cmark does NOT emit Start(Paragraph) for table cells,
                // so we push the Paragraph here to collect cell inlines.
                self.stack.push(BuildContext::Paragraph {
                    start: span.start,
                    inlines: Vec::new(),
                });
            }

            Event::Start(Tag::Table(alignment)) => {
                let aligs: Vec<TableAlignment> = alignment
                    .iter()
                    .map(|a| TableAlignment::from_pulldown(*a))
                    .collect();
                self.stack.push(BuildContext::Table {
                    start: span.start,
                    alignments: aligs,
                    header: Vec::new(),
                    current_row_cells: Vec::new(),
                    rows: Vec::new(),
                    is_header_row: true,
                });
            }

            Event::Start(Tag::CodeBlock(kind)) => {
                let (lang, indented) = match kind {
                    CodeBlockKind::Fenced(lang) => (Some(lang.to_string()), false),
                    CodeBlockKind::Indented => (None, true),
                };
                let fence_len = if indented {
                    0
                } else {
                    let fence_line = &self.text[span.start..span.end];
                    fence_line.chars().take_while(|&c| c == '`').count()
                };
                self.stack.push(BuildContext::CodeFence {
                    start: span.start,
                    lang,
                    fence_len,
                    indented,
                });
            }

            Event::Start(Tag::HtmlBlock) => {
                self.stack
                    .push(BuildContext::HtmlBlock { start: span.start });
            }

            Event::Start(Tag::FootnoteDefinition(label)) => {
                self.stack.push(BuildContext::FootnoteDef {
                    label: label.to_string(),
                    start: span.start,
                    children: Vec::new(),
                });
            }

            // ── Inline Start events ────────────────────────────────────
            Event::Start(Tag::Emphasis) => {
                self.stack.push(BuildContext::InlineStack {
                    kind: InlineStackKind::Emph,
                    _start: span.start,
                    _dest: String::new(),
                    inlines: Vec::new(),
                });
            }

            Event::Start(Tag::Strong) => {
                self.stack.push(BuildContext::InlineStack {
                    kind: InlineStackKind::Strong,
                    _start: span.start,
                    _dest: String::new(),
                    inlines: Vec::new(),
                });
            }

            Event::Start(Tag::Strikethrough) => {
                self.stack.push(BuildContext::InlineStack {
                    kind: InlineStackKind::Strike,
                    _start: span.start,
                    _dest: String::new(),
                    inlines: Vec::new(),
                });
            }

            Event::Start(Tag::Link { dest_url, .. }) => {
                self.stack.push(BuildContext::InlineStack {
                    kind: InlineStackKind::Link,
                    _start: span.start,
                    _dest: dest_url.to_string(),
                    inlines: Vec::new(),
                });
            }

            Event::Start(Tag::Image { dest_url, .. }) => {
                self.stack.push(BuildContext::InlineStack {
                    kind: InlineStackKind::Image,
                    _start: span.start,
                    _dest: dest_url.to_string(),
                    inlines: Vec::new(),
                });
            }

            // ── Block-level End events ─────────────────────────────────
            Event::End(TagEnd::Paragraph) => {
                if let Some(BuildContext::Paragraph { inlines, start }) = self.stack.pop() {
                    if !inlines.is_empty() || span.start != span.end {
                        let block = Block {
                            span: start..span.end,
                            kind: BlockKind::Paragraph { inlines },
                        };
                        self.emit_block(block);
                    }
                }
            }

            Event::End(TagEnd::Heading(_)) => {
                if let Some(BuildContext::Heading {
                    level,
                    start,
                    inlines,
                }) = self.stack.pop()
                {
                    let block = Block {
                        span: start..span.end,
                        kind: BlockKind::Heading { level, inlines },
                    };
                    self.emit_block(block);
                }
            }

            Event::End(TagEnd::BlockQuote(_)) => {
                if let Some(BuildContext::BlockQuote { start, children }) = self.stack.pop() {
                    let block = Block {
                        span: start..span.end,
                        kind: BlockKind::BlockQuote { children },
                    };
                    self.emit_block(block);
                }
            }

            Event::End(TagEnd::List(_)) => {
                if let Some(BuildContext::List {
                    start,
                    ordered,
                    tight,
                    items,
                }) = self.stack.pop()
                {
                    let block = Block {
                        span: start..span.end,
                        kind: BlockKind::List {
                            ordered,
                            tight,
                            items,
                        },
                    };
                    self.emit_block(block);
                }
            }

            Event::End(TagEnd::Item) => {
                if let Some(BuildContext::ListItem { start, children }) = self.stack.pop() {
                    let task = self.detect_task_checkbox(start, span.end);

                    // Find the parent list to get tightness
                    let tight = self
                        .stack
                        .iter()
                        .rev()
                        .find_map(|ctx| {
                            if let BuildContext::List { tight, .. } = ctx {
                                Some(*tight)
                            } else {
                                None
                            }
                        })
                        .unwrap_or(false);

                    // If this item has children with blank separation, the list is loose
                    let tight = if !children.is_empty() {
                        let item_text = &self.text[start..span.end];
                        let first_child_start =
                            children.first().map(|c| c.span.start).unwrap_or(span.end);
                        let gap = &item_text[..first_child_start - start];
                        if gap.contains("\n\n") || gap.contains("\r\n\r\n") {
                            false
                        } else {
                            tight
                        }
                    } else {
                        tight
                    };

                    let item = ListItem {
                        span: start..span.end,
                        task,
                        children,
                    };

                    // Add item to parent list
                    if let Some(BuildContext::List {
                        items,
                        tight: ref mut ltight,
                        ..
                    }) = self.stack.last_mut()
                    {
                        if !tight {
                            *ltight = false;
                        }
                        items.push(item);
                    }
                }
            }

            Event::End(TagEnd::Table) => {
                if let Some(BuildContext::Table {
                    start,
                    alignments,
                    header,
                    current_row_cells,
                    mut rows,
                    is_header_row,
                }) = self.stack.pop()
                {
                    // Finalize any remaining cells
                    if !current_row_cells.is_empty() {
                        if is_header_row {
                            // Shouldn't happen, but handle gracefully
                        } else {
                            rows.push(current_row_cells);
                        }
                    }
                    let block = Block {
                        span: start..span.end,
                        kind: BlockKind::Table {
                            alignments,
                            header,
                            rows,
                        },
                    };
                    self.emit_block(block);
                }
            }

            Event::End(TagEnd::CodeBlock) => {
                if let Some(BuildContext::CodeFence {
                    start,
                    lang,
                    fence_len,
                    indented,
                }) = self.stack.pop()
                {
                    let content_start = start + fence_len + 1;
                    let content_end = span.end.saturating_sub(fence_len + 1);
                    let block = Block {
                        span: start..span.end,
                        kind: BlockKind::CodeFence {
                            lang,
                            content_span: content_start..content_end,
                            indented,
                        },
                    };
                    self.emit_block(block);
                }
            }

            Event::End(TagEnd::HtmlBlock) => {
                if let Some(BuildContext::HtmlBlock { start }) = self.stack.pop() {
                    let block = Block {
                        span: start..span.end,
                        kind: BlockKind::HtmlBlock {
                            content_span: start..span.end,
                        },
                    };
                    self.emit_block(block);
                }
            }

            Event::End(TagEnd::FootnoteDefinition) => {
                if let Some(BuildContext::FootnoteDef {
                    label,
                    start,
                    children,
                }) = self.stack.pop()
                {
                    let block = Block {
                        span: start..span.end,
                        kind: BlockKind::FootnoteDef { label, children },
                    };
                    self.emit_block(block);
                }
            }

            Event::End(TagEnd::TableCell) => {
                // Pop the Paragraph context pushed by Start(TableCell) and
                // route its inlines to the Table's current row.
                if let Some(BuildContext::Paragraph { inlines, start: _ }) = self.stack.pop() {
                    if let Some(BuildContext::Table {
                        alignments,
                        header,
                        current_row_cells,
                        rows,
                        is_header_row,
                        ..
                    }) = self.stack.last_mut()
                    {
                        current_row_cells.push(inlines);
                        if current_row_cells.len() == alignments.len() {
                            if *is_header_row {
                                *header = std::mem::take(current_row_cells);
                                *is_header_row = false;
                            } else {
                                rows.push(std::mem::take(current_row_cells));
                            }
                        }
                    }
                }
            }

            // ── Inline End events ──────────────────────────────────────
            Event::End(TagEnd::Emphasis) => {
                if let Some(BuildContext::InlineStack {
                    kind: InlineStackKind::Emph,
                    ..
                }) = self.stack.pop()
                {
                    let inner = self.take_inline_stack_inlines();
                    self.push_inline(Inline::Emph(inner));
                }
            }

            Event::End(TagEnd::Strong) => {
                if let Some(BuildContext::InlineStack {
                    kind: InlineStackKind::Strong,
                    ..
                }) = self.stack.pop()
                {
                    let inner = self.take_inline_stack_inlines();
                    self.push_inline(Inline::Strong(inner));
                }
            }

            Event::End(TagEnd::Strikethrough) => {
                if let Some(BuildContext::InlineStack {
                    kind: InlineStackKind::Strike,
                    ..
                }) = self.stack.pop()
                {
                    let inner = self.take_inline_stack_inlines();
                    self.push_inline(Inline::Strike(inner));
                }
            }

            Event::End(TagEnd::Link) => {
                if let Some(BuildContext::InlineStack {
                    kind: InlineStackKind::Link,
                    _dest,
                    ..
                }) = self.stack.pop()
                {
                    let text = self.take_inline_stack_inlines();
                    self.push_inline(Inline::Link {
                        text,
                        dest: _dest.clone(),
                    });
                }
            }

            Event::End(TagEnd::Image) => {
                if let Some(BuildContext::InlineStack {
                    kind: InlineStackKind::Image,
                    _dest,
                    ..
                }) = self.stack.pop()
                {
                    let inlines = self.take_inline_stack_inlines();
                    let alt = inlines
                        .into_iter()
                        .filter_map(|i| match i {
                            Inline::Text(t) => Some(t),
                            _ => None,
                        })
                        .collect();
                    self.push_inline(Inline::Image {
                        alt,
                        dest: _dest.clone(),
                    });
                }
            }

            // ── Rule (emitted as a standalone event, no Start/End pair) ──
            Event::Rule => {
                self.blocks.push(Block {
                    span: span.start..span.end,
                    kind: BlockKind::Rule,
                });
            }

            // ── Inline events ──────────────────────────────────────────
            Event::Text(text) => {
                self.push_inline(Inline::Text(text.to_string()));
            }

            Event::Code(code) => {
                self.push_inline(Inline::Code(code.to_string()));
            }

            Event::SoftBreak => {
                self.push_inline(Inline::SoftBreak);
            }

            Event::HardBreak => {
                self.push_inline(Inline::HardBreak);
            }

            Event::FootnoteReference(label) => {
                self.push_inline(Inline::FootnoteRef(label.to_string()));
            }

            Event::Html(html) => {
                self.push_inline(Inline::Html(html.to_string()));
            }

            Event::InlineHtml(html) => {
                self.push_inline(Inline::Html(html.to_string()));
            }

            // ── Events we don't need to handle specially ───────────────
            Event::Start(Tag::MetadataBlock(_)) => {}
            Event::End(TagEnd::MetadataBlock(_)) => {}

            _ => {}
        }
    }

    /// Push an inline onto the topmost inline-accumulating context.
    fn push_inline(&mut self, inline: Inline) {
        if let Some(
            BuildContext::InlineStack { inlines, .. }
            | BuildContext::Paragraph { inlines, .. }
            | BuildContext::Heading { inlines, .. },
        ) = self.stack.last_mut()
        {
            inlines.push(inline);
        }
    }

    /// Take the inlines from the topmost InlineStack context.
    fn take_inline_stack_inlines(&mut self) -> Vec<Inline> {
        for ctx in self.stack.iter_mut().rev() {
            if let BuildContext::InlineStack { inlines, .. } = ctx {
                return std::mem::take(inlines);
            }
        }
        Vec::new()
    }

    /// Detect if a list item has a task checkbox by examining source text.
    fn detect_task_checkbox(&self, item_start: usize, item_end: usize) -> Option<bool> {
        let item_text = &self.text[item_start..item_end];
        let first_line = item_text.lines().next().unwrap_or(item_text);
        if first_line.contains("[x]") || first_line.contains("[X]") {
            Some(true)
        } else if first_line.contains("[ ]") {
            Some(false)
        } else {
            None
        }
    }

    /// Emit a completed block — either to a parent context or to top-level.
    fn emit_block(&mut self, block: Block) {
        for ctx in self.stack.iter_mut().rev() {
            match ctx {
                BuildContext::BlockQuote { children, .. } => {
                    children.push(block);
                    return;
                }
                BuildContext::ListItem { children, .. } => {
                    children.push(block);
                    return;
                }
                BuildContext::FootnoteDef { children, .. } => {
                    children.push(block);
                    return;
                }
                BuildContext::Paragraph { .. } => {
                    continue;
                }
                BuildContext::InlineStack { .. } => {
                    continue;
                }
                _ => {
                    break;
                }
            }
        }
        self.blocks.push(block);
    }

    /// Flush any remaining open blocks at the end of parsing.
    fn flush_remaining(&mut self, blocks: &mut Vec<Block>) {
        let text_end = self.text.len();
        while let Some(ctx) = self.stack.pop() {
            match ctx {
                BuildContext::Paragraph { start, inlines } => {
                    if !inlines.is_empty() {
                        blocks.push(Block {
                            span: start..text_end,
                            kind: BlockKind::Paragraph { inlines },
                        });
                    }
                }
                BuildContext::Heading {
                    level,
                    start,
                    inlines,
                } => {
                    blocks.push(Block {
                        span: start..text_end,
                        kind: BlockKind::Heading { level, inlines },
                    });
                }
                BuildContext::BlockQuote { start, children } => {
                    blocks.push(Block {
                        span: start..text_end,
                        kind: BlockKind::BlockQuote { children },
                    });
                }
                BuildContext::List {
                    start,
                    ordered,
                    tight,
                    items,
                } => {
                    blocks.push(Block {
                        span: start..text_end,
                        kind: BlockKind::List {
                            ordered,
                            tight,
                            items,
                        },
                    });
                }
                BuildContext::ListItem { start, children } => {
                    let task = self.detect_task_checkbox(start, text_end);
                    blocks.push(Block {
                        span: start..text_end,
                        kind: BlockKind::List {
                            ordered: None,
                            tight: false,
                            items: vec![ListItem {
                                span: start..text_end,
                                task,
                                children,
                            }],
                        },
                    });
                }
                BuildContext::Table {
                    start,
                    alignments,
                    header,
                    current_row_cells,
                    mut rows,
                    is_header_row,
                } => {
                    // Finalize any remaining cells
                    if !current_row_cells.is_empty() {
                        if is_header_row {
                            // Shouldn't happen, but handle gracefully
                        } else {
                            rows.push(current_row_cells);
                        }
                    }
                    blocks.push(Block {
                        span: start..text_end,
                        kind: BlockKind::Table {
                            alignments,
                            header,
                            rows,
                        },
                    });
                }
                BuildContext::CodeFence {
                    start,
                    lang,
                    fence_len,
                    indented,
                } => {
                    let content_start = start + fence_len + 1;
                    let content_end = text_end.saturating_sub(fence_len + 1);
                    blocks.push(Block {
                        span: start..text_end,
                        kind: BlockKind::CodeFence {
                            lang,
                            content_span: content_start..content_end,
                            indented,
                        },
                    });
                }
                BuildContext::HtmlBlock { start } => {
                    blocks.push(Block {
                        span: start..text_end,
                        kind: BlockKind::HtmlBlock {
                            content_span: start..text_end,
                        },
                    });
                }
                BuildContext::FootnoteDef {
                    label,
                    start,
                    children,
                } => {
                    blocks.push(Block {
                        span: start..text_end,
                        kind: BlockKind::FootnoteDef { label, children },
                    });
                }
                BuildContext::InlineStack { .. } => {}
            }
        }
    }
}
