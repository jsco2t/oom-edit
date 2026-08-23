# Task 003: Fit rendered tables to an 80-column floor

Delegation: worker-eligible

## Goal

Make rendered Markdown tables deterministically fit an 80-display-cell minimum layout instead of allowing independent 40-cell columns to produce approximately 120-cell tables.

## Context

The core already wraps a cell after its independently computed width, but it never budgets the sum of borders, padding, and all columns against the available rendered width. This task adds width allocation inside the table block only; Task 004 will expose content when a viewport is narrower than the 80-cell floor.

## Scope

### In scope

- Passing the current renderer-neutral content width into table layout.
- Deterministic allocation of the available table width across columns, retaining the existing natural-width cap and alignment behavior.
- Cell wrapping at allocated display widths.
- Border/separator consistency, Unicode display-width handling, source provenance, and rendered snapshot tests.

### Out of scope

- Horizontal viewport state or screen cropping.
- Changing the supported pipe-table Markdown grammar.
- Multiline/grid/HTML table support.
- Changing wrapping widths for non-table blocks.

## Implementation requirements

- Use `max(available_width, 80)` display cells as the table's total target width. Account for all vertical borders and one-cell padding on both sides of every column before distributing the content budget.
- Start from natural cell widths capped by the existing `CELL_CAP`, then shrink deterministically until the total fits the target. Preserve a nonzero content width per column when structural overhead permits; document and test the unavoidable overflow case where borders/padding alone exceed the target.
- Never split UTF-8. Use display width for wide and combining characters, and keep all data and border rows at identical display width.
- Preserve alignment on the first wrapped row, existing continuation-row behavior, inline semantic styles, exact source ranges, and source-less generated borders/padding.
- Do not inflate small natural tables merely to fill the target.

## Acceptance criteria

- [ ] A representative table that currently renders near 120 cells renders at no more than 80 display cells with an 80-cell or narrower available width.
- [ ] The same table may use more width, up to the supplied budget, on a wider surface without exceeding the per-cell cap.
- [ ] Narrowed cells wrap all content without loss, and every top/separator/data/bottom row has identical display width.
- [ ] Left, center, and right alignment remain correct after shrinking and wrapping.
- [ ] CJK, combining-mark, escaped/entity, inline-code, repeated-text, and link-containing cells preserve byte-exact source provenance and semantic styles.
- [ ] Non-table rendered blocks continue to reflow at the actual supplied viewport width.

## Validation

- `make test`

## Dependencies

- Task 002

## Expected areas of change

- `crates/oom-edit-core/src/rendered/mod.rs`
- `crates/oom-edit-core/src/rendered/table.rs`
- `crates/oom-edit-core/src/rendered/tests/`
- `crates/oom-edit-core/tests/goldens/vw9_tables.txt`
- `crates/oom-edit/src/snapshot_tests.rs`
- `crates/oom-edit/tests/snapshots/rendered_table.txt`

## Risks / notes

A naive equal split wastes space and wraps short columns unnecessarily; a naive byte split corrupts Unicode and provenance. Keep the allocator narrow and deterministic rather than introducing a general layout solver.
