//! Clipboard sink trait for the core.
//!
//! The core emits `Effect::ClipboardWrite` and the host routes it to its
//! [`ClipboardSink`] implementation. The TUI provides `Osc52Clipboard`.

use std::fmt;
use std::ops::Range;

use crate::style::{RenderedLayout, RenderedSelection, RenderedSelectionRow, SelectionShape};
use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
use unicode_width::UnicodeWidthChar;

/// Clipboard text represented both as Markdown source and rendered plain text.
///
/// Hosts choose the representation according to their own user-facing policy.
/// Keeping both forms in the effect lets the reusable core remain the sole
/// owner of Markdown parsing and rendered source provenance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClipboardContent {
    markdown: String,
    plain_text: String,
}

impl ClipboardContent {
    /// Create clipboard content from explicitly prepared representations.
    pub fn new(markdown: String, plain_text: String) -> Self {
        Self {
            markdown,
            plain_text,
        }
    }

    /// Create clipboard content by rendering a Markdown fragment as plain text.
    pub fn from_markdown(markdown: String) -> Self {
        let plain_text = markdown_to_plain_text(&markdown);
        Self::new(markdown, plain_text)
    }

    /// Create format-invariant clipboard content such as a copied URL.
    pub fn invariant(text: String) -> Self {
        Self::new(text.clone(), text)
    }

    /// Return the exact Markdown-source representation.
    pub fn markdown(&self) -> &str {
        &self.markdown
    }

    /// Return the rendered, syntax-free plain-text representation.
    pub fn plain_text(&self) -> &str {
        &self.plain_text
    }
}

pub(crate) fn rendered_selection_content(
    selection: &RenderedSelection,
    layout: &RenderedLayout,
    document: &str,
) -> ClipboardContent {
    let constructs = inline_construct_spans(document);
    let visible_sources = visible_source_ranges(layout);
    match selection.shape {
        SelectionShape::Character => {
            let markdown = markdown_for_ranges(
                &selection.source_ranges,
                &selection.source_ranges,
                &constructs,
                &visible_sources,
                document,
            );
            ClipboardContent::from_markdown(markdown)
        }
        SelectionShape::Line => {
            let markdown = concatenate_ranges(&selection.source_ranges, document);
            ClipboardContent::from_markdown(markdown)
        }
        SelectionShape::Block => {
            let mut markdown_rows = Vec::with_capacity(selection.rows.len());
            let mut plain_rows = Vec::with_capacity(selection.rows.len());
            for row in &selection.rows {
                let markdown = markdown_for_ranges(
                    &row.source_ranges,
                    &selection.source_ranges,
                    &constructs,
                    &visible_sources,
                    document,
                );
                let plain_text = rendered_plain_text_for_row(row, layout);
                markdown_rows.push(markdown);
                plain_rows.push(plain_text);
            }
            ClipboardContent::new(markdown_rows.join("\n"), plain_rows.join("\n"))
        }
    }
}

fn rendered_plain_text_for_row(row: &RenderedSelectionRow, layout: &RenderedLayout) -> String {
    let Some(line) = layout.lines.get(row.row) else {
        return String::new();
    };
    line.atoms
        .iter()
        .zip(display_groups(&line.styled.text))
        .filter(|(atom, _)| {
            atom.source.as_ref().is_some_and(|source| {
                row.source_ranges
                    .iter()
                    .any(|selected| source.start < selected.end && selected.start < source.end)
            }) && row
                .columns
                .iter()
                .any(|columns| atom.columns.start < columns.end && columns.start < atom.columns.end)
        })
        .map(|(_, group)| group)
        .collect()
}

fn display_groups(text: &str) -> Vec<String> {
    let mut groups: Vec<String> = Vec::new();
    for character in text.chars() {
        if character.width().unwrap_or(0) == 0 {
            if let Some(previous) = groups.last_mut() {
                previous.push(character);
            } else {
                groups.push(character.to_string());
            }
        } else {
            groups.push(character.to_string());
        }
    }
    groups
}

fn markdown_for_ranges(
    row_ranges: &[Range<usize>],
    all_selected_ranges: &[Range<usize>],
    constructs: &[Range<usize>],
    visible_sources: &[Range<usize>],
    document: &str,
) -> String {
    let mut ranges = row_ranges.to_vec();
    for construct in constructs {
        let first_visible =
            visible_sources.partition_point(|source| source.start < construct.start);
        let after_visible = visible_sources.partition_point(|source| source.start < construct.end);
        let visible = &visible_sources[first_visible..after_visible];
        if visible.is_empty()
            || !visible
                .iter()
                .filter(|source| construct.start <= source.start && source.end <= construct.end)
                .all(|source| range_is_covered(source, all_selected_ranges))
        {
            continue;
        }

        let Some(first) = visible
            .iter()
            .find(|source| construct.start <= source.start && source.end <= construct.end)
        else {
            continue;
        };
        let last = visible
            .iter()
            .rev()
            .find(|source| construct.start <= source.start && source.end <= construct.end)
            .expect("the first visible construct source was found");
        if range_is_covered(first, row_ranges) && construct.start < first.start {
            ranges.push(construct.start..first.start);
        }
        if range_is_covered(last, row_ranges) && last.end < construct.end {
            ranges.push(last.end..construct.end);
        }
    }
    concatenate_ranges(&normalize_ranges(ranges), document)
}

fn visible_source_ranges(layout: &RenderedLayout) -> Vec<Range<usize>> {
    let mut sources: Vec<_> = layout
        .lines
        .iter()
        .flat_map(|line| &line.atoms)
        .filter_map(|atom| atom.source.clone())
        .collect();
    sources.sort_by_key(|range| (range.start, range.end));
    sources.dedup();
    sources
}

fn range_is_covered(target: &Range<usize>, selected: &[Range<usize>]) -> bool {
    selected
        .iter()
        .any(|range| range.start <= target.start && target.end <= range.end)
}

fn concatenate_ranges(ranges: &[Range<usize>], document: &str) -> String {
    ranges
        .iter()
        .filter_map(|range| document.get(range.clone()))
        .collect()
}

fn normalize_ranges(mut ranges: Vec<Range<usize>>) -> Vec<Range<usize>> {
    ranges.retain(|range| range.start < range.end);
    ranges.sort_by_key(|range| (range.start, range.end));
    let mut normalized: Vec<Range<usize>> = Vec::with_capacity(ranges.len());
    for range in ranges {
        if let Some(last) = normalized.last_mut() {
            if range.start <= last.end {
                last.end = last.end.max(range.end);
                continue;
            }
        }
        normalized.push(range);
    }
    normalized
}

fn markdown_options() -> Options {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_FOOTNOTES);
    options.insert(Options::ENABLE_TASKLISTS);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options
}

fn inline_construct_spans(markdown: &str) -> Vec<Range<usize>> {
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum InlineKind {
        Emphasis,
        Strong,
        Strikethrough,
        Link,
        Image,
    }

    fn start_kind(tag: &Tag<'_>) -> Option<InlineKind> {
        match tag {
            Tag::Emphasis => Some(InlineKind::Emphasis),
            Tag::Strong => Some(InlineKind::Strong),
            Tag::Strikethrough => Some(InlineKind::Strikethrough),
            Tag::Link { .. } => Some(InlineKind::Link),
            Tag::Image { .. } => Some(InlineKind::Image),
            _ => None,
        }
    }

    fn end_kind(tag: TagEnd) -> Option<InlineKind> {
        match tag {
            TagEnd::Emphasis => Some(InlineKind::Emphasis),
            TagEnd::Strong => Some(InlineKind::Strong),
            TagEnd::Strikethrough => Some(InlineKind::Strikethrough),
            TagEnd::Link => Some(InlineKind::Link),
            TagEnd::Image => Some(InlineKind::Image),
            _ => None,
        }
    }

    let mut stack: Vec<(InlineKind, usize)> = Vec::new();
    let mut spans = Vec::new();
    for (event, span) in Parser::new_ext(markdown, markdown_options()).into_offset_iter() {
        match event {
            Event::Start(tag) => {
                if let Some(kind) = start_kind(&tag) {
                    stack.push((kind, span.start));
                }
            }
            Event::End(tag) => {
                let Some(kind) = end_kind(tag) else {
                    continue;
                };
                if let Some(index) = stack.iter().rposition(|(open, _)| *open == kind) {
                    let (_, start) = stack.remove(index);
                    spans.push(start..span.end);
                }
            }
            Event::Code(_) => spans.push(span),
            _ => {}
        }
    }
    spans
}

fn markdown_to_plain_text(markdown: &str) -> String {
    fn push_newline(output: &mut String) {
        if output.ends_with('\t') {
            output.pop();
        }
        if !output.is_empty() && !output.ends_with('\n') {
            output.push('\n');
        }
    }

    let mut output = String::new();
    for event in Parser::new_ext(markdown, markdown_options()) {
        match event {
            Event::Text(text) | Event::Code(text) | Event::Html(text) | Event::InlineHtml(text) => {
                output.push_str(&text)
            }
            Event::SoftBreak => output.push(' '),
            Event::HardBreak => output.push('\n'),
            Event::FootnoteReference(label) => {
                output.push('[');
                output.push_str(&label);
                output.push(']');
            }
            Event::End(
                TagEnd::Paragraph
                | TagEnd::Heading(_)
                | TagEnd::Item
                | TagEnd::CodeBlock
                | TagEnd::TableHead
                | TagEnd::TableRow,
            ) => push_newline(&mut output),
            Event::End(TagEnd::TableCell) => {
                if !output.ends_with('\t') && !output.ends_with('\n') {
                    output.push('\t');
                }
            }
            Event::Rule => push_newline(&mut output),
            Event::TaskListMarker(_) | Event::Start(_) | Event::End(_) => {}
            _ => {}
        }
    }
    if !markdown.ends_with('\n') && !markdown.ends_with('\r') {
        while output.ends_with('\n') {
            output.pop();
        }
    }
    output
}

/// Error returned by clipboard operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClipboardError {
    /// The payload exceeds the application's clipboard limit (100 KiB).
    TooLarge,
    /// The terminal refused the write (e.g. not supported).
    NotSupported,
    /// A generic I/O or encoding error.
    Other(String),
}

impl fmt::Display for ClipboardError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ClipboardError::TooLarge => write!(f, "payload exceeds 100 KiB clipboard limit"),
            ClipboardError::NotSupported => write!(f, "clipboard not supported by terminal"),
            ClipboardError::Other(msg) => write!(f, "{msg}"),
        }
    }
}

impl std::error::Error for ClipboardError {}

/// A sink for clipboard write operations.
///
/// The TUI implements this via OSC 52 escape emission; headless hosts may
/// write to stdout, a file, or drop the data.
pub trait ClipboardSink {
    /// Write `text` to the system clipboard.
    fn copy(&mut self, text: &str) -> Result<(), ClipboardError>;
}

/// A recording clipboard sink for tests — captures every copy call.
///
/// Implements [`ClipboardSink`] by storing each payload in a `Vec<String>`.
#[derive(Debug, Default)]
pub struct RecordingClipboardSink {
    captures: Vec<String>,
}

impl RecordingClipboardSink {
    /// Return all captured payloads in order.
    pub fn captures(&self) -> &[String] {
        &self.captures
    }

    /// Clear all captured payloads.
    pub fn clear(&mut self) {
        self.captures.clear();
    }
}

impl ClipboardSink for RecordingClipboardSink {
    fn copy(&mut self, text: &str) -> Result<(), ClipboardError> {
        // Recording sink accepts anything (no size limit).
        self.captures.push(text.to_string());
        Ok(())
    }
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clipboard_content_preserves_markdown_and_renders_plain_text() {
        let markdown = "# `App` and *emphasis* with [link](https://example.test) &amp; \\*\n";
        let content = ClipboardContent::from_markdown(markdown.to_string());

        assert_eq!(content.markdown(), markdown);
        assert_eq!(content.plain_text(), "App and emphasis with link & *\n");
    }

    #[test]
    fn plain_text_uses_logical_table_rows_without_markdown_borders() {
        let content = ClipboardContent::from_markdown(
            "| Name | Value |\n| --- | --- |\n| alpha | `one` |\n".to_string(),
        );

        assert_eq!(content.plain_text(), "Name\tValue\nalpha\tone\n");
    }

    #[test]
    fn invariant_content_has_equal_representations() {
        let content = ClipboardContent::invariant("https://example.test".to_string());
        assert_eq!(content.markdown(), content.plain_text());
    }

    #[test]
    fn recording_sink_captures_payload() {
        let mut sink = RecordingClipboardSink::default();
        sink.copy("hello").unwrap();
        sink.copy("world").unwrap();
        assert_eq!(sink.captures(), &["hello", "world"]);
    }

    #[test]
    fn recording_sink_clear() {
        let mut sink = RecordingClipboardSink::default();
        sink.copy("hello").unwrap();
        sink.clear();
        assert!(sink.captures().is_empty());
    }

    #[test]
    fn clipboard_error_display() {
        assert_eq!(
            format!("{}", ClipboardError::TooLarge),
            "payload exceeds 100 KiB clipboard limit"
        );
        assert_eq!(
            format!("{}", ClipboardError::NotSupported),
            "clipboard not supported by terminal"
        );
        assert_eq!(
            format!("{}", ClipboardError::Other("boom".to_string())),
            "boom"
        );
    }
}
