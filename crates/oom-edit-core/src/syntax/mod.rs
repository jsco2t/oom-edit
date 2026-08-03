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

use std::ops::Range;

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

// ── Injection highlight queries ─────────────────────────────────────────────

/// Minimal highlight query for languages that don't ship a bundled one.
/// Covers at least: keywords, strings, comments, numbers, functions, types.
const MINIMAL_QUERY: &str = r#"
(keyword) @keyword
(string) @string
(comment) @comment
(number) @number
(function) @function
(type) @type
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
    query: std::sync::Arc<Query>,
    /// For fence blocks: the language tag from the info string.
    #[allow(dead_code)]
    fence_lang: Option<String>,
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
    /// Edits are applied in order (sorted by start offset) and
    /// injection regions are re-resolved for changed ranges.
    pub fn apply_edit(&mut self, edits: &[TextEdit]) {
        if edits.is_empty() {
            return;
        }

        // Sort edits by start offset (ascending)
        let mut sorted_edits: Vec<_> = edits.to_vec();
        sorted_edits.sort_by_key(|e| e.range.start);

        // Apply to the markdown tree
        for edit in &sorted_edits {
            self.md_tree.edit(&tree_sitter::InputEdit {
                start_byte: edit.range.start,
                old_end_byte: edit.range.end,
                new_end_byte: edit.range.start + edit.new_text_len,
                start_position: byte_to_point(&self.text, edit.range.start),
                old_end_position: byte_to_point(&self.text, edit.range.end),
                new_end_position: byte_to_point(&self.text, edit.range.start + edit.new_text_len),
            });
        }

        // Update the text by applying edits (before re-parse, so tree positions match)
        apply_edits_to_string(&mut self.text, &sorted_edits);

        // Re-parse incrementally with the updated text
        self.md_tree = self
            .md_parser
            .parse(&self.text, Some(&self.md_tree))
            .expect("incremental re-parse should succeed");

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

        // Collect all spans from a single query run over the whole document
        let all_spans = self.collect_all_spans();

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
            // all_spans uses absolute byte offsets as start_col/end_col
            let mut spans: Vec<Span> = all_spans
                .iter()
                .filter(|s| {
                    let abs_start = s.start_col;
                    let abs_end = s.end_col;
                    abs_start < line_end && abs_end > line_start
                })
                .map(|s| Span {
                    start_col: s.start_col.saturating_sub(line_start),
                    end_col: s.end_col.saturating_sub(line_start),
                    style: s.style,
                })
                .collect();

            // Sort and merge spans
            spans.sort_by_key(|s| s.start_col);
            spans = merge_overlapping_spans(spans);

            result.push(StyledLine {
                text: line_text.to_string(),
                spans,
            });
        }

        result
    }

    /// Run the markdown highlight query once and collect all spans,
    /// taking injections into account.
    fn collect_all_spans(&self) -> Vec<Span> {
        let mut spans = Vec::new();
        let text = &self.text;
        let text_bytes = text.as_bytes();

        let mut cursor = QueryCursor::new();
        let root = self.md_tree.root_node();
        let mut matches = cursor.matches(&self.md_query, root, text_bytes);

        while let Some(m) = matches.next() {
            for capture in m.captures {
                let capture_name = self.md_query.capture_names()[capture.index as usize];
                let style = md_capture_to_style(capture_name);
                let n = capture.node;

                let start_col = n.start_byte();
                let end_col = n.end_byte();

                if start_col < end_col {
                    spans.push(Span {
                        start_col,
                        end_col,
                        style,
                    });
                }
            }
        }

        // Run inline query on inline nodes
        self.collect_inline_spans(&mut spans, root, text);

        // Check for injection overlaps
        for injection in &self.injections {
            let inj_start = injection.start;
            let inj_end = injection.end;

            // Find nodes that overlap with this injection
            let mut inj_cursor = QueryCursor::new();
            let mut inj_parser = Parser::new();
            if inj_parser.set_language(&injection.language).is_ok() {
                if let Some(inj_tree) = inj_parser.parse(&text[inj_start..inj_end], None) {
                    let inj_root = inj_tree.root_node();

                    while let Some(m) = inj_cursor
                        .matches(
                            &injection.query,
                            inj_root,
                            &text.as_bytes()[inj_start..inj_end],
                        )
                        .next()
                    {
                        for capture in m.captures {
                            let capture_name =
                                injection.query.capture_names()[capture.index as usize];
                            let style = captures::capture_to_style(capture_name);
                            let inj_node = capture.node;

                            let abs_start = inj_start + inj_node.start_byte();
                            let abs_end = inj_start + inj_node.end_byte();

                            if abs_start < abs_end {
                                spans.push(Span {
                                    start_col: abs_start,
                                    end_col: abs_end,
                                    style,
                                });
                            }
                        }
                    }
                }
            }
        }

        spans
    }

    /// Collect inline spans by parsing inline regions with the markdown inline grammar.
    fn collect_inline_spans(&self, spans: &mut Vec<Span>, node: tree_sitter::Node, text: &str) {
        // Walk the tree to find inline nodes
        let mut cursor = node.walk();
        if !cursor.goto_first_child() {
            return;
        }
        loop {
            let child = cursor.node();
            let kind = child.kind();

            // Recurse into children
            if child.child_count() > 0 {
                self.collect_inline_spans(spans, child, text);
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
                                let n = capture.node;

                                let start_col = base_offset + n.start_byte();
                                let end_col = base_offset + n.end_byte();

                                if start_col < end_col {
                                    spans.push(Span {
                                        start_col,
                                        end_col,
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
        self.injections.clear();

        // Clone the tree to avoid borrow checker issues
        let tree = self.md_tree.clone();
        let text = self.text.clone();

        self.walk_for_injections(tree.root_node(), &text);
    }

    /// Walk the markdown tree and collect injection regions.
    /// Uses a recursive approach to avoid cursor lifetime issues.
    fn walk_for_injections(&mut self, node: tree_sitter::Node, text: &str) {
        let kind = node.kind().to_string();

        // Check for front matter (minus_metadata / plus_metadata)
        if kind == "minus_metadata" || kind == "plus_metadata" {
            let is_yaml = kind == "minus_metadata";
            let lang = if is_yaml {
                tree_sitter_yaml::LANGUAGE.into()
            } else {
                tree_sitter_toml_ng::LANGUAGE.into()
            };

            if let Ok(query) = Query::new(&lang, get_highlight_query(&lang)) {
                self.injections.push(Injection {
                    start: node.start_byte(),
                    end: node.end_byte(),
                    language: lang,
                    query: std::sync::Arc::new(query),
                    fence_lang: None,
                });
            }
            return; // Don't recurse into front matter
        }

        // Check for fenced code blocks
        if kind == "fenced_code_block" {
            if let Some(info_string) = find_info_string(&node, text) {
                if let Some(lang_def) = languages::find_by_alias(&info_string) {
                    let lang = (lang_def.language_fn)();
                    if let Ok(query) = Query::new(&lang, get_highlight_query(&lang)) {
                        if let Some(content) = find_code_fence_content(&node) {
                            self.injections.push(Injection {
                                start: content.start_byte(),
                                end: content.end_byte(),
                                language: lang,
                                query: std::sync::Arc::new(query),
                                fence_lang: Some(info_string),
                            });
                        }
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
                self.walk_for_injections(cursor.node(), text);
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
/// This is used by the View layout renderer for fenced code blocks: each
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

    let lang_def = languages::find_by_alias(lang);
    let lang_obj = lang_def.map(|d| (d.language_fn)());

    let Some(lang_obj) = lang_obj else {
        // Unknown language — return unstyled
        return highlight_lines_for_text(text);
    };

    let query = Query::new(&lang_obj, get_highlight_query(&lang_obj)).ok();
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
    let mut all_spans: Vec<Span> = Vec::new();
    let mut matches = cursor.matches(&query, root, text_bytes);

    while let Some(m) = matches.next() {
        for capture in m.captures {
            let capture_name = query.capture_names()[capture.index as usize];
            let style = captures::capture_to_style(capture_name);
            let node = capture.node;
            let start = node.start_byte();
            let end = node.end_byte();
            if start < end {
                all_spans.push(Span {
                    start_col: start,
                    end_col: end,
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

        let mut spans: Vec<Span> = all_spans
            .iter()
            .filter(|s| s.start_col < line_end && s.end_col > line_start)
            .map(|s| Span {
                start_col: s.start_col.saturating_sub(line_start),
                end_col: s.end_col.saturating_sub(line_start),
                style: s.style,
            })
            .collect();

        spans.sort_by_key(|s| s.start_col);
        spans = merge_overlapping_spans(spans);

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

// ── Helper: get highlight query for a language ─────────────────────────────

/// Get the highlight query source for a given tree-sitter language.
fn get_highlight_query(_lang: &tree_sitter::Language) -> &'static str {
    // Use the minimal query for all injected languages.
    // The grammar crates' bundled queries would require build-time
    // embedding which is complex; the minimal query covers the
    // required categories (keywords, strings, comments, numbers,
    // functions, types).
    MINIMAL_QUERY
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

// ── Helper: byte offset → (row, col) ───────────────────────────────────────

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

/// Apply a list of sorted, non-overlapping text edits to a string.
/// Edits must be sorted by `range.start` in ascending order.
fn apply_edits_to_string(text: &mut String, edits: &[TextEdit]) {
    if edits.is_empty() {
        return;
    }

    let mut result = String::with_capacity(text.len());
    let mut last_end = 0;

    for edit in edits {
        let start = edit.range.start;
        let end = edit.range.end;
        // Ensure we don't go out of bounds
        let start = start.min(text.len());
        let end = end.min(text.len());

        // Copy the unchanged prefix
        result.push_str(&text[last_end..start]);
        // Insert the new text
        result.push_str(&edit.new_text);
        last_end = end;
    }

    // Copy the remaining suffix
    result.push_str(&text[last_end..]);
    *text = result;
}

// ── Helper: merge overlapping spans ─────────────────────────────────────────

/// Merge overlapping or adjacent spans, keeping the later span (higher style
/// priority) when they overlap. Spans must be sorted by `start_col` before
/// calling.
fn merge_overlapping_spans(mut spans: Vec<Span>) -> Vec<Span> {
    if spans.len() <= 1 {
        return spans;
    }

    // Sort by start_col, then by end_col descending (wider spans first)
    spans.sort_by_key(|s| s.start_col);

    let mut merged: Vec<Span> = Vec::with_capacity(spans.len());

    for span in spans {
        if let Some(last) = merged.last_mut() {
            if span.start_col < last.end_col {
                // Overlapping — keep both spans (don't merge)
                merged.push(span);
            } else if span.start_col == last.end_col {
                // Adjacent — merge if same style
                if last.style == span.style {
                    last.end_col = span.end_col;
                } else {
                    merged.push(span);
                }
            } else {
                merged.push(span);
            }
        } else {
            merged.push(span);
        }
    }

    merged
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(test)]
    use proptest::prelude::*;

    #[test]
    fn highlighter_new_parses_simple_markdown() {
        let h = Highlighter::new("# Hello\n\nWorld\n");
        let lines = h.highlight_lines(0..3);
        assert_eq!(lines.len(), 3);
        assert!(!lines[0].spans.is_empty(), "heading line should have spans");
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
    fn highlighter_applies_edit_incrementally() {
        let mut h = Highlighter::new("# Hello\n\nWorld\n");
        let before = h.highlight_lines(0..3);

        // Insert a character
        h.apply_edit(&[TextEdit {
            range: 2..2,
            new_text_len: 1,
            new_text: "x".to_string(),
        }]);

        let after = h.highlight_lines(0..3);
        assert_eq!(after[0].text.len(), before[0].text.len() + 1);
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
        fn incremental_equivalence_proptest(
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
                        .map(|i| ((byte.wrapping_add(i as u8)) % 95 + 32) as char)
                        .collect();

                    let edit_end = (edit_start + ((byte.wrapping_add(10)) as usize) % current_text.len().max(1)).min(current_text.len());

                    let edit = TextEdit {
                        range: edit_start..edit_end,
                        new_text_len: new_text.len(),
                        new_text: new_text.clone(),
                    };

                    highlighter.apply_edit(std::slice::from_ref(&edit));

                    if edit_end > current_text.len() {
                        continue;
                    }
                    let start = edit_start.min(current_text.len());
                    let end = edit_end.min(current_text.len());
                    current_text.replace_range(start..end, &new_text);

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
}
