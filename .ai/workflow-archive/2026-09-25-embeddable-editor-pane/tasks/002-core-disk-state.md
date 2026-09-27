# Task 002: Versioned core disk state and safe reload

Delegation: main-only

## Goal

Give core version-bound disk observations, safe missing-file saves, atomic reload and guarded path retargeting without changing editing ownership.

## Context

`Document` currently stores length/mtime and skips the save check when a previously-backed file disappears; App reload reconstructs the session. The core must provide safe primitives before the pane orchestrates disk prompts.

## Scope

### In scope

FR-050–053 and FR-116; opaque existence/identity/metadata/content versions, typed I/O errors, save acknowledgement/recreation, atomic reload preserving appropriate state, and retarget validation.

### Out of scope

Pane polling/prompts, host vault policy and alternate lifecycle executor.

## Implementation requirements

- First add behavioral tests for Unbacked/NeverCreated/Unchanged/Modified/Missing/IoError, same-length-and-mtime replacement, stale keep-mine/recreate choices, denied/missing saves, force semantics, and failures at pre/post atomic replacement boundaries. Assert exact disk bytes and retained live text/undo after faults.
- Preserve `Document` as identity/serialization/version owner and `LiveDocument` as sole mutation gateway. A changed-text reload validates and parses candidate before mutation, clamps source cursor, resets undo and returns to Normal; identical normalized text preserves mode/undo/cursor while refreshing disk/serialization baseline. Pane will later clamp viewport.
- Guard retarget against conflicting destination bytes and preserve dirty/text/undo/cursor and unresolved conflict state. Action validation must content-check immediately before save/reload/recreate.
- If using already-vendored sha2 0.10.9, promote it to an exact direct core dependency only after the required license, maintenance/popularity, vendored-diff and hand-roll assessment in `docs/dependencies.md`; update lock/vendor and run deny/audit. Do not invent cryptography or add new unsafe code.
- Export only owned core DTOs deliberately and extend public-API/dependency guards.

## Acceptance criteria

- [ ] All six disk-state categories and permission-vs-missing distinctions have exact tests.
- [ ] A missing previously-backed file cannot be recreated without a version-bound explicit decision; a later replacement invalidates that decision.
- [ ] Byte-identical reload preserves undo/mode/cursor; changed reload is atomic and clamps source position; failed candidate leaves live state untouched.
- [ ] Retarget changes identity without writing or silently acknowledging a conflict; post-replace fsync failure is distinguishable from pre-commit failure and retains dirty buffer state.
- [ ] Core facade/dependency guards and any dependency checklist are updated.

## Validation

`make test`

`make bench-check`

`make deny`

`make audit`

## Dependencies

001

## Expected areas of change

`crates/oom-edit-core/src/document.rs`, `session.rs`, `session/live_document.rs`, `error.rs`, `lib.rs`, core I/O/integration/API tests, `docs/dependencies.md`, `Cargo.toml`, `Cargo.lock`, `vendor/` if needed.

## Risks / notes

Keep the existing temp/write/fsync/rename/parent-fsync protocol. Document the unavoidable race with noncooperating external writers accurately. Never emit a durable-success signal after a committed-but-uncertain fsync result.
