//! Deterministic capacity accounting shared by performance gates.

use std::mem::size_of;

use oom_edit_core::{
    AnalysisDiagnostic, FrontMatter, JumpTarget, MarkdownAnalysis, RenderedLayout, RenderedLine,
    RenderedSourceAtom, Span, Value,
};

/// Lower bound for heap capacity retained by one read-only analysis result.
/// Parser scratch allocations are excluded; the body remains borrowed.
pub(crate) fn analysis_retained_bytes(analysis: &MarkdownAnalysis<'_>) -> usize {
    let mut bytes = analysis
        .first_h1
        .as_ref()
        .map_or(0, |heading| heading.text.capacity());
    bytes += analysis.diagnostics.capacity() * size_of::<AnalysisDiagnostic>();
    bytes += analysis
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic.message.capacity())
        .sum::<usize>();
    if let FrontMatter::Yaml(Ok(value)) | FrontMatter::Toml(Ok(value)) = &analysis.front_matter {
        bytes += value_retained_bytes(value);
    }
    bytes
}

fn value_retained_bytes(value: &Value) -> usize {
    match value {
        Value::Str(text) => text.capacity(),
        Value::Seq(items) => {
            items.capacity() * size_of::<Value>()
                + items.iter().map(value_retained_bytes).sum::<usize>()
        }
        Value::Map(entries) => entries
            .iter()
            .map(|(key, value)| key.capacity() + value_retained_bytes(value))
            .sum(),
        Value::Num(_) | Value::Bool(_) => 0,
    }
}

/// Count heap capacity owned directly or transitively by a rendered layout.
pub(crate) fn rendered_layout_heap_bytes(layout: &RenderedLayout) -> usize {
    let mut bytes = layout.lines.capacity() * size_of::<RenderedLine>();
    for line in &layout.lines {
        bytes += line.styled.text.capacity();
        bytes += line.styled.spans.capacity() * size_of::<Span>();
        bytes += line.atoms.capacity() * size_of::<RenderedSourceAtom>();
    }
    bytes += layout.line_numbers.capacity() * size_of::<Option<usize>>();
    bytes += layout.jump_targets.capacity() * size_of::<JumpTarget>();
    bytes += layout.link_index.capacity() * size_of::<(usize, String)>();
    bytes += layout
        .link_index
        .iter()
        .map(|(_, destination)| destination.capacity())
        .sum::<usize>();
    bytes
}

/// Whether `larger` is at most 2.25 times `smaller`, using exact integers.
pub(crate) fn within_large_layout_scaling(smaller: u128, larger: u128) -> bool {
    larger.saturating_mul(4) <= smaller.saturating_mul(9)
}
