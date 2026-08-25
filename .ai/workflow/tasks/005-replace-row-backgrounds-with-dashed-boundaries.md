# Task 005: Replace row backgrounds with dashed boundaries

Delegation: main-only

## Goal

Remove the alternating table-row background feature completely and replace it
with faint, source-less dashed lines between adjacent rendered table body rows.

## Context

Task 003 introduced configurable alternating backgrounds, but the user has
superseded that approach because its drawbacks outweigh its readability gain.
The core table builder already owns logical row grouping, wrapping, allocated
column widths, and synthetic borders, so it is the canonical place to create a
row boundary whose geometry tracks layout width. This task supersedes Task
003's final behavior while preserving Tasks 001, 002, and 004.

## Scope

### In scope

- Complete removal of alternating-row core, config, App, renderer, theme,
  documentation, API-guard, test, and snapshot behavior.
- Synthetic ASCII-dash boundary rows between adjacent logical body rows.
- Muted/faint styling for dash glyphs using the existing semantic style model.
- Wrapped-row, provenance, selection/navigation, width, resize, and snapshot
  regression coverage.

### Out of scope

- Changing Markdown table parsing or source syntax.
- Changing the solid header/body separator or outer table borders.
- Changing column allocation, cell padding, alignment, or the 40-column cap.
- Adding a setting or runtime command for dashed boundaries.
- Altering word-boundary wrapping, wrapped-table selection semantics, Trouble
  behavior, or unrelated theme colors.

## Implementation requirements

- Remove `RenderedLineRole::TableAlternateRow`, `UiSlot::TableAlternateRow`,
  `EditorConfig::alternating_table_rows`, App/render settings and constructor
  parameters, README documentation, theme rows, and obsolete striping tests.
- Restore ordinary `RenderedLineRole::Document` for every table line unless an
  independently established role applies. Update curated public API and
  exhaustive registry guards for the removal.
- Insert one boundary after every body row except the last, only after all of
  that logical row's wrapped continuation lines. Do not add a dashed boundary
  after the header, inside a wrapped row, or before the bottom border.
- Retain the table's vertical edge and inter-column border glyphs. Fill each
  allocated column interior, including its table-owned padding allocation,
  completely with ASCII `-` glyphs so the boundary has exactly the same display
  width as every other table row and never extends outside the table.
- Style only the dash fragments with `SemanticStyle::Muted`; keep border glyphs
  on the ordinary table style. Do not add a new theme slot or color.
- Every boundary atom must be synthetic (`source: None`). Its rendered line is
  synthetic and uses the existing nearest-preceding-content source convention
  only for line-level navigation metadata.
- Rebuild boundary geometry from the same current `col_widths` used by data
  rows. Add a non-vacuous resize regression whose inputs force different
  allocations at two widths and prove the boundary changes with the table.
- Preserve exact selection/operator behavior: dash glyphs cannot enter
  character, line, or block source ranges, and navigation across them remains
  deterministic. Update focused tests and intentional goldens/snapshots.
- Older config text containing `alternating_table_rows` may continue to parse
  through existing unknown-field compatibility, but serialization and runtime
  state must expose no such feature.

## Acceptance criteria

- [ ] No alternating-row setting, runtime field, core role, theme slot,
  background composition, README entry, or obsolete striping test remains.
- [ ] Tables with zero or one body row contain no dashed body boundary; tables
  with N body rows contain exactly N-1 boundaries.
- [ ] Wrapped logical rows receive exactly one boundary after their last visual
  continuation and none between continuations.
- [ ] Each boundary uses ordinary vertical borders plus ASCII dashes filling
  every column interior, matches the table display width, and dash spans resolve
  through `SemanticStyle::Muted` at every built-in theme/capability tier.
- [ ] Boundary lines and atoms are source-less, remain outside selected source
  ranges and operator payloads, and do not destabilize rendered navigation.
- [ ] Re-rendering the same table after a width-changing resize recomputes both
  table column allocation and boundary dash lengths correctly.
- [ ] Public API guards and intentional golden/snapshot fixtures are current,
  and the existing Tasks 001, 002, and 004 behavior remains green.

## Validation

- `make test`

## Dependencies

- 001
- 002
- 003
- 004

## Expected areas of change

- `README.md`
- `crates/oom-edit-core/src/rendered/table.rs`
- `crates/oom-edit-core/src/rendered/mod.rs`
- `crates/oom-edit-core/src/rendered/tests/blocks.rs`
- `crates/oom-edit-core/src/style.rs`
- `crates/oom-edit-core/tests/public_api.rs`
- `crates/oom-edit-core/tests/session_integration.rs`
- `crates/oom-edit-core/tests/goldens/vw9_tables.txt`
- `crates/oom-edit/src/config.rs`
- `crates/oom-edit/src/lib.rs`
- `crates/oom-edit/src/app.rs`
- `crates/oom-edit/src/screens/rendered.rs`
- `crates/oom-edit/src/theme.rs`
- `crates/oom-edit/src/snapshot_tests.rs`
- `crates/oom-edit/tests/snapshots/rendered_table.txt`

## Risks / notes

The boundary is a complete synthetic layout row, not source content and not a
surface background. Tests must count logical rows rather than visual lines,
especially when cells wrap to different heights. A resize test must exceed the
80-column table floor and use naturally wide cells so the allocated column
widths actually differ.
