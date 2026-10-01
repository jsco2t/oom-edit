# Task 003: Indexed rendered cursor mapping

Delegation: main-only

## Goal

Remove repeated source-prefix scans from rendered cursor mapping while preserving every mapping choice.

## Context

The fallback calls a newline-count callback for each rendered content row when the cursor lacks a source-backed atom. `Highlighter` already maintains source line starts across edits.

## Scope

### In scope

- Make session remapping use the maintained source line index or equivalent indexed offset lookup.
- Retain the current atom-first, containing-line, boundary and nearest-after precedence.
- Add differential/property tests for source-backed and source-less cursor positions and large scaling cases.

### Out of scope

- Changed navigation semantics, new public cursor DTOs or viewport-lazy layout.

## Implementation requirements

- Avoid building a duplicate full-document line index on every remap.
- Cover LF, CRLF, blank first/last lines, Unicode, escaped/entity text, table delimiters, fences, list markers, wrapping and Select anchors.
- Demonstrate indexed behavior with structural counters in addition to timings.

## Acceptance criteria

- [ ] Mapping is byte/row exact against a retained test reference over generated and fixed edge cases.
- [ ] No callback rescans `text[..offset]` for each rendered row; scaling tests reject the previous quadratic path.
- [ ] Core and public-pane cursor tests pass.

## Validation

- `make test-first-frame`
- `make test-pane-public`
- `make bench-check`

## Dependencies

002

## Expected areas of change

`crates/oom-edit-core/src/rendered/nav.rs`, `crates/oom-edit-core/src/session.rs`, `crates/oom-edit-core/src/syntax/mod.rs`, tests

## Risks / notes

The no-atom fallback has subtle tie-breaking; comparing only final line counts would miss cursor regressions.
