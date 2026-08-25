# Task 002: Source-driven wrapped-table selection

Delegation: main-only

## Goal

Make rendered character selection follow canonical selected source text through
wrapped table cells without highlighting or operating on unrelated table text.

## Context

Wrapped tables are visually row-major but Markdown source is cell-major within
each logical row. The current character-selection display projection treats the
endpoint span as a continuous screen interval and can cross unrelated cells or
rows. Task 001 establishes correct mapped fragments; this task makes selection
consume those mappings correctly.

## Scope

### In scope

- Character-selection source-range derivation and visual re-projection.
- Multiple independent display intervals on a rendered row where required.
- TUI painting of all selection intervals.
- Operator projection, resize remapping, UTF-8, and public API guard updates.

### Out of scope

- Changing Vim motion grammar or selection key bindings.
- Changing line-wise or block-wise selection semantics.
- Table row colors or Trouble presentation.

## Implementation requirements

- Treat canonical source ranges as the authority for character selection.
- Project selected ranges independently through source-backed rendered atoms;
  never fill synthetic table borders, padding, or unrelated atoms merely
  because they lie between screen coordinates.
- Represent and paint every independent interval required on a visual row.
- Keep `vim.rs` isolated from rendered-layout DTOs; translate to the private
  `ProjectedSelection` operation model in `session.rs`.
- Preserve exact source operation ranges for forward and reverse selection,
  endpoint swapping, word and vertical motions, yank/delete/change, and layout
  rebuilds.
- Update curated crate-root compile/API guards for any deliberate public DTO
  shape change.
- Add regressions for selection within a wrapped cell beside populated cells,
  crossing continuation rows and logical rows, reversed endpoints, Unicode,
  resize, and all three selection shapes.

## Acceptance criteria

- [ ] Character selection confined to one wrapped table cell paints only the selected fragments in that cell on every continuation row.
- [ ] Populated neighboring cells and adjacent logical rows never receive a selection carrier unless their source atoms are selected.
- [ ] Forward and reverse selections expose identical normalized source ranges, and yank/delete/change use those exact ranges once.
- [ ] A selection can require multiple display intervals on one row without collapsing them into a source-unrelated envelope.
- [ ] Character selection remains stable across width changes; line and block selections retain their existing projection and operator behavior.
- [ ] UTF-8 boundaries, wide display groups, synthetic provenance, and public API guards remain valid.

## Validation

- `make test`

## Dependencies

- 001

## Expected areas of change

- `crates/oom-edit-core/src/rendered/nav.rs`
- `crates/oom-edit-core/src/session.rs`
- `crates/oom-edit-core/src/style.rs`
- `crates/oom-edit-core/tests/conformance/mod.rs`
- `crates/oom-edit-core/tests/session_integration.rs`
- `crates/oom-edit-core/tests/public_api.rs`
- `crates/oom-edit/src/screens/rendered.rs`
- `crates/oom-edit/src/snapshot_tests.rs`
- `crates/oom-edit/tests/snapshots/rendered_select.txt`

## Risks / notes

Do not solve this by deriving source from rendered substrings. Display and
operation projections must agree, but the consumer-owned Vim DTO boundary must
remain intact. Block selection still intentionally describes a rectangle and
must not inherit character-selection interval semantics.
