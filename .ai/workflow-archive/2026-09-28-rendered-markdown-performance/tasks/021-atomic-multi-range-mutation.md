# Task 021: Atomic multi-range live mutation

Delegation: main-only

## Goal

Apply ordinary projected disjoint `d`/`c` edits as one authoritative text and derived-cache transaction without repeated whole-document work.

## Context

Three sequential edits currently cause three highlighter/model refreshes. The Vim fallback clones and replaces the whole buffer. This task owns the mutation gateway and source-analysis side; task 022 owns rendered publication.

## Scope

### In scope

- Bounded multi-range mutation inside the private Vim wrapper with one undo/register effect and exact sequential edit coordinates.
- A batch-aware `LiveDocument` transaction and source analysis that applies all tree edits, then performs the necessary final-text parse/rediscovery once.
- Unit/property/differential tests for disjoint selections and wide semantic fallbacks.

### Out of scope

- A new public mutable session escape, separate text buffer, or relaxed selection semantics.
- Assuming all parser changes stay within the edited ranges.

## Implementation requirements

- Preserve the current `VimEffect::Edited` sequential-coordinate contract, canonical `VimCore` buffer, register contents, clipboard effects and one-step undo/redo.
- Keep synchronous source highlighting and front matter accurate before the mutation gateway returns.
- Reuse unchanged injection parse trees where safe; invalidate and widen for delimiter, language, reference and multiline-syntax propagation.
- Avoid routine full-buffer `replace_all` for a short projected multi-range deletion and verify text equivalence to the former reference path.

## Acceptance criteria

- [ ] Ordinary exact-fixture prose selection emits the correct ordered edits and one undo while avoiding routine full-buffer replacement.
- [ ] The highlighter performs no repeated full Markdown parse/rediscovery for the same logical mutation; source spans and front matter match a fresh highlighter after every operation.
- [ ] Selection yank/copy output, partial-line semantics, Unicode, CRLF, fence and global-semantic fallbacks remain exact.
- [ ] No regression in the Rust/Go 100-cycle <50 ms gate or core-only/public-pane behavior.

## Validation

- `make test-incremental`
- `make bench-acceptance-1mb-prose-cycles-record`
- `make bench-acceptance-1mb-edit-cycles`
- `make check`

## Dependencies

020

## Expected areas of change

`crates/oom-edit-core/src/vim.rs`, `crates/oom-edit-core/src/session/live_document.rs`, `crates/oom-edit-core/src/syntax/mod.rs`, `crates/oom-edit-core/tests/`

## Risks / notes

An edit list is ordered relative to each intermediate text. A batch implementation must not reinterpret all ranges against the original text or publish an intermediate derived state.
