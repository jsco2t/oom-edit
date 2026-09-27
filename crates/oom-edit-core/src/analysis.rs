//! Lightweight, read-only Markdown analysis over borrowed source.

use std::ops::Range;

use pulldown_cmark::{Event, HeadingLevel, Parser, Tag, TagEnd};

use crate::frontmatter::{front_matter_span, parse_front_matter, FrontMatter};
use crate::rendered::markdown_options;

/// One parser diagnostic with an exact source byte span.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnalysisDiagnostic {
    /// Human-readable parser detail.
    pub message: String,
    /// UTF-8 byte range of the affected source construct.
    pub source_span: Range<usize>,
}

/// Decoded text and source ownership of the first top-level H1.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnalysisHeading {
    /// Visible heading text after Markdown escape/entity decoding.
    pub text: String,
    /// UTF-8 byte range of the complete heading construct.
    pub source_span: Range<usize>,
}

/// Owned metadata and spans alongside a borrowed, unchanged body slice.
#[derive(Debug)]
pub struct MarkdownAnalysis<'a> {
    /// Parsed YAML/TOML metadata, if present.
    pub front_matter: FrontMatter,
    /// UTF-8 byte range of the leading metadata construct, when present.
    pub front_matter_span: Option<Range<usize>>,
    /// Borrowed body bytes; malformed metadata leaves the entire source as body.
    pub body: &'a str,
    /// UTF-8 byte range in the original source corresponding to `body`.
    pub body_span: Range<usize>,
    /// First top-level H1, excluding fenced and nested examples.
    pub first_h1: Option<AnalysisHeading>,
    /// Metadata diagnostics; Markdown body parsing remains non-fatal.
    pub diagnostics: Vec<AnalysisDiagnostic>,
}

/// Analyze Markdown without allocating an editor or changing the borrowed source.
pub fn analyze_markdown(source: &str) -> MarkdownAnalysis<'_> {
    let front_matter = parse_front_matter(source);
    let front_matter_span = front_matter_span(source);
    let malformed = match &front_matter {
        FrontMatter::Yaml(Err(error)) | FrontMatter::Toml(Err(error)) => Some(error.message()),
        _ => None,
    };
    let diagnostics = malformed
        .map(|message| AnalysisDiagnostic {
            message: message.to_string(),
            source_span: front_matter_span.clone().unwrap_or(0..source.len()),
        })
        .into_iter()
        .collect();
    let body_start = if malformed.is_some() {
        0
    } else {
        front_matter_span.as_ref().map_or(0, |span| span.end)
    };
    let body_span = body_start..source.len();
    let body = &source[body_span.clone()];
    let first_h1 = first_top_level_h1(body, body_start);
    MarkdownAnalysis {
        front_matter,
        front_matter_span,
        body,
        body_span,
        first_h1,
        diagnostics,
    }
}

fn first_top_level_h1(body: &str, source_offset: usize) -> Option<AnalysisHeading> {
    let mut container_depth = 0usize;
    let mut heading_start = None;
    let mut heading_text = String::new();
    for (event, range) in Parser::new_ext(body, markdown_options()).into_offset_iter() {
        match event {
            Event::Start(
                Tag::BlockQuote(_) | Tag::List(_) | Tag::Item | Tag::FootnoteDefinition(_),
            ) => {
                container_depth += 1;
            }
            Event::End(
                TagEnd::BlockQuote(_) | TagEnd::List(_) | TagEnd::Item | TagEnd::FootnoteDefinition,
            ) => {
                container_depth = container_depth.saturating_sub(1);
            }
            Event::Start(Tag::Heading {
                level: HeadingLevel::H1,
                ..
            }) if container_depth == 0 => {
                heading_start = Some(range.start);
                heading_text.clear();
            }
            Event::End(TagEnd::Heading(HeadingLevel::H1)) if heading_start.is_some() => {
                return Some(AnalysisHeading {
                    text: heading_text.trim().to_string(),
                    source_span: heading_start.unwrap() + source_offset..range.end + source_offset,
                });
            }
            Event::Text(text)
            | Event::Code(text)
            | Event::Html(text)
            | Event::InlineHtml(text)
            | Event::FootnoteReference(text)
                if heading_start.is_some() =>
            {
                heading_text.push_str(&text);
            }
            Event::SoftBreak | Event::HardBreak if heading_start.is_some() => {
                heading_text.push(' ');
            }
            _ => {}
        }
    }
    None
}
