//! Syntax highlighting pipeline.
//!
//! `Highlighter` provides incremental tree-sitter highlighting of markdown
//! source with YAML/TOML front-matter and fenced code-block injections,
//! emitting renderer-agnostic [`StyledLine`]s.
//!
//! See architecture §6.3, plan §6.4 (FR-4.1–4.6), and task T06.

mod captures;
mod languages;

use languages::LangDef;

use std::{cell::RefCell, collections::HashMap, ops::Range, sync::Arc};

#[cfg(test)]
use std::cell::Cell;

use tree_sitter::{Parser, Point, Query, QueryCursor, StreamingIterator, Tree};

use crate::style::{SemanticStyle, Span, StyledLine};
use crate::vim::{collapsed_descending_edit, TextEdit};

// ── Markdown highlight query ────────────────────────────────────────────────

/// The highlight query for markdown block structure. Covers all FR-4.1
/// block-level elements: headings, fence delimiters, list markers,
/// blockquotes, thematic breaks, HTML, front-matter, tables, and link
/// reference definitions.
///
/// Inline elements (emphasis, strong, inline code, strikethrough, links)
/// are handled by the inline injection grammar, not this block-level query.
///
/// Node types are from the pinned tree-sitter-md grammar, including its
/// level-specific ATX markers and Setext underlines.
const MD_HIGHLIGHT_QUERY: &str = r#"
(atx_heading (atx_h1_marker)) @heading.1
(atx_heading (atx_h2_marker)) @heading.2
(atx_heading (atx_h3_marker)) @heading.3
(atx_heading (atx_h4_marker)) @heading.4
(atx_heading (atx_h5_marker)) @heading.5
(atx_heading (atx_h6_marker)) @heading.6
(setext_heading (setext_h1_underline)) @heading.1
(setext_heading (setext_h2_underline)) @heading.2
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
(pipe_table_header) @strong
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
(html_tag) @html.inline
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

/// A parsed source injection retained for repeat viewport reads.
struct ParsedInjection {
    start: usize,
    end: usize,
    language_name: &'static str,
    tree: Tree,
}

/// Current source styling for one independently reparsed Markdown region.
/// The complete tree remains a positional index for unchanged regions.
struct SourcePatch {
    bytes: Range<usize>,
    root_kind: String,
    first_line: usize,
    lines: Vec<StyledLine>,
    spell_exclusions: Vec<Range<usize>>,
}

const SOURCE_PARSE_CACHE_MAX_ENTRIES: usize = 8;
// Source-byte budget bounds retained trees approximately; tree allocation is grammar-dependent.
const SOURCE_PARSE_CACHE_MAX_SOURCE_BYTES: usize = 1024 * 1024;
const BOUNDED_SOURCE_MIN_DOCUMENT_BYTES: usize = 64 * 1024;
const BOUNDED_SOURCE_MAX_REGION_BYTES: usize = 8 * 1024;

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
    Bounded,
    FenceInterior,
    BlockNeutral,
    Skipped,
}

// ── Highlighter ─────────────────────────────────────────────────────────────

pub(crate) enum FenceEditStatus {
    NotIncremental,
    Incremental {
        changed_source: Option<Range<usize>>,
    },
}

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
    /// Byte offset of each physical line start, maintained with edits.
    line_starts: Vec<usize>,
    /// The main markdown block parse tree.
    md_tree: Tree,
    /// Markdown block highlight query.
    md_query: Query,
    /// Markdown inline highlight query.
    md_inline_query: Query,
    /// Incremental parser for the markdown tree.
    md_parser: Parser,
    /// Current replacements for regions whose root-tree syntax is stale.
    patches: Vec<SourcePatch>,
    /// Injection regions (front matter, fenced code blocks).
    injections: Vec<Injection>,
    /// Cached complete front-matter byte span, including parser edge cases.
    spell_front_matter_span: Option<Range<usize>>,
    /// Normalized labels owned by retained link-reference definitions.
    spell_reference_labels: Vec<String>,
    /// Compiled injection queries, keyed by canonical registry language name.
    query_cache: RefCell<HashMap<&'static str, Arc<Query>>>,
    /// Bounded, lazy source-injection parse cache. Edits invalidate affected entries.
    source_parse_cache: RefCell<Vec<ParsedInjection>>,
    fence_edit_status: FenceEditStatus,
    #[cfg(test)]
    query_compile_count: Cell<usize>,
    #[cfg(test)]
    source_injection_parse_count: Cell<usize>,
    /// Most recent parser route, exposed only to regression tests.
    #[cfg(test)]
    last_parse_path: ParsePath,
    /// Full Markdown block parses; ordinary inline edits must not increase it.
    #[cfg(test)]
    block_parse_count: Cell<usize>,
    /// Structural byte range changed by the most recent Markdown reparse.
    #[cfg(test)]
    last_changed_bytes: usize,
    #[cfg(test)]
    last_changed_ranges: usize,
    #[cfg(test)]
    last_changed_lines: usize,
    #[cfg(test)]
    injection_scan_count: usize,
    #[cfg(test)]
    reference_scan_count: usize,
    #[cfg(test)]
    last_bounded_bytes: usize,
    #[cfg(test)]
    last_bounded_lines: usize,
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
        Self::new_with_queries(text, HashMap::new())
    }

    fn new_with_queries(text: &str, queries: HashMap<&'static str, Arc<Query>>) -> Self {
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

        let spell_reference_labels = collect_spell_reference_labels(tree.root_node(), text);
        let mut highlighter = Self {
            text: text.to_string(),
            line_starts: line_start_indices(text),
            md_tree: tree,
            md_query: query,
            md_inline_query: inline_query,
            md_parser: parser,
            patches: Vec::new(),
            injections: Vec::new(),
            spell_front_matter_span: crate::frontmatter::front_matter_span(text),
            spell_reference_labels,
            query_cache: RefCell::new(queries),
            source_parse_cache: RefCell::new(Vec::new()),
            fence_edit_status: FenceEditStatus::NotIncremental,
            #[cfg(test)]
            query_compile_count: Cell::new(0),
            #[cfg(test)]
            source_injection_parse_count: Cell::new(0),
            #[cfg(test)]
            last_parse_path: ParsePath::Full,
            #[cfg(test)]
            block_parse_count: Cell::new(1),
            #[cfg(test)]
            last_changed_bytes: 0,
            #[cfg(test)]
            last_changed_ranges: 0,
            #[cfg(test)]
            last_changed_lines: 0,
            #[cfg(test)]
            injection_scan_count: 0,
            #[cfg(test)]
            reference_scan_count: 1,
            #[cfg(test)]
            last_bounded_bytes: 0,
            #[cfg(test)]
            last_bounded_lines: 0,
        };

        // Discover injection regions
        highlighter.discover_injections();

        highlighter
    }

    /// Return the current document text.
    pub fn text(&self) -> &str {
        &self.text
    }

    pub(crate) fn fence_changed_source_range(&self) -> Option<&Range<usize>> {
        match &self.fence_edit_status {
            FenceEditStatus::Incremental { changed_source } => changed_source.as_ref(),
            FenceEditStatus::NotIncremental => None,
        }
    }

    pub(crate) fn fence_edit_is_incremental(&self) -> bool {
        matches!(self.fence_edit_status, FenceEditStatus::Incremental { .. })
    }

    /// Source line starts, kept in sync with every edit for indexed lookups.
    pub(crate) fn line_starts(&self) -> &[usize] {
        &self.line_starts
    }

    /// Test-only evidence for source reparse propagation and injection work.
    #[cfg(test)]
    pub(crate) fn work_snapshot(&self) -> (usize, usize, usize, usize, usize, usize) {
        (
            self.block_parse_count.get(),
            self.last_changed_ranges,
            self.last_changed_bytes,
            self.injections.len(),
            self.injection_scan_count,
            self.reference_scan_count,
        )
    }

    #[cfg(test)]
    pub(crate) fn changed_lines_snapshot(&self) -> usize {
        self.last_changed_lines
    }

    #[cfg(test)]
    pub(crate) fn bounded_work_snapshot(&self) -> (usize, usize, usize) {
        (
            self.last_bounded_bytes,
            self.last_bounded_lines,
            self.patches.len(),
        )
    }

    /// Return parser-owned block ranges that cannot contain checked prose.
    ///
    /// The walk prunes nodes outside `scan`, returns only owned byte ranges,
    /// and stops below excluded leaves so parser types do not cross the seam.
    #[allow(
        dead_code,
        reason = "the Phase 3 exclusion seam is exercised directly before SpellState consumes it"
    )]
    pub(crate) fn spell_block_exclusion_ranges(&self, scan: Range<usize>) -> Vec<Range<usize>> {
        let scan = scan.start.min(self.text.len())..scan.end.min(self.text.len());
        if scan.start >= scan.end {
            return Vec::new();
        }

        let mut ranges = Vec::new();
        let mut cursor = scan.start;
        for patch in &self.patches {
            if patch.bytes.end <= cursor || patch.bytes.start >= scan.end {
                continue;
            }
            let unchanged_end = patch.bytes.start.min(scan.end);
            if cursor < unchanged_end {
                collect_spell_block_exclusions(
                    self.md_tree.root_node(),
                    &(cursor..unchanged_end),
                    &mut ranges,
                );
            }
            for exclusion in &patch.spell_exclusions {
                let start = exclusion.start.max(scan.start);
                let end = exclusion.end.min(scan.end);
                if start < end {
                    ranges.push(start..end);
                }
            }
            cursor = patch.bytes.end.min(scan.end);
        }
        if cursor < scan.end {
            collect_spell_block_exclusions(
                self.md_tree.root_node(),
                &(cursor..scan.end),
                &mut ranges,
            );
        }
        if let Some(front_matter) = &self.spell_front_matter_span {
            let start = front_matter.start.max(scan.start);
            let end = front_matter.end.min(scan.end);
            if start < end {
                ranges.push(start..end);
            }
        }
        ranges.sort_by_key(|range| (range.start, range.end));
        ranges
    }

    #[allow(
        dead_code,
        reason = "the Phase 3 exclusion scanner owns cloned definition labels across ticks"
    )]
    pub(crate) fn spell_reference_labels(&self) -> Vec<String> {
        self.spell_reference_labels.clone()
    }

    fn bounded_source_region(
        &self,
        edit: &TextEdit,
        allow_local_markers: bool,
    ) -> Option<(Range<usize>, String)> {
        if self.text.len() < BOUNDED_SOURCE_MIN_DOCUMENT_BYTES {
            return None;
        }
        if self.patches.is_empty() && edit_is_provably_block_neutral(&self.text, edit) {
            return None;
        }
        let start = edit.range.start.min(edit.range.end);
        let end = edit.range.start.max(edit.range.end);
        if start < 4
            || end > self.text.len()
            || self
                .spell_front_matter_span
                .as_ref()
                .is_some_and(|span| start <= span.end && end >= span.start)
            || (!allow_local_markers && source_edit_may_change_global_semantics(&self.text, edit))
        {
            return None;
        }
        let root = self.md_tree.root_node();
        let mut ancestor = root.descendant_for_byte_range(start, end)?;
        while ancestor.id() != root.id() {
            if ancestor.kind() == "link_reference_definition" {
                return None;
            }
            ancestor = ancestor.parent()?;
        }
        let candidate = if allow_local_markers {
            self.bounded_section_group(edit)?
        } else if let Some(patch) = self
            .patches
            .iter()
            .find(|patch| patch.bytes.start <= start && end <= patch.bytes.end)
        {
            (patch.bytes.clone(), patch.root_kind.clone())
        } else {
            let root = self.md_tree.root_node();
            let node = root.named_children(&mut root.walk()).find(|node| {
                node.start_byte() <= start && start < node.end_byte() && end <= node.end_byte()
            })?;
            if node.end_byte() - node.start_byte() <= BOUNDED_SOURCE_MAX_REGION_BYTES {
                (node.start_byte()..node.end_byte(), node.kind().to_string())
            } else {
                let mut child = node.descendant_for_byte_range(start, end)?;
                while !matches!(child.kind(), "paragraph" | "pipe_table") {
                    child = child.parent()?;
                    if child.id() == node.id() {
                        return None;
                    }
                }
                if child.start_byte() > start || end > child.end_byte() {
                    return None;
                }
                let next_line = self
                    .line_starts
                    .partition_point(|line| *line < child.end_byte());
                let span_end = self
                    .line_starts
                    .get(next_line)
                    .copied()
                    .unwrap_or(self.text.len());
                (child.start_byte()..span_end, child.kind().to_string())
            }
        };
        let span = &candidate.0;
        if span.end - span.start > BOUNDED_SOURCE_MAX_REGION_BYTES
            || self.line_starts.binary_search(&span.start).is_err()
            || (span.end != self.text.len() && self.line_starts.binary_search(&span.end).is_err())
            || self.patches.iter().any(|patch| {
                patch.bytes != *span && patch.bytes.start < span.end && patch.bytes.end > span.start
            })
        {
            return None;
        }
        Some(candidate)
    }

    fn bounded_section_group(&self, edit: &TextEdit) -> Option<(Range<usize>, String)> {
        let start = edit.range.start;
        let end = edit.range.end;
        if let Some(patch) = self
            .patches
            .iter()
            .find(|patch| patch.bytes.start <= start && end <= patch.bytes.end)
        {
            if patch.root_kind == "section_group" {
                return Some((patch.bytes.clone(), patch.root_kind.clone()));
            }
        }
        let root = self.md_tree.root_node();
        let mut first = root.descendant_for_byte_range(start, start + 1)?;
        while first.kind() != "section" {
            first = first.parent()?;
        }
        let mut last = root.descendant_for_byte_range(end.saturating_sub(1), end)?;
        while last.kind() != "section" {
            last = last.parent()?;
        }
        let parent = first.parent()?;
        if parent.kind() != "section" || last.parent()?.id() != parent.id() {
            return None;
        }
        let span = first.start_byte()..last.end_byte();
        (span.start <= start && end <= span.end).then_some((span, "section_group".to_string()))
    }

    fn edit_stays_inside_local_section(&self, edit: &TextEdit) -> bool {
        let start = edit.range.start;
        let end = edit.range.end;
        if start > end || end > self.text.len() || (start == end && edit.new_text.is_empty()) {
            return false;
        }
        let line_start = self.text[..start].rfind('\n').map_or(0, |at| at + 1);
        let line_end = self.text[end..]
            .find('\n')
            .map_or(self.text.len(), |at| end + at);
        let old = &self.text[line_start..line_end];
        let mut new = String::with_capacity(old.len() - (end - start) + edit.new_text.len());
        new.push_str(&self.text[line_start..start]);
        new.push_str(&edit.new_text);
        new.push_str(&self.text[end..line_end]);
        old.lines().chain(new.lines()).all(|line| {
            let line = line.trim_start_matches([' ', '\t']);
            !(line.starts_with("# ") || line.starts_with("#\t"))
                && !line.starts_with('<')
                && !line.starts_with("===")
                && !line.starts_with("```")
                && !line.starts_with("~~~")
                && !line.starts_with("+++")
                && !line.contains("]:")
                && !line.contains("[^")
        })
    }

    pub(crate) fn local_section_window(&self, edit: &TextEdit) -> Option<Range<usize>> {
        self.edit_stays_inside_local_section(edit)
            .then(|| self.bounded_source_region(edit, true))
            .flatten()
            .and_then(|(span, kind)| (kind == "section_group").then_some(span))
    }

    fn apply_bounded_source_edit(&mut self, edit: &TextEdit, local_section: bool) -> bool {
        let Some((old_span, root_kind)) = self.bounded_source_region(edit, local_section) else {
            return false;
        };
        if local_section && root_kind != "section_group" {
            return false;
        }
        let Some(tree_edit) = input_edit_indexed(&self.line_starts, edit) else {
            return false;
        };
        let start = edit.range.start.min(edit.range.end);
        let end = edit.range.start.max(edit.range.end);
        let delta = edit.new_text_len as isize - (end - start) as isize;
        let line_delta = edit.new_text.bytes().filter(|byte| *byte == b'\n').count() as isize
            - self.text[start..end]
                .bytes()
                .filter(|byte| *byte == b'\n')
                .count() as isize;
        let first_line = self.line_starts.binary_search(&old_span.start).unwrap();
        self.md_tree.edit(&tree_edit);
        apply_edit_to_line_starts(&mut self.line_starts, edit);
        apply_edit_to_string(&mut self.text, edit);
        let new_span = old_span.start..old_span.end.checked_add_signed(delta).unwrap();
        let mut fragment = Self::new_with_queries(
            &self.text[new_span.clone()],
            self.query_cache.borrow().clone(),
        );
        let fragment_root = fragment.md_tree.root_node();
        let mut fragment_cursor = fragment_root.walk();
        let mut fragment_children = fragment_root.named_children(&mut fragment_cursor);
        let only_root = fragment_children.next();
        let same_root = if root_kind == "section_group" {
            only_root.is_some_and(|node| node.kind() == "section")
                && fragment_children.all(|node| node.kind() == "section")
        } else if let Some(node) = only_root {
            if fragment_children.next().is_some() {
                false
            } else if root_kind == "section" {
                node.kind() == "section"
            } else {
                let mut nested_cursor = node.walk();
                let mut nested = node.named_children(&mut nested_cursor);
                nested.next().is_some_and(|child| child.kind() == root_kind)
                    && nested.next().is_none()
            }
        } else {
            false
        };
        if !same_root {
            self.reparse_after_bounded_expansion();
            return true;
        }
        let fragment_lines = fragment.highlight_lines(0..fragment.line_starts.len());
        let spell_exclusions = fragment
            .spell_block_exclusion_ranges(0..fragment.text.len())
            .into_iter()
            .map(|range| new_span.start + range.start..new_span.start + range.end)
            .collect();
        self.query_cache.get_mut().extend(
            fragment
                .query_cache
                .borrow()
                .iter()
                .map(|(name, query)| (*name, Arc::clone(query))),
        );
        #[cfg(test)]
        self.query_compile_count
            .set(self.query_compile_count.get() + fragment.query_compile_count.get());
        let mut injections = Vec::new();
        for mut injection in std::mem::take(&mut self.injections) {
            if injection.start < old_span.end && injection.end > old_span.start {
                continue;
            }
            if injection.start >= old_span.end {
                injection.start = injection.start.checked_add_signed(delta).unwrap();
                injection.end = injection.end.checked_add_signed(delta).unwrap();
            }
            injections.push(injection);
        }
        for mut injection in std::mem::take(&mut fragment.injections) {
            injection.start += new_span.start;
            injection.end += new_span.start;
            injections.push(injection);
        }
        injections.sort_by_key(|injection| injection.start);
        self.injections = injections;
        self.source_parse_cache.get_mut().retain_mut(|entry| {
            if entry.start < old_span.end && entry.end > old_span.start {
                return false;
            }
            if entry.start >= old_span.end {
                entry.start = entry.start.checked_add_signed(delta).unwrap();
                entry.end = entry.end.checked_add_signed(delta).unwrap();
            }
            true
        });
        let mut patches = Vec::with_capacity(self.patches.len() + 1);
        for mut patch in std::mem::take(&mut self.patches) {
            if patch.bytes == old_span {
                continue;
            }
            if patch.bytes.start >= old_span.end {
                patch.bytes.start = patch.bytes.start.checked_add_signed(delta).unwrap();
                patch.bytes.end = patch.bytes.end.checked_add_signed(delta).unwrap();
                patch.first_line = patch.first_line.checked_add_signed(line_delta).unwrap();
                for exclusion in &mut patch.spell_exclusions {
                    exclusion.start = exclusion.start.checked_add_signed(delta).unwrap();
                    exclusion.end = exclusion.end.checked_add_signed(delta).unwrap();
                }
            }
            patches.push(patch);
        }
        patches.push(SourcePatch {
            bytes: new_span.clone(),
            root_kind,
            first_line,
            lines: fragment_lines,
            spell_exclusions,
        });
        patches.sort_by_key(|patch| patch.bytes.start);
        self.patches = patches;
        self.spell_front_matter_span = crate::frontmatter::front_matter_span(&self.text);
        #[cfg(test)]
        {
            self.last_parse_path = ParsePath::Bounded;
            self.last_bounded_bytes = new_span.end - new_span.start;
            self.last_bounded_lines = self
                .patches
                .iter()
                .find(|patch| patch.bytes == new_span)
                .unwrap()
                .lines
                .len();
            self.last_changed_ranges = 0;
            self.last_changed_bytes = 0;
            self.last_changed_lines = 0;
        }
        true
    }

    fn reparse_after_bounded_expansion(&mut self) {
        self.md_tree = self
            .md_parser
            .parse(&self.text, Some(&self.md_tree))
            .expect("re-parse should succeed");
        self.patches.clear();
        self.spell_front_matter_span = crate::frontmatter::front_matter_span(&self.text);
        self.spell_reference_labels =
            collect_spell_reference_labels(self.md_tree.root_node(), &self.text);
        self.source_parse_cache.get_mut().clear();
        self.discover_injections();
        #[cfg(test)]
        {
            self.last_parse_path = ParsePath::Incremental;
            self.block_parse_count.set(self.block_parse_count.get() + 1);
            self.reference_scan_count += 1;
            self.last_bounded_bytes = 0;
            self.last_bounded_lines = 0;
            self.last_changed_ranges = 1;
            self.last_changed_bytes = self.text.len();
            self.last_changed_lines = self.line_starts.len();
        }
    }

    /// Preserve parsed injections outside a block-neutral edit and keep their
    /// document coordinates current. An edit on an injection boundary needs
    /// the Markdown tree to decide which side owns the inserted text.
    fn rebase_unchanged_injections(&mut self, edit: &TextEdit) -> bool {
        let start = edit.range.start.min(edit.range.end);
        let end = edit.range.start.max(edit.range.end);
        if self.injections.iter().any(|injection| {
            (start <= injection.start && end > injection.start)
                || (start < injection.end && end >= injection.end)
                || (start == end && (start == injection.start || start == injection.end))
        }) {
            return false;
        }
        let delta = edit.new_text_len as isize - (end - start) as isize;
        for injection in &mut self.injections {
            if end <= injection.start {
                injection.start = injection.start.checked_add_signed(delta).unwrap();
                injection.end = injection.end.checked_add_signed(delta).unwrap();
            } else if start < injection.end && end > injection.start {
                injection.end = injection.end.checked_add_signed(delta).unwrap();
            }
        }
        self.source_parse_cache.get_mut().retain_mut(|entry| {
            if end <= entry.start {
                entry.start = entry.start.checked_add_signed(delta).unwrap();
                entry.end = entry.end.checked_add_signed(delta).unwrap();
                true
            } else {
                start >= entry.end
            }
        });
        true
    }

    fn edit_touches_reference_definition(&self, edit: &TextEdit) -> bool {
        let start = edit.range.start.min(edit.range.end);
        let end = edit.range.start.max(edit.range.end);
        let Some(mut node) = self
            .md_tree
            .root_node()
            .descendant_for_byte_range(start, end)
        else {
            return false;
        };
        loop {
            if node.kind() == "link_reference_definition" {
                return true;
            }
            let Some(parent) = node.parent() else {
                return false;
            };
            node = parent;
        }
    }

    /// Reparse only a known code injection when an edit cannot change its
    /// surrounding Markdown fence structure.
    fn apply_interior_fence_edit(&mut self, edit: &TextEdit) -> bool {
        let start = edit.range.start;
        let end = edit.range.end;
        if start > end
            || end > self.text.len()
            || edit.new_text_len != edit.new_text.len()
            || !self.patches.is_empty()
            || !self.text.is_char_boundary(start)
            || !self.text.is_char_boundary(end)
        {
            return false;
        }
        let affected_start = self.text[..start].rfind('\n').map_or(0, |at| at + 1);
        let affected_end = self.text[end..]
            .find('\n')
            .map_or(self.text.len(), |at| end + at);
        let replacement = format!(
            "{}{}{}",
            &self.text[affected_start..start],
            edit.new_text,
            &self.text[end..affected_end]
        );
        if self.text[affected_start..affected_end]
            .lines()
            .any(fence_delimiter_prefix)
            || replacement.lines().any(fence_delimiter_prefix)
        {
            return false;
        }
        let Some(index) = self.injections.iter().position(|injection| {
            injection.kind == InjectionKind::Fence
                && injection.start <= start
                && end < injection.end
        }) else {
            return false;
        };
        let injection = &self.injections[index];
        let old_start = injection.start;
        let old_end = injection.end;
        let language = injection.language.clone();
        let Some(entry_index) = self
            .source_parse_cache
            .borrow()
            .iter()
            .position(|entry| entry.start == old_start && entry.end == old_end)
        else {
            return false;
        };
        let old_code = &self.text[old_start..old_end];
        let relative = TextEdit {
            range: start - old_start..end - old_start,
            new_text_len: edit.new_text_len,
            new_text: edit.new_text.clone(),
        };
        let Some(code_input) = input_edit_indexed(&line_start_indices(old_code), &relative) else {
            return false;
        };
        let Some(document_input) = input_edit_indexed(&self.line_starts, edit) else {
            return false;
        };
        let delta = edit.new_text_len as isize - (end - start) as isize;
        let Some(new_end) = old_end.checked_add_signed(delta) else {
            return false;
        };

        let mut code_tree = self.source_parse_cache.get_mut()[entry_index].tree.clone();
        code_tree.edit(&code_input);
        self.md_tree.edit(&document_input);
        apply_edit_to_line_starts(&mut self.line_starts, edit);
        apply_edit_to_string(&mut self.text, edit);
        let mut parser = Parser::new();
        parser.set_language(&language).unwrap();
        let updated_code_tree = parser
            .parse(&self.text[old_start..new_end], Some(&code_tree))
            .expect("the known code grammar parses after a local edit");
        self.fence_edit_status = FenceEditStatus::Incremental {
            changed_source: code_tree
                .changed_ranges(&updated_code_tree)
                .map(|range| old_start + range.start_byte..old_start + range.end_byte)
                .reduce(|first, next| first.start.min(next.start)..first.end.max(next.end)),
        };
        self.injections[index].end = new_end;
        for injection in &mut self.injections[index + 1..] {
            injection.start = injection.start.checked_add_signed(delta).unwrap();
            injection.end = injection.end.checked_add_signed(delta).unwrap();
        }
        for entry in self.source_parse_cache.get_mut() {
            if entry.start == old_start && entry.end == old_end {
                entry.end = new_end;
                entry.tree = updated_code_tree.clone();
            } else if entry.start >= old_end {
                entry.start = entry.start.checked_add_signed(delta).unwrap();
                entry.end = entry.end.checked_add_signed(delta).unwrap();
            }
        }
        #[cfg(test)]
        {
            self.last_parse_path = ParsePath::FenceInterior;
            self.last_changed_ranges = code_tree.changed_ranges(&updated_code_tree).count();
            self.last_changed_bytes = edit.new_text_len + end - start;
            self.last_changed_lines = code_input
                .old_end_position
                .row
                .max(code_input.new_end_position.row)
                .saturating_sub(code_input.start_position.row)
                + 1;
            self.last_bounded_bytes = 0;
            self.last_bounded_lines = 0;
        }
        true
    }

    /// Apply a batch of text edits and update the retained parse tree.
    ///
    /// This is the FR-4.4 incremental highlighting path: affected trees are
    /// re-parsed unless every edit is proven to be an ASCII word-only change
    /// that cannot alter Markdown block structure. That narrow exception uses
    /// `Tree::edit` for coordinates and the live inline parse for styling.
    ///
    /// Edits are applied in their incoming sequential order. Injection
    /// regions are retained for block-neutral edits and rediscovered when
    /// Markdown structure may have changed.
    pub fn apply_edit(&mut self, edits: &[TextEdit]) {
        if edits.is_empty() {
            return;
        }

        self.fence_edit_status = FenceEditStatus::NotIncremental;

        if edits.len() == 1 && self.apply_interior_fence_edit(&edits[0]) {
            return;
        }
        if edits.len() == 1 && self.apply_bounded_source_edit(&edits[0], false) {
            return;
        }
        if let Some(collapsed) = collapsed_descending_edit(&self.text, edits) {
            if self.local_section_window(&collapsed).is_some()
                && self.apply_bounded_source_edit(&collapsed, true)
            {
                return;
            }
        }

        #[cfg(test)]
        {
            self.last_parse_path = ParsePath::Skipped;
            self.last_changed_bytes = 0;
            self.last_changed_ranges = 0;
            self.last_changed_lines = 0;
            self.last_bounded_bytes = 0;
            self.last_bounded_lines = 0;
        }

        let mut rediscover_injections = false;
        let mut tree_was_edited = false;
        let mut block_reparse_required = !self.patches.is_empty();
        for edit in edits {
            // Every coordinate is relative to the document produced by the
            // preceding entry, so compute and apply the tree edit before
            // applying the identical replacement to the working text.
            let block_neutral = edit_is_provably_block_neutral(&self.text, edit)
                && !self.edit_touches_reference_definition(edit);
            block_reparse_required |= !block_neutral;
            if !block_neutral
                || edit_may_create_injection(&self.text, edit)
                || (!rediscover_injections && !self.rebase_unchanged_injections(edit))
            {
                rediscover_injections = true;
            }
            if let Some(tree_edit) = input_edit_indexed(&self.line_starts, edit) {
                self.md_tree.edit(&tree_edit);
                tree_was_edited = true;
            }
            apply_edit_to_line_starts(&mut self.line_starts, edit);
            apply_edit_to_string(&mut self.text, edit);
        }
        self.spell_front_matter_span = crate::frontmatter::front_matter_span(&self.text);

        if !tree_was_edited {
            return;
        }

        self.patches.clear();

        if block_reparse_required {
            #[cfg(test)]
            let edited_tree = self.md_tree.clone();
            let old_tree = Some(&self.md_tree);
            self.md_tree = self
                .md_parser
                .parse(&self.text, old_tree)
                .expect("re-parse should succeed");
            #[cfg(test)]
            {
                self.block_parse_count
                    .set(self.block_parse_count.get().saturating_add(1));
                for changed in edited_tree.changed_ranges(&self.md_tree) {
                    self.last_changed_ranges += 1;
                    self.last_changed_bytes += changed.end_byte - changed.start_byte;
                    self.last_changed_lines += changed
                        .end_point
                        .row
                        .saturating_sub(changed.start_point.row)
                        + 1;
                }
            }
            #[cfg(test)]
            {
                self.last_parse_path = ParsePath::Incremental;
            }
        } else {
            #[cfg(test)]
            {
                self.last_parse_path = ParsePath::BlockNeutral;
            }
        }
        if block_reparse_required {
            self.spell_reference_labels =
                collect_spell_reference_labels(self.md_tree.root_node(), &self.text);
            #[cfg(test)]
            {
                self.reference_scan_count += 1;
            }
        }

        if rediscover_injections {
            self.source_parse_cache.get_mut().clear();
            self.discover_injections();
        }
    }

    /// Highlight the given line range and return styled lines.
    ///
    /// This is the primary rendering API: it always returns fully-styled
    /// lines for the requested viewport (FR-4.4). The visible viewport is
    /// never rendered un-highlighted while a re-parse completes.
    ///
    /// `lines` is a 0-based inclusive range of line indices.
    pub fn highlight_lines(&self, lines: Range<usize>) -> Vec<StyledLine> {
        if self.patches.is_empty() {
            return self.highlight_lines_tree(lines);
        }
        let line_count = if self.text.ends_with('\n') && !self.text.is_empty() {
            self.line_starts.len().saturating_sub(1)
        } else {
            self.line_starts.len()
        };
        let end = lines.end.min(line_count);
        if lines.start >= end {
            return self.highlight_lines_tree(lines);
        }
        let mut result = Vec::with_capacity(end - lines.start);
        let mut cursor = lines.start;
        while cursor < end {
            let index = self
                .patches
                .partition_point(|patch| patch.first_line + patch.lines.len() <= cursor);
            match self.patches.get(index) {
                Some(patch) if patch.first_line <= cursor => {
                    let count = (end - cursor).min(patch.first_line + patch.lines.len() - cursor);
                    result.extend_from_slice(
                        &patch.lines[cursor - patch.first_line..cursor - patch.first_line + count],
                    );
                    cursor += count;
                }
                next => {
                    let next_line = next.map_or(end, |patch| patch.first_line.min(end));
                    result.extend(self.highlight_lines_tree(cursor..next_line));
                    cursor = next_line;
                }
            }
        }
        result
    }

    fn highlight_lines_tree(&self, lines: Range<usize>) -> Vec<StyledLine> {
        if lines.start >= lines.end {
            return Vec::new();
        }

        let text = &self.text;
        let line_starts = &self.line_starts;

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
        cursor.set_byte_range(range.clone());
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
            if let Some(inj_tree) = self.source_injection_tree(injection) {
                let inj_root = inj_tree.root_node();
                let relative_start = range.start.saturating_sub(inj_start);
                let relative_end = range.end.saturating_sub(inj_start).min(inj_end - inj_start);
                inj_cursor.set_byte_range(relative_start..relative_end);
                let injection_bytes = &text.as_bytes()[inj_start..inj_end];
                let mut matches = inj_cursor.matches(&injection.query, inj_root, injection_bytes);
                let mut injection_spans = Vec::new();

                while let Some(m) = matches.next() {
                    for capture in m.captures {
                        let capture_name = injection.query.capture_names()[capture.index as usize];
                        let inj_node = capture.node;
                        if injection.kind == InjectionKind::FrontMatter
                            && injection.language_name == "toml"
                            && capture_name
                                .strip_prefix('@')
                                .unwrap_or(capture_name)
                                .starts_with("property")
                            && inj_node.kind() == "pair"
                        {
                            // tree-sitter-toml-ng captures the complete
                            // pair as @property in addition to its key.
                            // Keeping that broad capture would flatten the
                            // value into FmKey after overlap resolution.
                            continue;
                        }
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

            // Parse ordinary inline nodes and table cells with the inline
            // grammar. The block grammar exposes table-cell payload directly
            // rather than wrapping it in an `inline` node.
            if kind == "inline" || kind == "pipe_table_cell" {
                // Inline constructs can cross physical-line and viewport
                // boundaries. Parse the complete containing node for grammar
                // context, then constrain query work and emitted captures to
                // the requested viewport.
                let inline_start = child.start_byte();
                let inline_end = child.end_byte();
                let inline_text = &text[inline_start..inline_end];
                let inline_bytes = inline_text.as_bytes();

                let mut inline_parser = Parser::new();
                let inline_language: tree_sitter::Language = tree_sitter_md::INLINE_LANGUAGE.into();
                if inline_parser.set_language(&inline_language).is_ok() {
                    if let Some(inline_tree) = inline_parser.parse(inline_bytes, None) {
                        let mut inline_cursor = QueryCursor::new();
                        inline_cursor.set_byte_range(
                            range.start.saturating_sub(inline_start)
                                ..range
                                    .end
                                    .saturating_sub(inline_start)
                                    .min(inline_bytes.len()),
                        );
                        let mut matches = inline_cursor.matches(
                            &self.md_inline_query,
                            inline_tree.root_node(),
                            inline_bytes,
                        );

                        let base_offset = inline_start;
                        while let Some(m) = matches.next() {
                            for capture in m.captures {
                                let capture_name =
                                    self.md_inline_query.capture_names()[capture.index as usize];
                                let style = md_capture_to_style(capture_name);
                                let inline_node = capture.node;

                                let abs_start = base_offset + inline_node.start_byte();
                                let abs_end = base_offset + inline_node.end_byte();

                                if abs_start < abs_end
                                    && abs_end > range.start
                                    && abs_start < range.end
                                {
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
        #[cfg(test)]
        {
            self.injection_scan_count += 1;
        }
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
            let query = self
                .query_for(meta.language)
                .expect("registered injection highlight query should compile");

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

    /// Return one query per canonical language for source and rendered fences.
    fn query_for(&self, language: &'static LangDef) -> Option<Arc<Query>> {
        if let Some(cached) = self.query_cache.borrow().get(language.name) {
            return Some(Arc::clone(cached));
        }
        let compiled =
            Arc::new(Query::new(&(language.language_fn)(), language.highlights_query).ok()?);
        self.query_cache
            .borrow_mut()
            .insert(language.name, Arc::clone(&compiled));
        #[cfg(test)]
        self.query_compile_count
            .set(self.query_compile_count.get() + 1);
        Some(compiled)
    }

    /// Parse source injections on demand, retaining only a small bounded set.
    fn source_injection_tree(&self, injection: &Injection) -> Option<Tree> {
        let mut cache = self.source_parse_cache.borrow_mut();
        if let Some(index) = cache.iter().position(|entry| {
            entry.start == injection.start
                && entry.end == injection.end
                && entry.language_name == injection.language_name
        }) {
            let entry = cache.remove(index);
            let tree = entry.tree.clone();
            cache.push(entry);
            return Some(tree);
        }
        drop(cache);

        let mut parser = Parser::new();
        parser.set_language(&injection.language).ok()?;
        let tree = parser.parse(&self.text[injection.start..injection.end], None)?;
        #[cfg(test)]
        self.source_injection_parse_count
            .set(self.source_injection_parse_count.get() + 1);

        let source_bytes = injection.end - injection.start;
        if source_bytes <= SOURCE_PARSE_CACHE_MAX_SOURCE_BYTES {
            let mut cache = self.source_parse_cache.borrow_mut();
            let mut cached_bytes: usize = cache.iter().map(|entry| entry.end - entry.start).sum();
            while !cache.is_empty()
                && (cache.len() >= SOURCE_PARSE_CACHE_MAX_ENTRIES
                    || cached_bytes + source_bytes > SOURCE_PARSE_CACHE_MAX_SOURCE_BYTES)
            {
                let evicted = cache.remove(0);
                cached_bytes -= evicted.end - evicted.start;
            }
            cache.push(ParsedInjection {
                start: injection.start,
                end: injection.end,
                language_name: injection.language_name,
                tree: tree.clone(),
            });
        }
        Some(tree)
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

        // Inline highlighting is complete in the markdown block-query
        // captures; inline nodes do not require a separate injection pass.
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

#[allow(
    dead_code,
    reason = "helper for the Phase 3 exclusion seam exercised by its focused tests"
)]
fn collect_spell_block_exclusions(
    node: tree_sitter::Node<'_>,
    scan: &Range<usize>,
    ranges: &mut Vec<Range<usize>>,
) {
    if node.end_byte() <= scan.start || node.start_byte() >= scan.end {
        return;
    }
    if matches!(
        node.kind(),
        "fenced_code_block"
            | "indented_code_block"
            | "html_block"
            | "minus_metadata"
            | "plus_metadata"
            | "link_reference_definition"
    ) {
        let start = node.start_byte().max(scan.start);
        let end = node.end_byte().min(scan.end);
        if start < end {
            ranges.push(start..end);
        }
        return;
    }

    let child_count = node.child_count();
    let mut low = 0;
    let mut high = child_count;
    while low < high {
        let middle = low + (high - low) / 2;
        let child = node
            .child(middle.try_into().expect("child index must fit u32"))
            .expect("child index must be valid");
        if child.end_byte() <= scan.start {
            low = middle + 1;
        } else {
            high = middle;
        }
    }
    for index in low..child_count {
        let child = node
            .child(index.try_into().expect("child index must fit u32"))
            .expect("child index must be valid");
        if child.start_byte() >= scan.end {
            break;
        }
        collect_spell_block_exclusions(child, scan, ranges);
    }
}

fn collect_spell_reference_labels(root: tree_sitter::Node<'_>, text: &str) -> Vec<String> {
    fn walk(node: tree_sitter::Node<'_>, text: &str, labels: &mut Vec<String>) {
        if node.kind() == "link_reference_definition" {
            if let Some(label) = find_descendant_kind(node, "link_label") {
                if let Some(normalized) = crate::spell::normalize_reference_label(
                    &text[label.start_byte()..label.end_byte()],
                ) {
                    labels.push(normalized);
                }
            }
            return;
        }
        let mut cursor = node.walk();
        if cursor.goto_first_child() {
            loop {
                walk(cursor.node(), text, labels);
                if !cursor.goto_next_sibling() {
                    break;
                }
            }
        }
    }

    fn find_descendant_kind<'tree>(
        node: tree_sitter::Node<'tree>,
        expected: &str,
    ) -> Option<tree_sitter::Node<'tree>> {
        if node.kind() == expected {
            return Some(node);
        }
        let mut cursor = node.walk();
        if cursor.goto_first_child() {
            loop {
                if let Some(found) = find_descendant_kind(cursor.node(), expected) {
                    return Some(found);
                }
                if !cursor.goto_next_sibling() {
                    break;
                }
            }
        }
        None
    }

    let mut labels = Vec::new();
    walk(root, text, &mut labels);
    labels.sort_unstable();
    labels.dedup();
    labels
}

// ── Standalone snippet highlighting ─────────────────────────────────────────

fn fence_delimiter_prefix(line: &str) -> bool {
    let line = line.trim_start_matches([' ', '\t']);
    line.starts_with("```") || line.starts_with("~~~")
}

impl Highlighter {
    /// Reuse the source injection tree when its exact content span is also a
    /// rendered code fence, keeping both views on the same parsed code.
    pub(crate) fn highlight_fence_span(
        &self,
        lang: &str,
        content_span: &Range<usize>,
    ) -> Vec<StyledLine> {
        let Some(text) = self.text.get(content_span.clone()) else {
            return empty_snippet_line();
        };
        let injection = self.injections.iter().find(|injection| {
            injection.kind == InjectionKind::Fence
                && injection.start <= content_span.start
                && content_span.end <= injection.end
        });
        if let Some(injection) = injection {
            if let Some(tree) = self.source_injection_tree(injection) {
                if injection.start == content_span.start
                    && self.text[content_span.end..injection.end]
                        .bytes()
                        .all(|byte| byte == b'\n')
                {
                    if let Some(language) = languages::find_by_alias(lang) {
                        if let Some(query) = self.query_for(language) {
                            let injection_text = &self.text[injection.start..injection.end];
                            let mut spans =
                                collect_snippet_spans_from_tree(injection_text, &tree, &query);
                            spans.retain(|span| span.start_byte < text.len());
                            for span in &mut spans {
                                span.end_byte = span.end_byte.min(text.len());
                            }
                            return highlight_snippet_with_spans(text, spans);
                        }
                    }
                }
            }
        }
        self.highlight_snippet(lang, text)
    }

    /// Highlight a rendered fence with the same compiled query used by source injections.
    pub(crate) fn highlight_snippet(&self, lang: &str, text: &str) -> Vec<StyledLine> {
        if text.is_empty() {
            return empty_snippet_line();
        }
        let Some(language) = languages::find_by_alias(lang) else {
            return code_block_lines_for_text(text);
        };
        let Some(query) = self.query_for(language) else {
            return code_block_lines_for_text(text);
        };
        highlight_snippet_with_query(text, &(language.language_fn)(), &query)
    }
}

/// Uncached reference used to compare rendered output with the prior path.
#[cfg(test)]
fn highlight_snippet(lang: &str, text: &str) -> Vec<StyledLine> {
    if text.is_empty() {
        return empty_snippet_line();
    }
    let Some(lang_def) = languages::find_by_alias(lang) else {
        return code_block_lines_for_text(text);
    };
    let lang_obj = (lang_def.language_fn)();
    let query = Query::new(&lang_obj, lang_def.highlights_query).ok();
    let Some(query) = query else {
        return code_block_lines_for_text(text);
    };
    highlight_snippet_with_query(text, &lang_obj, &query)
}

fn empty_snippet_line() -> Vec<StyledLine> {
    vec![StyledLine {
        text: String::new(),
        spans: Vec::new(),
    }]
}

/// Parse snippets independently even when their compiled query is shared.
fn highlight_snippet_with_query(
    text: &str,
    lang_obj: &tree_sitter::Language,
    query: &Query,
) -> Vec<StyledLine> {
    let Some(all_spans) = collect_snippet_spans(text, lang_obj, query) else {
        return code_block_lines_for_text(text);
    };

    highlight_snippet_with_spans(text, all_spans)
}

fn highlight_snippet_with_spans(text: &str, all_spans: Vec<ByteSpan>) -> Vec<StyledLine> {
    // Sweep each capture into only the physical lines it intersects.
    let line_starts = line_start_indices(text);
    let num_lines = if text.ends_with('\n') && !text.is_empty() {
        line_starts.len().saturating_sub(1)
    } else {
        line_starts.len()
    };
    let (mut grouped_spans, _) = group_snippet_spans(text, &line_starts, num_lines, &all_spans);

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

        let mut spans = merge_overlapping_spans(std::mem::take(&mut grouped_spans[line_idx]));
        spans.sort_by_key(|span| span.start_col);

        result.push(StyledLine {
            text: line_text.to_string(),
            spans,
        });
    }

    result
}

fn collect_snippet_spans(
    text: &str,
    lang_obj: &tree_sitter::Language,
    query: &Query,
) -> Option<Vec<ByteSpan>> {
    let mut parser = Parser::new();
    if parser.set_language(lang_obj).is_err() {
        return None;
    }
    let tree = parser.parse(text, None)?;

    Some(collect_snippet_spans_from_tree(text, &tree, query))
}

fn collect_snippet_spans_from_tree(text: &str, tree: &Tree, query: &Query) -> Vec<ByteSpan> {
    let text_bytes = text.as_bytes();
    let root = tree.root_node();
    let mut cursor = QueryCursor::new();

    // Seed the low-priority fallback before collecting granular captures.
    let mut all_spans = vec![ByteSpan {
        start_byte: 0,
        end_byte: text.len(),
        style: SemanticStyle::CodeBlock,
    }];
    let mut matches = cursor.matches(query, root, text_bytes);

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

    all_spans
}

/// Preserve capture priority while visiting only intersected lines.
fn group_snippet_spans(
    text: &str,
    line_starts: &[usize],
    num_lines: usize,
    all_spans: &[ByteSpan],
) -> (Vec<Vec<RankedSpan>>, usize) {
    let mut grouped = vec![Vec::new(); num_lines];
    let mut visits = 0;
    for (priority, span) in all_spans.iter().enumerate() {
        let first = line_starts
            .partition_point(|start| *start <= span.start_byte)
            .saturating_sub(1)
            .min(num_lines);
        let end = line_starts
            .partition_point(|start| *start < span.end_byte)
            .min(num_lines);
        for line_idx in first..end {
            visits += 1;
            let line_start = line_starts[line_idx];
            let next_start = line_starts.get(line_idx + 1).copied().unwrap_or(text.len());
            let line_end = if next_start > line_start && text.as_bytes()[next_start - 1] == b'\n' {
                next_start - 1
            } else {
                next_start
            };
            if span.start_byte >= line_end || span.end_byte <= line_start {
                continue;
            }
            let line_text = &text[line_start..line_end];
            grouped[line_idx].push(RankedSpan {
                span: Span {
                    start_col: byte_offset_to_char_index(
                        line_text,
                        span.start_byte.max(line_start) - line_start,
                    ),
                    end_col: byte_offset_to_char_index(
                        line_text,
                        span.end_byte.min(line_end) - line_start,
                    ),
                    style: span.style,
                },
                priority,
            });
        }
    }
    (grouped, visits)
}

/// Split fallback text into lines with an explicit `CodeBlock` base style.
fn code_block_lines_for_text(text: &str) -> Vec<StyledLine> {
    text.lines()
        .map(|line| StyledLine {
            text: line.to_string(),
            spans: (!line.is_empty())
                .then(|| Span {
                    start_col: 0,
                    end_col: line.chars().count(),
                    style: SemanticStyle::CodeBlock,
                })
                .into_iter()
                .collect(),
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
    if let Some(style) = match name {
        "heading.1" => Some(SemanticStyle::Heading1),
        "heading.2" => Some(SemanticStyle::Heading2),
        "heading.3" => Some(SemanticStyle::Heading3),
        "heading.4" => Some(SemanticStyle::Heading4),
        "heading.5" => Some(SemanticStyle::Heading5),
        "heading.6" => Some(SemanticStyle::Heading6),
        _ => None,
    } {
        return style;
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
fn input_edit_indexed(line_starts: &[usize], edit: &TextEdit) -> Option<tree_sitter::InputEdit> {
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
    let start_position = byte_to_point_indexed(line_starts, a);

    Some(tree_sitter::InputEdit {
        start_byte: a,
        old_end_byte: b,
        new_end_byte,
        start_position,
        old_end_position: byte_to_point_indexed(line_starts, b),
        new_end_position: compute_new_end_point(start_position, &edit.new_text),
    })
}

#[cfg(test)]
fn input_edit(old_text: &str, edit: &TextEdit) -> Option<tree_sitter::InputEdit> {
    input_edit_indexed(&line_start_indices(old_text), edit)
}

/// Convert a byte offset to a tree-sitter point through cached physical-line
/// starts. Tree-sitter columns are UTF-8 byte columns, so no prefix scan is
/// required after locating the line.
fn byte_to_point_indexed(line_starts: &[usize], byte: usize) -> Point {
    let row = line_starts
        .partition_point(|line_start| *line_start <= byte)
        .saturating_sub(1);
    Point::new(row, byte.saturating_sub(line_starts[row]))
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
    #[cfg(test)]
    LINE_INDEX_BUILDS.with(|count| count.set(count.get().saturating_add(1)));
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

#[cfg(test)]
thread_local! {
    static LINE_INDEX_BUILDS: Cell<usize> = const { Cell::new(0) };
}

#[cfg(test)]
pub(crate) fn line_index_build_count() -> usize {
    LINE_INDEX_BUILDS.with(Cell::get)
}

/// A document with no existing injections only needs rediscovery when an
/// edit can form a fenced-code or front-matter delimiter in its local lines.
pub(crate) fn source_edit_may_change_global_semantics(text: &str, edit: &TextEdit) -> bool {
    fn block_prefix(line: &str) -> Option<String> {
        let trimmed = line.trim_start_matches([' ', '\t']);
        let marker = trimmed
            .bytes()
            .next()
            .filter(|byte| matches!(byte, b'`' | b'~' | b'#' | b'>' | b'-' | b'+' | b'*'))?;
        let count = trimmed.bytes().take_while(|byte| *byte == marker).count();
        if matches!(marker, b'`' | b'~') && count < 3 {
            return None;
        }
        Some(trimmed[..count].to_string())
    }

    let start = edit.range.start.min(edit.range.end);
    let end = edit.range.start.max(edit.range.end);
    let line_start = text[..start].rfind('\n').map_or(0, |index| index + 1);
    let line_end = text[end..]
        .find('\n')
        .map_or(text.len(), |index| end + index);
    let old = &text[line_start..line_end];
    let new = format!(
        "{}{}{}",
        &text[line_start..start],
        edit.new_text,
        &text[end..line_end]
    );
    if old.lines().chain(new.lines()).any(|line| {
        let trimmed = line.trim_start_matches([' ', '\t']);
        (trimmed.starts_with('[') && trimmed.contains("]:")) || trimmed.starts_with('<')
    }) {
        return true;
    }
    let old_markers = old.lines().filter_map(block_prefix).collect::<Vec<_>>();
    let new_markers = new.lines().filter_map(block_prefix).collect::<Vec<_>>();
    old_markers != new_markers
}

fn edit_may_create_injection(text: &str, edit: &TextEdit) -> bool {
    let start = edit.range.start.min(edit.range.end);
    let end = edit.range.start.max(edit.range.end);
    let window_start = text[..start].rfind('\n').map_or(0, |newline| newline + 1);
    let window_end = text[end..]
        .find('\n')
        .map_or(text.len(), |newline| end + newline + 1);
    let mut window = text[window_start..start].to_string();
    window.push_str(&edit.new_text);
    window.push_str(&text[end..window_end]);
    window.contains("```")
        || window.contains("~~~")
        || window.lines().any(|line| {
            line.trim_end_matches('\r') == "---" || line.trim_end_matches('\r') == "+++"
        })
}

/// Prove the narrow case where a block reparse cannot change Markdown block
/// structure: an ASCII word-character edit inside a non-indented line that
/// begins with an ASCII letter. At a line start, only an explicit blank or
/// metadata delimiter separates it from the preceding Markdown node;
/// otherwise `Tree::edit` can assign inserted text to that node. Other edits
/// take the canonical Tree-sitter incremental parse path. `Tree::edit` keeps byte
/// coordinates current, and inline syntax is parsed from live viewport text.
fn edit_is_provably_block_neutral(text: &str, edit: &TextEdit) -> bool {
    let start = edit.range.start.min(edit.range.end);
    let end = edit.range.start.max(edit.range.end);
    let removed = &text[start..end];
    if removed.contains(['\r', '\n']) || edit.new_text.contains(['\r', '\n']) {
        return false;
    }
    if !removed.bytes().all(|byte| byte.is_ascii_alphanumeric())
        || !edit
            .new_text
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric())
    {
        return false;
    }
    let line_start = text[..start].rfind('\n').map_or(0, |newline| newline + 1);
    let line_end = text[end..]
        .find('\n')
        .map_or(text.len(), |newline| end + newline);
    if end == line_end && start == end {
        return false;
    }
    if start == line_start {
        let previous = text[..line_start]
            .strip_suffix('\n')
            .unwrap_or("")
            .rsplit('\n')
            .next()
            .unwrap_or("")
            .trim();
        if !previous.is_empty() && !matches!(previous, "---" | "+++") {
            return false;
        }
    }
    let old_line = &text[line_start..line_end];
    let mut new_line = text[line_start..start].to_string();
    new_line.push_str(&edit.new_text);
    new_line.push_str(&text[end..line_end]);
    old_line
        .as_bytes()
        .first()
        .is_some_and(u8::is_ascii_alphabetic)
        && new_line
            .as_bytes()
            .first()
            .is_some_and(u8::is_ascii_alphabetic)
}

/// Update cached line starts for one sequential byte-range replacement.
fn apply_edit_to_line_starts(line_starts: &mut Vec<usize>, edit: &TextEdit) {
    let start = edit.range.start.min(edit.range.end);
    let end = edit.range.start.max(edit.range.end);
    let first_removed = line_starts.partition_point(|offset| *offset <= start);
    let first_after = line_starts.partition_point(|offset| *offset <= end);
    let inserted_starts: Vec<_> = edit
        .new_text
        .char_indices()
        .filter_map(|(offset, character)| (character == '\n').then_some(start + offset + 1))
        .collect();
    let shifted_from = first_removed + inserted_starts.len();
    line_starts.splice(first_removed..first_after, inserted_starts);

    let removed_len = end - start;
    let inserted_len = edit.new_text.len();
    let delta = inserted_len as i128 - removed_len as i128;
    for offset in &mut line_starts[shifted_from..] {
        *offset = usize::try_from(*offset as i128 + delta)
            .expect("line start must remain non-negative after a valid edit");
    }
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
    use crate::input::{KeyCode, KeyCodeKind, KeyInput, Modifiers};
    use crate::realistic_fixtures as fixtures;
    use crate::rendered::BlockModel;
    use crate::style::{LineKind, RenderedLayout, RenderedLineRole};
    use crate::vim::{VimCore, VimEffect};

    #[cfg(test)]
    use proptest::prelude::*;

    #[test]
    fn bounded_fence_source_prototype_handles_propagation_and_delimiter_fallback() {
        let cases = [
            (
                "rust-raw-string",
                "# Title\n\n```rust\nfn main() {\nlet s = r###\"opening\ncontinuation\n\"###;\nlet x = 1;\n}\n```\n\nTail\n",
                "let s = r###\"opening\n",
            ),
            (
                "go-comment",
                "# Title\n\n```go\npackage main\n/* opening\ncomment content\n*/\nfunc main() {}\n```\n\nTail\n",
                "/* opening\n",
            ),
            (
                "unicode-crlf",
                "# Café\r\n\r\n```rust\r\nfn café() {\r\nlet π = 3;\r\n}\r\n```\r\n\r\nTail\r\n",
                "let π = 3;\r\n",
            ),
        ];
        for (name, text, removed) in cases {
            let start = text.find(removed).unwrap();
            let end = start + removed.len();
            let mut syntax = Highlighter::new(text);
            let _ = syntax.highlight_lines(0..syntax.line_starts().len());
            assert!(
                syntax.apply_interior_fence_edit(&TextEdit {
                    range: start..end,
                    new_text_len: 0,
                    new_text: String::new(),
                }),
                "{name}"
            );
            let edited = format!("{}{}", &text[..start], &text[end..]);
            let fresh = Highlighter::new(&edited);
            let all_lines = 0..fresh.line_starts().len();
            assert_eq!(
                syntax.highlight_lines(all_lines.clone()),
                fresh.highlight_lines(all_lines),
                "{name} did not propagate exact source styles"
            );
            let closing = edited.rfind("```").unwrap();
            let delimiter_edit = TextEdit {
                range: closing..closing + 3,
                new_text_len: 0,
                new_text: String::new(),
            };
            assert!(!syntax.apply_interior_fence_edit(&delimiter_edit));
            syntax.apply_edit(&[delimiter_edit]);
            let globally_edited = format!("{}{}", &edited[..closing], &edited[closing + 3..]);
            let fresh = Highlighter::new(&globally_edited);
            let all_lines = 0..fresh.line_starts().len();
            assert_eq!(
                syntax.highlight_lines(all_lines.clone()),
                fresh.highlight_lines(all_lines),
                "{name} global fallback after local edit differs"
            );
        }
    }

    #[test]
    fn warmed_fence_edits_use_incremental_code_parse_through_public_gateway() {
        for (language, code) in [
            ("rust", "let s = r###\"open\ninside\n\"###;\nlet x = 1;\n"),
            ("go", "/* open\ninside\n*/\nfunc main() {}\n"),
        ] {
            let original = format!("# Heading\n\n```{language}\n{code}```\n\nTail.\n");
            let mut highlighter = Highlighter::new(&original);
            let _ = highlighter.highlight_lines(0..highlighter.line_starts().len());
            let start = original.find("inside\n").unwrap();
            let edit = TextEdit {
                range: start..start + "inside\n".len(),
                new_text_len: 0,
                new_text: String::new(),
            };
            highlighter.apply_edit(std::slice::from_ref(&edit));
            assert_eq!(highlighter.last_parse_path, ParsePath::FenceInterior);
            let edited = format!("{}{}", &original[..start], &original[edit.range.end..]);
            let fresh = Highlighter::new(&edited);
            assert_eq!(
                highlighter.highlight_lines(0..highlighter.line_starts().len()),
                fresh.highlight_lines(0..fresh.line_starts().len())
            );
            let restore = TextEdit {
                range: start..start,
                new_text_len: "inside\n".len(),
                new_text: "inside\n".to_string(),
            };
            highlighter.apply_edit(&[restore]);
            assert_eq!(highlighter.last_parse_path, ParsePath::FenceInterior);
            let fresh = Highlighter::new(&original);
            assert_eq!(
                highlighter.highlight_lines(0..highlighter.line_starts().len()),
                fresh.highlight_lines(0..fresh.line_starts().len())
            );
            let partial = TextEdit {
                range: start..start + 3,
                new_text_len: 2,
                new_text: "IN".to_string(),
            };
            highlighter.apply_edit(std::slice::from_ref(&partial));
            assert_eq!(highlighter.last_parse_path, ParsePath::FenceInterior);
            let partially_edited = format!(
                "{}{}{}",
                &original[..partial.range.start],
                partial.new_text,
                &original[partial.range.end..]
            );
            let fresh = Highlighter::new(&partially_edited);
            assert_eq!(
                highlighter.highlight_lines(0..highlighter.line_starts().len()),
                fresh.highlight_lines(0..fresh.line_starts().len())
            );
        }
    }

    #[test]
    fn first_and_last_code_characters_keep_the_warmed_fence_parse() {
        for (language, code) in [
            ("rust", "fn alpha() {}\nlet tail = 1;\n"),
            ("go", "func alpha() {}\nvar tail = 1;\n"),
        ] {
            let mut text = format!("# Heading\n\n```{language}\n{code}```\n\nTail.\n");
            let mut highlighter = Highlighter::new(&text);
            highlighter.highlight_lines(0..highlighter.line_starts().len());
            let first = if language == "rust" {
                "fn alpha()"
            } else {
                "func alpha()"
            };
            for (needle, replacement, at_end) in [(first, "x", false), ("tail = 1;", "!", true)] {
                let at = text.find(needle).unwrap();
                let range = if at_end {
                    at + needle.len() - 1..at + needle.len()
                } else {
                    at..at + 1
                };
                let edit = TextEdit {
                    range: range.clone(),
                    new_text_len: replacement.len(),
                    new_text: replacement.to_string(),
                };
                highlighter.apply_edit(std::slice::from_ref(&edit));
                text.replace_range(range, replacement);
                assert_eq!(highlighter.last_parse_path, ParsePath::FenceInterior);
                let fresh = Highlighter::new(&text);
                assert_eq!(
                    highlighter.highlight_lines(0..highlighter.line_starts().len()),
                    fresh.highlight_lines(0..fresh.line_starts().len())
                );
            }
        }
    }

    #[test]
    fn rendered_fence_build_primes_the_matching_source_tree() {
        let text = "# Heading\n\n```rust\nfn alpha() { let n = 1; }\n```\n\n```go\nfunc beta() { n := 2 }\n```\n";
        let highlighter = Highlighter::new(text);
        let model = BlockModel::build(text, crate::frontmatter::front_matter_span(text));
        for block in &model.blocks {
            if let crate::rendered::BlockKind::CodeFence {
                content_span, lang, ..
            } = &block.kind
            {
                assert!(highlighter.injections.iter().any(|injection| {
                    injection.start == content_span.start
                        && injection.end >= content_span.end
                        && text[content_span.end..injection.end]
                            .bytes()
                            .all(|byte| byte == b'\n')
                }));
                assert_eq!(
                    highlighter.highlight_fence_span(lang.as_deref().unwrap_or(""), content_span),
                    highlighter.highlight_snippet(
                        lang.as_deref().unwrap_or(""),
                        &text[content_span.clone()]
                    )
                );
            }
        }
        let layout = RenderedLayout::build(&model, 80, &highlighter);
        assert!(!layout.lines.is_empty());
        let cache = highlighter.source_parse_cache.borrow();
        assert_eq!(cache.len(), 2);
        for injection in &highlighter.injections {
            assert!(cache
                .iter()
                .any(|entry| { entry.start == injection.start && entry.end == injection.end }));
        }
    }

    const VIEWPORT_MARKDOWN: &str =
        "# Heading\n\nIntro paragraph.\n\nMiddle paragraph.\n\n```rust\nfn main() {}\n```\nTail paragraph.\n";

    #[test]
    #[ignore = "exact 1 MiB incremental injection diagnostic is run by its benchmark target"]
    fn acceptance_1mb_injection_incremental_profile() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/kitchen-sink-1mb.md");
        let original = std::fs::read_to_string(path).unwrap();
        for (language, first_line) in [("rust", 3000), ("go", 20000)] {
            let start = original
                .split_inclusive('\n')
                .take(first_line)
                .map(str::len)
                .sum::<usize>();
            let end = start
                + original[start..]
                    .split_inclusive('\n')
                    .take(15)
                    .map(str::len)
                    .sum::<usize>();
            let highlighter = Highlighter::new(&original);
            let injection = highlighter
                .injections
                .iter()
                .find(|injection| injection.start < start && end < injection.end)
                .unwrap();
            assert_eq!(injection.language_name, language);
            let old_content = &original[injection.start..injection.end];
            let local_start = start - injection.start;
            let local_end = end - injection.start;
            let relative_edit = TextEdit {
                range: local_start..local_end,
                new_text_len: 0,
                new_text: String::new(),
            };
            let mut edited_tree = highlighter.source_injection_tree(injection).unwrap();
            edited_tree.edit(
                &input_edit_indexed(&line_start_indices(old_content), &relative_edit).unwrap(),
            );
            let new_content = format!(
                "{}{}",
                &old_content[..local_start],
                &old_content[local_end..]
            );
            let mut parser = Parser::new();
            parser.set_language(&injection.language).unwrap();
            let started = std::time::Instant::now();
            let incremental = parser.parse(&new_content, Some(&edited_tree)).unwrap();
            let incremental_ns = started.elapsed().as_nanos();
            let changed = edited_tree.changed_ranges(&incremental).collect::<Vec<_>>();
            let started = std::time::Instant::now();
            let complete = parser.parse(&new_content, None).unwrap();
            let complete_ns = started.elapsed().as_nanos();
            assert_eq!(
                incremental.root_node().to_sexp(),
                complete.root_node().to_sexp()
            );
            println!(
                "INJECTION\t{language}\t{incremental_ns}\t{complete_ns}\t{}\t{}\t{}",
                old_content.len(),
                changed.len(),
                changed
                    .iter()
                    .map(|range| range.end_byte - range.start_byte)
                    .sum::<usize>(),
            );
        }
    }

    #[test]
    #[ignore = "exact 1 MiB bounded source feasibility diagnostic is run by its benchmark target"]
    fn acceptance_1mb_bounded_source_fence_matches_fresh() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/kitchen-sink-1mb.md");
        let original = std::fs::read_to_string(path).unwrap();
        for (language, first_line) in [("rust", 3000), ("go", 20000)] {
            let start = original
                .split_inclusive('\n')
                .take(first_line)
                .map(str::len)
                .sum::<usize>();
            let end = start
                + original[start..]
                    .split_inclusive('\n')
                    .take(15)
                    .map(str::len)
                    .sum::<usize>();
            let removed = original[start..end].to_string();
            let edit = TextEdit {
                range: start..end,
                new_text_len: 0,
                new_text: String::new(),
            };
            let mut syntax = Highlighter::new(&original);
            let _ = syntax.highlight_lines(first_line..first_line + 15);
            let started = std::time::Instant::now();
            assert!(syntax.apply_interior_fence_edit(&edit));
            let apply_ns = started.elapsed().as_nanos();
            let expected = format!("{}{}", &original[..start], &original[end..]);
            assert_eq!(syntax.text(), expected);
            let started = std::time::Instant::now();
            let affected = syntax.highlight_lines(first_line..first_line + 15);
            let highlight_ns = started.elapsed().as_nanos();
            let fresh = Highlighter::new(&expected);
            assert_eq!(syntax.line_starts(), fresh.line_starts());
            for window in [
                0..10,
                first_line.saturating_sub(5)..first_line + 20,
                first_line + 200..first_line + 210,
            ] {
                assert_eq!(
                    syntax.highlight_lines(window.clone()),
                    fresh.highlight_lines(window),
                    "{language} source styling differs"
                );
            }
            assert_eq!(affected, fresh.highlight_lines(first_line..first_line + 15));
            let restore = TextEdit {
                range: start..start,
                new_text_len: removed.len(),
                new_text: removed,
            };
            assert!(syntax.apply_interior_fence_edit(&restore));
            assert_eq!(syntax.text(), original);
            println!("SOURCE_FENCE\t{language}\t{apply_ns}\t{highlight_ns}\t15");
        }
    }

    const SOURCE_DIFFERENTIAL_CASES: &[(&str, &str)] = &[
        (
            "yaml",
            "---\ntitle: λ\n---\n\n# Heading\n\nProse *emphasis*.\n",
        ),
        ("toml", "+++\ntitle = 'é'\n+++\n\nProse **strong**.\n"),
        (
            "list",
            "- alpha\n  - nested *β*\n- second `code`\n\nTail.\n",
        ),
        ("table", "| A | B |\n|---|---|\n| λ | `é` |\n\nTail.\n"),
        (
            "rust-fence",
            "```rust\nfn main() { println!(\"é\"); }\n```\n\nTail.\n",
        ),
        ("unknown-fence", "```unknown\nraw *text*\n```\n\nTail.\n"),
        (
            "reference",
            "[ref]: /path\n\nA [link][ref] and ![image](url).\n",
        ),
        ("crlf", "# λ\r\n\r\n> Quoted &amp; escaped \\* text.\r\n"),
        (
            "html",
            "<script>\nlet value = 1;\n</script>\n\nText ~~old~~.\n",
        ),
    ];

    #[test]
    fn source_differential_manifest_covers_every_construct() {
        let names = SOURCE_DIFFERENTIAL_CASES
            .iter()
            .map(|(name, _)| *name)
            .collect::<Vec<_>>();
        assert_eq!(
            names,
            [
                "yaml",
                "toml",
                "list",
                "table",
                "rust-fence",
                "unknown-fence",
                "reference",
                "crlf",
                "html",
            ]
        );
        assert!(SOURCE_DIFFERENTIAL_CASES
            .iter()
            .all(|(_, text)| !text.is_empty()));
    }

    proptest! {
        #![proptest_config(ProptestConfig {
            cases: 64,
            rng_seed: proptest::test_runner::RngSeed::Fixed(0x0eed_5eed),
            failure_persistence: None,
            ..ProptestConfig::default()
        })]

        #[test]
        fn source_edit_and_undo_match_fresh_for_all_constructs(
            case in 0usize..SOURCE_DIFFERENTIAL_CASES.len(),
            start_choice in any::<u8>(),
            removed_chars in 0usize..3,
            replacement in prop::sample::select(vec!["", "x", "\n", "`", "**", "é", "[ref]: /new\n"]),
        ) {
            let before = SOURCE_DIFFERENTIAL_CASES[case].1;
            let boundaries = before
                .char_indices()
                .map(|(offset, _)| offset)
                .chain([before.len()])
                .collect::<Vec<_>>();
            let index = start_choice as usize % boundaries.len();
            let start = boundaries[index];
            let end = boundaries[(index + removed_chars).min(boundaries.len() - 1)];
            let replacement = if start == end && replacement.is_empty() { "x" } else { replacement };
            let removed = before[start..end].to_string();
            let mut highlighter = Highlighter::new(before);
            highlighter.apply_edit(&[TextEdit {
                range: start..end,
                new_text_len: replacement.len(),
                new_text: replacement.into(),
            }]);
            let mut current = before.to_string();
            current.replace_range(start..end, replacement);
            for reverse in [false, true] {
                if reverse {
                    highlighter.apply_edit(&[TextEdit {
                        range: start..start + replacement.len(),
                        new_text_len: removed.len(),
                        new_text: removed.clone(),
                    }]);
                    current.replace_range(start..start + replacement.len(), &removed);
                }
                let fresh = Highlighter::new(&current);
                prop_assert_eq!(highlighter.text(), current.as_str());
                prop_assert_eq!(&highlighter.line_starts, &fresh.line_starts);
                prop_assert_eq!(highlighter.highlight_lines(0..usize::MAX), fresh.highlight_lines(0..usize::MAX));
                prop_assert_eq!(&highlighter.spell_reference_labels, &fresh.spell_reference_labels);
                prop_assert_eq!(&highlighter.spell_front_matter_span, &fresh.spell_front_matter_span);
                prop_assert_eq!(
                    highlighter.spell_block_exclusion_ranges(0..current.len()),
                    fresh.spell_block_exclusion_ranges(0..current.len())
                );
                let injection_key = |injection: &Injection| {
                    (injection.start, injection.end, injection.language_name, injection.kind)
                };
                prop_assert_eq!(
                    highlighter.injections.iter().map(injection_key).collect::<Vec<_>>(),
                    fresh.injections.iter().map(injection_key).collect::<Vec<_>>()
                );
            }
        }
    }

    #[test]
    fn realistic_source_tree_exposes_bounded_root_children() {
        for class in ["mixed", "prose", "lists", "tables"] {
            let text = fixtures::generate(class, 1024 * 1024);
            let highlighter = Highlighter::new(&text);
            let root = highlighter.md_tree.root_node();
            let children = root.named_children(&mut root.walk()).collect::<Vec<_>>();
            let largest = children
                .iter()
                .max_by_key(|node| node.end_byte() - node.start_byte())
                .unwrap();
            let nested = largest
                .named_children(&mut largest.walk())
                .collect::<Vec<_>>();
            let nested_largest = nested
                .iter()
                .max_by_key(|node| node.end_byte() - node.start_byte())
                .unwrap();
            if class == "lists" {
                let items = nested_largest
                    .named_children(&mut nested_largest.walk())
                    .collect::<Vec<_>>();
                let item = items
                    .iter()
                    .max_by_key(|node| node.end_byte() - node.start_byte())
                    .unwrap();
                assert_eq!(children.len(), 1);
                assert_eq!(nested_largest.kind(), "list");
                assert!(items.len() > 1000);
                assert!(item.end_byte() - item.start_byte() <= 128);
            } else if class == "mixed" {
                assert!(children.len() > 100);
                assert!(largest.end_byte() - largest.start_byte() <= 4096);
            } else {
                assert_eq!(children.len(), 1);
                assert!(nested_largest.end_byte() - nested_largest.start_byte() <= 256);
            }
            assert!(!children.is_empty());
        }
    }

    #[test]
    fn exact_one_megabyte_prose_and_list_batches_match_fresh_source_after_undo() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/kitchen-sink-1mb.md");
        let text = std::fs::read_to_string(path).unwrap();
        assert_eq!(text.len(), 1_048_722);
        for ranges in [
            vec![18003..18518, 18524..18551, 18556..18557],
            std::iter::once(3948..4243).collect(),
        ] {
            let edits = ranges
                .into_iter()
                .rev()
                .map(|range| TextEdit {
                    range,
                    new_text_len: 0,
                    new_text: String::new(),
                })
                .collect::<Vec<_>>();
            let collapsed = collapsed_descending_edit(&text, &edits).unwrap();
            let mut source = Highlighter::new(&text);
            source.apply_edit(&edits);
            assert_eq!(source.last_parse_path, ParsePath::Bounded);
            let mut final_text = text.clone();
            for edit in &edits {
                final_text.replace_range(edit.range.clone(), &edit.new_text);
            }
            assert_eq!(source.text(), final_text);
            let fresh = Highlighter::new(&final_text);
            for lines in [0..8, 125..155, 590..625, 20_000..20_010, 24_350..24_370] {
                assert_eq!(
                    source.highlight_lines(lines.clone()),
                    fresh.highlight_lines(lines)
                );
            }
            assert_eq!(
                source.spell_block_exclusion_ranges(
                    collapsed.range.start..collapsed.range.start + collapsed.new_text_len
                ),
                fresh.spell_block_exclusion_ranges(
                    collapsed.range.start..collapsed.range.start + collapsed.new_text_len
                ),
            );
            let inverse = TextEdit {
                range: collapsed.range.start..collapsed.range.start + collapsed.new_text_len,
                new_text_len: collapsed.range.len(),
                new_text: text[collapsed.range.clone()].to_string(),
            };
            source.apply_edit(&[inverse]);
            assert_eq!(source.text(), text);
            let fresh = Highlighter::new(&text);
            for lines in [0..8, 125..155, 590..625, 20_000..20_010, 24_350..24_370] {
                assert_eq!(
                    source.highlight_lines(lines.clone()),
                    fresh.highlight_lines(lines)
                );
            }
        }
    }

    #[test]
    fn realistic_changed_section_parses_with_exact_source_styling() {
        let before = fixtures::generate("mixed", 1024 * 1024);
        for midpoint in [false, true] {
            let line = if midpoint {
                before.lines().count() / 2
            } else {
                6
            };
            let start: usize = before.split_inclusive('\n').take(line).map(str::len).sum();
            let end = start + before[start..].find('\n').unwrap() + 1;
            let old = Highlighter::new(&before);
            let root = old.md_tree.root_node();
            let section = root
                .named_children(&mut root.walk())
                .find(|node| node.start_byte() <= start && end <= node.end_byte())
                .unwrap();
            let span = section.start_byte()..section.end_byte() - (end - start);
            let after = format!("{}{}", &before[..start], &before[end..]);
            let local = Highlighter::new(&after[span.clone()]);
            let fresh = Highlighter::new(&after);
            let first_line = after[..span.start]
                .bytes()
                .filter(|byte| *byte == b'\n')
                .count();
            let count = local.text().lines().count();
            assert_eq!(
                local.highlight_lines(0..count),
                fresh.highlight_lines(first_line..first_line + count),
                "isolated section must retain exact source semantics",
            );
        }
    }

    #[test]
    fn isolated_prose_and_table_children_match_fresh_source_styling() {
        for (class, kind) in [("prose", "paragraph"), ("tables", "pipe_table")] {
            let before = fixtures::generate(class, 1024 * 1024);
            let source = Highlighter::new(&before);
            let root = source.md_tree.root_node();
            let section = root.named_child(0).unwrap();
            let child = section
                .named_children(&mut section.walk())
                .find(|node| node.kind() == kind && node.start_byte() >= before.len() / 2)
                .unwrap();
            let span_start = child.start_byte();
            let span_end = source
                .line_starts
                .iter()
                .copied()
                .find(|line| *line >= child.end_byte())
                .unwrap_or(before.len());
            let (edit_start, edit_end, inserted) = if class == "prose" {
                let within = before[span_start..child.end_byte()]
                    .char_indices()
                    .nth(20)
                    .unwrap()
                    .0;
                (span_start + within, span_start + within, "\n")
            } else {
                let rows = source
                    .line_starts
                    .iter()
                    .copied()
                    .filter(|line| span_start <= *line && *line < span_end)
                    .collect::<Vec<_>>();
                (rows[3], span_end, "")
            };
            let delta = inserted.len() as isize - (edit_end - edit_start) as isize;
            let after = format!(
                "{}{}{}",
                &before[..edit_start],
                inserted,
                &before[edit_end..]
            );
            let new_span_end = span_end.checked_add_signed(delta).unwrap();
            let local = Highlighter::new(&after[span_start..new_span_end]);
            let fresh = Highlighter::new(&after);
            let first_line = after[..span_start]
                .bytes()
                .filter(|byte| *byte == b'\n')
                .count();
            let count = local.text().lines().count();
            assert_eq!(
                local.highlight_lines(0..count),
                fresh.highlight_lines(first_line..first_line + count),
                "{class} child must parse with complete source context",
            );
            let mut retained = Highlighter::new(&before);
            retained.apply_edit(&[TextEdit {
                range: edit_start..edit_end,
                new_text_len: inserted.len(),
                new_text: inserted.into(),
            }]);
            assert_eq!(retained.last_parse_path, ParsePath::Bounded, "{class}");
            assert!(retained.bounded_work_snapshot().0 <= BOUNDED_SOURCE_MAX_REGION_BYTES);
            assert_eq!(retained.line_starts, fresh.line_starts);
            assert_eq!(
                retained.highlight_lines(first_line.saturating_sub(4)..first_line + count + 4),
                fresh.highlight_lines(first_line.saturating_sub(4)..first_line + count + 4),
                "{class} retained source must remain exact",
            );
        }
    }

    #[test]
    fn bounded_source_line_delete_and_undo_match_fresh_analysis() {
        let before = fixtures::generate("mixed", 1024 * 1024);
        let midpoint = before
            .lines()
            .enumerate()
            .skip(before.lines().count() / 2)
            .find_map(|(line, content)| content.starts_with("## ").then_some(line + 2))
            .unwrap();
        for line in [6, midpoint] {
            let start: usize = before.split_inclusive('\n').take(line).map(str::len).sum();
            let end = start + before[start..].find('\n').unwrap() + 1;
            let deleted = &before[start..end];
            let mut highlighter = Highlighter::new(&before);
            for (edit, current) in [
                (
                    TextEdit {
                        range: start..end,
                        new_text_len: 0,
                        new_text: String::new(),
                    },
                    format!("{}{}", &before[..start], &before[end..]),
                ),
                (
                    TextEdit {
                        range: start..start,
                        new_text_len: deleted.len(),
                        new_text: deleted.to_string(),
                    },
                    before.clone(),
                ),
            ] {
                highlighter.apply_edit(&[edit]);
                assert_eq!(highlighter.text(), current);
                assert_eq!(highlighter.last_parse_path, ParsePath::Bounded);
                assert!(highlighter.last_bounded_bytes <= BOUNDED_SOURCE_MAX_REGION_BYTES);
                assert_eq!(highlighter.changed_lines_snapshot(), 0);
                let fresh = Highlighter::new(&current);
                assert_eq!(highlighter.line_starts, fresh.line_starts);
                assert_eq!(
                    highlighter.spell_reference_labels,
                    fresh.spell_reference_labels
                );
                let injection_key = |injection: &Injection| {
                    (
                        injection.start,
                        injection.end,
                        injection.language_name,
                        injection.kind,
                    )
                };
                assert_eq!(
                    highlighter
                        .injections
                        .iter()
                        .map(injection_key)
                        .collect::<Vec<_>>(),
                    fresh
                        .injections
                        .iter()
                        .map(injection_key)
                        .collect::<Vec<_>>(),
                );
                assert_eq!(
                    highlighter.spell_block_exclusion_ranges(0..current.len()),
                    fresh.spell_block_exclusion_ranges(0..current.len()),
                );
                let line_count = fresh.line_starts.len();
                for first in [0, line.saturating_sub(20), line_count.saturating_sub(80)] {
                    let end = (first + 80).min(line_count);
                    assert_eq!(
                        highlighter.highlight_lines(first..end),
                        fresh.highlight_lines(first..end),
                        "source viewport must match at line {first}",
                    );
                }
            }
        }
    }

    #[test]
    fn two_distant_source_patches_remain_exact_through_reverse_edits() {
        fn line_range(text: &str, line: usize) -> Range<usize> {
            let start: usize = text.split_inclusive('\n').take(line).map(str::len).sum();
            start..start + text[start..].find('\n').unwrap() + 1
        }

        fn verify(current: &str, highlighter: &Highlighter, anchors: &[usize]) {
            let fresh = Highlighter::new(current);
            assert_eq!(highlighter.line_starts, fresh.line_starts);
            assert_eq!(
                highlighter.spell_reference_labels,
                fresh.spell_reference_labels
            );
            assert_eq!(
                highlighter.spell_block_exclusion_ranges(0..current.len()),
                fresh.spell_block_exclusion_ranges(0..current.len()),
            );
            let injection_key = |injection: &Injection| {
                (
                    injection.start,
                    injection.end,
                    injection.language_name,
                    injection.kind,
                )
            };
            assert_eq!(
                highlighter
                    .injections
                    .iter()
                    .map(injection_key)
                    .collect::<Vec<_>>(),
                fresh
                    .injections
                    .iter()
                    .map(injection_key)
                    .collect::<Vec<_>>(),
            );
            for &anchor in anchors {
                let line = fresh.line_starts.partition_point(|start| *start <= anchor) - 1;
                let range = line.saturating_sub(10)..line + 40;
                assert_eq!(
                    highlighter.highlight_lines(range.clone()),
                    fresh.highlight_lines(range)
                );
            }
        }

        let mut current = fixtures::generate("mixed", 1024 * 1024);
        let mut highlighter = Highlighter::new(&current);
        let top = line_range(&current, 6);
        let top_text = current[top.clone()].to_string();
        highlighter.apply_edit(&[TextEdit {
            range: top.clone(),
            new_text_len: 0,
            new_text: String::new(),
        }]);
        current.replace_range(top.clone(), "");
        assert_eq!(highlighter.last_parse_path, ParsePath::Bounded);
        verify(&current, &highlighter, &[top.start, current.len() - 1]);

        let mid_heading = current
            .lines()
            .enumerate()
            .skip(current.lines().count() / 2)
            .find_map(|(line, content)| content.starts_with("## ").then_some(line + 2))
            .unwrap();
        let middle = line_range(&current, mid_heading);
        let middle_text = current[middle.clone()].to_string();
        highlighter.apply_edit(&[TextEdit {
            range: middle.clone(),
            new_text_len: 0,
            new_text: String::new(),
        }]);
        current.replace_range(middle.clone(), "");
        assert_eq!(highlighter.last_parse_path, ParsePath::Bounded);
        assert_eq!(highlighter.patches.len(), 2);
        verify(
            &current,
            &highlighter,
            &[top.start, middle.start, current.len() - 1],
        );

        for (start, deleted) in [(middle.start, middle_text), (top.start, top_text)] {
            highlighter.apply_edit(&[TextEdit {
                range: start..start,
                new_text_len: deleted.len(),
                new_text: deleted.clone(),
            }]);
            current.insert_str(start, &deleted);
            assert_eq!(highlighter.last_parse_path, ParsePath::Bounded);
            verify(
                &current,
                &highlighter,
                &[top.start, start, current.len() - 1],
            );
        }
    }

    #[test]
    fn fence_language_rebuilds_local_injection_and_delimiter_expands_globally() {
        let mut current = fixtures::generate("mixed", 1024 * 1024);
        let mut highlighter = Highlighter::new(&current);
        let label = current.find("```rust").unwrap() + 3;
        highlighter.apply_edit(&[TextEdit {
            range: label..label + 4,
            new_text_len: 7,
            new_text: "unknown".into(),
        }]);
        current.replace_range(label..label + 4, "unknown");
        assert_eq!(highlighter.last_parse_path, ParsePath::Bounded);
        let fresh = Highlighter::new(&current);
        let first_line = highlighter
            .line_starts
            .partition_point(|start| *start <= label)
            - 1;
        assert_eq!(
            highlighter.highlight_lines(first_line..first_line + 20),
            fresh.highlight_lines(first_line..first_line + 20)
        );
        assert_eq!(highlighter.injections.len(), fresh.injections.len());
        assert!(!highlighter
            .injections
            .iter()
            .any(|injection| injection.start <= label && label < injection.end));

        let delimiter = current.find("```unknown").unwrap();
        highlighter.apply_edit(&[TextEdit {
            range: delimiter..delimiter + 1,
            new_text_len: 0,
            new_text: String::new(),
        }]);
        current.remove(delimiter);
        assert_eq!(highlighter.last_parse_path, ParsePath::Incremental);
        assert!(highlighter.patches.is_empty());
        let fresh = Highlighter::new(&current);
        assert_eq!(
            highlighter.highlight_lines(first_line..first_line + 50),
            fresh.highlight_lines(first_line..first_line + 50)
        );
        assert_eq!(
            highlighter.spell_reference_labels,
            fresh.spell_reference_labels
        );
    }

    #[test]
    fn reference_definition_edit_invalidates_existing_source_patches() {
        let mut current = fixtures::generate("mixed-reference", 1024 * 1024);
        let mut highlighter = Highlighter::new(&current);
        let start: usize = current.split_inclusive('\n').take(8).map(str::len).sum();
        let end = start + current[start..].find('\n').unwrap() + 1;
        highlighter.apply_edit(&[TextEdit {
            range: start..end,
            new_text_len: 0,
            new_text: String::new(),
        }]);
        current.replace_range(start..end, "");
        assert_eq!(highlighter.last_parse_path, ParsePath::Bounded);
        assert_eq!(highlighter.patches.len(), 1);

        let definition = current.find("[shared]:").unwrap();
        highlighter.apply_edit(&[TextEdit {
            range: definition..definition,
            new_text_len: 1,
            new_text: "#".into(),
        }]);
        current.insert(definition, '#');
        assert_eq!(highlighter.last_parse_path, ParsePath::Incremental);
        assert!(highlighter.patches.is_empty());
        let fresh = Highlighter::new(&current);
        assert_eq!(
            highlighter.spell_reference_labels,
            fresh.spell_reference_labels
        );
        assert_eq!(highlighter.injections.len(), fresh.injections.len());
        for first in [0, 30, fresh.line_starts.len().saturating_sub(50)] {
            assert_eq!(
                highlighter.highlight_lines(first..first + 50),
                fresh.highlight_lines(first..first + 50)
            );
        }
    }

    #[test]
    fn seeded_large_source_edit_undo_sequence_matches_fresh_analysis() {
        const SEED: u64 = 0x5eed_cafe_1234_5678;
        let mut seed = SEED;
        let mut current = fixtures::generate("mixed", 1024 * 1024);
        let mut highlighter = Highlighter::new(&current);
        for step in 0..16 {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            let prose_lines = current
                .lines()
                .enumerate()
                .filter_map(|(line, content)| {
                    content.starts_with("A paragraph with").then_some(line)
                })
                .collect::<Vec<_>>();
            let line = prose_lines[(seed as usize) % prose_lines.len()];
            let start: usize = current.split_inclusive('\n').take(line).map(str::len).sum();
            let end = start + current[start..].find('\n').unwrap() + 1;
            let (old_range, inserted) = if step % 2 == 0 {
                (start..end, "")
            } else {
                (start..start, "\n")
            };
            let removed = current[old_range.clone()].to_string();
            highlighter.apply_edit(&[TextEdit {
                range: old_range.clone(),
                new_text_len: inserted.len(),
                new_text: inserted.into(),
            }]);
            current.replace_range(old_range, inserted);
            for reverse in [false, true] {
                if reverse {
                    highlighter.apply_edit(&[TextEdit {
                        range: start..start + inserted.len(),
                        new_text_len: removed.len(),
                        new_text: removed.clone(),
                    }]);
                    current.replace_range(start..start + inserted.len(), &removed);
                }
                assert_eq!(
                    highlighter.last_parse_path,
                    ParsePath::Bounded,
                    "seed {SEED:x}, step {step}, reverse {reverse}"
                );
                assert!(highlighter.last_bounded_bytes <= BOUNDED_SOURCE_MAX_REGION_BYTES);
                let fresh = Highlighter::new(&current);
                assert_eq!(highlighter.line_starts, fresh.line_starts);
                assert_eq!(
                    highlighter.spell_reference_labels,
                    fresh.spell_reference_labels
                );
                let injection_key = |injection: &Injection| {
                    (
                        injection.start,
                        injection.end,
                        injection.language_name,
                        injection.kind,
                    )
                };
                assert_eq!(
                    highlighter
                        .injections
                        .iter()
                        .map(injection_key)
                        .collect::<Vec<_>>(),
                    fresh
                        .injections
                        .iter()
                        .map(injection_key)
                        .collect::<Vec<_>>(),
                );
                assert_eq!(
                    highlighter.spell_block_exclusion_ranges(0..current.len()),
                    fresh.spell_block_exclusion_ranges(0..current.len()),
                );
                let edit_line = fresh.line_starts.partition_point(|offset| *offset <= start) - 1;
                let last_line = fresh.line_starts.len().saturating_sub(80);
                for first in [0, edit_line.saturating_sub(20), last_line] {
                    assert_eq!(
                        highlighter.highlight_lines(first..first + 80),
                        fresh.highlight_lines(first..first + 80),
                        "seed {SEED:x}, step {step}, reverse {reverse}, line {first}"
                    );
                }
            }
        }
    }

    fn reference_snippet_groups(
        text: &str,
        line_starts: &[usize],
        num_lines: usize,
        spans: &[ByteSpan],
    ) -> Vec<Vec<RankedSpan>> {
        (0..num_lines)
            .map(|line_idx| {
                let line_start = line_starts[line_idx];
                let next_start = line_starts.get(line_idx + 1).copied().unwrap_or(text.len());
                let line_end =
                    if next_start > line_start && text.as_bytes()[next_start - 1] == b'\n' {
                        next_start - 1
                    } else {
                        next_start
                    };
                let line_text = &text[line_start..line_end];
                spans
                    .iter()
                    .enumerate()
                    .filter(|(_, span)| span.start_byte < line_end && span.end_byte > line_start)
                    .map(|(priority, span)| RankedSpan {
                        span: Span {
                            start_col: byte_offset_to_char_index(
                                line_text,
                                span.start_byte.max(line_start) - line_start,
                            ),
                            end_col: byte_offset_to_char_index(
                                line_text,
                                span.end_byte.min(line_end) - line_start,
                            ),
                            style: span.style,
                        },
                        priority,
                    })
                    .collect()
            })
            .collect()
    }

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
    fn prose_batching_preserves_named_tree_ranges_and_complete_highlighting() {
        use sha2::{Digest, Sha256};
        use std::fmt::Write;

        fn record(node: tree_sitter::Node<'_>, output: &mut String) {
            if node.is_named() {
                writeln!(
                    output,
                    "{} {:?} {:?}",
                    node.kind(),
                    node.byte_range(),
                    node.range()
                )
                .unwrap();
            }
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                record(child, output);
            }
        }

        let cases = [
            "Ordinary words *emphasis* **strong** `code` λ &amp; \\* literal.\n\nNext words here.\n",
            "Title words with *emphasis*\n===\n\nSecond title\n---\n",
            "First line *starts\nand ends* on the second line.  \nThird line.\n",
            "Header words | second\n--- | ---\ncell words | other words\n",
            "Ordinary escaped \\| pipe.\n\nNot a table | words\n\nNext paragraph.\n",
            "[id]:\nurl/path \"title words\"\n\nLinked [words][id].\n",
            "[id]: /path\n  \"second line title words\"\n\nNormal words.\n",
            "[unfinished reference words]\nNormal words on next line.\n\nPlain [link](url) and ![image](image).\n",
            "# Heading words\n\n- list words\n  continued words\n\n> quote words\n> next words\n",
            "```rust extra words\nfn main() {}\n\nOther raw words\n```\n\nPlain words.\n",
            "---\ntitle: Words here\n---\n\nFirst prose words.\n",
            "<script>\nfirst words\n\nsecond words\n</script>\n\nPlain words.\n",
            "First CRLF words.\r\n\r\nSecond CRLF words.\r\n",
            "λ first Unicode words.\n\nAlpha e\u{301} and 世界 words.\n",
        ];
        let mut output = String::new();
        for text in cases {
            let highlighter = Highlighter::new(text);
            record(highlighter.md_tree.root_node(), &mut output);
            writeln!(output, "{:?}", highlighter.highlight_lines(0..usize::MAX)).unwrap();
            writeln!(output, "{:?}", highlighter.spell_reference_labels).unwrap();
        }
        assert_eq!(
            format!("{:x}", Sha256::digest(output)),
            "204fe1f5973599c2f2778221ba4c4b84ebf0325c4d1b74c34ed2c392521d9dd7"
        );
    }

    #[test]
    fn prose_batching_incremental_structural_edits_match_fresh_parses() {
        for initial in [
            "Alpha *words* λ.\n\nBeta `words`.\n",
            "Alpha words.\r\n\r\nBeta words.\r\n",
            "Title words\n---\n\nBeta words.\n",
            "[id]:\nurl/path \"title words\"\n\nBeta [words][id].\n",
            "> Alpha words\n>\n> Beta words\n\nGamma words.\n",
            "- Alpha words\n\n  Beta words\n\nGamma words.\n",
            "```rust\nAlpha words\n\nBeta words\n```\n\nGamma words.\n",
            "<script>\nAlpha words\n\nBeta words\n</script>\n\nGamma words.\n",
        ] {
            for offset in initial
                .char_indices()
                .map(|(offset, _)| offset)
                .chain([initial.len()])
            {
                for replacement in ["\n", "\r\n", "# ", "|", "[", "é", "```\n"] {
                    let edit = TextEdit {
                        range: offset..offset,
                        new_text_len: replacement.len(),
                        new_text: replacement.into(),
                    };
                    assert_edit_matches_fresh(initial, edit);
                }
            }
            for (offset, character) in initial.char_indices() {
                assert_edit_matches_fresh(
                    initial,
                    TextEdit {
                        range: offset..offset + character.len_utf8(),
                        new_text_len: 0,
                        new_text: String::new(),
                    },
                );
            }
        }
    }

    #[test]
    fn source_headings_preserve_atx_and_setext_levels() {
        let text = concat!(
            "# h1_é\n",
            "## h2_plain *h2_em* `h2_code`\n",
            "### h3_token\n",
            "#### h4_token\n",
            "##### h5_token\n",
            "###### h6_token\n",
            "setext_h1\n===========\n",
            "setext_h2\n-----------\n",
            "> ## nested_h2\n",
        );
        let lines = Highlighter::new(text).highlight_lines(0..usize::MAX);

        for (line_index, token, expected) in [
            (0, "h1_é", SemanticStyle::Heading1),
            (1, "h2_plain", SemanticStyle::Heading2),
            (2, "h3_token", SemanticStyle::Heading3),
            (3, "h4_token", SemanticStyle::Heading4),
            (4, "h5_token", SemanticStyle::Heading5),
            (5, "h6_token", SemanticStyle::Heading6),
            (6, "setext_h1", SemanticStyle::Heading1),
            (8, "setext_h2", SemanticStyle::Heading2),
            (10, "nested_h2", SemanticStyle::Heading2),
        ] {
            assert_eq!(
                style_covering_token(&lines[line_index], token),
                expected,
                "wrong heading style for {token:?}: {:?}",
                lines[line_index].spans
            );
        }

        for (line_index, expected) in [
            (0, SemanticStyle::Heading1),
            (2, SemanticStyle::Heading3),
            (3, SemanticStyle::Heading4),
            (4, SemanticStyle::Heading5),
            (5, SemanticStyle::Heading6),
            (6, SemanticStyle::Heading1),
            (7, SemanticStyle::Heading1),
            (8, SemanticStyle::Heading2),
            (9, SemanticStyle::Heading2),
        ] {
            assert_eq!(
                lines[line_index].spans,
                vec![Span {
                    start_col: 0,
                    end_col: lines[line_index].text.chars().count(),
                    style: expected,
                }],
                "heading line {line_index} must retain its exact full-line base range"
            );
        }

        assert_eq!(
            style_covering_token(&lines[1], "##"),
            SemanticStyle::Heading2
        );
        assert_eq!(
            style_covering_token(&lines[10], "##"),
            SemanticStyle::Heading2
        );

        assert_eq!(
            style_covering_token(&lines[1], "h2_em"),
            SemanticStyle::Emphasis
        );
        assert_eq!(
            style_covering_token(&lines[1], "h2_code"),
            SemanticStyle::CodeSpan
        );
    }

    #[test]
    fn source_table_header_uses_strong_style() {
        let text = concat!(
            "| header_plain | *header_emphasis* |\n",
            "| --- | --- |\n",
            "| body_plain | body_other |\n",
        );
        let lines = Highlighter::new(text).highlight_lines(0..usize::MAX);

        assert_eq!(
            style_covering_token(&lines[0], "header_plain"),
            SemanticStyle::Strong
        );
        assert_eq!(
            style_covering_token(&lines[0], "header_emphasis"),
            SemanticStyle::Emphasis
        );
        assert_eq!(
            style_covering_token(&lines[2], "body_plain"),
            SemanticStyle::Text
        );
    }

    #[test]
    fn highlight_snippet_uses_code_block_as_base_style() {
        let rust = highlight_snippet(
            "rust",
            "fn captured_function() {\n    let uncaptured_界 = \"captured_string\"; // captured_comment\n\n}\n",
        );

        for (line, token, expected) in [
            (0, "fn", SemanticStyle::Keyword),
            (0, "captured_function", SemanticStyle::Function),
            (1, "uncaptured_界", SemanticStyle::CodeBlock),
            (1, "captured_string", SemanticStyle::StringLit),
            (1, "captured_comment", SemanticStyle::Comment),
        ] {
            assert_eq!(style_covering_token(&rust[line], token), expected);
        }
        assert!(rust[2].text.is_empty());
        assert!(rust[2].spans.is_empty());

        let unknown = highlight_snippet("unknown-language", "unknown_界\n\nsecond_unknown\n");
        assert_eq!(unknown.len(), 3);
        assert_eq!(
            style_covering_token(&unknown[0], "unknown_界"),
            SemanticStyle::CodeBlock
        );
        assert!(unknown[1].text.is_empty());
        assert!(unknown[1].spans.is_empty());
        assert_eq!(
            style_covering_token(&unknown[2], "second_unknown"),
            SemanticStyle::CodeBlock
        );
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
    fn highlighter_line_index_tracks_sequential_edits() {
        let mut highlighter = Highlighter::new("α\nbeta\n🙂 end\n");
        let assert_index = |highlighter: &Highlighter| {
            assert_eq!(
                highlighter.line_starts,
                line_start_indices(highlighter.text()),
                "cached starts must equal a full rebuild"
            );
        };
        assert_index(&highlighter);

        highlighter.apply_edit(&[TextEdit {
            range: 0..0,
            new_text_len: 6,
            new_text: "start\n".to_string(),
        }]);
        assert_index(&highlighter);

        let beta = highlighter.text().find("beta").unwrap();
        highlighter.apply_edit(&[TextEdit {
            range: beta..beta + 4,
            new_text_len: 0,
            new_text: String::new(),
        }]);
        assert_index(&highlighter);

        let emoji = highlighter.text().find('🙂').unwrap();
        highlighter.apply_edit(&[TextEdit {
            range: emoji..emoji + '🙂'.len_utf8(),
            new_text_len: "界\nnew".len(),
            new_text: "界\nnew".to_string(),
        }]);
        assert_index(&highlighter);

        // Both coordinates are sequential: the second edit sees the prefix
        // inserted by the first edit in this same engine fan-out batch.
        highlighter.apply_edit(&[
            TextEdit {
                range: 0..0,
                new_text_len: 2,
                new_text: "p\n".to_string(),
            },
            TextEdit {
                range: 2..7,
                new_text_len: 7,
                new_text: "renamed".to_string(),
            },
        ]);
        assert_index(&highlighter);

        let before_rebuilds = line_index_build_count();
        for start in 0..highlighter.line_starts.len().min(20) {
            let _ = highlighter.highlight_lines(start..start + 3);
        }
        assert_eq!(line_index_build_count(), before_rebuilds);
    }

    #[test]
    fn block_parser_skips_inline_edits_but_reparses_block_transitions() {
        let mut highlighter = Highlighter::new("# Heading\n\nordinary paragraph\n");
        assert_eq!(highlighter.block_parse_count.get(), 1);
        assert_eq!(highlighter.last_parse_path, ParsePath::Full);

        let paragraph = highlighter.text().find("ordinary").unwrap();
        highlighter.apply_edit(&[TextEdit {
            range: paragraph..paragraph + 1,
            new_text_len: 1,
            new_text: "O".to_string(),
        }]);
        assert_eq!(highlighter.block_parse_count.get(), 1);
        assert_eq!(highlighter.last_parse_path, ParsePath::BlockNeutral);
        assert_eq!(
            highlighter.highlight_lines(0..3),
            Highlighter::new(highlighter.text()).highlight_lines(0..3)
        );

        highlighter.apply_edit(&[TextEdit {
            range: 0..1,
            new_text_len: 1,
            new_text: "x".to_string(),
        }]);
        assert_eq!(highlighter.block_parse_count.get(), 2);
        assert_eq!(
            highlighter.highlight_lines(0..3),
            Highlighter::new(highlighter.text()).highlight_lines(0..3)
        );
    }

    #[test]
    fn word_insertion_at_paragraph_boundary_reparses_source_ownership() {
        let text = "#\nr<%(PW<w\nDEFG\nSBqs&\n";
        let mut highlighter = Highlighter::new(text);
        let edit = TextEdit {
            range: 2..2,
            new_text_len: 3,
            new_text: "JKL".to_string(),
        };
        highlighter.apply_edit(&[edit]);
        assert_eq!(highlighter.text(), "#\nJKLr<%(PW<w\nDEFG\nSBqs&\n");
        assert_eq!(
            highlighter.highlight_lines(0..5),
            Highlighter::new(highlighter.text()).highlight_lines(0..5),
        );
    }

    #[test]
    fn block_parser_reparses_same_marker_class_when_structure_can_change() {
        let mut highlighter = Highlighter::new("Title\n---\n");
        highlighter.apply_edit(&[TextEdit {
            range: 6..9,
            new_text_len: 3,
            new_text: "***".to_string(),
        }]);

        assert_eq!(highlighter.block_parse_count.get(), 2);
        assert_eq!(
            highlighter.highlight_lines(0..2),
            Highlighter::new("Title\n***\n").highlight_lines(0..2)
        );
    }

    #[test]
    fn inline_highlighting_keeps_context_across_viewport_boundary() {
        let text = "*hello\nworld* after\n";
        let highlighter = Highlighter::new(text);
        let full = highlighter.highlight_lines(0..2);
        let partial = highlighter.highlight_lines(1..2);

        assert_eq!(partial, full[1..2]);
        assert!(partial[0]
            .spans
            .iter()
            .any(|span| span.style == SemanticStyle::Emphasis));
    }

    #[test]
    fn completing_frontmatter_closer_discovers_injection() {
        let initial = "---\ntitle: Hello\n";
        let mut highlighter = Highlighter::new(initial);
        assert!(highlighter.injections.is_empty());

        highlighter.apply_edit(&[TextEdit {
            range: initial.len()..initial.len(),
            new_text_len: 4,
            new_text: "---\n".to_string(),
        }]);

        assert!(!highlighter.injections.is_empty());
        assert!(highlighter.highlight_lines(1..2)[0]
            .spans
            .iter()
            .any(|span| span.style == SemanticStyle::FmKey));
    }

    #[test]
    fn pasting_complete_frontmatter_discovers_injection() {
        let mut highlighter = Highlighter::new("");
        let pasted = "---\ntitle: X\n---\n";
        highlighter.apply_edit(&[TextEdit {
            range: 0..0,
            new_text_len: pasted.len(),
            new_text: pasted.to_string(),
        }]);

        assert!(!highlighter.injections.is_empty());
        assert!(highlighter.highlight_lines(1..2)[0]
            .spans
            .iter()
            .any(|span| span.style == SemanticStyle::FmKey));
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
        assert_eq!(highlighter.query_cache.borrow().len(), 1);
        assert!(Arc::ptr_eq(
            &highlighter.injections[0].query,
            &highlighter.injections[1].query
        ));
        let cached_query = Arc::clone(
            highlighter
                .query_cache
                .borrow()
                .get("rust")
                .expect("Rust query should be cached"),
        );

        let insertion_offset = text.find("first").expect("first function name");
        highlighter.apply_edit(&[TextEdit {
            range: insertion_offset..insertion_offset,
            new_text_len: 1,
            new_text: "x".to_string(),
        }]);

        assert_eq!(highlighter.query_cache.borrow().len(), 1);
        assert!(Arc::ptr_eq(
            &cached_query,
            highlighter
                .query_cache
                .borrow()
                .get("rust")
                .expect("Rust query should remain cached")
        ));
        assert!(highlighter
            .injections
            .iter()
            .all(|injection| Arc::ptr_eq(&cached_query, &injection.query)));
    }

    #[test]
    fn rendered_fences_share_canonical_queries_across_aliases_layouts_and_edits() {
        let text = "```rust\nfn first() {}\n```\n\n```rust\nfn second() {}\n```\n\n```py\nprint('hi')\n```\n\n```unknown\nno grammar\n```\n";
        let mut highlighter = Highlighter::new(text);
        assert_eq!(highlighter.query_compile_count.get(), 2);

        for (lang, snippet) in [
            ("rust", "fn first() {}\n"),
            ("py", "print('hi')\n"),
            ("PYTHON", "print('hi')\n"),
            ("unknown", "no grammar\n"),
        ] {
            assert_eq!(
                highlighter.highlight_snippet(lang, snippet),
                highlight_snippet(lang, snippet),
                "cached and uncached styled lines differ for {lang}"
            );
        }
        assert_eq!(highlighter.query_compile_count.get(), 2);

        let model = BlockModel::build(text, None);
        let wide = RenderedLayout::build(&model, 80, &highlighter);
        let narrow = RenderedLayout::build(&model, 34, &highlighter);
        assert_eq!(wide, RenderedLayout::build(&model, 80, &highlighter));
        assert_eq!(narrow, RenderedLayout::build(&model, 34, &highlighter));
        assert_eq!(highlighter.query_compile_count.get(), 2);

        let offset = text.find("first").expect("first fence content");
        highlighter.apply_edit(&[TextEdit {
            range: offset..offset + 5,
            new_text_len: 5,
            new_text: "third".to_string(),
        }]);
        let edited = highlighter.text();
        let edited_model = BlockModel::build(edited, None);
        let edited_layout = RenderedLayout::build(&edited_model, 80, &highlighter);
        assert_ne!(edited_layout, wide);
        assert_eq!(highlighter.query_compile_count.get(), 2);

        let reloaded = Highlighter::new(edited);
        assert_eq!(reloaded.query_compile_count.get(), 2);
        assert_eq!(
            edited_layout,
            RenderedLayout::build(&edited_model, 80, &reloaded)
        );
        assert_eq!(reloaded.query_compile_count.get(), 2);
    }

    #[test]
    fn rendered_only_language_compiles_once_and_unknown_never_compiles() {
        let highlighter = Highlighter::new("plain prose\n");
        assert_eq!(highlighter.query_compile_count.get(), 0);
        for _ in 0..3 {
            assert_eq!(
                highlighter.highlight_snippet("golang", "package main\n"),
                highlight_snippet("go", "package main\n")
            );
            assert_eq!(
                highlighter.highlight_snippet("unknown", "opaque\n"),
                highlight_snippet("unknown", "opaque\n")
            );
        }
        assert_eq!(highlighter.query_compile_count.get(), 1);
        assert_eq!(highlighter.query_cache.borrow().len(), 1);
    }

    #[test]
    fn long_valid_and_malformed_snippet_groups_match_quadratic_reference() {
        let cases = [
            (
                "rust",
                format!("fn main() {{\n{} }}\n", "let value = 123;\n".repeat(4096)),
            ),
            ("rust", "let = + ???;\n".repeat(4096)),
            ("py", "print('café')\r\n".repeat(4096)),
        ];
        for (alias, snippet) in cases {
            let language = languages::find_by_alias(alias).expect("known language");
            let grammar = (language.language_fn)();
            let query = Query::new(&grammar, language.highlights_query).expect("valid query");
            let spans = collect_snippet_spans(&snippet, &grammar, &query).expect("parsed snippet");
            let starts = line_start_indices(&snippet);
            let lines = starts.len().saturating_sub(1);
            let (grouped, _) = group_snippet_spans(&snippet, &starts, lines, &spans);
            assert_eq!(
                grouped,
                reference_snippet_groups(&snippet, &starts, lines, &spans),
                "span grouping changed for {alias}"
            );
            let highlighter = Highlighter::new("plain text\n");
            let highlighted = highlighter.highlight_snippet(alias, &snippet);
            assert_eq!(highlighted.len(), lines);
            assert!(highlighted.iter().any(|line| !line.spans.is_empty()));
        }
    }

    #[test]
    fn long_unicode_fence_keeps_byte_exact_rendered_atoms() {
        let text = format!("```rust\n{}```\n", "let café = \"界\";\n".repeat(1024));
        let highlighter = Highlighter::new(&text);
        let model = BlockModel::build(&text, None);
        let layout = RenderedLayout::build(&model, 120, &highlighter);
        let rows = layout
            .lines
            .iter()
            .filter(|line| {
                line.role == RenderedLineRole::CodeFence && line.kind == LineKind::Content
            })
            .collect::<Vec<_>>();
        assert_eq!(rows.len(), 1024);
        for row in rows {
            let mut ranges = row
                .atoms
                .iter()
                .filter_map(|atom| atom.source.clone())
                .collect::<Vec<_>>();
            ranges.sort_by_key(|range| range.start);
            assert_eq!(
                ranges.first().map(|range| range.start),
                Some(row.source.start),
                "source={:?}, text={:?}, atoms={:?}",
                row.source,
                row.styled.text,
                ranges
            );
            let visible_end = row.source.end
                - usize::from(text.as_bytes().get(row.source.end - 1) == Some(&b'\n'));
            assert_eq!(ranges.last().map(|range| range.end), Some(visible_end));
            assert!(ranges.windows(2).all(|pair| pair[0].end == pair[1].start));
            assert_eq!(&text[row.source.start..visible_end], "let café = \"界\";");
        }
    }

    #[test]
    fn snippet_grouping_visits_only_intersected_lines() {
        let text = "value\n".repeat(4096);
        let starts = line_start_indices(&text);
        let mut spans = vec![ByteSpan {
            start_byte: 0,
            end_byte: text.len(),
            style: SemanticStyle::CodeBlock,
        }];
        spans.extend((0..4096).map(|line| ByteSpan {
            start_byte: starts[line],
            end_byte: starts[line] + 5,
            style: SemanticStyle::Keyword,
        }));
        let (grouped, visits) = group_snippet_spans(&text, &starts, 4096, &spans);
        assert_eq!(
            grouped,
            reference_snippet_groups(&text, &starts, 4096, &spans)
        );
        assert!(visits <= 8192, "visited {visits} span/line pairs");
    }

    #[test]
    fn source_injection_parse_is_reused_and_incrementally_updated_by_fence_edits() {
        let mut text = format!(
            "intro\n```rust\nfn main() {{\n{} }}\n```\noutro\n",
            "let value = 123;\n".repeat(256)
        );
        let mut highlighter = Highlighter::new(&text);
        assert_eq!(highlighter.source_injection_parse_count.get(), 0);
        let first = highlighter.highlight_lines(2..12);
        assert_eq!(highlighter.source_injection_parse_count.get(), 1);
        assert_eq!(first, highlighter.highlight_lines(2..12));
        assert_eq!(highlighter.source_injection_parse_count.get(), 1);
        highlighter.highlight_lines(100..110);
        assert_eq!(highlighter.source_injection_parse_count.get(), 1);

        let offset = text.find("value").expect("inside fence");
        highlighter.apply_edit(&[TextEdit {
            range: offset..offset + 5,
            new_text_len: 5,
            new_text: "other".to_string(),
        }]);
        assert_eq!(highlighter.last_parse_path, ParsePath::FenceInterior);
        text.replace_range(offset..offset + 5, "other");
        assert_eq!(
            highlighter.highlight_lines(2..12),
            Highlighter::new(&text).highlight_lines(2..12)
        );
        assert_eq!(highlighter.source_injection_parse_count.get(), 1);

        highlighter.apply_edit(&[TextEdit {
            range: 0..0,
            new_text_len: 2,
            new_text: "x\n".to_string(),
        }]);
        text.insert_str(0, "x\n");
        assert_eq!(
            highlighter.highlight_lines(3..13),
            Highlighter::new(&text).highlight_lines(3..13)
        );
        assert_eq!(highlighter.source_injection_parse_count.get(), 2);
    }

    #[test]
    fn block_neutral_edits_rebase_injections_without_rediscovery() {
        let mut text = concat!(
            "---\ntitle: Performance fixture\n---\n",
            "intro paragraph for the editor\n",
            "```rust\nfn first() { let value = 123; }\n```\n",
            "middle paragraph for the editor\n",
            "```rust\nfn second() { let value = 456; }\n```\n",
        )
        .to_string();
        let mut highlighter = Highlighter::new(&text);
        let scan_count = highlighter.injection_scan_count;
        highlighter.highlight_lines(0..highlighter.line_starts.len());
        assert_eq!(highlighter.source_parse_cache.borrow().len(), 3);

        for (old, new, path, affected_cache_entries) in [
            ("intro", "opening", ParsePath::BlockNeutral, 3),
            ("123", "12345", ParsePath::FenceInterior, 3),
            ("fixture", "document", ParsePath::BlockNeutral, 2),
        ] {
            let start = text.find(old).unwrap();
            highlighter.apply_edit(&[TextEdit {
                range: start..start + old.len(),
                new_text_len: new.len(),
                new_text: new.to_string(),
            }]);
            text.replace_range(start..start + old.len(), new);
            let fresh = Highlighter::new(&text);
            assert_eq!(highlighter.last_parse_path, path, "edit {old:?} to {new:?}");
            assert_eq!(highlighter.injection_scan_count, scan_count);
            assert_eq!(
                highlighter
                    .injections
                    .iter()
                    .map(|injection| (injection.start, injection.end, injection.language_name))
                    .collect::<Vec<_>>(),
                fresh
                    .injections
                    .iter()
                    .map(|injection| (injection.start, injection.end, injection.language_name))
                    .collect::<Vec<_>>(),
            );
            assert_eq!(
                highlighter.source_parse_cache.borrow().len(),
                affected_cache_entries,
            );
            assert_eq!(
                highlighter.highlight_lines(0..highlighter.line_starts.len()),
                fresh.highlight_lines(0..fresh.line_starts.len()),
            );
        }
    }

    #[test]
    fn html_block_boundary_edit_uses_wide_source_reparse() {
        let mut text = "ordinary paragraph line for a large note\n".repeat(3500);
        text.push_str("\n<div>\ninside html\n</div>\n\nfinal paragraph\n");
        let mut highlighter = Highlighter::new(&text);
        let start = text.find("<div>").unwrap();
        highlighter.apply_edit(&[TextEdit {
            range: start..start + 5,
            new_text_len: 5,
            new_text: "plain".to_string(),
        }]);
        text.replace_range(start..start + 5, "plain");
        let fresh = Highlighter::new(&text);
        assert_eq!(highlighter.last_parse_path, ParsePath::Incremental);
        assert_eq!(highlighter.bounded_work_snapshot().2, 0);
        assert_eq!(highlighter.line_starts, fresh.line_starts);
        assert_eq!(
            highlighter.spell_block_exclusion_ranges(0..text.len()),
            fresh.spell_block_exclusion_ranges(0..text.len()),
        );
        let first = highlighter
            .line_starts
            .partition_point(|line| *line < start);
        assert_eq!(
            highlighter.highlight_lines(first.saturating_sub(2)..first + 7),
            fresh.highlight_lines(first.saturating_sub(2)..first + 7),
        );
    }

    #[test]
    fn source_injection_cache_stays_bounded_after_many_fences() {
        let text = (0..20)
            .map(|number| format!("```rust\nlet value = {number};\n```\n"))
            .collect::<String>();
        let highlighter = Highlighter::new(&text);
        for line in (1..60).step_by(3) {
            highlighter.highlight_lines(line..line + 1);
        }
        assert_eq!(highlighter.source_injection_parse_count.get(), 20);
        assert_eq!(
            highlighter.source_parse_cache.borrow().len(),
            SOURCE_PARSE_CACHE_MAX_ENTRIES
        );
        assert!(
            highlighter
                .source_parse_cache
                .borrow()
                .iter()
                .map(|entry| entry.end - entry.start)
                .sum::<usize>()
                <= SOURCE_PARSE_CACHE_MAX_SOURCE_BYTES
        );
    }

    #[test]
    fn source_injection_cache_releases_stale_entries_across_edits_and_languages() {
        fn replace(highlighter: &mut Highlighter, text: &mut String, old: &str, new: &str) {
            let start = text.find(old).expect("fixture token exists");
            highlighter.apply_edit(&[TextEdit {
                range: start..start + old.len(),
                new_text_len: new.len(),
                new_text: new.to_string(),
            }]);
            text.replace_range(start..start + old.len(), new);
            assert_eq!(highlighter.text(), text.as_str());
            let fresh = Highlighter::new(text);
            assert_eq!(
                highlighter.highlight_lines(0..highlighter.line_starts.len()),
                fresh.highlight_lines(0..fresh.line_starts.len()),
            );
            let cache = highlighter.source_parse_cache.borrow();
            assert!(cache.len() <= SOURCE_PARSE_CACHE_MAX_ENTRIES);
            assert!(
                cache
                    .iter()
                    .map(|entry| entry.end - entry.start)
                    .sum::<usize>()
                    <= SOURCE_PARSE_CACHE_MAX_SOURCE_BYTES
            );
            assert!(cache.iter().all(|entry| {
                entry.start < entry.end
                    && entry.end <= text.len()
                    && highlighter.injections.iter().any(|injection| {
                        injection.start == entry.start
                            && injection.end == entry.end
                            && injection.language_name == entry.language_name
                    })
            }));
        }

        let mut text = format!(
            "---\ntitle: cache ownership\n---\n```rust\n{}\n```\n```go\n{}\n```\n",
            "let rust_value = 123;\n".repeat(64),
            "goValue := 456\n".repeat(64),
        );
        let mut highlighter = Highlighter::new(&text);
        highlighter.highlight_lines(0..highlighter.line_starts.len());
        for _ in 0..12 {
            replace(
                &mut highlighter,
                &mut text,
                "rust_value = 123",
                "rust_value = 789",
            );
            replace(
                &mut highlighter,
                &mut text,
                "rust_value = 789",
                "rust_value = 123",
            );
            replace(&mut highlighter, &mut text, "```rust", "```go  ");
            replace(&mut highlighter, &mut text, "```go  ", "```rust");
            replace(
                &mut highlighter,
                &mut text,
                "goValue := 456",
                "goValue := 012",
            );
            replace(
                &mut highlighter,
                &mut text,
                "goValue := 012",
                "goValue := 456",
            );
        }
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
        for (i, expected) in [
            SemanticStyle::Heading1,
            SemanticStyle::Heading2,
            SemanticStyle::Heading3,
            SemanticStyle::Heading4,
            SemanticStyle::Heading5,
            SemanticStyle::Heading6,
        ]
        .into_iter()
        .enumerate()
        {
            let lines = h.highlight_lines(i..i + 1);
            assert_eq!(lines.len(), 1);
            let has_heading = lines[0].spans.iter().any(|s| s.style == expected);
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
                    prop_assert_eq!(
                        &highlighter.line_starts,
                        &line_start_indices(&current_text),
                        "cached line starts diverged after generated sequential edit"
                    );

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
                            "line {} span count mismatch after byte {}, range {:?}, insert {:?}, current text {:?}, parse path {:?}",
                            i,
                            byte,
                            edit.range,
                            edit.new_text,
                            current_text,
                            highlighter.last_parse_path,
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

    fn style_covering_token(line: &StyledLine, token: &str) -> SemanticStyle {
        let byte_start = line
            .text
            .find(token)
            .unwrap_or_else(|| panic!("missing token {token:?} in {:?}", line.text));
        let start = line.text[..byte_start].chars().count();
        let end = start + token.chars().count();
        line.spans
            .iter()
            .find(|span| span.start_col <= start && span.end_col >= end)
            .map_or(SemanticStyle::Text, |span| span.style)
    }

    fn assert_edit_matches_fresh(initial: &str, edit: TextEdit) -> Vec<StyledLine> {
        let mut expected_text = initial.to_string();
        let start = edit.range.start.min(edit.range.end);
        let end = edit.range.start.max(edit.range.end);
        expected_text.replace_range(start..end, &edit.new_text);

        let mut highlighter = Highlighter::new(initial);
        highlighter.apply_edit(&[edit]);

        assert_eq!(highlighter.text(), expected_text);
        assert!(matches!(
            highlighter.last_parse_path,
            ParsePath::Incremental | ParsePath::BlockNeutral
        ));
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
        assert!(matches!(
            highlighter.last_parse_path,
            ParsePath::Incremental | ParsePath::BlockNeutral
        ));
        assert_eq!(
            highlighter.highlight_lines(0..1000),
            Highlighter::new(&expected).highlight_lines(0..1000)
        );
    }
}
