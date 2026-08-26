# Task 011: Reduce Gutter Right Padding

Delegation: main-only

## Goal

Remove one trailing blank cell between the aligned line number and document text in both source and rendered modes while preserving the round-2 leading diagnostic marker exactly as implemented.

## Context

Round 2 established the accepted `marker + aligned number + separator` order, but the shared formatter still reserves a two-cell content gap. This produces `  73  |Line of text` for an unmarked row and `W 73  |Line of text` for a warning row. The requested compact layout is `  73 |Line of text` and `W 73 |Line of text`: only the final trailing blank cell is removed.

## Scope

### In scope

- The shared gutter content-gap constant, formatter width, and derived source/rendered layout geometry.
- Exact marked and unmarked gutter strings, absolute/relative number alignment, digit boundaries, and narrow clipping.
- Source/rendered document origin, viewport, cursor, wrapping, horizontal-scroll, selection/provenance, and snapshot expectations affected by the intentional one-column reduction.
- Focused regression tests and reviewed golden fixtures affected solely by the gutter shift.

### Out of scope

- Diagnostic marker position, glyphs, severity mapping, styling, projection, publication, snapshot ownership, or Trouble behavior.
- Line-number policy, relative-number semantics, number-field width, leading alignment, gutter theme palettes, or new configuration.
- Documentation-schema, core/public API, dependency, lockfile, vendor, license, or retained performance-evidence changes.

## Implementation requirements

- Change the shared gutter content gap from two cells to one; do not add a renderer-specific width adjustment.
- Preserve the formatter's existing number field width and right alignment. Absolute, current, and signed hybrid-relative labels must lose only one trailing blank cell.
- Keep a present diagnostic marker in gutter column zero and retain its existing glyph, style, modifier, background, and precedence. Do not change marker selection or placement logic unless a test exposes a purely mechanical compatibility issue from the shorter formatted row.
- Reduce the computed gutter width and source/rendered body origin by exactly one cell. All cursor, viewport, wrapping, scrolling, selection, and provenance calculations must use the shared derived width.
- Preserve gutter-background fill, continuation/synthetic-row behavior, and bounded clipped rendering for zero and narrow rectangles.
- Update exact tests and snapshots rather than weakening cell assertions or accepting unrelated golden changes.
- Keep render work viewport-bounded/read-only and preserve all performance budgets and evidence formats.

## Acceptance criteria

- [ ] Line 73 has exact unmarked gutter `  73 ` and exact warning gutter `W 73 `, composing as `  73 |Line of text` and `W 73 |Line of text` respectively.
- [ ] The diagnostic marker remains at column zero with unchanged `E`/`W`/`I`/`H` glyph, severity priority, foreground, gutter background, modifier, and monochrome accessibility behavior in source and rendered modes.
- [ ] `gutter_width` is exactly 5 cells for documents ending at lines 9, 10, and 100, and 6 cells for line 1000; the absolute/relative number field retains its previous width and alignment.
- [ ] Exact absolute, current-line, and signed hybrid-relative matrices at 9/10/999/1000 boundaries contain one trailing content-gap cell and never lose a sign or digit.
- [ ] Source and rendered body origins move exactly one column left, available text widths gain exactly one column, and cursor, viewport, wrapping, selection/provenance, and horizontal scrolling remain internally consistent.
- [ ] Wrapped, synthetic, and below-document rows remain marker-free where applicable and retain complete gutter-background styling; zero and narrow areas remain bounded and panic-free.
- [ ] Updated golden fixtures show only the intended one-column gutter/body shift; no unrelated documentation, API, dependency, lockfile, vendor, theme, license, or evidence changes occur.

## Validation

- `make test`
- `make bench-check`

## Dependencies

- Task 010

## Expected areas of change

- `crates/oom-edit/src/widgets/status_bar.rs`
- `crates/oom-edit/src/screens/editor.rs`
- `crates/oom-edit/src/screens/rendered.rs`
- `crates/oom-edit/src/app.rs`
- `crates/oom-edit/src/snapshot_tests.rs` and intentional golden fixtures if affected

## Risks / notes

The shared constant already feeds gutter width and formatting in both renderers, so the smallest correct implementation should be localized. The verification burden is broader because that width also determines document origin, cursor placement, wrapping, and viewport capacity. Preserve the marker-first composition from round 2; this task changes spacing after the number, not diagnostic behavior.
