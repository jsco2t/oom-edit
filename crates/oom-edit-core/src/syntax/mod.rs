//! Syntax highlighting pipeline.
//!
//! `Highlighter` provides incremental tree-sitter highlighting of markdown
//! source with YAML/TOML front-matter and fenced code-block injections,
//! emitting renderer-agnostic [`StyledLine`]s.
//!
//! See architecture §6.3, plan §6.4 (FR-4.1–4.6), and task T06.

mod captures;
mod languages;

pub use captures::capture_to_style;
pub use languages::{find_by_alias, find_by_name, resolve_language, LangDef, LANGUAGES};

use std::{collections::HashMap, ops::Range, sync::Arc};

use tree_sitter::{Parser, Point, Query, QueryCursor, StreamingIterator, Tree};

use crate::style::{SemanticStyle, Span, StyledLine};
use crate::vim::TextEdit;

// ── Markdown highlight query ────────────────────────────────────────────────

/// The highlight query for markdown block structure. Covers all FR-4.1
/// block-level elements: headings, fence delimiters, list markers,
/// blockquotes, thematic breaks, HTML, front-matter, tables, and link
/// reference definitions.
///
/// Inline elements (emphasis, strong, inline code, strikethrough, links)
/// are handled by the inline injection grammar, not this block-level query.
///
/// Node types are from tree-sitter-md's actual grammar (not nvim-treesitter
/// extensions like atx_h1_marker which are not in the upstream grammar).
const MD_HIGHLIGHT_QUERY: &str = r#"
(atx_heading) @heading
(setext_heading) @heading
(fenced_code_block) @fence.block
(fenced_code_block_delimiter) @fence.delimiter
(code_fence_content) @fence.content
(info_string) @fence.info
(language) @fence.language
(list_marker_plus) @list.marker
(list_marker_minus) @list.marker
(list_marker_star) @list.marker
(list_marker_dot) @list.marker
(list_marker_parenthesis) @list.marker
(block_quote) @quote.block
(block_quote_marker) @quote.marker
(thematic_break) @rule
(html_block) @html.block
(minus_metadata) @fm.yaml
(plus_metadata) @fm.toml
(pipe_table) @table
"#;

/// Highlight query for markdown inline content (emphasis, code spans, links, etc.).
const MD_INLINE_QUERY: &str = r#"
(emphasis) @emphasis
(emphasis_delimiter) @emphasis.marker
(strong_emphasis) @strong
(strikethrough) @strikethrough
(code_span) @code.span
(code_span_delimiter) @code.span.delimiter
(link_destination) @link.url
(link_text) @link.text
(link_label) @link.label
(link_title) @link.title
"#;

// ── Injection region ────────────────────────────────────────────────────────

/// A region inside the markdown document that should be highlighted with a
/// non-markdown grammar (front matter, fenced code block, or inline region).
#[derive(Debug)]
struct Injection {
    /// Byte offset where this region starts in the document.
    start: usize,
    /// Byte offset where this region ends.
    end: usize,
    /// The tree-sitter language to use for highlighting.
    language: tree_sitter::Language,
    /// The query to run for this language (wrapped in Arc for sharing).
    query: Arc<Query>,
    /// Canonical registry language name used for context-sensitive styling.
    language_name: &'static str,
    /// Whether this region is front matter or a fenced code block.
    kind: InjectionKind,
    /// For fence blocks: the language tag from the info string.
    #[allow(dead_code)]
    fence_lang: Option<String>,
}

/// Immutable metadata collected from the markdown tree before injection
/// queries are resolved through the mutable cache.
struct InjectionMeta {
    start: usize,
    end: usize,
    language: &'static LangDef,
    kind: InjectionKind,
    fence_lang: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct RankedSpan {
    span: Span,
    priority: usize,
}

/// A semantic span expressed in absolute UTF-8 byte offsets.
///
/// Tree-sitter and all range/intersection processing use byte coordinates.
/// This private type keeps those values distinct from the character-indexed
/// public [`Span`] type until a [`StyledLine`] is constructed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ByteSpan {
    start_byte: usize,
    end_byte: usize,
    style: SemanticStyle,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum InjectionKind {
    FrontMatter,
    Fence,
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ParsePath {
    Full,
    Incremental,
    Skipped,
}

// ── Highlighter ─────────────────────────────────────────────────────────────

/// Incremental tree-sitter highlighter for markdown documents.
///
/// The `Highlighter` owns:
/// - A markdown block parser + tree
/// - Per-language parsers for injected regions (front matter, fences, inline)
/// - Pre-compiled highlight queries for each language
///
/// See architecture §6.3 for the full pipeline description.
pub struct Highlighter {
    /// The full document text.
    text: String,
    /// The main markdown block parse tree.
    md_tree: Tree,
    /// Markdown block highlight query.
    md_query: Query,
    /// Markdown inline highlight query.
    md_inline_query: Query,
    /// Incremental parser for the markdown tree.
    md_parser: Parser,
    /// Injection regions (front matter, fenced code blocks).
    injections: Vec<Injection>,
    /// Compiled injection queries, keyed by canonical registry language name.
    query_cache: HashMap<&'static str, Arc<Query>>,
    /// Most recent parser route, exposed only to regression tests.
    #[cfg(test)]
    last_parse_path: ParsePath,
}

impl Highlighter {
    /// Create a new `Highlighter` for the given document text.
    ///
    /// Parses the full document with the markdown block grammar and
    /// discovers all injection regions (front matter, fenced code blocks,
    /// inline regions).
    ///
    /// FR-4.1 / FR-4.2 / FR-4.3.
    pub fn new(text: &str) -> Self {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_md::LANGUAGE.into())
            .expect("tree-sitter-md language should be valid");

        let tree = parser
            .parse(text, None)
            .expect("markdown parse should succeed");
        let md_language: tree_sitter::Language = tree_sitter_md::LANGUAGE.into();
        let query = Query::new(&md_language, MD_HIGHLIGHT_QUERY)
            .expect("markdown highlight query should compile");
        let inline_language: tree_sitter::Language = tree_sitter_md::INLINE_LANGUAGE.into();
        let inline_query = Query::new(&inline_language, MD_INLINE_QUERY)
            .expect("markdown inline highlight query should compile");

        let mut highlighter = Self {
            text: text.to_string(),
            md_tree: tree,
            md_query: query,
            md_inline_query: inline_query,
            md_parser: parser,
            injections: Vec::new(),
            query_cache: HashMap::new(),
            #[cfg(test)]
            last_parse_path: ParsePath::Full,
        };

        // Discover injection regions
        highlighter.discover_injections();

        highlighter
    }

    /// Return the current document text.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Apply a batch of text edits and incrementally re-parse.
    ///
    /// This is the FR-4.4 incremental highlighting path: only the affected
    /// trees are re-parsed, keeping keystroke latency within NFR-3 budget.
    ///
    /// Edits are applied in their incoming sequential order and injection
    /// regions are re-resolved after the batch.
    pub fn apply_edit(&mut self, edits: &[TextEdit]) {
        if edits.is_empty() {
            return;
        }

        #[cfg(test)]
        {
            self.last_parse_path = ParsePath::Skipped;
        }

        let mut working_text = self.text.clone();

        let mut tree_was_edited = false;
        for edit in edits {
            // Every coordinate is relative to the document produced by the
            // preceding entry, so compute and apply the tree edit before
            // applying the identical replacement to the working text.
            if let Some(tree_edit) = input_edit(&working_text, edit) {
                self.md_tree.edit(&tree_edit);
                tree_was_edited = true;
            }
            apply_edit_to_string(&mut working_text, edit);
        }
        self.text = working_text;

        if !tree_was_edited {
            return;
        }

        let old_tree = Some(&self.md_tree);
        self.md_tree = self
            .md_parser
            .parse(&self.text, old_tree)
            .expect("re-parse should succeed");
        #[cfg(test)]
        {
            self.last_parse_path = ParsePath::Incremental;
        }

        // Re-discover injections
        self.discover_injections();
    }

    /// Highlight the given line range and return styled lines.
    ///
    /// This is the primary rendering API: it always returns fully-styled
    /// lines for the requested viewport (FR-4.4). The visible viewport is
    /// never rendered un-highlighted while a re-parse completes.
    ///
    /// `lines` is a 0-based inclusive range of line indices.
    pub fn highlight_lines(&self, lines: Range<usize>) -> Vec<StyledLine> {
        if lines.start >= lines.end {
            return Vec::new();
        }

        let text = &self.text;
        let line_starts = line_start_indices(text);

        // Number of actual lines: if text ends with newline, last element is
        // the position after the final newline (not a real line start).
        let num_lines = if text.ends_with('\n') && !text.is_empty() {
            line_starts.len().saturating_sub(1)
        } else {
            line_starts.len()
        };

        // Clamp to valid range
        let end = lines.end.min(num_lines);
        let start = lines.start.min(end);

        if start >= end {
            return vec![StyledLine {
                text: String::new(),
                spans: Vec::new(),
            }];
        }

        // Compute the byte range covering all requested lines
        let range_start = line_starts[start];
        let range_end = if end < line_starts.len() {
            line_starts[end]
        } else {
            text.len()
        };

        // Collect spans scoped to the requested byte range
        let spans = self.collect_spans_in_range(range_start..range_end);

        let mut result = Vec::with_capacity(end - start);

        for line_idx in start..end {
            let line_start = line_starts[line_idx];
            let next_line_start = if line_idx + 1 < line_starts.len() {
                line_starts[line_idx + 1]
            } else {
                text.len()
            };
            // Strip trailing newline if present
            let line_end =
                if next_line_start > line_start && text.as_bytes()[next_line_start - 1] == b'\n' {
                    next_line_start - 1
                } else {
                    next_line_start
                };
            let line_text = &text[line_start..line_end.min(text.len())];

            // Filter spans that overlap with this line's byte range
            let line_spans: Vec<RankedSpan> = spans
                .iter()
                .enumerate()
                .filter(|(_, s)| {
                    let abs_start = s.start_byte;
                    let abs_end = s.end_byte;
                    abs_start < line_end && abs_end > line_start
                })
                .map(|(priority, s)| {
                    let relative_start = s.start_byte.max(line_start) - line_start;
                    let relative_end = s.end_byte.min(line_end) - line_start;
                    RankedSpan {
                        span: Span {
                            start_col: byte_offset_to_char_index(line_text, relative_start),
                            end_col: byte_offset_to_char_index(line_text, relative_end),
                            style: s.style,
                        },
                        priority,
                    }
                })
                .collect();

            let mut line_spans = merge_overlapping_spans(line_spans);
            line_spans.sort_by_key(|span| span.start_col);
            debug_assert!(
                line_spans
                    .windows(2)
                    .all(|pair| pair[0].end_col <= pair[1].start_col),
                "highlight_lines produced overlapping spans on line {line_idx}: {line_spans:?}"
            );

            result.push(StyledLine {
                text: line_text.to_string(),
                spans: line_spans,
            });
        }

        result
    }

    /// Collect spans scoped to a byte range, limiting the query to only
    /// the requested region of the document.
    fn collect_spans_in_range(&self, range: Range<usize>) -> Vec<ByteSpan> {
        let mut spans = Vec::new();
        let text = &self.text;
        let text_bytes = text.as_bytes();
        let root = self.md_tree.root_node();

        // Get the smallest subtree covering the requested byte range
        let end = range.end.min(text.len());
        if range.start >= end {
            return spans;
        }
        let subtree = root
            .descendant_for_byte_range(range.start, end)
            .expect("byte range must be within document");

        // Run the markdown highlight query on the subtree
        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(&self.md_query, subtree, text_bytes);

        while let Some(m) = matches.next() {
            for capture in m.captures {
                let capture_name = self.md_query.capture_names()[capture.index as usize];
                let style = md_capture_to_style(capture_name);
                let n = capture.node;

                let start_byte = n.start_byte().max(range.start);
                let end_byte = n.end_byte().min(range.end);

                if start_byte < end_byte {
                    spans.push(ByteSpan {
                        start_byte,
                        end_byte,
                        style,
                    });
                }
            }
        }

        // Collect inline spans only for nodes within the range
        self.collect_inline_spans_in_range(&mut spans, subtree, text, &range);

        // Check for injection overlaps only for injections that overlap with the range
        for injection in &self.injections {
            let inj_start = injection.start;
            let inj_end = injection.end;

            // Skip injections that don't overlap with the requested range
            if inj_end <= range.start || inj_start >= range.end {
                continue;
            }

            let mut inj_cursor = QueryCursor::new();
            let mut inj_parser = Parser::new();
            if inj_parser.set_language(&injection.language).is_ok() {
                if let Some(inj_tree) = inj_parser.parse(&text[inj_start..inj_end], None) {
                    let inj_root = inj_tree.root_node();
                    let relative_start = range.start.saturating_sub(inj_start);
                    let relative_end = range.end.saturating_sub(inj_start).min(inj_end - inj_start);
                    inj_cursor.set_byte_range(relative_start..relative_end);
                    let injection_bytes = &text.as_bytes()[inj_start..inj_end];
                    let mut matches =
                        inj_cursor.matches(&injection.query, inj_root, injection_bytes);
                    let mut injection_spans = Vec::new();

                    while let Some(m) = matches.next() {
                        for capture in m.captures {
                            let capture_name =
                                injection.query.capture_names()[capture.index as usize];
                            let inj_node = capture.node;
                            let style = injection_capture_to_style(
                                injection.kind,
                                injection.language_name,
                                capture_name,
                                inj_node.kind(),
                            );

                            let abs_start = inj_start + inj_node.start_byte();
                            let abs_end = inj_start + inj_node.end_byte();
                            let clipped_start = abs_start.max(range.start);
                            let clipped_end = abs_end.min(range.end);

                            if clipped_start < clipped_end {
                                injection_spans.push(ByteSpan {
                                    start_byte: clipped_start,
                                    end_byte: clipped_end,
                                    style,
                                });
                            }
                        }
                    }
                    injection_spans.sort_by_key(|span| {
                        matches!(
                            span.style,
                            SemanticStyle::FmKey | SemanticStyle::FmDelimiter
                        )
                    });
                    spans.extend(injection_spans);
                }
            }
        }

        spans
    }

    /// Collect inline spans only for nodes within a byte range.
    fn collect_inline_spans_in_range(
        &self,
        spans: &mut Vec<ByteSpan>,
        node: tree_sitter::Node,
        text: &str,
        range: &Range<usize>,
    ) {
        let mut cursor = node.walk();
        if !cursor.goto_first_child() {
            return;
        }
        loop {
            let child = cursor.node();
            let kind = child.kind();

            // Skip nodes entirely outside the range
            if child.end_byte() <= range.start || child.start_byte() >= range.end {
                if !cursor.goto_next_sibling() {
                    break;
                }
                continue;
            }

            // Recurse into children
            if child.child_count() > 0 {
                self.collect_inline_spans_in_range(spans, child, text, range);
            }

            // For inline nodes, parse with the inline grammar
            if kind == "inline" {
                let inline_text = &text[child.start_byte()..child.end_byte()];
                let inline_bytes = inline_text.as_bytes();

                let mut inline_parser = Parser::new();
                let inline_language: tree_sitter::Language = tree_sitter_md::INLINE_LANGUAGE.into();
                if inline_parser.set_language(&inline_language).is_ok() {
                    if let Some(inline_tree) = inline_parser.parse(inline_bytes, None) {
                        let mut inline_cursor = QueryCursor::new();
                        let mut matches = inline_cursor.matches(
                            &self.md_inline_query,
                            inline_tree.root_node(),
                            inline_bytes,
                        );

                        let base_offset = child.start_byte();
                        while let Some(m) = matches.next() {
                            for capture in m.captures {
                                let capture_name =
                                    self.md_inline_query.capture_names()[capture.index as usize];
                                let style = md_capture_to_style(capture_name);
                                let inline_node = capture.node;

                                let abs_start = base_offset + inline_node.start_byte();
                                let abs_end = base_offset + inline_node.end_byte();

                                if abs_start < abs_end {
                                    spans.push(ByteSpan {
                                        start_byte: abs_start,
                                        end_byte: abs_end,
                                        style,
                                    });
                                }
                            }
                        }
                    }
                }
            }

            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }

    /// Discover all injection regions in the document.
    fn discover_injections(&mut self) {
        let metadata = self.collect_injection_metadata();
        self.build_injections(metadata);
    }

    /// Collect injection ranges and language identities without mutating the
    /// highlighter or cloning its text and markdown tree.
    fn collect_injection_metadata(&self) -> Vec<InjectionMeta> {
        let mut metadata = Vec::new();
        Self::walk_for_injections(self.md_tree.root_node(), &self.text, &mut metadata);
        metadata
    }

    /// Resolve injection metadata through the compiled-query cache.
    fn build_injections(&mut self, metadata: Vec<InjectionMeta>) {
        self.injections.clear();

        for meta in metadata {
            let language = (meta.language.language_fn)();
            let query = if let Some(cached) = self.query_cache.get(meta.language.name) {
                Arc::clone(cached)
            } else {
                let compiled = Arc::new(
                    Query::new(&language, meta.language.highlights_query())
                        .expect("registered injection highlight query should compile"),
                );
                self.query_cache
                    .insert(meta.language.name, Arc::clone(&compiled));
                compiled
            };

            self.injections.push(Injection {
                start: meta.start,
                end: meta.end,
                language,
                query,
                language_name: meta.language.name,
                kind: meta.kind,
                fence_lang: meta.fence_lang,
            });
        }
    }

    /// Walk the markdown tree and collect immutable injection metadata.
    /// Uses a recursive approach to avoid cursor lifetime issues.
    fn walk_for_injections(node: tree_sitter::Node, text: &str, metadata: &mut Vec<InjectionMeta>) {
        let kind = node.kind();

        // Check for front matter (minus_metadata / plus_metadata)
        if kind == "minus_metadata" || kind == "plus_metadata" {
            let is_yaml = kind == "minus_metadata";
            let language_name = if is_yaml { "yaml" } else { "toml" };
            let language = languages::find_by_name(language_name)
                .expect("front-matter language should exist in the registry");
            let delimiter = if is_yaml { "---" } else { "+++" };
            let content_range = front_matter_content_range(&node, text, delimiter);

            metadata.push(InjectionMeta {
                start: content_range.start,
                end: content_range.end,
                language,
                kind: InjectionKind::FrontMatter,
                fence_lang: None,
            });
            return; // Don't recurse into front matter
        }

        // Check for fenced code blocks
        if kind == "fenced_code_block" {
            if let Some(info_string) = find_info_string(&node, text) {
                if let Some(lang_def) = languages::find_by_alias(&info_string) {
                    if let Some(content) = find_code_fence_content(&node) {
                        metadata.push(InjectionMeta {
                            start: content.start_byte(),
                            end: content.end_byte(),
                            language: lang_def,
                            kind: InjectionKind::Fence,
                            fence_lang: Some(info_string),
                        });
                    }
                }
            }
            return; // Don't recurse into fenced code blocks
        }

        // Check for inline regions (deferred - inline highlighting is done
        // by the markdown block query captures, not by separate injections)
        if kind == "inline" {
            return;
        }

        // Recurse into children
        let mut cursor = node.walk();
        if cursor.goto_first_child() {
            loop {
                Self::walk_for_injections(cursor.node(), text, metadata);
                if !cursor.goto_next_sibling() {
                    break;
                }
            }
        }
    }
}

// ── Standalone snippet highlighting ─────────────────────────────────────────

/// Highlight a standalone code snippet with the given language.
///
/// This is used by the rendered layout renderer for fenced code blocks: each
/// fence is highlighted as an independent snippet, not as part of the
/// document's incremental tree (FR-3.5).
///
/// Unknown languages return the text unstyled (CodeBlock style).
pub fn highlight_snippet(lang: &str, text: &str) -> Vec<StyledLine> {
    if text.is_empty() {
        return vec![StyledLine {
            text: String::new(),
            spans: Vec::new(),
        }];
    }

    let Some(lang_def) = languages::find_by_alias(lang) else {
        // Unknown language — return unstyled
        return highlight_lines_for_text(text);
    };
    let lang_obj = (lang_def.language_fn)();

    let query = Query::new(&lang_obj, lang_def.highlights_query()).ok();
    let Some(query) = query else {
        return highlight_lines_for_text(text);
    };

    let mut parser = Parser::new();
    if parser.set_language(&lang_obj).is_err() {
        return highlight_lines_for_text(text);
    }

    let tree = parser.parse(text, None);

    let Some(tree) = tree else {
        return highlight_lines_for_text(text);
    };

    let text_bytes = text.as_bytes();
    let root = tree.root_node();
    let mut cursor = QueryCursor::new();

    // Collect all spans from the query
    let mut all_spans: Vec<ByteSpan> = Vec::new();
    let mut matches = cursor.matches(&query, root, text_bytes);

    while let Some(m) = matches.next() {
        for capture in m.captures {
            let capture_name = query.capture_names()[capture.index as usize];
            let style = captures::capture_to_style(capture_name);
            let node = capture.node;
            let start = node.start_byte();
            let end = node.end_byte();
            if start < end {
                all_spans.push(ByteSpan {
                    start_byte: start,
                    end_byte: end,
                    style,
                });
            }
        }
    }

    // Group spans by line and produce StyledLines
    let line_starts = line_start_indices(text);
    let num_lines = if text.ends_with('\n') && !text.is_empty() {
        line_starts.len().saturating_sub(1)
    } else {
        line_starts.len()
    };

    let mut result = Vec::with_capacity(num_lines);
    for line_idx in 0..num_lines {
        let line_start = line_starts[line_idx];
        let next_start = if line_idx + 1 < line_starts.len() {
            line_starts[line_idx + 1]
        } else {
            text.len()
        };
        let line_end = if next_start > line_start && text.as_bytes()[next_start - 1] == b'\n' {
            next_start - 1
        } else {
            next_start
        };
        let line_text = &text[line_start..line_end.min(text.len())];

        let spans: Vec<RankedSpan> = all_spans
            .iter()
            .enumerate()
            .filter(|(_, s)| s.start_byte < line_end && s.end_byte > line_start)
            .map(|(priority, s)| {
                let relative_start = s.start_byte.max(line_start) - line_start;
                let relative_end = s.end_byte.min(line_end) - line_start;
                RankedSpan {
                    span: Span {
                        start_col: byte_offset_to_char_index(line_text, relative_start),
                        end_col: byte_offset_to_char_index(line_text, relative_end),
                        style: s.style,
                    },
                    priority,
                }
            })
            .collect();

        let mut spans = merge_overlapping_spans(spans);
        spans.sort_by_key(|span| span.start_col);

        result.push(StyledLine {
            text: line_text.to_string(),
            spans,
        });
    }

    result
}

/// Helper: split text into lines and return unstyled StyledLines.
fn highlight_lines_for_text(text: &str) -> Vec<StyledLine> {
    text.lines()
        .map(|line| StyledLine {
            text: line.to_string(),
            spans: Vec::new(),
        })
        .collect()
}

// ── Helper: find info_string child of a fenced code block ───────────────────

/// Find the `info_string` node inside a fenced code block and return the
/// language identifier text.
fn find_info_string(node: &tree_sitter::Node, text: &str) -> Option<String> {
    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            let child = cursor.node();
            if child.kind() == "info_string" {
                if let Some(lang_node) = child.named_child(0) {
                    return Some(text[lang_node.start_byte()..lang_node.end_byte()].to_string());
                }
                return None;
            }
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
    None
}

// ── Helper: find code_fence_content child ───────────────────────────────────

/// Find the `code_fence_content` child of a fenced code block.
fn find_code_fence_content<'a>(node: &tree_sitter::Node<'a>) -> Option<tree_sitter::Node<'a>> {
    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            if cursor.node().kind() == "code_fence_content" {
                return Some(cursor.node());
            }
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
    None
}

/// Return the parseable body of a front-matter node, excluding its opening
/// and closing Markdown delimiter lines.
fn front_matter_content_range(
    node: &tree_sitter::Node,
    text: &str,
    delimiter: &str,
) -> Range<usize> {
    let node_start = node.start_byte();
    let node_end = node.end_byte();
    let node_text = &text[node_start..node_end];
    let content_start = node_text
        .find('\n')
        .map_or(node_end, |newline| node_start + newline + 1);
    let trimmed = node_text.trim_end_matches(['\r', '\n']);
    let closing_start = trimmed.rfind('\n').map_or(0, |newline| newline + 1);
    let closing_line = trimmed[closing_start..].trim_end_matches('\r');
    let content_end = if closing_line == delimiter {
        node_start + closing_start
    } else {
        node_end
    };

    content_start.min(content_end)..content_end
}

// ── Helper: markdown capture → SemanticStyle ────────────────────────────────

/// Map injected-language captures, preserving the dedicated front-matter
/// key/value style slots required by FR-4.2.
fn injection_capture_to_style(
    kind: InjectionKind,
    language_name: &str,
    capture: &str,
    node_kind: &str,
) -> SemanticStyle {
    if kind == InjectionKind::FrontMatter {
        let capture = capture.strip_prefix('@').unwrap_or(capture);
        if capture.starts_with("property")
            || (language_name == "toml" && capture.starts_with("type"))
            || (language_name == "toml" && node_kind == "quoted_key")
        {
            return SemanticStyle::FmKey;
        }
        return match captures::capture_to_style(capture) {
            SemanticStyle::StringLit
            | SemanticStyle::NumberLit
            | SemanticStyle::TypeName
            | SemanticStyle::Variable
            | SemanticStyle::Text => SemanticStyle::FmValue,
            style => style,
        };
    }

    captures::capture_to_style(capture)
}

// ── Helper: markdown capture → SemanticStyle ────────────────────────────────

/// Map a markdown-specific capture name to a `SemanticStyle`.
fn md_capture_to_style(capture: &str) -> SemanticStyle {
    // Strip the `@` prefix
    let name = capture.strip_prefix('@').unwrap_or(capture);

    // Heading styles
    if name.starts_with("heading") {
        return SemanticStyle::Heading1;
    }
    if name.starts_with("emphasis") {
        return SemanticStyle::Emphasis;
    }
    if name.starts_with("strong") {
        return SemanticStyle::Strong;
    }
    if name.starts_with("strikethrough") {
        return SemanticStyle::Strikethrough;
    }
    if name.starts_with("code.span") {
        return SemanticStyle::CodeSpan;
    }
    if name.starts_with("fence") {
        if name.starts_with("fence.delimiter") {
            return SemanticStyle::Muted;
        }
        if name.starts_with("fence.info") || name.starts_with("fence.language") {
            return SemanticStyle::CodeBlock;
        }
        if name.starts_with("fence.content") {
            return SemanticStyle::CodeBlock;
        }
        return SemanticStyle::CodeBlock;
    }
    if name.starts_with("list.marker") {
        return SemanticStyle::ListMarker;
    }
    if name.starts_with("quote") {
        return SemanticStyle::Quote;
    }
    if name.starts_with("link") {
        if name.starts_with("link.url") || name.starts_with("link.destination") {
            return SemanticStyle::LinkUrl;
        }
        if name.starts_with("link.text") || name.starts_with("link.label") {
            return SemanticStyle::Link;
        }
        return SemanticStyle::Link;
    }
    if name.starts_with("rule") {
        return SemanticStyle::Rule;
    }
    if name.starts_with("html") {
        return SemanticStyle::HtmlRaw;
    }
    if name.starts_with("fm") {
        if name.starts_with("fm.delimiter") {
            return SemanticStyle::FmDelimiter;
        }
        if name.starts_with("fm.yaml") || name.starts_with("fm.toml") {
            return SemanticStyle::FmDelimiter;
        }
        return SemanticStyle::FmValue;
    }
    if name.starts_with("inline") {
        return SemanticStyle::Text;
    }

    // Default
    SemanticStyle::Text
}

// ── Helpers: byte offsets ──────────────────────────────────────────────────

/// Convert a UTF-8 byte offset within one line to a character index.
#[inline]
fn byte_offset_to_char_index(text: &str, byte_offset: usize) -> usize {
    if byte_offset == 0 {
        return 0;
    }
    if byte_offset >= text.len() {
        return text.chars().count();
    }

    text.char_indices()
        .take_while(|(index, _)| *index < byte_offset)
        .count()
}

/// Construct a tree-sitter edit from a replacement against `old_text`.
fn input_edit(old_text: &str, edit: &TextEdit) -> Option<tree_sitter::InputEdit> {
    // Skip true no-ops while allowing insertions through to the incremental
    // parse path.
    if edit.range.start == edit.range.end && edit.new_text_len == 0 {
        return None;
    }

    let start_byte = edit.range.start;
    let end_byte = edit.range.end;
    // Normalize backward-range edits (hjkl produces these for operations like
    // Backward engine deletion; the range represents the same character deletion or
    // replacement, just with inverted bounds).
    let (a, b) = if start_byte < end_byte {
        (start_byte, end_byte)
    } else {
        (end_byte, start_byte)
    };
    let new_end_byte = a + edit.new_text_len;
    let start_position = byte_to_point(old_text, a);

    Some(tree_sitter::InputEdit {
        start_byte: a,
        old_end_byte: b,
        new_end_byte,
        start_position,
        old_end_position: byte_to_point(old_text, b),
        new_end_position: compute_new_end_point(start_position, &edit.new_text),
    })
}

/// Convert a byte offset in `text` to a `(row, col)` point.
fn byte_to_point(text: &str, byte: usize) -> Point {
    let mut row = 0usize;
    let mut col = 0usize;
    for (i, c) in text.char_indices() {
        if i >= byte {
            break;
        }
        if c == '\n' {
            row += 1;
            col = 0;
        } else {
            col += c.len_utf8();
        }
    }
    Point::new(row, col)
}

/// Compute the `Point` where replacement text ends, given the `Point`
/// where the replacement starts.
fn compute_new_end_point(start: Point, new_text: &str) -> Point {
    let mut row = start.row;
    let mut col = start.column;
    for c in new_text.chars() {
        if c == '\n' {
            row += 1;
            col = 0;
        } else {
            col += c.len_utf8();
        }
    }
    Point::new(row, col)
}

// ── Helper: line start indices ──────────────────────────────────────────────

/// Return a vector where `result[i]` is the byte offset of the first
/// character of line `i` (0-based).
fn line_start_indices(text: &str) -> Vec<usize> {
    let mut starts = Vec::new();
    starts.push(0);
    for (i, c) in text.char_indices() {
        if c == '\n' {
            let offset = i + c.len_utf8();
            starts.push(offset);
        }
    }
    starts
}

// ── Helper: apply edits to a string ─────────────────────────────────────────

/// Apply one sequential replacement to the current document state.
fn apply_edit_to_string(text: &mut String, edit: &TextEdit) {
    assert_eq!(
        edit.new_text_len,
        edit.new_text.len(),
        "TextEdit replacement length must match its UTF-8 byte length"
    );
    let start = edit.range.start.min(edit.range.end);
    let end = edit.range.start.max(edit.range.end);
    assert!(
        end <= text.len() && text.is_char_boundary(start) && text.is_char_boundary(end),
        "TextEdit range {start}..{end} is not a valid UTF-8 slice of the {}-byte working document",
        text.len()
    );
    text.replace_range(start..end, &edit.new_text);
}

// ── Helper: merge overlapping spans ─────────────────────────────────────────

/// Resolve overlaps using explicit collection-order priority, where later
/// captures override earlier, broader captures. Adjacent intervals with the
/// same winning style are coalesced.
fn merge_overlapping_spans(spans: Vec<RankedSpan>) -> Vec<Span> {
    let mut boundaries = Vec::with_capacity(spans.len() * 2);
    for ranked in &spans {
        if ranked.span.start_col < ranked.span.end_col {
            boundaries.push(ranked.span.start_col);
            boundaries.push(ranked.span.end_col);
        }
    }
    boundaries.sort_unstable();
    boundaries.dedup();

    let mut merged: Vec<Span> = Vec::new();
    for interval in boundaries.windows(2) {
        let start = interval[0];
        let end = interval[1];
        let Some(winner) = spans
            .iter()
            .filter(|ranked| ranked.span.start_col <= start && ranked.span.end_col >= end)
            .max_by_key(|ranked| ranked.priority)
        else {
            continue;
        };

        if let Some(previous) = merged.last_mut() {
            if previous.end_col == start && previous.style == winner.span.style {
                previous.end_col = end;
                continue;
            }
        }

        merged.push(Span {
            start_col: start,
            end_col: end,
            style: winner.span.style,
        });
    }

    debug_assert!(
        merged
            .windows(2)
            .all(|pair| pair[0].end_col <= pair[1].start_col),
        "merge_overlapping_spans produced overlapping output: {merged:?}"
    );

    merged
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vim::{KeyCode, KeyCodeKind, KeyInput, Modifiers, VimCore, VimEffect};

    #[cfg(test)]
    use proptest::prelude::*;

    const VIEWPORT_MARKDOWN: &str =
        "# Heading\n\nIntro paragraph.\n\nMiddle paragraph.\n\n```rust\nfn main() {}\n```\nTail paragraph.\n";

    #[test]
    fn new_end_point_empty_text() {
        assert_eq!(
            compute_new_end_point(Point::new(3, 5), ""),
            Point::new(3, 5)
        );
    }

    #[test]
    fn new_end_point_single_char() {
        assert_eq!(
            compute_new_end_point(Point::new(0, 0), "x"),
            Point::new(0, 1)
        );
    }

    #[test]
    fn new_end_point_newline() {
        assert_eq!(
            compute_new_end_point(Point::new(2, 10), "\n"),
            Point::new(3, 0)
        );
    }

    #[test]
    fn new_end_point_multi_line() {
        assert_eq!(
            compute_new_end_point(Point::new(0, 0), "abc\ndef\n"),
            Point::new(2, 0)
        );
    }

    #[test]
    fn new_end_point_multi_line_with_trailing_text() {
        assert_eq!(
            compute_new_end_point(Point::new(2, 10), "abc\ndef"),
            Point::new(3, 3)
        );
    }

    #[test]
    fn new_end_point_multibyte_utf8() {
        assert_eq!(
            compute_new_end_point(Point::new(0, 0), "a\u{00e9}b"),
            Point::new(0, 4)
        );
    }

    #[test]
    fn byte_offset_to_char_index_handles_utf8_and_boundaries() {
        assert_eq!(byte_offset_to_char_index("ascii", 0), 0);
        assert_eq!(byte_offset_to_char_index("ascii", 3), 3);
        assert_eq!(byte_offset_to_char_index("ascii", 5), 5);
        assert_eq!(byte_offset_to_char_index("ascii", usize::MAX), 5);

        let mixed = "aé界🙂z";
        assert_eq!(byte_offset_to_char_index(mixed, 0), 0);
        assert_eq!(byte_offset_to_char_index(mixed, 1), 1);
        assert_eq!(byte_offset_to_char_index(mixed, 3), 2);
        assert_eq!(byte_offset_to_char_index(mixed, 6), 3);
        assert_eq!(byte_offset_to_char_index(mixed, 10), 4);
        assert_eq!(byte_offset_to_char_index(mixed, mixed.len()), 5);
        assert_eq!(byte_offset_to_char_index(mixed, mixed.len() + 10), 5);
    }

    #[test]
    fn forward_range_input_edit_regression() {
        let edit = TextEdit {
            range: 6..12,
            new_text_len: 3,
            new_text: "abc".to_string(),
        };
        let actual = input_edit("first\nsecond line\n", &edit).expect("non-empty edit");

        assert_eq!(actual.start_byte, 6);
        assert_eq!(actual.old_end_byte, 12);
        assert_eq!(actual.new_end_byte, 9);
        assert_eq!(actual.start_position, Point::new(1, 0));
        assert_eq!(actual.old_end_position, Point::new(1, 6));
        assert_eq!(actual.new_end_position, Point::new(1, 3));
    }

    #[test]
    fn backward_range_input_edit_uses_normalized_start() {
        let edit = TextEdit {
            range: Range { start: 12, end: 6 },
            new_text_len: 3,
            new_text: "abc".to_string(),
        };
        let actual = input_edit("first\nsecond line\n", &edit).expect("non-empty edit");

        assert_eq!(actual.start_byte, 6);
        assert_eq!(actual.old_end_byte, 12);
        assert_eq!(actual.new_end_byte, 9);
        assert_eq!(actual.start_position, Point::new(1, 0));
        assert_eq!(actual.old_end_position, Point::new(1, 6));
        assert_eq!(actual.new_end_position, Point::new(1, 3));
    }

    #[test]
    fn replacement_input_edit_uses_new_text_for_end_position() {
        let edit = TextEdit {
            range: 5..6,
            new_text_len: 1,
            new_text: "\n".to_string(),
        };
        let actual = input_edit("alpha beta\n", &edit).expect("non-empty edit");

        assert_eq!(actual.new_end_byte, 6);
        assert_eq!(actual.new_end_position, Point::new(1, 0));
    }

    #[test]
    fn insertion_produces_valid_input_edit() {
        let edit = TextEdit {
            range: 8..8,
            new_text_len: 3,
            new_text: "abc".to_string(),
        };
        let actual = input_edit("# title\nbody\n", &edit).expect("insertion edit");

        assert_eq!(actual.start_byte, 8);
        assert_eq!(actual.old_end_byte, 8);
        assert_eq!(actual.new_end_byte, 11);
        assert_eq!(actual.start_position, Point::new(1, 0));
        assert_eq!(actual.old_end_position, Point::new(1, 0));
        assert_eq!(actual.new_end_position, Point::new(1, 3));
    }

    #[test]
    fn vim_batch_bottom_up_deletions_match_fresh_highlighting() {
        let mut vim = VimCore::new("## Heading\n- item\n> quote");
        let mut highlighter = Highlighter::new(&vim.text());

        apply_vim_key(&mut vim, &mut highlighter, ctrl_char_key('v'));
        apply_vim_key(&mut vim, &mut highlighter, char_key('j'));
        apply_vim_key(&mut vim, &mut highlighter, char_key('j'));
        apply_vim_key(&mut vim, &mut highlighter, char_key('l'));
        let edits = apply_vim_key(&mut vim, &mut highlighter, char_key('x'));

        assert_eq!(vim.text(), " Heading\nitem\nquote");
        assert_eq!(edits.len(), 3);
        assert!(edits.iter().all(|edit| edit.new_text.is_empty()));
        assert!(edits
            .windows(2)
            .all(|pair| pair[0].range.start > pair[1].range.start));
    }

    #[test]
    fn vim_batch_visual_block_insertions_rebase_replacement_slices() {
        let mut vim = VimCore::new("# aa\n- bb\n> cc");
        let mut highlighter = Highlighter::new(&vim.text());
        let mut edits = Vec::new();

        edits.extend(apply_vim_key(
            &mut vim,
            &mut highlighter,
            ctrl_char_key('v'),
        ));
        edits.extend(apply_vim_key(&mut vim, &mut highlighter, char_key('j')));
        edits.extend(apply_vim_key(&mut vim, &mut highlighter, char_key('j')));
        edits.extend(apply_vim_key(&mut vim, &mut highlighter, char_key('I')));
        edits.extend(apply_vim_key(&mut vim, &mut highlighter, char_key('X')));
        edits.extend(apply_vim_key(
            &mut vim,
            &mut highlighter,
            special_key(KeyCodeKind::Esc),
        ));

        assert_eq!(vim.text(), "X# aa\nX- bb\nX> cc");
        assert_eq!(edits.len(), 3);
        assert_eq!(
            edits
                .iter()
                .map(|edit| edit.new_text.as_str())
                .collect::<Vec<_>>(),
            vec!["X", "X", "X"]
        );
    }

    #[test]
    fn vim_batch_same_row_splits_stay_right_to_left() {
        let mut vim = VimCore::new("# abcdef");
        let mut highlighter = Highlighter::new(&vim.text());

        let edits = vim.split_lines_for_test(0, vec![2, 4], vec![false, false]);
        highlighter.apply_edit(&edits);
        assert_vim_highlighting_matches_fresh(&vim, &highlighter);

        assert_eq!(vim.text(), "# \nab\ncdef");
        assert_eq!(edits.len(), 2);
        assert_eq!(
            edits
                .iter()
                .map(|edit| edit.range.start)
                .collect::<Vec<_>>(),
            vec![4, 2]
        );
        assert_eq!(
            edits
                .iter()
                .map(|edit| edit.new_text.as_str())
                .collect::<Vec<_>>(),
            vec!["\n", "\n"]
        );
    }

    #[test]
    fn vim_batch_multibyte_replacements_preserve_different_byte_deltas() {
        let mut vim = VimCore::new("café\ncafe\ncaff");
        let mut highlighter = Highlighter::new(&vim.text());

        let edits = vim.replace_chars_for_test(&[(2, 3, 'x'), (1, 3, 'x'), (0, 3, 'x')]);
        highlighter.apply_edit(&edits);
        assert_vim_highlighting_matches_fresh(&vim, &highlighter);

        assert_eq!(vim.text(), "cafx\ncafx\ncafx");
        assert_eq!(edits.len(), 3);
        assert_eq!(
            edits
                .iter()
                .map(|edit| {
                    edit.new_text_len as isize - (edit.range.end - edit.range.start) as isize
                })
                .collect::<Vec<_>>(),
            vec![0, 0, -1]
        );
        assert_eq!(
            edits
                .iter()
                .map(|edit| edit.new_text.as_str())
                .collect::<Vec<_>>(),
            vec!["x", "x", "x"]
        );
    }

    #[test]
    fn insertion_containing_batch_applies_every_replacement_sequentially() {
        let mut highlighter = Highlighter::new("# alpha\n");
        let edits = [
            TextEdit {
                range: 2..3,
                new_text_len: 0,
                new_text: String::new(),
            },
            TextEdit {
                range: 6..6,
                new_text_len: 2,
                new_text: "**".to_string(),
            },
        ];

        highlighter.apply_edit(&edits);

        let expected = "# lpha**\n";
        assert_eq!(highlighter.text(), expected);
        assert_eq!(highlighter.last_parse_path, ParsePath::Incremental);
        assert_eq!(
            highlighter.highlight_lines(0..1000),
            Highlighter::new(expected).highlight_lines(0..1000)
        );
    }

    #[test]
    fn highlighter_new_parses_simple_markdown() {
        let h = Highlighter::new("# Hello\n\nWorld\n");
        let lines = h.highlight_lines(0..3);
        assert_eq!(lines.len(), 3);
        assert!(!lines[0].spans.is_empty(), "heading line should have spans");
    }

    #[test]
    fn non_ascii_heading_span_uses_character_indices() {
        let text = "# café\n";
        let lines = Highlighter::new(text).highlight_lines(0..1);
        let heading = lines[0]
            .spans
            .iter()
            .find(|span| span.style == SemanticStyle::Heading1)
            .expect("heading should have a Heading1 span");

        assert_eq!(heading.start_col, 0);
        assert_eq!(heading.end_col, "# café".chars().count());
        assert_ne!(heading.end_col, "# café".len());
    }

    #[test]
    fn non_ascii_fenced_string_span_uses_character_indices() {
        let text = "```rust\nlet x = \"über\";\n```\n";
        let lines = Highlighter::new(text).highlight_lines(0..3);
        let code_line = &lines[1];
        let string_span = code_line
            .spans
            .iter()
            .find(|span| span.style == SemanticStyle::StringLit)
            .expect("Rust string literal should be highlighted");

        assert_eq!(string_span.start_col, 8);
        assert_eq!(string_span.end_col, 14);
        assert_eq!(span_text(code_line, string_span), "\"über\"");
    }

    #[test]
    fn non_ascii_standalone_snippet_span_uses_character_indices() {
        let lines = highlight_snippet("rust", "let café = \"über\";\n");
        let code_line = &lines[0];
        let string_span = code_line
            .spans
            .iter()
            .find(|span| span.style == SemanticStyle::StringLit)
            .expect("Rust string literal should be highlighted");

        assert_eq!(string_span.start_col, 11);
        assert_eq!(string_span.end_col, 17);
        assert_eq!(span_text(code_line, string_span), "\"über\"");
    }

    #[test]
    fn non_ascii_yaml_front_matter_spans_use_character_indices() {
        let text = "---\ntítulo: café\n---\n";
        let lines = Highlighter::new(text).highlight_lines(0..3);
        let yaml_line = &lines[1];
        let key_span = yaml_line
            .spans
            .iter()
            .find(|span| span.style == SemanticStyle::FmKey)
            .expect("YAML key should be highlighted");
        let value_span = yaml_line
            .spans
            .iter()
            .find(|span| span.style == SemanticStyle::FmValue)
            .expect("YAML value should be highlighted");

        assert_eq!(span_text(yaml_line, key_span), "título");
        assert_eq!(key_span.end_col, "título".chars().count());
        assert_eq!(span_text(yaml_line, value_span), "café");
        assert_eq!(value_span.end_col, yaml_line.text.chars().count());
    }

    #[test]
    fn highlighter_handles_empty_document() {
        let h = Highlighter::new("");
        let lines = h.highlight_lines(0..1);
        assert_eq!(lines.len(), 1);
        assert!(lines[0].text.is_empty());
    }

    #[test]
    fn highlighter_frontmatter_yaml() {
        let text = "---\ntitle: Hello\nauthor: Test\n---\n# Content\n";
        let h = Highlighter::new(text);
        let lines = h.highlight_lines(0..5);
        assert_eq!(lines.len(), 5);
        let has_delimiter = lines[0]
            .spans
            .iter()
            .any(|s| s.style == SemanticStyle::FmDelimiter);
        assert!(has_delimiter, "front matter delimiter should be styled");
    }

    #[test]
    fn highlighter_frontmatter_toml() {
        let text = "+++\ntitle = \"Hello\"\nauthor = \"Test\"\n+++\n# Content\n";
        let h = Highlighter::new(text);
        let lines = h.highlight_lines(0..5);
        assert_eq!(lines.len(), 5);
        let has_delimiter = lines[0]
            .spans
            .iter()
            .any(|s| s.style == SemanticStyle::FmDelimiter);
        assert!(
            has_delimiter,
            "TOML front matter delimiter should be styled"
        );
    }

    #[test]
    fn highlighter_fenced_code_block() {
        let text = "```rust\nfn main() {}\n```\n";
        let h = Highlighter::new(text);
        let lines = h.highlight_lines(0..3);
        assert_eq!(lines.len(), 3);
        let has_muted = lines[0]
            .spans
            .iter()
            .any(|s| s.style == SemanticStyle::Muted);
        assert!(has_muted, "fence delimiter should be Muted");
    }

    #[test]
    fn highlighter_fenced_code_unknown_language() {
        let text = "```unknown\nsome code\n```\n";
        let h = Highlighter::new(text);
        let lines = h.highlight_lines(0..3);
        assert_eq!(lines.len(), 3);
    }

    #[test]
    fn highlighter_line_range_clamped() {
        let h = Highlighter::new("line1\nline2\nline3\n");
        let lines = h.highlight_lines(0..100);
        assert!(lines.len() <= 3);
    }

    #[test]
    fn highlighter_empty_range() {
        let h = Highlighter::new("line1\nline2\n");
        let lines = h.highlight_lines(2..2);
        assert!(lines.is_empty());
    }

    #[test]
    fn viewport_partial_range_no_hang() {
        let h = Highlighter::new(VIEWPORT_MARKDOWN);
        let lines = h.highlight_lines(0..2);

        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].text, "# Heading");
        assert_eq!(lines[1].text, "");
    }

    #[test]
    fn viewport_mid_range_no_hang() {
        let h = Highlighter::new(VIEWPORT_MARKDOWN);
        let lines = h.highlight_lines(3..5);

        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].text, "");
        assert_eq!(lines[1].text, "Middle paragraph.");
    }

    #[test]
    fn viewport_last_line_only() {
        let h = Highlighter::new("# Heading\none\ntwo\nthree\nlast");
        let lines = h.highlight_lines(4..5);

        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].text, "last");
    }

    #[test]
    fn viewport_with_inline_at_boundary() {
        let h = Highlighter::new("# Heading\n\n*emphasis* and `code`\nTrailing block.\n");
        let lines = h.highlight_lines(0..3);

        assert_eq!(lines.len(), 3);
        assert_eq!(lines[2].text, "*emphasis* and `code`");
        assert!(lines[2]
            .spans
            .windows(2)
            .all(|pair| pair[0].end_col <= pair[1].start_col));
        assert!(lines[2]
            .spans
            .iter()
            .any(|span| span.style == SemanticStyle::Emphasis));
        assert!(lines[2]
            .spans
            .iter()
            .any(|span| span.style == SemanticStyle::CodeSpan));
    }

    #[test]
    fn viewport_excludes_all_nodes() {
        let h = Highlighter::new("# Heading\n\nParagraph.\n");
        let lines = h.highlight_lines(100..200);

        assert_eq!(lines.len(), 1);
        assert!(lines[0].text.is_empty());
        assert!(lines[0].spans.is_empty());
    }

    #[test]
    fn insertion_incremental_equivalence() {
        let initial = "# Heading\n\nParagraph with *emphasis*.\n";
        let lines = assert_edit_matches_fresh(
            initial,
            TextEdit {
                range: 4..4,
                new_text_len: 1,
                new_text: "x".to_string(),
            },
        );

        assert_eq!(lines[0].text, "# Hexading");
    }

    #[test]
    fn multi_char_insertion_incremental() {
        let initial = "# Heading\n\nParagraph.\n";
        let inserted = "hello world";
        let lines = assert_edit_matches_fresh(
            initial,
            TextEdit {
                range: 0..0,
                new_text_len: inserted.len(),
                new_text: inserted.to_string(),
            },
        );

        assert_eq!(lines[0].text, "hello world# Heading");
    }

    #[test]
    fn noop_edit_still_filtered() {
        let initial = "# Heading\n\nParagraph.\n";
        let edit = TextEdit {
            range: 5..5,
            new_text_len: 0,
            new_text: String::new(),
        };
        assert!(input_edit(initial, &edit).is_none());

        let mut highlighter = Highlighter::new(initial);
        highlighter.apply_edit(&[edit]);

        assert_eq!(highlighter.text(), initial);
        assert_eq!(highlighter.last_parse_path, ParsePath::Skipped);
        assert_eq!(
            highlighter.highlight_lines(0..1000),
            Highlighter::new(initial).highlight_lines(0..1000)
        );
    }

    #[test]
    fn insertion_in_fenced_block() {
        let initial = "```rust\nfn main() {\n}\n```\n";
        let insertion_offset = initial.find("}\n").expect("closing brace");
        let inserted = "    let answer = 42;\n";
        let edit = TextEdit {
            range: insertion_offset..insertion_offset,
            new_text_len: inserted.len(),
            new_text: inserted.to_string(),
        };
        let mut expected_text = initial.to_string();
        expected_text.insert_str(insertion_offset, inserted);

        let mut highlighter = Highlighter::new(initial);
        highlighter.apply_edit(&[edit]);
        let fresh = Highlighter::new(&expected_text);

        let lines = highlighter.highlight_lines(0..1000);
        assert_eq!(highlighter.last_parse_path, ParsePath::Incremental);
        assert_eq!(lines, fresh.highlight_lines(0..1000));
        let injection_ranges = highlighter
            .injections
            .iter()
            .map(|injection| {
                (
                    injection.start,
                    injection.end,
                    injection.fence_lang.as_deref(),
                )
            })
            .collect::<Vec<_>>();
        let fresh_injection_ranges = fresh
            .injections
            .iter()
            .map(|injection| {
                (
                    injection.start,
                    injection.end,
                    injection.fence_lang.as_deref(),
                )
            })
            .collect::<Vec<_>>();

        assert_eq!(lines[2].text, "    let answer = 42;");
        assert_eq!(injection_ranges, fresh_injection_ranges);
    }

    #[test]
    fn highlighter_delete_via_edit() {
        let mut h = Highlighter::new("# Hello\n");
        let _before = h.highlight_lines(0..1);

        // Delete "Hello"
        h.apply_edit(&[TextEdit {
            range: 2..7,
            new_text_len: 0,
            new_text: String::new(),
        }]);

        let after = h.highlight_lines(0..1);
        assert_eq!(after[0].text, "# ");
    }

    #[test]
    fn backward_range_delete_incremental() {
        let lines = assert_edit_matches_fresh(
            "# Heading\n\nParagraph with *emphasis* here.\n",
            TextEdit {
                range: Range { start: 10, end: 5 },
                new_text_len: 0,
                new_text: String::new(),
            },
        );

        assert!(!lines.is_empty());
    }

    #[test]
    fn backward_range_replace_incremental() {
        let lines = assert_edit_matches_fresh(
            "# Heading\n\nParagraph with *emphasis* here.\n",
            TextEdit {
                range: Range { start: 10, end: 5 },
                new_text_len: 3,
                new_text: "abc".to_string(),
            },
        );

        assert!(!lines.is_empty());
    }

    #[test]
    fn newline_insertion_incremental() {
        let initial = "Paragraph with *emphasis* here.\n";
        let before_line_count = Highlighter::new(initial).highlight_lines(0..100).len();
        let lines = assert_edit_matches_fresh(
            initial,
            TextEdit {
                range: 15..15,
                new_text_len: 1,
                new_text: "\n".to_string(),
            },
        );

        assert_eq!(lines.len(), before_line_count + 1);
    }

    #[test]
    fn newline_replacement_incremental() {
        let initial = "Paragraph with *emphasis* here.\n";
        let before_line_count = Highlighter::new(initial).highlight_lines(0..100).len();
        let lines = assert_edit_matches_fresh(
            initial,
            TextEdit {
                range: 14..15,
                new_text_len: 1,
                new_text: "\n".to_string(),
            },
        );

        assert_eq!(lines.len(), before_line_count + 1);
    }

    #[test]
    fn multi_line_paste_incremental() {
        let initial = "# Heading\n\nParagraph here.\n";
        let before_line_count = Highlighter::new(initial).highlight_lines(0..100).len();
        let lines = assert_edit_matches_fresh(
            initial,
            TextEdit {
                range: 12..21,
                new_text_len: 20,
                new_text: "first\n**bold**\nlast\n".to_string(),
            },
        );

        assert_eq!(lines.len(), before_line_count + 3);
    }

    #[test]
    fn highlighter_unterminated_fence_no_panic() {
        let text = "```rust\nno closing fence\n";
        let h = Highlighter::new(text);
        let lines = h.highlight_lines(0..3);
        assert!(!lines.is_empty());
    }

    #[test]
    fn highlighter_nested_fences_in_blockquotes_no_panic() {
        let text = "> ```rust\n> fn main() {}\n> ```\n";
        let h = Highlighter::new(text);
        let lines = h.highlight_lines(0..3);
        assert_eq!(lines.len(), 3);
    }

    #[test]
    fn highlighter_only_frontmatter() {
        let text = "---\ntitle: Hello\n---\n";
        let h = Highlighter::new(text);
        let lines = h.highlight_lines(0..4);
        assert_eq!(lines.len(), 3);
    }

    #[test]
    fn highlighter_multiple_fences() {
        let text = "```rust\nfn main() {}\n```\n\n```python\nprint('hi')\n```\n";
        let h = Highlighter::new(text);
        let lines = h.highlight_lines(0..8);
        // Document has 7 lines (ends with \n)
        assert_eq!(lines.len(), 7);
    }

    #[test]
    fn injection_viewport_mid_fence() {
        let text = long_rust_fence_for_test();
        let partial = assert_viewport_matches_full(text, 6..11);
        let line_starts = line_start_indices(text);
        let viewport_bytes = line_starts[6]..line_starts[11];
        let highlighter = Highlighter::new(text);
        let spans = highlighter.collect_spans_in_range(viewport_bytes.clone());

        assert!(
            partial[0]
                .spans
                .iter()
                .any(|span| span.style == SemanticStyle::Comment),
            "a viewport beginning inside a block comment must retain its opening context"
        );
        assert!(spans.iter().all(|span| {
            span.start_byte >= viewport_bytes.start && span.end_byte <= viewport_bytes.end
        }));
        assert!(spans.iter().any(|span| {
            span.style == SemanticStyle::Comment && span.start_byte == viewport_bytes.start
        }));
    }

    #[test]
    fn injection_viewport_end_fence() {
        let text = long_rust_fence_for_test();
        let partial = assert_viewport_matches_full(text, 1..7);
        let line_starts = line_start_indices(text);
        let viewport_bytes = line_starts[1]..line_starts[7];
        let highlighter = Highlighter::new(text);
        let spans = highlighter.collect_spans_in_range(viewport_bytes.clone());

        assert_eq!(partial.len(), 6);
        assert!(partial
            .iter()
            .any(|line| line.spans.iter().any(|span| matches!(
                span.style,
                SemanticStyle::Keyword | SemanticStyle::NumberLit
            ))));
        assert!(spans.iter().all(|span| {
            span.start_byte >= viewport_bytes.start && span.end_byte <= viewport_bytes.end
        }));
        assert!(spans.iter().any(|span| {
            span.style == SemanticStyle::Comment && span.end_byte == viewport_bytes.end
        }));
    }

    #[test]
    fn injection_viewport_front_matter_partial() {
        let text = "---\n\
                    title: Injection viewport\n\
                    owner: editor-team\n\
                    description: |\n\
                      first visible scalar line\n\
                      second visible scalar line\n\
                    tags: [rust, markdown]\n\
                    enabled: true\n\
                    ---\n\
                    # Body\n";
        let partial = assert_viewport_matches_full(text, 4..10);

        assert!(partial.iter().any(|line| line
            .spans
            .iter()
            .any(|span| span.style == SemanticStyle::FmKey)));
        assert!(partial.iter().any(|line| line
            .spans
            .iter()
            .any(|span| span.style == SemanticStyle::FmValue)));
    }

    #[test]
    fn injection_after_edit_correct() {
        let initial = long_rust_fence_for_test();
        let insertion_offset = initial.find("let answer").expect("Rust assignment");
        let inserted = "pub ";
        let mut expected = initial.to_string();
        expected.insert_str(insertion_offset, inserted);

        let mut highlighter = Highlighter::new(initial);
        highlighter.apply_edit(&[TextEdit {
            range: insertion_offset..insertion_offset,
            new_text_len: inserted.len(),
            new_text: inserted.to_string(),
        }]);

        assert_eq!(
            highlighter.highlight_lines(0..1000),
            Highlighter::new(&expected).highlight_lines(0..1000)
        );
    }

    #[test]
    fn injection_queries_are_shared_and_reused_after_edit() {
        let text = "```rust\nfn first() {}\n```\n\n```rust\nfn second() {}\n```\n";
        let mut highlighter = Highlighter::new(text);

        assert_eq!(highlighter.injections.len(), 2);
        assert_eq!(highlighter.query_cache.len(), 1);
        assert!(Arc::ptr_eq(
            &highlighter.injections[0].query,
            &highlighter.injections[1].query
        ));
        let cached_query = Arc::clone(
            highlighter
                .query_cache
                .get("rust")
                .expect("Rust query should be cached"),
        );

        let insertion_offset = text.find("first").expect("first function name");
        highlighter.apply_edit(&[TextEdit {
            range: insertion_offset..insertion_offset,
            new_text_len: 1,
            new_text: "x".to_string(),
        }]);

        assert_eq!(highlighter.query_cache.len(), 1);
        assert!(Arc::ptr_eq(
            &cached_query,
            highlighter
                .query_cache
                .get("rust")
                .expect("Rust query should remain cached")
        ));
        assert!(highlighter
            .injections
            .iter()
            .all(|injection| Arc::ptr_eq(&cached_query, &injection.query)));
    }

    #[test]
    fn fenced_language_spans_override_code_block_fallback() {
        let highlighter = Highlighter::new("```rust\nfn main() {}\n```\n");
        let lines = highlighter.highlight_lines(0..3);
        let code_line = &lines[1];

        assert!(code_line
            .spans
            .windows(2)
            .all(|pair| pair[0].end_col <= pair[1].start_col));
        assert!(code_line.spans.iter().any(|span| {
            span.start_col == 0 && span.end_col == 2 && span.style == SemanticStyle::Keyword
        }));
    }

    #[test]
    fn yaml_fence_uses_code_styles_not_front_matter_styles() {
        let highlighter = Highlighter::new("```yaml\nkey: value\n```\n");
        let lines = highlighter.highlight_lines(0..3);

        assert!(lines[1]
            .spans
            .iter()
            .all(|span| !matches!(span.style, SemanticStyle::FmKey | SemanticStyle::FmValue)));
        assert!(lines[1]
            .spans
            .iter()
            .any(|span| span.style == SemanticStyle::StringLit));
    }

    #[test]
    fn toml_front_matter_quoted_keys_use_key_style() {
        let text = "+++\n\"display name\" = \"oom\"\ndatabase.\"user name\" = \"editor\"\n+++\n";
        let highlighter = Highlighter::new(text);
        let lines = highlighter.highlight_lines(0..4);

        for (line_index, expected_key) in [(1, "\"display name\""), (2, "\"user name\"")] {
            let line = &lines[line_index];
            assert!(
                line.spans.iter().any(|span| {
                    span.style == SemanticStyle::FmKey
                        && &line.text[span.start_col..span.end_col] == expected_key
                }),
                "expected {expected_key:?} to be an FmKey in {line:?}"
            );
            assert!(line
                .spans
                .iter()
                .any(|span| span.style == SemanticStyle::FmValue));
        }
    }

    #[test]
    fn yaml_front_matter_internal_punctuation_is_not_a_delimiter() {
        let text = "---\ndefaults: &defaults\n  enabled: true\ncopy: *defaults\n---\n";
        let highlighter = Highlighter::new(text);
        let lines = highlighter.highlight_lines(0..5);

        assert!(lines[0]
            .spans
            .iter()
            .any(|span| span.style == SemanticStyle::FmDelimiter));
        assert!(lines[4]
            .spans
            .iter()
            .any(|span| span.style == SemanticStyle::FmDelimiter));

        for (line_index, marker) in [(1, '&'), (3, '*')] {
            let line = &lines[line_index];
            let marker_col = line.text.find(marker).expect("YAML marker");
            assert!(line.spans.iter().any(|span| {
                span.style == SemanticStyle::Punct
                    && span.start_col <= marker_col
                    && span.end_col > marker_col
            }));
            assert!(line.spans.iter().all(|span| {
                span.style != SemanticStyle::FmDelimiter
                    || marker_col < span.start_col
                    || marker_col >= span.end_col
            }));
        }
    }

    #[test]
    fn merge_same_style_overlap() {
        let merged = merge_overlapping_spans(vec![
            ranked_span(0, 10, SemanticStyle::Heading1, 0),
            ranked_span(5, 15, SemanticStyle::Heading1, 1),
        ]);

        assert_eq!(merged, vec![span(0, 15, SemanticStyle::Heading1)]);
    }

    #[test]
    fn merge_different_style_overlap_later_wins() {
        let merged = merge_overlapping_spans(vec![
            ranked_span(0, 10, SemanticStyle::Heading1, 0),
            ranked_span(5, 8, SemanticStyle::Emphasis, 1),
        ]);

        assert_eq!(
            merged,
            vec![
                span(0, 5, SemanticStyle::Heading1),
                span(5, 8, SemanticStyle::Emphasis),
                span(8, 10, SemanticStyle::Heading1),
            ]
        );
    }

    #[test]
    fn merge_later_fully_covers_earlier() {
        let merged = merge_overlapping_spans(vec![
            ranked_span(2, 5, SemanticStyle::Heading1, 0),
            ranked_span(0, 10, SemanticStyle::Emphasis, 1),
        ]);

        assert_eq!(merged, vec![span(0, 10, SemanticStyle::Emphasis)]);
    }

    #[test]
    fn merge_triple_nested() {
        let merged = merge_overlapping_spans(vec![
            ranked_span(0, 20, SemanticStyle::Heading1, 0),
            ranked_span(2, 4, SemanticStyle::Emphasis, 1),
            ranked_span(3, 5, SemanticStyle::Strong, 2),
        ]);

        assert_eq!(
            merged,
            vec![
                span(0, 2, SemanticStyle::Heading1),
                span(2, 3, SemanticStyle::Emphasis),
                span(3, 5, SemanticStyle::Strong),
                span(5, 20, SemanticStyle::Heading1),
            ]
        );
        assert!(merged
            .windows(2)
            .all(|pair| pair[0].end_col <= pair[1].start_col));
    }

    #[test]
    fn merge_adjacent_same_style() {
        let merged = merge_overlapping_spans(vec![
            ranked_span(0, 5, SemanticStyle::Heading1, 0),
            ranked_span(5, 10, SemanticStyle::Heading1, 1),
        ]);

        assert_eq!(merged, vec![span(0, 10, SemanticStyle::Heading1)]);
    }

    #[test]
    fn merge_adjacent_different_style() {
        let merged = merge_overlapping_spans(vec![
            ranked_span(0, 5, SemanticStyle::Heading1, 0),
            ranked_span(5, 10, SemanticStyle::Emphasis, 1),
        ]);

        assert_eq!(
            merged,
            vec![
                span(0, 5, SemanticStyle::Heading1),
                span(5, 10, SemanticStyle::Emphasis),
            ]
        );
    }

    #[test]
    fn merge_no_overlap() {
        let merged = merge_overlapping_spans(vec![
            ranked_span(0, 3, SemanticStyle::Heading1, 0),
            ranked_span(5, 8, SemanticStyle::Emphasis, 1),
        ]);

        assert_eq!(
            merged,
            vec![
                span(0, 3, SemanticStyle::Heading1),
                span(5, 8, SemanticStyle::Emphasis),
            ]
        );
    }

    #[test]
    fn merge_empty_and_single() {
        assert!(merge_overlapping_spans(Vec::new()).is_empty());

        let single = span(0, 5, SemanticStyle::Heading1);
        assert_eq!(
            merge_overlapping_spans(vec![RankedSpan {
                span: single.clone(),
                priority: 7,
            }]),
            vec![single]
        );
    }

    #[test]
    fn merge_priority_survives_start_sort_pressure() {
        let merged = merge_overlapping_spans(vec![
            ranked_span(10, 20, SemanticStyle::Heading1, 0),
            ranked_span(0, 15, SemanticStyle::Emphasis, 1),
        ]);

        assert_eq!(
            merged,
            vec![
                span(0, 15, SemanticStyle::Emphasis),
                span(15, 20, SemanticStyle::Heading1),
            ]
        );
    }

    #[test]
    fn highlight_lines_spans_non_overlapping() {
        let text = "---\ntitle: Span contract\nenabled: true\n---\n# Hello *world*\n\n```rust\nfn main() {}\n```\n";
        let lines = Highlighter::new(text).highlight_lines(0..usize::MAX);

        for (line_index, line) in lines.iter().enumerate() {
            assert!(
                line.spans
                    .windows(2)
                    .all(|pair| pair[0].end_col <= pair[1].start_col),
                "spans must be non-overlapping on line {line_index}: {:?}",
                line.spans
            );
        }

        let heading = &lines[4];
        let world_col = heading.text.find("world").expect("emphasized word");
        assert!(heading.spans.iter().any(|span| {
            span.start_col <= world_col
                && span.end_col > world_col
                && span.style == SemanticStyle::Emphasis
        }));

        let rust = &lines[7];
        let fn_col = rust.text.find("fn").expect("Rust function keyword");
        assert!(rust.spans.iter().any(|span| {
            span.start_col <= fn_col
                && span.end_col > fn_col
                && span.style == SemanticStyle::Keyword
        }));
    }

    #[test]
    fn highlighter_inline_code() {
        let text = "Use `code` in text.\n";
        let h = Highlighter::new(text);
        let lines = h.highlight_lines(0..1);
        assert_eq!(lines.len(), 1);
        let has_code_span = lines[0]
            .spans
            .iter()
            .any(|s| s.style == SemanticStyle::CodeSpan);
        assert!(has_code_span, "inline code should have CodeSpan style");
    }

    #[test]
    fn highlighter_emphasis_and_strong() {
        let text = "*italic* and **bold** and ~~strikethrough~~\n";
        let h = Highlighter::new(text);
        let lines = h.highlight_lines(0..1);
        assert_eq!(lines.len(), 1);
        let styles: Vec<_> = lines[0].spans.iter().map(|s| s.style).collect();
        assert!(
            styles.contains(&SemanticStyle::Emphasis),
            "should have Emphasis style"
        );
        assert!(
            styles.contains(&SemanticStyle::Strong),
            "should have Strong style"
        );
        assert!(
            styles.contains(&SemanticStyle::Strikethrough),
            "should have Strikethrough style"
        );
    }

    #[test]
    fn highlighter_list_markers() {
        let text = "- item 1\n- item 2\n* item 3\n+ item 4\n";
        let h = Highlighter::new(text);
        let lines = h.highlight_lines(0..4);
        assert_eq!(lines.len(), 4);
        let has_list_marker = lines[0]
            .spans
            .iter()
            .any(|s| s.style == SemanticStyle::ListMarker);
        assert!(has_list_marker, "list items should have ListMarker style");
    }

    #[test]
    fn highlighter_headings() {
        let text = "# H1\n## H2\n### H3\n#### H4\n##### H5\n###### H6\n";
        let h = Highlighter::new(text);
        for i in 0..6 {
            let lines = h.highlight_lines(i..i + 1);
            assert_eq!(lines.len(), 1);
            let has_heading = lines[0]
                .spans
                .iter()
                .any(|s| matches!(s.style, SemanticStyle::Heading1));
            assert!(has_heading, "line {} should have heading style", i);
        }
    }

    #[test]
    fn highlighter_thematic_break() {
        let text = "---\n\nSome text\n";
        let h = Highlighter::new(text);
        let lines = h.highlight_lines(0..3);
        assert_eq!(lines.len(), 3);
        let has_rule = lines[0]
            .spans
            .iter()
            .any(|s| s.style == SemanticStyle::Rule);
        assert!(has_rule, "thematic break should have Rule style");
    }

    #[test]
    fn highlighter_links() {
        let text = "[link text](https://example.com)\n";
        let h = Highlighter::new(text);
        let lines = h.highlight_lines(0..1);
        assert_eq!(lines.len(), 1);
        let styles: Vec<_> = lines[0].spans.iter().map(|s| s.style).collect();
        assert!(
            styles.contains(&SemanticStyle::Link),
            "should have Link style"
        );
        assert!(
            styles.contains(&SemanticStyle::LinkUrl),
            "should have LinkUrl style"
        );
    }

    #[test]
    fn highlighter_blockquote() {
        let text = "> quoted text\n";
        let h = Highlighter::new(text);
        let lines = h.highlight_lines(0..1);
        assert_eq!(lines.len(), 1);
        let has_quote = lines[0]
            .spans
            .iter()
            .any(|s| s.style == SemanticStyle::Quote);
        assert!(has_quote, "blockquote should have Quote style");
    }

    #[test]
    fn incremental_equivalence_random_edits() {
        let fixture = mixed_fixture_for_test();
        let mut highlighter = Highlighter::new(&fixture);

        let test_edits = vec![
            TextEdit {
                range: 0..0,
                new_text_len: 5,
                new_text: "test ".to_string(),
            },
            TextEdit {
                range: 2..5,
                new_text_len: 0,
                new_text: String::new(),
            },
            TextEdit {
                range: 10..10,
                new_text_len: 3,
                new_text: "abc".to_string(),
            },
            TextEdit {
                range: fixture.len().saturating_sub(5)..fixture.len(),
                new_text_len: 0,
                new_text: String::new(),
            },
        ];

        for edit in &test_edits {
            highlighter.apply_edit(std::slice::from_ref(edit));
        }

        let lines = highlighter.highlight_lines(0..100);
        assert!(!lines.is_empty());
    }

    #[cfg(test)]
    proptest! {
        #![proptest_config(proptest::prelude::ProptestConfig {
            cases: 150,
            ..proptest::prelude::ProptestConfig::default()
        })]

        #[test]
        fn proptest_with_backward_and_newlines(
            text in r"[\x20-\x7E\n]{10,500}",
            edit_ops in proptest::collection::vec(
                proptest::collection::vec(proptest::prelude::any::<u8>(), 1..5),
                1..10
            )
        ) {
            let mut highlighter = Highlighter::new(&text);
            let mut current_text = text.clone();

            for op_group in &edit_ops {
                for &byte in op_group {
                    let edit_start = (byte as usize) % (current_text.len() + 1);
                    let insert_len = ((byte.wrapping_add(1)) as usize) % 10;
                    let new_text: String = (0..insert_len)
                        .map(|i| {
                            if byte.wrapping_add(i as u8).is_multiple_of(5) {
                                '\n'
                            } else {
                                ((byte.wrapping_add(i as u8)) % 95 + 32) as char
                            }
                        })
                        .collect();

                    let edit_end = (edit_start + ((byte.wrapping_add(10)) as usize) % current_text.len().max(1)).min(current_text.len());
                    let normalized_start = edit_start.min(edit_end);
                    let normalized_end = edit_start.max(edit_end);
                    let range = if byte % 2 == 0 {
                        normalized_start..normalized_end
                    } else {
                        normalized_end..normalized_start
                    };

                    let edit = TextEdit {
                        range,
                        new_text_len: new_text.len(),
                        new_text: new_text.clone(),
                    };

                    highlighter.apply_edit(std::slice::from_ref(&edit));

                    current_text.replace_range(normalized_start..normalized_end, &new_text);

                    prop_assert_eq!(highlighter.text(), &current_text);

                    let incremental = highlighter.highlight_lines(0..1000);
                    let fresh = Highlighter::new(&current_text).highlight_lines(0..1000);

                    prop_assert_eq!(
                        incremental.len(),
                        fresh.len(),
                        "line count mismatch after edit"
                    );

                    for i in 0..incremental.len().min(fresh.len()) {
                        prop_assert_eq!(
                            &incremental[i].text,
                            &fresh[i].text,
                            "line {} text mismatch after edit",
                            i
                        );
                        prop_assert!(
                            incremental[i]
                                .spans
                                .windows(2)
                                .all(|pair| pair[0].end_col <= pair[1].start_col),
                            "incremental spans overlap on line {}: {:?}",
                            i,
                            incremental[i].spans
                        );
                        prop_assert!(
                            fresh[i]
                                .spans
                                .windows(2)
                                .all(|pair| pair[0].end_col <= pair[1].start_col),
                            "fresh spans overlap on line {}: {:?}",
                            i,
                            fresh[i].spans
                        );
                        prop_assert_eq!(
                            incremental[i].spans.len(),
                            fresh[i].spans.len(),
                            "line {} span count mismatch after edit",
                            i
                        );

                        for j in 0..incremental[i].spans.len().min(fresh[i].spans.len()) {
                            prop_assert_eq!(
                                incremental[i].spans[j].start_col,
                                fresh[i].spans[j].start_col,
                                "line {} span {} start_col mismatch",
                                i,
                                j
                            );
                            prop_assert_eq!(
                                incremental[i].spans[j].end_col,
                                fresh[i].spans[j].end_col,
                                "line {} span {} end_col mismatch",
                                i,
                                j
                            );
                            prop_assert_eq!(
                                incremental[i].spans[j].style,
                                fresh[i].spans[j].style,
                                "line {} span {} style mismatch",
                                i,
                                j
                            );
                        }
                    }
                }
            }
        }

        #[test]
        fn all_highlight_spans_stay_within_unicode_character_bounds(
            random_chars in proptest::collection::vec(any::<char>(), 0..128)
        ) {
            let random_text: String = random_chars.into_iter().collect();
            let text = format!("# é{random_text}\n");
            let lines = Highlighter::new(&text).highlight_lines(0..usize::MAX);

            for (line_index, line) in lines.iter().enumerate() {
                let char_count = line.text.chars().count();
                for span in &line.spans {
                    prop_assert!(
                        span.start_col <= span.end_col,
                        "line {line_index} has reversed span [{}, {})",
                        span.start_col,
                        span.end_col
                    );
                    prop_assert!(
                        span.end_col <= char_count,
                        "line {line_index} span [{}, {}) exceeds {char_count} characters in {:?}",
                        span.start_col,
                        span.end_col,
                        line.text
                    );
                }
            }
        }
    }

    #[cfg(debug_assertions)]
    #[test]
    fn timing_sanity_100kb() {
        let mut fixture = String::new();
        for i in 0..2500 {
            fixture.push_str(&format!(
                "## Heading {}\n\nParagraph {} with some text to make it longer.\n\n",
                i, i
            ));
        }
        assert!(
            fixture.len() >= 100_000,
            "fixture should be >= 100KB (got {})",
            fixture.len()
        );

        let mut h = Highlighter::new(&fixture);

        let start = std::time::Instant::now();
        h.apply_edit(&[TextEdit {
            range: 500..500,
            new_text_len: 1,
            new_text: "x".to_string(),
        }]);
        let elapsed = start.elapsed();

        assert!(
            elapsed < std::time::Duration::from_secs(5),
            "apply_edit on 100KB should complete within 5s (debug mode)"
        );
    }

    #[cfg(debug_assertions)]
    #[test]
    fn timing_sanity_injection_heavy_edit() {
        let fixture = injection_heavy_fixture_for_test(12);
        let insertion_offset = fixture.find("fn rust_0").expect("first Rust fence") + 3;
        let mut highlighter = Highlighter::new(&fixture);
        let insert = TextEdit {
            range: insertion_offset..insertion_offset,
            new_text_len: 1,
            new_text: "x".to_string(),
        };
        let delete = TextEdit {
            range: insertion_offset..insertion_offset + 1,
            new_text_len: 0,
            new_text: String::new(),
        };

        for _ in 0..3 {
            highlighter.apply_edit(std::slice::from_ref(&insert));
            highlighter.apply_edit(std::slice::from_ref(&delete));
        }

        let iterations = 20;
        let start = std::time::Instant::now();
        for _ in 0..iterations {
            highlighter.apply_edit(std::slice::from_ref(&insert));
            highlighter.apply_edit(std::slice::from_ref(&delete));
        }
        let average = start.elapsed() / (iterations * 2);

        assert!(
            average < std::time::Duration::from_millis(50),
            "apply_edit on an injection-heavy document averaged {average:?}; expected <50ms in debug mode"
        );
    }

    #[test]
    fn no_panic_empty_file() {
        let h = Highlighter::new("");
        let _ = h.highlight_lines(0..1);
    }

    #[test]
    fn no_panic_only_frontmatter() {
        let h = Highlighter::new("---\ntitle: Hello\n---\n");
        let _ = h.highlight_lines(0..4);
    }

    #[test]
    fn no_panic_unterminated_fence() {
        let h = Highlighter::new("```rust\nno closing fence\n");
        let _ = h.highlight_lines(0..3);
    }

    #[test]
    fn no_panic_nested_fences_in_blockquotes() {
        let h = Highlighter::new("> ```rust\n> fn main() {}\n> ```\n");
        let _ = h.highlight_lines(0..3);
    }

    #[test]
    fn no_panic_multiple_frontmatter_blocks() {
        let h = Highlighter::new("---\ntitle: A\n---\n---\ntitle: B\n---\n");
        let _ = h.highlight_lines(0..8);
    }

    #[test]
    fn no_panic_crlf_line_endings() {
        let h = Highlighter::new("# Hello\r\n\r\nWorld\r\n");
        let _ = h.highlight_lines(0..3);
    }

    #[test]
    fn no_panic_very_long_line() {
        let h = Highlighter::new(&"x".repeat(100_000));
        let _ = h.highlight_lines(0..1);
    }

    #[test]
    fn no_panic_unicode_heavy() {
        let h = Highlighter::new("# Hello 世界 🌍\n\nEmoji and unicode: αβγδε\n");
        let _ = h.highlight_lines(0..3);
    }

    #[test]
    fn no_panic_ordered_list_marker_followed_by_high_unicode() {
        let h = Highlighter::new("# é\n1\u{80000}\n");
        let _ = h.highlight_lines(0..2);
    }

    #[test]
    fn no_panic_mixed_frontmatter_and_fences() {
        let h = Highlighter::new(
            "---\ntitle: Test\n---\n\n\
             # Heading\n\n\
             ```rust\nfn main() {}\n```\n\n\
             +++\nkey = \"value\"\n+++\n\n\
             ```python\nprint('hi')\n```\n",
        );
        let _ = h.highlight_lines(0..20);
    }

    #[test]
    fn no_panic_html_blocks() {
        let h = Highlighter::new(
            "<div>\n  <p>HTML block</p>\n</div>\n\n\
             <span>inline</span>\n",
        );
        let _ = h.highlight_lines(0..5);
    }

    #[test]
    fn no_panic_nested_list() {
        let h =
            Highlighter::new("- item 1\n  - nested 1\n  - nested 2\n- item 2\n    - deep nested\n");
        let _ = h.highlight_lines(0..5);
    }

    #[test]
    fn no_panic_table() {
        let h = Highlighter::new(
            "| Header 1 | Header 2 |\n\
             |----------|----------|\n\
             | Cell 1   | Cell 2   |\n",
        );
        let _ = h.highlight_lines(0..3);
    }

    #[test]
    fn no_panic_footnotes() {
        let h = Highlighter::new(
            "Text with a footnote[^1].\n\n\
             [^1]: This is the footnote definition.\n",
        );
        let _ = h.highlight_lines(0..3);
    }

    fn mixed_fixture_for_test() -> String {
        let mut doc = String::new();
        doc.push_str("---\ntitle: Test\n---\n\n");
        doc.push_str("# Hello World\n\n");
        doc.push_str("Some *emphasis* and **bold** text.\n\n");
        doc.push_str("```rust\nfn main() {}\n```\n\n");
        doc.push_str("- item 1\n- item 2\n");
        doc
    }

    fn long_rust_fence_for_test() -> &'static str {
        "Before\n\
         ```rust\n\
         fn main() {\n\
             let answer = 42;\n\
             let label = \"value\";\n\
             /* block comment begins\n\
                and continues here\n\
                before ending here */\n\
             let doubled = answer * 2;\n\
             if doubled > 42 {\n\
                 println!(\"{label}: {doubled}\");\n\
             }\n\
         }\n\
         ```\n\
         After\n"
    }

    #[cfg(debug_assertions)]
    fn injection_heavy_fixture_for_test(repetitions: usize) -> String {
        let mut document = String::new();
        for index in 0..repetitions {
            document.push_str(&format!("```rust\nfn rust_{index}() {{}}\n```\n\n"));
            document.push_str(&format!("```python\nprint({index})\n```\n\n"));
            document.push_str(&format!("```yaml\nvalue: {index}\n```\n\n"));
            document.push_str(&format!("```toml\nvalue = {index}\n```\n\n"));
            document.push_str(&format!(
                "```javascript\nfunction value{index}() {{ return {index}; }}\n```\n\n"
            ));
        }
        document
    }

    fn assert_viewport_matches_full(text: &str, viewport: Range<usize>) -> Vec<StyledLine> {
        let highlighter = Highlighter::new(text);
        let full = language_spans_only(highlighter.highlight_lines(0..text.lines().count()));
        let partial = language_spans_only(highlighter.highlight_lines(viewport.clone()));

        assert_eq!(partial, full[viewport].to_vec());
        partial
    }

    fn language_spans_only(mut lines: Vec<StyledLine>) -> Vec<StyledLine> {
        for line in &mut lines {
            line.spans.retain(|span| {
                matches!(
                    span.style,
                    SemanticStyle::Keyword
                        | SemanticStyle::Function
                        | SemanticStyle::TypeName
                        | SemanticStyle::StringLit
                        | SemanticStyle::NumberLit
                        | SemanticStyle::Comment
                        | SemanticStyle::Operator
                        | SemanticStyle::Variable
                        | SemanticStyle::Punct
                        | SemanticStyle::FmKey
                        | SemanticStyle::FmValue
                )
            });
            let char_count = line.text.chars().count();
            for span in &mut line.spans {
                span.start_col = span.start_col.min(char_count);
                span.end_col = span.end_col.min(char_count);
            }
        }
        lines
    }

    fn span(start_col: usize, end_col: usize, style: SemanticStyle) -> Span {
        Span {
            start_col,
            end_col,
            style,
        }
    }

    fn ranked_span(
        start_col: usize,
        end_col: usize,
        style: SemanticStyle,
        priority: usize,
    ) -> RankedSpan {
        RankedSpan {
            span: span(start_col, end_col, style),
            priority,
        }
    }

    fn span_text(line: &StyledLine, span: &Span) -> String {
        line.text
            .chars()
            .skip(span.start_col)
            .take(span.end_col - span.start_col)
            .collect()
    }

    fn assert_edit_matches_fresh(initial: &str, edit: TextEdit) -> Vec<StyledLine> {
        let mut expected_text = initial.to_string();
        let start = edit.range.start.min(edit.range.end);
        let end = edit.range.start.max(edit.range.end);
        expected_text.replace_range(start..end, &edit.new_text);

        let mut highlighter = Highlighter::new(initial);
        highlighter.apply_edit(&[edit]);

        assert_eq!(highlighter.text(), expected_text);
        assert_eq!(highlighter.last_parse_path, ParsePath::Incremental);
        let incremental = highlighter.highlight_lines(0..1000);
        let fresh = Highlighter::new(&expected_text).highlight_lines(0..1000);
        assert_eq!(incremental, fresh);
        incremental
    }

    fn char_key(c: char) -> KeyInput {
        KeyInput {
            code: KeyCode {
                kind: KeyCodeKind::Char(c),
            },
            mods: Modifiers::default(),
        }
    }

    fn ctrl_char_key(c: char) -> KeyInput {
        KeyInput {
            code: KeyCode {
                kind: KeyCodeKind::Char(c),
            },
            mods: Modifiers {
                ctrl: true,
                ..Modifiers::default()
            },
        }
    }

    fn special_key(kind: KeyCodeKind) -> KeyInput {
        KeyInput {
            code: KeyCode { kind },
            mods: Modifiers::default(),
        }
    }

    fn apply_vim_key(
        vim: &mut VimCore,
        highlighter: &mut Highlighter,
        key: KeyInput,
    ) -> Vec<TextEdit> {
        let mut applied = Vec::new();
        for effect in vim.handle_key(key) {
            if let VimEffect::Edited { edits } = effect {
                highlighter.apply_edit(&edits);
                applied.extend(edits);
            }
        }
        if !applied.is_empty() {
            assert_vim_highlighting_matches_fresh(vim, highlighter);
        }
        applied
    }

    fn assert_vim_highlighting_matches_fresh(vim: &VimCore, highlighter: &Highlighter) {
        let expected = vim.text();
        assert_eq!(highlighter.text(), expected);
        assert_eq!(highlighter.last_parse_path, ParsePath::Incremental);
        assert_eq!(
            highlighter.highlight_lines(0..1000),
            Highlighter::new(&expected).highlight_lines(0..1000)
        );
    }
}
