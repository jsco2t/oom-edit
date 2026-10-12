# Task 002: Verify and document autosave

Delegation: main-only

## Goal

Complete observable acceptance coverage and explain autosave configuration and host responsibilities using the implemented public API.

## Context

Task 001 delivers complete behavior and regression tests. This task reviews the integrated feature as a public consumer, closes any remaining acceptance coverage gaps, and documents user-visible semantics so opt-in saves are predictable.

## Scope

### In scope

Public-consumer test completeness; standalone/split-host parity; configuration and embedding documentation; final boundary/inventory checks; high-confidence corrections inside the approved plan.

### Out of scope

New triggers, configurable intervals, command registry changes, speculative refactors, publishing, committing or archiving this package, and unrelated fixes.

## Implementation requirements

1. Map each package acceptance criterion to a concrete executable test from task 001 or an additional focused test. Add only coverage that exercises observable behavior or architectural constraints; do not mirror implementation details.
2. Confirm public consumers can opt into either/both settings, receive live-update reports, tick while hidden, supply timestamps before focus changes, and drain events once. Cover correct Saved/Failed/DiskWriteCommitted TabId/request correlation and unchanged active-tab/MRU behavior.
3. Verify all four public modes and native pending-input suspension retain their established behavior. Cover subsequent typing after an Insert autosave and undo/redo across the confirmed save baseline, plus exact CRLF/final-newline serialization.
4. Review mixed named/unnamed/new/clean/background batches and the skip/no-queue policy. Confirm close/retarget/external-change tokens and a manual save confirmation remain untouched by due/focus triggers.
5. Update README's default config example and autosave description with independently selectable settings, the fixed cadence, all-tab scope, unnamed/new-file rules, best-effort terminal focus support, blocked-trigger behavior, and conflict/error handling. Explain how the user can retry explicitly.
6. Update DEVELOPER's host focus/time/config/event contract. Document the guard option, tick-before-focus timestamp rule, timer operation while unfocused, and draining save events produced by tick/set_focused. Update related public docs/consumer examples only where the new API changes their claims.
7. Update exact API/dependency/host tests and requirement inventories if any authoritative existing requirement changes. Preserve existing IDs/coverage; do not invent claimed coverage from Markdown descriptions.
8. Run the listed validations and the final package gate. Resolve warning/error failures; report supply-chain warning findings under the actual zero-advisory policy. No dependency change is expected.

## Acceptance criteria

- [ ] Every final plan acceptance criterion has executable evidence or an explicit documentation assertion where appropriate.
- [ ] External-consumer and host parity tests demonstrate both triggers, timing/config behavior, correct identities/events, state preservation, serialization and failures without real-time waiting.
- [ ] Public docs include valid copy-paste config for periodic only, focus only or both, and precisely state defaults, scope, skips and terminal limitations.
- [ ] Host docs and examples match the implemented focus-reporting, timestamp, live-config, deadline and event-drain contract.
- [ ] Existing manual save/close/disk workflows, private API boundaries, exact mode/registry guards and dependency direction remain valid.
- [ ] All specified validations and the full final gate pass without warnings or unresolved blocking concerns.

## Validation

- `make test-config-public`
- `make test-pane-public`
- `make test-pane-lifecycle`
- `make test-pane-disk`
- `make test-standalone-host`
- `make test-embedding-example`
- `make test-public-api`
- `make test-package-guards`
- `make build-examples`
- `make doc`
- `make coverage-check`
- Standard task gate: `make check`

## Dependencies

001

## Expected areas of change

- `README.md`, `DEVELOPER.md`, relevant public host/input docs.
- `crates/oom-edit/tests/config_public.rs`, `public_api.rs`, `pane_public.rs`, `pane_lifecycle.rs`, `pane_disk.rs`, `standalone_parity.rs`, `embedding_example.rs`, `dependency_hygiene.rs`, `package_guards.rs`.
- Existing session/document regression tests where needed for exact serialization/save-baseline assertions.
- Host example and downstream fixture documentation if affected.
- `coverage/` only if an existing authoritative requirement or executable mapping changes.

## Risks / notes

Remain main-only because acceptance spans autosave data integrity and host transaction contracts. Focus support cannot be guaranteed for a terminal that emits no reports; verify protocol and routing, and describe the limitation honestly. Do not weaken failing tests or silently change the approved skip/retry policy. No archive plans are inputs to this work.
