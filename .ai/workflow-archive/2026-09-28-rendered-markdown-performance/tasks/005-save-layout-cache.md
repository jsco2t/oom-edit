# Task 005: Preserve layout across metadata-only saves

Delegation: main-only

## Goal

Stop successful saves from rebuilding an unchanged rendered layout.

## Context

Both save paths currently call `RenderedState::invalidate()` after writing unchanged live text. This repeats the full layout on the next pane frame, even for a clean save.

## Scope

### In scope

- Remove content-layout invalidation from save paths that do not change text.
- Ensure pane presentation still refreshes dirty/saved metadata and notices.
- Test ordinary, version-checked, clean, dirty, save-as/retarget, failed and uncertain saves.

### Out of scope

- Atomic-write protocol changes or relaxing external-version validation.

## Implementation requirements

- Preserve no-data-loss and single-use host transaction behavior.
- Use layout-build counters and public frame assertions; do not infer correctness from timing alone.
- Edits and external reloads must continue to invalidate correctly.

## Acceptance criteria

- [ ] A successful save without text mutation retains the same rendered layout generation and updates saved-state presentation.
- [ ] Failed/uncertain saves preserve existing error and dirty-state semantics.
- [ ] Edits, width changes and reloads still rebuild when their source or geometry changes.

## Validation

- `make test-pane-lifecycle`
- `make test-pane-disk`
- `make bench-check`

## Dependencies

004

## Expected areas of change

`crates/oom-edit-core/src/session.rs`, `crates/oom-edit/src/app.rs`, relevant lifecycle tests

## Risks / notes

The pane frame cache and core layout cache have different invalidation causes; status changes must repaint without throwing away content layout.
