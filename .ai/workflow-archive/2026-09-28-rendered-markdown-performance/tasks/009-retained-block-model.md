# Task 009: Retained Markdown model and global dependencies

Delegation: main-only

## Goal

Retain exact unchanged rendered Markdown blocks across edits while rebuilding
all locally and globally affected semantic blocks.

## Context

`BlockModel::build` currently reparses the entire text with pulldown-cmark on
every invalidated render. Task 007 supplies the proven local boundary rule;
Task 008 makes the source-side mutation transaction exact. The rendered model
also owns links, footnotes, front matter and navigation dependencies that may
reach beyond the edited block.

## Scope

### In scope

- Build a private retained block/source-span index updated from exact edits.
- Reparse the smallest proven-safe Markdown region, rebase following spans and
  preserve parser-leaf provenance for unchanged blocks.
- Track reference links, footnotes, jump targets and front-matter dependencies;
  invalidate dependents when definitions or structural boundaries change.
- Keep the existing complete `BlockModel::build` as the independent test
  oracle, and add differential/property tests for edits, undo and reload.

### Out of scope

- Width-dependent row layout, viewport painting or a new public parser API.

## Implementation requirements

- Retained blocks are derived data, not a second mutable text owner. The
  mutation transaction must publish a model for the current text or fail the
  operation safely; no stale model may drive editing or navigation.
- Exact UTF-8 source spans and leaf atoms must survive inserts/deletes before
  a block, repeated text, escapes/entities and nested containers.
- Document-wide references must be indexed, not silently treated as local.
  Correctly fall back to a wider synchronous rebuild for genuinely global
  edits and record its scope.
- No parser replacement or dependency addition without a separate
  plan-change review.

## Acceptance criteria

- [ ] Fresh full-model and retained-model comparisons pass for headings,
  paragraphs, lists, tables, fences, HTML, front matter, links and footnotes
  after deterministic and property-based edit sequences.
- [ ] The fixed 1 MiB ordinary local edit reuses unchanged blocks according
  to counters; global-definition and delimiter edits invalidate every affected
  dependent and match the fresh reference.
- [ ] Byte-exact source provenance and content transformation behavior remain
  unchanged, including Unicode, CRLF, ambiguous duplicate text and escapes.
- [ ] Reload and undo produce a current complete model without retaining stale
  spans or document-wide indices.

## Validation

- `make test-incremental`
- `make test-first-frame-leaves`
- `make test-realistic-performance`

## Dependencies

008

## Expected areas of change

`crates/oom-edit-core/src/rendered/blocks.rs`,
`crates/oom-edit-core/src/rendered/`,
`crates/oom-edit-core/src/session.rs`, `crates/oom-edit-core/tests/`

## Risks / notes

Pulldown's full-document parser may resolve constructs from distant text.
The retained model must prove its boundary rule against complete reparses;
changed source bytes alone are not sufficient invalidation evidence.
