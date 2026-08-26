//! Deterministic capacity accounting shared by performance gates.

use std::mem::size_of;

use oom_edit_core::{JumpTarget, RenderedLayout, RenderedLine, RenderedSourceAtom, Span};

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
