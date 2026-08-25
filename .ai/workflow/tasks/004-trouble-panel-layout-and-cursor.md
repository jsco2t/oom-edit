# Task 004: Trouble panel layout and cursor ownership

Delegation: worker-eligible

## Goal

Align Trouble entry columns and ensure the modal owns presentation without
leaking the underlying document cursor.

## Context

Trouble currently formats each source location at its natural width, shifting
severity/provider/message text between rows. Screens also request their cursor
before App draws the overlay; Trouble has a row carrier but no cursor of its
own, leaving the document cursor visible through the modal.

## Scope

### In scope

- Pure, deterministic Trouble column-width calculation and row formatting.
- Stable formatting while scrolling and refreshing.
- App/screen cursor visibility plumbing for modal overlay ownership.
- Trouble, screen, App, and snapshot regression tests.

### Out of scope

- Diagnostic sorting/filtering, new providers, mouse handling, or multi-cursor.
- Changing Trouble navigation, refresh, stale-jump, or lifecycle semantics.
- Table rendering or theme redesign.

## Implementation requirements

- Compute leading display widths from the complete current Trouble entry
  snapshot so columns do not shift while scrolling.
- Align location, severity, provider, and message starts for all visible rows;
  keep the selected-row glyph and severity styling intact.
- Continue to clip safely through ratatui on narrow terminals and retain the
  progress/warning/footer capacity rules.
- Suppress the document screen's cursor request when a modal overlay is open.
  Overlays that need a text cursor may still request one explicitly; Trouble
  must leave the frame without a cursor position.
- Cover Trouble over Normal, Select, and Insert screens and verify closing or a
  successful jump restores ordinary cursor behavior.
- Update the Trouble snapshot to show aligned columns.

## Acceptance criteria

- [ ] Entries with one- and multi-digit line/column values align severity, provider, and message columns.
- [ ] Alignment remains stable across scrolling, refresh, selection changes, empty/pending/unavailable/stale states, and narrow terminals.
- [ ] While Trouble is open over Normal, Select, or Insert, ratatui reports the terminal cursor hidden and the selected row remains visibly marked.
- [ ] Closing Trouble or completing a valid jump restores the underlying mode's normal cursor visibility and position on the next frame.
- [ ] Trouble input exclusivity and existing navigation/jump behavior remain unchanged.
- [ ] The Trouble snapshot and focused unit/App tests cover the corrected output.

## Validation

- `make test`

## Dependencies

- 003

## Expected areas of change

- `crates/oom-edit/src/overlay/trouble.rs`
- `crates/oom-edit/src/overlay/mod.rs`
- `crates/oom-edit/src/app.rs`
- `crates/oom-edit/src/screens/rendered.rs`
- `crates/oom-edit/src/screens/editor.rs`
- `crates/oom-edit/src/snapshot_tests.rs`
- `crates/oom-edit/tests/snapshots/trouble.txt`

## Risks / notes

Ratatui cursor visibility is determined by whether the final frame contains a
cursor request. The fix should prevent the background screen from setting one,
not place a fake cursor in a harmless-looking modal cell. Widths must use
display cells rather than assuming byte length if future provider/location
labels gain non-ASCII text.
