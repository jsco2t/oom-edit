# Task 005: Palette question-mark shortcuts

Delegation: main-only

## Goal

Open the existing command palette with bare `?` in Normal mode and Space+`?` in rendered Space contexts.

## Context

Space+`h` already opens the palette. Bare `?` currently enters core backward search, while App's binding registry is the source for dispatch and UI projections.

## Scope

### In scope

- Registry-backed `?` palette bindings, dispatch, quick hints, which-key continuations, palette rows, and snapshots.
- Tests for context exclusivity, original Space+`h`, `/` search, and unknown Space chord forwarding.

### Out of scope

- Replacing the palette UI, changing core search behavior for embedding hosts, or adding new commands.

## Implementation requirements

- Declare all App-owned shortcuts in the static command registry; avoid a second handwritten binding list.
- Dispatch bare `?` only in Normal mode, before forwarding the key to core. Preserve `/` search and keyboard modifier semantics.
- Dispatch Space+`?` to the same `AppCommand::Help` as Space+`h` wherever the latter is valid.
- Keep command identity unique and avoid duplicate executable palette rows; update registry completeness/uniqueness tests to cover aliases.
- Keep overlays and confirmations modal, and retain unknown/incomplete Space forwarding behavior.

## Acceptance criteria

- [ ] Bare `?` in Normal opens the command palette, without opening the search prompt.
- [ ] Space+`?` and Space+`h` open the same palette in their supported rendered contexts.
- [ ] `/` still begins search; key modifiers and non-Normal contexts do not accidentally trigger the direct shortcut.
- [ ] Hints, which-key, palette metadata, and registry tests agree with the executable bindings.

## Validation

- `make test`

## Dependencies

004

## Expected areas of change

- `crates/oom-edit/src/command/registry.rs`
- `crates/oom-edit/src/command/keymap.rs`
- `crates/oom-edit/src/app.rs`
- `crates/oom-edit/src/widgets/hint_bar.rs`
- `crates/oom-edit/src/widgets/which_key.rs`
- `crates/oom-edit/src/snapshot_tests.rs`
- `crates/oom-edit/tests/snapshots/`

## Risks / notes

The direct TUI shortcut intentionally overrides core backward search in Normal mode. Core conformance remains valid for headless clients; TUI behavior needs its own explicit regression tests.
