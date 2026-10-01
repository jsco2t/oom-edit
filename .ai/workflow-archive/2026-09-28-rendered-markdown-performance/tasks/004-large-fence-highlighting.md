# Task 004: Linear large-fence highlighting and source parse reuse

Delegation: main-only

## Goal

Prevent large highlighted fences from paying line-count-times-span-count work in rendered view or a full parse on every source viewport frame.

## Context

The two headline fixes leave a 512 KiB Rust fence at about 1.8 s. `highlight_snippet` scans every capture for every line; source highlighting reparses each overlapping injection on each frame.

## Scope

### In scope

- Sweep captures into intersecting lines with the same capture precedence and UTF-8 columns.
- Cache/version parsed injection results inside `Highlighter`, and refresh/invalidate through the existing edit gateway.
- Add long valid and malformed code fixtures, edit cases, style/provenance equivalence and memory measurements.

### Out of scope

- A second Markdown parser, another mutable text owner or terminal-side code highlighting.

## Implementation requirements

- Preserve `SemanticStyle::CodeBlock` fallback, capture order, overlap ranking, blank lines, trailing newline and CRLF behavior.
- Reuse parsed results only when the corresponding source range and content generation are still valid; edits inside and outside a fence must be tested.
- Keep any retained parser state bounded by documented memory checks.

## Acceptance criteria

- [ ] Rendered styles and byte-exact source atoms match reference output across long code fixtures and aliases.
- [ ] Span-to-line work scales with actual intersections rather than all spans times all lines.
- [ ] Repeated unchanged source viewport frames do not reparse a whole overlapping fence; edits update output synchronously and correctly.
- [ ] Existing retained-layout/RSS gates remain green.

## Validation

- `make test-first-frame`
- `make bench-check`

## Dependencies

003

## Expected areas of change

`crates/oom-edit-core/src/syntax/mod.rs`, core syntax/property tests, `crates/oom-edit-core/perf/`

## Risks / notes

Parser error recovery dominates synthetic invalid Rust. Test real valid Rust separately so the optimization is not tuned only to malformed content.
