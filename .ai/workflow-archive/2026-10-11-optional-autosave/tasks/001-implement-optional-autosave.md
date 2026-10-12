# Task 001: Implement optional autosave

Delegation: main-only

## Goal

Deliver both disabled-by-default autosave triggers as complete pane behavior with noninteractive safe saves, real terminal focus routing, and automated tests for changed behavior.

## Context

Explicit saves already converge on App::execute_lifecycle and the core's atomic save protocol. The existing pane timer/focus APIs are sufficient host boundaries. Automatic requests need an explicit distinction from manual requests so external conflicts cannot open background confirmations.

## Scope

### In scope

Configuration and live reports; deterministic five-minute scheduler; actual focus transitions; all modified named tabs; lifecycle save-origin handling; conflict/error notices and correlated events; guard-owned focus reporting; standalone and split-host event routing; focused behavior and boundary regression tests.

### Out of scope

Recovery files, generalized save workers, configurable intervals, new commands or dependencies, and unrelated settings.

## Implementation requirements

1. Implement `Config.autosave` with root-exported `AutosaveConfig { every_five_minutes, on_focus_loss }`, both false by default. Extend ConfigField/ConfigApplyReport and live apply_config. Update exact API guards and affected literals. Test old/missing/partial config, all four combinations, invalid types, roundtrip, and live changes.
2. Implement the plan's scheduler as private closed state using injected Instants. Start at construction/enable +300 seconds; expiry runs once and re-arms at supplied now +300 seconds. Disabling removes its deadline; unchanged/unrelated config retains it. Manual/focus saves do not reset it. Merge with PaneTick deadlines.
3. Filter clean/unnamed tabs. Capture candidate TabIds; resolve each at dispatch. Use only current-path, nonforce, StayOpen saves through App::execute_lifecycle. Preserve active tab and identities.
4. Add typed private automatic save origin/trigger, avoiding string commands or a second writer. Inside canonical save handling, automatic conflicts/missing errors emit failures/notices without overwrite/recreation prompts. Preserve committed-uncertain semantics and independent candidate progress; do not hide failures behind later success.
5. Initiate cycles only when lifecycle is Idle with no overlay, command/search prompt, or pending grammar. Skip unsafe triggers as specified in plan.md, warn when modified named work is skipped, advance periodic cadence, and preserve all existing modal/transaction state. No persistent queued save targets.
6. Support ordinary Insert and Select saves; preserve source cursor, mode, selection, text, undo/redo, and subsequent edit dirty tracking. Do not synthesize Escape.
7. Trigger on one true focus-loss transition in the existing focus authority. Repeated false, gain, and tab switch do not trigger. Wire standalone and split-host timestamps through tick-before-focus.
8. Add owned TerminalGuardOptions.focus_change with a false library default; standalone/example explicitly enable reporting for their terminal session. Add typed failure/acquisition state and reverse-order cleanup in normal/rollback/panic/signal paths. Use existing crossterm; no terminal setup inside pane construction. Update startup/guard exact assertions and in-tree downstream consumers.
9. Test exact 300-second boundary, delayed ticks, disabled/combined settings, all-tab/unfocused scope, duplicate event suppression, unchanged-file filtering, and named-new/unnamed behavior using injected clocks and file-backed fixtures.
10. Test policy denial, external modification/deletion, mixed batch failures, I/O errors and committed uncertainty through the existing injected save observer/failure seams. Existing disk checks remain authoritative even when polling has not yet noticed a change. Assert no automatic confirmation, retained dirty work, exact disk contents, and correct typed events.
11. Extend lifecycle/dependency guards to prove scheduling and hosts do not independently invoke session/document writers. Extend public facade/privacy/guard assertions for each boundary added here.

## Acceptance criteria

- [ ] Config defaults, schema, roundtrip and live updates exactly match plan.md; API compilation/privacy guards cover added owned types, fields, and variants.
- [ ] Periodic timing/deadline tests cover just-before/equal/after expiry, delayed ticks, config changes, manual saves, and focus saves without real sleeps.
- [ ] Focus transitions use the existing authority and save all eligible tabs once; standalone and embedded routes have deterministic tests.
- [ ] File-backed tests show background/unfocused and Insert/Select saves preserve state, exact serialization, undo/redo and subsequent dirty tracking.
- [ ] Clean/unnamed filtering and named-new-file semantics are proven without invented paths or redundant writes.
- [ ] Conflicts/deletion/denial/failures/uncertain commits preserve safety and typed event semantics, never auto-open a confirmation, and allow independent successes.
- [ ] Exclusive interactions and lifecycle transactions prevent autosave interference and obey the specified skip policy.
- [ ] Focus reporting setup/restoration, rollback, panic, signal behavior and option defaults are covered by byte-exact and PTY guard tests.
- [ ] Core has no new terminal/clock/focus dependency; no duplicate text or save path, new dependency, stub, or unrelated edit is introduced.

## Validation

- `make test-config-public`
- `make test-public-api`
- `make test-pane-public`
- `make test-pane-lifecycle`
- `make test-pane-disk`
- `make test-standalone-host`
- `make test-embedding-example`
- `make terminal-guard-pty-test`
- Standard task gate: `make check`

## Dependencies

None

## Expected areas of change

- `crates/oom-edit/src/config.rs`, `lib.rs`, `app.rs`, private App autosave module if useful.
- `crates/oom-edit/src/lifecycle.rs`, `app/lifecycle_protocol.rs`, `pane.rs`.
- `crates/oom-edit/src/standalone.rs`, `event.rs`, `terminal_guard.rs`.
- `crates/oom-edit/examples/embedded.rs`, `examples/support/embedded_host.rs`.
- Config/public API/pane/lifecycle/disk/standalone/embedding/terminal guard and dependency tests.
- In-tree examples and independent-consumer fixture literals affected by new public fields.
- Existing core/session save regression tests only if needed to expose a save-baseline defect.

## Risks / notes

This task is main-only because it changes document I/O orchestration and terminal cleanup across boundaries. Read the exact safety, scheduling and skip policy in plan.md before implementing. Avoid false success after committed uncertainty, reused request contexts, identity changes during a cycle, and terminal setup in pane construction. Product tests ship with the implementation in this task; acceptance hardening in the next task does not excuse incomplete coverage here.
