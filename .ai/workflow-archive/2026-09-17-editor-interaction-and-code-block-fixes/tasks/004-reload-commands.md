# Task 004: Reload commands

Delegation: main-only

## Goal

Make `:e!` and `:reload` reload the active file, and `:reload-all` reload every open file-backed tab safely.

## Context

The current `:e!` effect has an empty path; App can open it as a new empty buffer. Multi-tab reload has no typed effect or lifecycle request.

## Scope

### In scope

- Core parsing and typed effects for reload commands.
- App lifecycle reload actions and target capture.
- Tests for successful and failing single/multi-tab reloads.

### Out of scope

- Automatic file watching or implicit reloads.

## Implementation requirements

- Route all reload I/O through `App::execute_lifecycle`; do not mutate sessions individually outside it.
- Resolve `:e!` and `:reload` to the current tab's existing path; refuse unnamed, missing, unreadable, or invalid UTF-8 targets without changing any tab.
- For `:reload-all`, capture all tab indices and paths, preload every replacement, then apply all only if every target succeeds. Retain tab order and active index. Report failures clearly.
- Dirty edits may be discarded only for explicit `:e!`, `:reload`, and `:reload-all`. Keep `:e {path}` and non-forced replacements protected.
- Update help/reference metadata and public API guards for any new effect or API.

## Acceptance criteria

- [ ] `:e!` and `:reload` read the active file's newest disk bytes and clear its dirty state.
- [ ] `:reload-all` refreshes every tab and keeps tab count, order, active index, and path identities.
- [ ] A failed target leaves all tabs, including dirty text, unchanged; no missing file becomes an empty replacement buffer.
- [ ] Existing save/close/open lifecycle behavior remains covered and passes.

## Validation

- `make test`

## Dependencies

- 003

## Expected areas of change

- `crates/oom-edit-core/src/session.rs`
- `crates/oom-edit/src/lifecycle.rs`
- `crates/oom-edit/src/app.rs`
- `crates/oom-edit/src/overlay/palette.rs`
- `crates/oom-edit-core/tests/public_api.rs` if public effects change

## Risks / notes

`EditorSession::open` supports new-file buffers for ordinary open. Reload must verify the existing file before using it, and all-tab reload must commit atomically at the App state level.
