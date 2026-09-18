# Task 003: Core pointer position and selection

Delegation: main-only

## Goal

Provide terminal-neutral session operations to position a cursor and construct a character selection from displayed document positions.

## Context

The reusable core owns the canonical cursor, rendered layout/source atoms, and Select state. App cannot safely reconstruct Markdown provenance or mutate those states independently.

## Scope

### In scope

- Session facade operations for rendered-point cursor movement and drag anchor/endpoint selection.
- Source viewport-point positioning for Insert, including wrap, skipped rows, and horizontal scroll.
- Deterministic fallback for synthetic/out-of-range positions, UTF-8 and display-cell boundaries.
- Compile-time public API guard updates and focused core integration tests.

### Out of scope

- Crossterm event handling, screen rectangles, themes, and TUI gesture state.

## Implementation requirements

- Accept only renderer-neutral inputs already exposed by the core or a minimal owned DTO if strictly needed.
- Use rendered source atoms and existing navigation/selection helpers; never claim source bytes for synthetic output.
- Keep canonical source cursor, rendered cursor, and Select endpoint synchronized through the session facade.
- Preserve exact UTF-8 boundaries, source selection ranges, and existing Vim-wrapper ownership.
- Avoid a second mutable text copy; read source frames/layout through existing session state.
- Update curated crate-root API tests for every new public operation/type.

## Acceptance criteria

- [ ] Positioning at a rendered text cell selects the correct source-backed atom, including wrapped text, tables, repeated text, wide Unicode, and horizontal display offsets.
- [ ] Synthetic rows and off-content clicks resolve deterministically without fabricating provenance or panicking.
- [ ] A rendered drag starts/updates character Select and produces exact source ranges through the existing projection.
- [ ] Source Insert positions map correctly through wrap, skip-rows, horizontal scrolling, and Unicode.
- [ ] Public API guards and core tests cover the new facade behavior.

## Validation

- `make test`

## Dependencies

002

## Expected areas of change

- `crates/oom-edit-core/src/session.rs`
- `crates/oom-edit-core/src/rendered/nav.rs`
- `crates/oom-edit-core/src/style.rs`
- `crates/oom-edit-core/src/lib.rs`
- `crates/oom-edit-core/tests/public_api.rs`
- `crates/oom-edit-core/tests/session_integration.rs`

## Risks / notes

Synthetic Markdown decorations, table borders, and wide characters need deliberate nearest-source and display-cell behavior. Core must not import TUI geometry or event types.
