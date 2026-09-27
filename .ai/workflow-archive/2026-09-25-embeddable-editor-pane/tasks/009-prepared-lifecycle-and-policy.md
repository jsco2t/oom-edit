# Task 009: Policy-checked prepared lifecycle

Delegation: main-only

## Goal

Complete policy-mediated file I/O and single-use prepared close/retarget protocols with exact correlated lifecycle outcomes.

## Context

App already has one `execute_lifecycle` owner, but its requests are index-relative and cannot support host vault switch, trash or rename safely. The public pane now has stable TabIds/events and must coordinate with that executor.

## Scope

### In scope

FR-034/035/040–043/057/101/113/114: close/close_all, quit routing, all-path policy, external-change token, prepare/commit/abort close and batch retarget, Busy/Stale/Denied/Io outcomes and ordered events.

### Out of scope

Actual oom file moves/trash/sync, watcher prompts, standalone loop rewrite.

## Implementation requirements

- Add transition/fault tests first: three dirty tabs save/discard/cancel closes none; saved first remains clean; commit closes captured IDs in order; abort/stale/Busy retains every buffer; unnamed dirty tab requests a path or cancel; force never bypasses policy.
- Make `execute_lifecycle` the only executor with closed request/confirmation states and captured TabIds. Exactly one terminal result per accepted request; Opened before ActiveTabChanged, Saved before Closed, Closed before AllClosed, Retargeted before any resulting reload. Emit AttentionRequired only for host-requested close confirmation.
- Validate typed operation/path policy at every initial/open/:e/:tabnew/save/saveas/copy/reload/retarget/new-path boundary, resolving existing targets or nearest existing parent and revalidating at operation boundary; cover symlink/outside/force paths without silently reinterpreting standalone unrestricted behavior.
- Acquire external-change token only after prerequisite dirty-tab saves; while held reject pane file I/O and polling visibly, allow buffer edits except frozen close targets, and reject conflicting requests Busy. Single-use commit performs captured close/retarget; abort or dropped token preserves tabs and reports cancellation. Batch retarget validates every destination/version before host move, then commits all in-memory identities together; failure aborts all.
- Preserve standalone `:tabnew`, dirty `:qa`, `:qa!`, `:q` and no `:wqa`; embedded accepted quit-all emits QuitAllRequested without process exit.

## Acceptance criteria

- [ ] Full three-tab cancel/commit/abort and unnamed-path matrix retains exact buffers/undo and emits exact ordered correlated outcomes.
- [ ] Every file I/O path, including bang and symlink aliases, is policy checked at the operation boundary; denied requests leave sentinel bytes untouched.
- [ ] Token suspension and target freeze are enforced under barrier-controlled conflicting operations; dropped/aborted tokens never commit.
- [ ] Retarget captures IDs, validates all destinations and either updates all bindings or none without changing text/undo/dirty/cursor.
- [ ] Standalone command semantics and single executor architectural guards remain intact.

## Validation

`make test`

`make bench-check`

## Dependencies

008

## Expected areas of change

`crates/oom-edit/src/app.rs`, `lifecycle.rs`, pane facade, `overlay/confirm.rs`, policy/transaction/host integration tests and API guards.

## Risks / notes

No disk suspension may block saves required to prepare a close. Failed post-replace fsync must emit committed-uncertain, retain dirty text and never trigger Closed/Saved success.
