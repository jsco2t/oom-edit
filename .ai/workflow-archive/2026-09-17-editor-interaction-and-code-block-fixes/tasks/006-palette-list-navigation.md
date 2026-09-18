# Task 006: Palette list navigation

Delegation: main-only

## Goal

Restore keyboard access to every help-palette row and keep the focused row visible when the list scrolls.

## Context

Round 1 restricted Up/Down and Tab/BackTab to enabled App commands. The reference rows below them cannot be selected, so the viewport never scrolls to them.

## Scope

### In scope

- Navigate all filtered rows, including disabled and read-only reference rows.
- Render a visible focus glyph on the selected row, with a distinct marker for a read-only row.
- Keep focus visible as list height changes; preserve filter reset and clamping behavior.
- Replace tests that assert references are unreachable, and update affected snapshots.

### Out of scope

- Opening Command mode from an ex row, owned by Task 007.
- Mouse navigation or changes to ex parsing.

## Implementation requirements

- Use one selected index into the filtered rows and the existing viewport-offset helper; avoid a second scroll owner.
- Up/Down and Tab/BackTab move one filtered row at a time and clamp at boundaries. Empty filters remain safe.
- The selected glyph or text must convey focus without relying on color; disabled state and actionability remain visibly distinct.
- Keep initial focus useful in Normal and Select and preserve the `?` help binding.

## Acceptance criteria

- [ ] Every filtered row can be reached by repeated Up/Down and Tab/BackTab; first/last clamping and empty lists work.
- [ ] A focused row beyond the initial list viewport appears onscreen at 40×12 and 80×24 sizes.
- [ ] App, disabled, and read-only rows have distinguishable focus/availability markers, and mode-specific initial focus remains correct.
- [ ] Existing palette filtering and modal routing tests pass.

## Validation

- `make test`

## Dependencies

- 001

## Expected areas of change

- `crates/oom-edit/src/overlay/palette.rs`
- `crates/oom-edit/src/app.rs` tests
- Palette snapshots

## Risks / notes

The viewport follows selection; a state-only navigation test is insufficient. Verify actual rendered cells below the modal fold.
