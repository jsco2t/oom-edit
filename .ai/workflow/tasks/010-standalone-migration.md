# Task 010: Standalone binary on the public pane

Delegation: main-only

## Goal

Make `oom-edit` a thin full-terminal host of `EditorPane` with v0.5.0 appearance, behavior and performance.

## Context

The pane protocol is now complete enough for all existing commands. Migration must prove that host and embedded behavior share one App code path before disk watching adds intentional changes.

## Scope

### In scope

FR-110/117, standalone argument/config/env/guard/event-loop wiring, cursor/synchronized-update output and public-host parity tests.

### Out of scope

New disk watcher behavior, oom UI or new commands.

## Implementation requirements

- Opening prerequisite, explicitly authorized on 2026-09-27: resolve the known first-frame absolute-limit failures now rather than in Task 014. Profile the unchanged fixture, optimize without reducing highlighting/rendering work or changing Markdown semantics, add meaningful regression assertions, and pass the unchanged full `make bench`, `make bench-check` and `make check` before migrating the host. Preserve Task 014's integrated performance proof.
- Reuse existing loop cadence and read-after-event timestamp sampling; filter repeats/releases exactly as baseline, coalesce bounded resize batches, run the same 5 s/8 ms idle policy, skip unchanged redraws and keep cursor/synchronized-update output host-owned.
- Load standalone config, environment theme inputs, spell/dictionary/clipboard paths and unrestricted file policy before terminal setup; pass owned values to pane. Full-terminal drawing copies owned cells to ratatui with origin translation once, inline hints/which-key and more-than-one-tab rule.
- Route QuitAllRequested and deliberate last-tab Closed to today's process exit behavior without adding `:wqa` or altering dirty `:qa`; preserve duplicate `:tabnew`.
- Replay baseline modes/motions/operators/registers/selections/undo/paste/search/ex/frontmatter/spell/clipboard traces through both public hosts, asserting text/cursor/mode/undo/events/frames. Review any snapshot change against the DRD compatibility ledger.
- Compare baseline and candidate existing TUI perf cases; investigate any disallowed wall/CPU/RSS/heap movement before proceeding.

## Acceptance criteria

- [ ] Known source/rendered first-frame absolute failures are fixed before host migration; original fixtures and limits pass without reduced output or deferred highlighting.
- [ ] `run` and binary use `EditorPane` for all editing/rendering; no parallel private App host path remains.
- [ ] Existing snapshots, conformance, lifecycle and key-table tests pass unchanged except documented DRD safety/persistence changes.
- [ ] Public-host parity cases assert exact text, cursor, mode, undo, event and frame behavior for all major editor surfaces.
- [ ] Existing performance contracts and same-host preliminary comparison show no disallowed regression.

## Validation

`make test`

`make bench-check`

`make bench`

## Dependencies

009

## Expected areas of change

`crates/oom-edit/src/lib.rs`, `main.rs`, `event.rs`, `app.rs`, host adapter, snapshot/parity/perf tests and `README.md` only for explicit behavior notes.

## Risks / notes

Capture v0.5.0 frame and input results before touching the loop. Check both render cost and end-to-end first-frame cost; frame conversion alone passing is insufficient.
