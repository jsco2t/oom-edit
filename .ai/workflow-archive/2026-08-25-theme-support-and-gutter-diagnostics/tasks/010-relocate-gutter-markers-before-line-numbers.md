# Task 010: Relocate Gutter Markers Before Line Numbers

Delegation: main-only

## Goal

Move every diagnostic glyph from the trailing content gap to the leading gutter cell so marked rows read `marker + aligned number + separator` in both source and rendered modes without changing gutter width or document geometry.

## Context

Round 1 completed the themed gutter and bounded per-tab marker projection, but acceptance feedback identified the marker order as incorrect. The shared painter currently computes the marker column from the start of the two-cell trailing content gap, producing the reported `  73W|Line of text`. The requested contract is `W 73 |Line of text`, with the severity signal first and the full number/separator preserved.

## Scope

### In scope

- Shared gutter span/cell composition for marked and unmarked numbered rows.
- Exact marker, padding, absolute/relative number, and trailing separator placement.
- Source Insert and rendered Normal/Select/Command presentation through the existing shared renderer.
- Theme/accessibility styling, wrapping/synthetic rows, viewport/horizontal scrolling, digit boundaries, and clipped widths.
- Focused README wording and regression tests/snapshots affected by the intentional layout change.

### Out of scope

- Gutter width, source-text origin, viewport calculations, line-number rules, or relative-number semantics.
- Marker severity/glyph mapping, diagnostic projection/publication, Trouble behavior, App ownership, or theme palettes.
- New dependencies, public/core APIs, performance evidence schemas, rebaseline, or candidate-trial replacement.

## Implementation requirements

- Keep `status_bar::gutter_width`, `GUTTER_CONTENT_GAP`, source/rendered text areas, and cursor coordinates unchanged.
- Compose a nonempty diagnostic marker by replacing the first gutter alignment cell; retain blank gutter background there when the numbered row has no marker.
- Keep every remaining formatter-produced number/sign/padding cell at its existing terminal coordinate and leave the final content separator gap clear; do not insert or shift a column.
- Preserve `UiSlot::GutterBackground`, ordinary/current number styling, severity role styling, and the fixed marker modifier without style bleed between cells.
- Continuation, synthetic, and below-document rows remain fully gutter-background-filled and marker-free.
- For a clipped nonzero gutter, the marker is the first visible cell and all remaining composition is bounded by the provided `Rect`.
- Keep render work viewport-bounded and read-only; do not modify marker snapshots, projection counters, or retained memory during paint.
- Update exact tests rather than weakening cell assertions or broad snapshots.

## Acceptance criteria

- [ ] A warning on line 73 produces the exact conceptual row `W 73 |Line of text`; without a warning the row remains `  73 |Line of text`, and both bodies begin at the same column.
- [ ] Error, warning, info, and hint markers occupy column zero and retain exact foreground/background/modifier behavior at TrueColor, ANSI-16, and Monochrome tiers.
- [ ] Absolute and hybrid-relative gutter matrices preserve signs, digits, current-line styling, and alignment at 9/10/999/1000 boundaries without overwriting the separator.
- [ ] Source wrapping and rendered synthetic rows show markers only on numbered physical-source rows; rendered horizontal scrolling does not move the marker.
- [ ] Width-zero/one/two and other clipped areas are panic-free and bounded; a present marker is the first visible cell whenever width is nonzero.
- [ ] Existing gutter width, body/text origin, cursor mapping, viewport width, App publication/invalidation, and render quiescence/memory assertions remain unchanged.
- [ ] README documentation states that the severity glyph precedes the aligned line number; no unrelated docs, API, dependency, lockfile, vendor, theme, or evidence changes occur.

## Validation

- `make test`
- `make bench-check`

## Dependencies

- Task 009

## Expected areas of change

- `crates/oom-edit/src/screens/editor.rs`
- `crates/oom-edit/src/screens/rendered.rs`
- `crates/oom-edit/src/app.rs`
- `crates/oom-edit/src/snapshot_tests.rs` and intentional golden fixtures if affected
- `README.md`

## Risks / notes

The formatter's leading padding doubles as the hybrid sign/alignment field, so the implementation must relocate the marker without deleting or shifting a `+`/`-` sign or changing total width. The source and rendered screens already share the painter; do not introduce mode-specific geometry paths. Narrow-terminal behavior must be explicitly asserted rather than inferred from normal-width output.
