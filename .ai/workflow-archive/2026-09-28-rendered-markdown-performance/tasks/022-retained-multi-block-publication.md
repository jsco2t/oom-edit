# Task 022: Retained multi-block and row publication

Delegation: main-only

## Goal

Publish an ordinary multi-range mixed-Markdown edit and its complete current-text frame without a routine full-document model or row rebuild.

## Context

`PendingProjectionChange::Multiple` currently drops the row cache. Repeated model application may also rebuild the full document when a selected range crosses block boundaries or deletes line breaks.

## Scope

### In scope

- Batch affected Markdown-block reconstruction with exact dependency invalidation.
- Atomic retained-row publication against the final text, including source and row-offset rebasing.
- Bounded Select-operation projection where measured necessary to meet the total-frame limit.
- Full-build differential and property tests.

### Out of scope

- Returning a stale frame, delaying editable state, or changing the public complete-layout contract.
- Claiming local work for delimiter/reference/footnote edits that truly propagate globally.

## Implementation requirements

- Keep provenance attached to parser leaves and preserve exact UTF-8 source ranges through wrapping, tables and synthetic glyphs.
- Model affected block windows and document-wide links/footnotes explicitly; if boundaries or dependencies cannot be proven stable, use the correct wide path and record why.
- Publish a single final-text row/index update for ordinary disjoint edits. Existing single-fence row splice remains fast.
- Test `d`, `c`, undo/redo, headings, prose, lists, tables, wrap, CRLF, Unicode, references, footnotes and fence boundaries against complete rebuilds.

## Acceptance criteria

- [ ] The representative exact-fixture 15-line prose `d`/`c` cycles pass p99 <50 ms key-to-owned-frame and no sample ≥100 ms, with at least 100 cycles per operation.
- [ ] Counters show bounded ordinary multi-block work and no full 1 MiB model/layout rebuild for this case.
- [ ] Complete cells, styles, source atoms, cursor/selection, links/footnotes, line counts and one-step undo match the full-build oracle.
- [ ] Rust/Go fence edit, source-first, standalone and embedded paths remain unchanged in fidelity and within their budgets.

## Validation

- `make test-incremental`
- `make bench-acceptance-1mb-prose-cycles`
- `make bench-acceptance-1mb-edit-cycles`
- `make bench-interactions`
- `make check`

## Dependencies

021

## Expected areas of change

`crates/oom-edit-core/src/rendered/retained.rs`, `crates/oom-edit-core/src/rendered/rows.rs`, `crates/oom-edit-core/src/rendered/nav.rs`, `crates/oom-edit-core/src/session/live_document.rs`, `crates/oom-edit-core/src/session.rs`, `crates/oom-edit-core/tests/`

## Risks / notes

The aggregate result must be based on final authoritative text, not a sequence of visible intermediate layouts. If exact incremental invalidation cannot be proven for a construct, widen only that operation, not ordinary stable prose edits.
