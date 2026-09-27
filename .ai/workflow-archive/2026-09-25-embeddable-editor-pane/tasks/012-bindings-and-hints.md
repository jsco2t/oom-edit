# Task 012: Structured binding registry and host hints

Delegation: main-only

## Goal

Export exact current hints, which-key, status and binding ownership from a single registry without changing dispatch behavior.

## Context

App Space commands, core Vim reference metadata and palette rows currently live in different places. Hosts need structured ownership, including collision information for modified keys and pending grammars.

## Scope

### In scope

FR-080–085: pure hint/which-key/status projections, command/reference roles, reserved-key ownership by mode/pending state, palette/which-key parity.

### Out of scope

Host-contributed commands or oom global keymap implementation.

## Implementation requirements

- Add red meta-tests that drive real dispatch for each mode, Insert/prompts, overlays, counts, registers, operators, g/Space prefixes, Alt/Ctrl-g/function keys, text entry and modal no-ops; assert exported ownership matches grammar consumption. Explicitly classify intentional host reservations, not absence of named editor commands.
- Consolidate VIM_REFERENCE into registry metadata with executable App/core and reference-only roles. Do not create a new dispatcher or advertise a metadata-only row as executable. Preserve exact App command dispatch, palette rows/order and which-key ordering.
- Export current hint cells including compact/disabled and overlay variants; which-key appears only after focused 150 ms pending Space. Export mode/path/dirty/spell/ruler status as data.
- Add completeness/uniqueness tests for IDs/sequences/modes and public API docs/guards.

## Acceptance criteria

- [ ] Hints and which-key data match standalone status/overlay content and ordering for the same state.
- [ ] Every executable/reference role and modified-key ownership agrees with real dispatch across tested contexts; mutation of registry/dispatch causes meta-test failure.
- [ ] Palette and standalone which-key snapshots remain unchanged.
- [ ] Status export reports exact active editor-line state without terminal types.

## Validation

`make test`

`make bench-check`

## Dependencies

011

## Expected areas of change

`crates/oom-edit/src/command/`, `overlay/palette.rs`, `widgets/hint_bar.rs`, `widgets/which_key.rs`, `app.rs`, pane facade, registry/dispatch/public API tests.

## Risks / notes

Core command rows are metadata unless the core actually dispatches them. Unknown App chords must still forward their first key unchanged to core.
