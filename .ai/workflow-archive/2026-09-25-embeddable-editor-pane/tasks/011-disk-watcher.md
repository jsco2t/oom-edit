# Task 011: Disk watching and safe-point reconciliation

Delegation: main-only

## Goal

Add shared standalone/embedded disk polling, notifications, safe reload and version-bound dirty/missing decisions.

## Context

Core now supplies version-safe disk primitives, and both hosts use the public pane. This task adds the DRD's intentional standalone safety behavior after the parity checkpoint.

## Scope

### In scope

FR-054–056/116, NFR-003, disk markers, DiskChangePending/ReloadedFromDisk events and token-aware scheduling.

### Out of scope

OS watchers, terminal focus event triggers or oom sync implementation.

## Implementation requirements

- Add red clock/filesystem matrix tests for clean/dirty × Modified/Missing/IoError × focused/hidden/Normal/Insert/search/pending/overlay/lifecycle; assert exact disk bytes, text/undo, marker and ordered events. Include same-size/same-mtime notification replacement and stale choice A→B.
- Probe backed tabs no more than once per 2 s in routine polling; notification and action validation content-check matching paths. Suspend polls/I/O under external-change token and reconcile once after commit/abort.
- Clean modified tabs reload only at focused active Normal safe points without pending grammar/search/overlay/lifecycle; dirty tabs present keep-mine/reload tied to observed version. Later disk changes re-prompt. Missing tabs retain text/undo and marker; save requires explicit version-bound recreation. Permission errors remain errors, not Missing.
- Background detection emits one DiskChangePending per new version and never AttentionRequired; revalidate before applying a choice. Preserve viewport clamped at pane level; identical normalized reload preserves mode/undo/cursor.
- Keep non-color marker under monochrome/accessible themes and no visible change until a genuine external change.

## Acceptance criteria

- [ ] Routine poll I/O counters prove ≤1 metadata probe per backed tab per 2 s, including 50-tab fixture and token suspension.
- [ ] Safe-point matrix prevents mid-edit/hidden prompt or reload; pending events are deduplicated per version with no attention stealing.
- [ ] Dirty/missing/version-stale choices never lose text/undo or overwrite later bytes; clean reload preserves/clamps position as specified.
- [ ] Standalone and embedded public-host tests observe identical disk behavior and explicit events.

## Validation

`make test`

`make bench-check`

`make bench`

## Dependencies

010

## Expected areas of change

`crates/oom-edit/src/app.rs`, pane facade, `event.rs`, `overlay/confirm.rs`, disk/host/perf tests, `README.md`/`CHANGELOG.md` when documenting intentional behavior.

## Risks / notes

Disk metadata is a hint. Content validation is required on notification/action. Do not reopen a dirty buffer or clear pending conflicts merely because path metadata changed.
