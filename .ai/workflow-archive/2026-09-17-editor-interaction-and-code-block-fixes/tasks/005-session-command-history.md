# Task 005: Session command history

Delegation: main-only

## Goal

Provide process-local Up/Down history for the last ten submitted ex commands across all tabs.

## Context

`EditorSession` owns Command input but only keeps the current buffer; Up and Down do nothing. Tabs are distinct sessions and can be replaced during reload.

## Scope

### In scope

- A bounded shared core history owner, command navigation state, and facade methods.
- App wiring of the same history into initial, new, and replaced tab sessions.
- Tests for ten-entry eviction, navigation, draft restoration, Escape, editing, cross-tab scope, and fresh-process reset.

### Out of scope

- Persistent history, search history, and command completion.

## Implementation requirements

- Keep command grammar, Up/Down navigation, and buffer edits inside `EditorSession`/core. App only supplies the shared process-lifetime history handle.
- Record submitted non-empty commands on Enter, including commands that produce error messages. Do not record Escape or blank input.
- Up starts at newest and clamps at oldest; Down moves to newer entries and then restores the exact pre-navigation draft.
- Editing or pasting after recall detaches from traversal without mutating stored history. Bound stored entries to ten; repeated submissions remain separate entries.
- New tabs and reload-created sessions continue to share the same in-memory history. No config or file writes.
- Re-export any new public handle through the curated core root and extend compile-time API guards.

## Acceptance criteria

- [ ] Up/Down select the last ten commands, clamp at both ends, and restore the draft after the newest entry.
- [ ] Edited recalls execute the edited text without corrupting stored entries; Escape closes Command mode without adding history.
- [ ] Commands entered in one tab are available from another, including after reload, and a new App process begins with empty history.
- [ ] No history persistence or new dependency is introduced.

## Validation

- `make test`

## Dependencies

- 003
- 004

## Expected areas of change

- `crates/oom-edit-core/src/session.rs`
- `crates/oom-edit-core/src/lib.rs`
- `crates/oom-edit-core/tests/public_api.rs`
- `crates/oom-edit/src/app.rs`

## Risks / notes

History is separate from the mutable document text. Keep one shared history owner across tab replacements and a per-prompt navigation cursor.
