# Task 004: TUI pointer interaction

Delegation: main-only

## Goal

Make primary-button clicks, drags, and wheel scrolling move the active document cursor and selection consistently with the visible viewport.

## Context

App currently drops all mouse events except wheel up/down, and wheel scroll leaves cursor state behind.

## Scope

### In scope

- Pointer-event routing and geometry translation for rendered and source screens.
- Closed gesture state for press/drag/release; interruption by modal, tab, and resize transitions.
- Wheel movement of viewport and cursor by actual scrolled rows.
- Tests for Normal, Select, Insert, off-body clicks, wrapped/horizontally scrolled content, mode changes after scrolling, and modal exclusivity.

### Out of scope

- Tab-bar and overlay control clicking, multi-click gestures, clipboard selection, and scroll-speed configuration.

## Implementation requirements

- Derive body bounds from the same tab/status geometry used during rendering and use the shared gutter/text width.
- Ignore off-document pointer events; a click on an in-body text cell moves the cursor via `EditorSession`.
- Begin a character selection on the first actual drag movement; update the endpoint while dragging and keep Select mode on release. Preserve the press anchor even if rendering/scrolling occurs between events.
- Use a single closed enum for gesture state. Clear it on release, modal takeover, tab switch, and interrupted gestures.
- Wheel input must keep the cursor with the scrolled view, including at document boundaries and after subsequent mode changes. Do not spuriously invoke scroll-follow to undo a wheel scroll.
- Mouse events cannot reach a document obscured by any modal overlay or confirmation.

## Acceptance criteria

- [ ] Clicking after a long rendered scroll moves cursor to the clicked visible source location and does not snap back.
- [ ] Insert clicks respect source wrap, horizontal scroll, Unicode, and current body offsets.
- [ ] Dragging changes to Select, paints and preserves the exact selected range, and release leaves the selection visible.
- [ ] Wheel scrolling up/down advances cursor and viewport together by the actual movement in rendered and source views; mode changes retain the viewed region.
- [ ] Modal and off-body events do not mutate the underlying document cursor or selection.

## Validation

- `make test`

## Dependencies

003

## Expected areas of change

- `crates/oom-edit/src/app.rs`
- `crates/oom-edit/src/screens/editor.rs`
- `crates/oom-edit/src/screens/rendered.rs`
- `crates/oom-edit/src/event.rs`

## Risks / notes

The event loop batches events between draws. Geometry and gesture anchoring must remain correct when drag events arrive without an intervening frame.
