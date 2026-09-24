# Task 003: Preserve loose-list item spacing

Delegation: main-only

## Goal

Retain blank-line separation between Markdown list items in rendered modes
whenever the authoritative block model classifies the list as loose.

## Context

The parser already computes `BlockKind::List { tight }` from CommonMark item
structure and source gaps. The renderer currently binds that field as
`tight: _tight` and ignores it, so source-separated bullets collapse together.

## Scope

### In scope

- Thread the existing `tight` value through list rendering.
- Insert one source-less blank presentation row between sibling items of a
  loose list at the correct nesting depth.
- Preserve tight-list output, nested-list ordering/indentation, task markers,
  wrapping, line numbers, and provenance.
- Add focused block-model/layout tests and update affected goldens/snapshots.

### Out of scope

- Changing CommonMark parsing or the authoritative Markdown specification.
- Adding blank rows to tight lists or indiscriminately between every child
  block inside a list item.
- Restyling list markers.

## Implementation requirements

- Add a failing source-to-layout regression for blank-line-separated bullets
  before the renderer change.
- Use the parser-owned `tight` flag; do not rescan source gaps in the renderer.
- The added blank row must be synthetic, contain no source atoms, and claim no
  source bytes even when nested or prefixed.
- Cover unordered, ordered, nested loose, task, and unchanged tight lists.

## Acceptance criteria

- [ ] Blank-line-separated unordered and ordered sibling items render with exactly one blank row between items.
- [ ] Tight lists render with no new blank rows.
- [ ] Nested loose lists preserve separation at their own depth without duplicate parent padding or prefixed whitespace on the blank row.
- [ ] Markers, continuation indentation, source mapping, line numbers, and task-item styling remain correct.

## Validation

- `cargo test -p oom-edit-core --offline --locked loose_list_preserves_blank_item_separation`
- `cargo test -p oom-edit-core --offline --locked list_`
- `cargo test -p oom-edit-core --offline --locked golden_vw6_bulleted_lists`
- `make check`

## Dependencies

- 002

## Expected areas of change

- `crates/oom-edit-core/src/rendered/mod.rs`
- `crates/oom-edit-core/src/rendered/tests/blocks.rs`
- `crates/oom-edit-core/src/rendered/tests/goldens.rs`
- affected rendered/TUI snapshot fixtures

## Risks / notes

Top-level block separation is already added elsewhere. The implementation must
avoid double blanks at list boundaries and must not run a synthetic blank row
through prefix code that would make it appear as indentation-filled content.
