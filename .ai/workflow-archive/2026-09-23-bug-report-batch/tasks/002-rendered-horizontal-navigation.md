# Task 002: Traverse rendered rows with horizontal motions

Delegation: main-only

## Goal

Allow rendered Left/`h` and Right/`l` motions to cross visual-row boundaries so
a user can reach the end of a wrapped sentence directly from the following
line.

## Context

`horizontal_point` currently collects source-backed atoms only from the active
rendered row and clamps within that row. Wrapped paragraphs therefore require
an Up motion followed by a full traversal. The layout already contains ordered,
source-backed atoms and source-less generated cells; navigation should operate
on that existing provenance.

## Scope

### In scope

- Cross-row previous/next atom traversal for `h`/Left and `l`/Right.
- Counted traversal across one or more rendered boundaries.
- Behavior across wrapped paragraphs, adjacent content rows, and source-less
  synthetic rows/padding.
- Normal and Select mode regression coverage, including canonical cursor and
  selection endpoint synchronization.

### Out of scope

- Source Insert arrow behavior or Vim's private source-line motion rules.
- Word-motion (`w`, `b`, `e`) semantics.
- Changing rendered layout or wrapping policy.

## Implementation requirements

- Add failing regression tests before implementation.
- Traverse only source-backed display atoms; borders, list markers, table
  padding, blank separators, and other synthetic output are never cursor stops.
- Preserve count semantics and symmetry at document boundaries.
- Commit the rendered cursor to the canonical source cursor through the
  existing session path and refresh Select endpoints without adding DTO imports
  to `vim.rs`.
- Add no parallel navigation state or external dependency.

## Acceptance criteria

- [ ] From the first source-backed atom on the line below a wrapped paragraph, Left and `h` land on the preceding applicable row's final source-backed atom.
- [ ] Right and `l` traverse the same boundary in reverse and all four inputs honor counts across multiple rows.
- [ ] Synthetic/source-less rows and glyphs are skipped rather than receiving cursor provenance.
- [ ] Normal canonical cursor state and rendered Select endpoints remain synchronized after cross-row motions.

## Validation

- `cargo test -p oom-edit-core --offline --locked horizontal_navigation_crosses_rendered_rows`
- `cargo test -p oom-edit-core --offline --locked rendered_horizontal`
- `cargo test -p oom-edit-core --offline --locked rendered_select`
- `make check`

## Dependencies

- 001

## Expected areas of change

- `crates/oom-edit-core/src/rendered/nav.rs`
- `crates/oom-edit-core/src/session.rs`
- `crates/oom-edit-core/tests/session_integration.rs`
- related conformance/property tests

## Risks / notes

Flattening every atom on every keypress is simple but may be unnecessarily
costly on large documents. Prefer a clear bounded row traversal using the
existing layout unless measurement shows a simpler representation is already
cheap. Correct provenance and cursor synchronization take priority over a new
index abstraction.
