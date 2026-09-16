# Task 002: Compact terminal spacing

Delegation: main-only

## Goal

Use the smallest gutter geometry that retains the diagnostic marker, line number, and one-cell gap, and add a one-cell right inset to the status row.

## Context

Gutter width currently includes a fixed three-digit minimum, and the ruler and prompt reach the last terminal column.

## Scope

### In scope

- Gutter width, line-number alignment, source/rendered text-width calculation, and marker placement.
- One-cell right padding in ordinary status, ruler, prompt, and which-key presentation.
- Exact-cell tests for single-, multi-, and four-digit line counts, signed relative labels, markers, and narrow windows; affected snapshots.

### Out of scope

- Mouse hit testing, cursor movement, and shortcut changes.

## Implementation requirements

- Compute width from actual digits plus the marker column and a single content gap; account for signed relative labels without overwriting digits when a marker is present.
- Keep gutter width stable when a marker appears/disappears, within a viewport and numbering mode.
- Use one shared geometry calculation for source and rendered screens. Remove only redundant presentation padding, never source Markdown indentation.
- Reserve one rightmost status cell without losing the ruler or prompt cursor at ordinary widths; clamp safely for very narrow terminals.
- Update snapshots and existing layout tests to the new exact geometry.

## Acceptance criteria

- [ ] Unindented document text begins exactly one cell after the final gutter digit, in marked and unmarked rows.
- [ ] Short documents use a narrower gutter than the current fixed three-digit minimum; relative signs and four-digit labels remain intact.
- [ ] The rightmost status-row cell is blank in normal, prompt, and hint presentations when width permits, and narrow layouts do not panic.
- [ ] Rendered/source snapshots show only the intended layout shift.

## Validation

- `make test`

## Dependencies

001

## Expected areas of change

- `crates/oom-edit/src/widgets/status_bar.rs`
- `crates/oom-edit/src/screens/editor.rs`
- `crates/oom-edit/src/screens/rendered.rs`
- `crates/oom-edit/src/app.rs`
- `crates/oom-edit/src/snapshot_tests.rs`
- `crates/oom-edit/tests/snapshots/`

## Risks / notes

Terminal geometry is integral cells. A marked three-digit line with one gap cannot be less than five cells wide. Snapshot changes must be checked against this constraint.
