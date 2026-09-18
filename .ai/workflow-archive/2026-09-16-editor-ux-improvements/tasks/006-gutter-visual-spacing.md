# Task 006: Gutter visual spacing follow-up

Delegation: main-only

## Goal

Give the gutter marker visual space within its existing cell while keeping the separator between the line number and document content.

## Context

The round-one gutter uses a large `●` in the first cell, places the number immediately beside it, and reserves one blank cell after the number. The user chose a smaller dot with intrinsic side space, then clarified that the trailing separator must remain.

## Scope

### In scope

- The TUI gutter marker glyph and severity styling.
- Exact-cell tests that preserve the existing line-number/content separator and body start in source and rendered views.
- Changed snapshots for the marker glyph.

### Out of scope

- Diagnostic classification, theme color selection, status-bar spelling symbol, status right inset, and editor mode/keyboard behavior.
- Core text, selection, parsing, and provenance behavior.
- Gutter width, viewport/cursor geometry, and pointer coordinate changes.

## Implementation requirements

- Render the circular `•` marker in the first gutter cell using the existing severity theme roles and non-color error modifier. Keep its one-cell placement stable on marked and unmarked lines, and preserve a distinguishable gutter background.
- Preserve the trailing one-cell gap after the final line-number digit. A three-digit absolute document continues to use `1 + 3 + 1 = 5` gutter cells; source content begins immediately after that separator.
- Leave the shared width calculation, source/rendered layout, cursor placement, viewport width, and pointer coordinate translation unchanged.
- Preserve line numbers and signs at digit transitions; handle empty and clipped/narrow terminal areas without overflow or panic.
- Update exact-cell tests and snapshots for the new glyph while asserting the existing separator and body start. Inspect representative output against the round-two gutter screenshot for visible side space around the dot.

## Acceptance criteria

- [ ] The `•` marker has visible side space within its single cell in a representative terminal rendering, and warning/error theme styling and error modifier remain intact.
- [ ] A marked or unmarked three-digit absolute row uses a five-cell gutter with one blank cell after the final digit; unindented content begins in cell five.
- [ ] Relative labels and one-, two-, and four-digit number widths remain correct; marker presence never moves the body start.
- [ ] Source and rendered views, narrow windows, cursor placement, and mouse clicks at the gutter/document seam remain correct with existing geometry.
- [ ] Relevant exact-cell tests and snapshots pass, and the project quality gate passes.

## Validation

- `make test`

## Dependencies

002, 004

## Expected areas of change

- `crates/oom-edit/src/gutter.rs`
- `crates/oom-edit/src/widgets/status_bar.rs`
- `crates/oom-edit/src/screens/editor.rs`
- `crates/oom-edit/src/screens/rendered.rs`
- `crates/oom-edit/src/snapshot_tests.rs`
- `crates/oom-edit/tests/snapshots/`

## Risks / notes

Ratatui cannot specify pixel padding inside a terminal cell. The visual side space depends on terminal font metrics. The exact cell geometry and glyph are automated assertions; visual inspection checks the remaining font-dependent appearance. Keeping the separator means the gutter width does not shrink. No new dependency is expected.
