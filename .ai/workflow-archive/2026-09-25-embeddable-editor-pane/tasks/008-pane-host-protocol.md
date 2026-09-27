# Task 008: EditorPane construction, input and tab protocol

Delegation: main-only

## Goal

Introduce the curated public `EditorPane` facade over App for construction, owned input, focus/time, stable tabs and ordered events.

## Context

The renderer, services and core primitives now exist, but hosts still cannot drive the private App. This task establishes the shared public host path before transactional lifecycle and standalone migration.

## Scope

### In scope

FR-001–004/006, FR-020–036 and FR-100: zero/N-tab construction, typed partial-open reports, generation-scoped IDs, host open-or-focus policy, tab metadata/MRU, owned key/paste/mouse, focus/input state, tick/idle and clipboard sink.

### Out of scope

Prepared close/retarget transactions (Task 009), standalone loop migration, disk polling and binding export.

## Implementation requirements

- Register and run red public-host cases before implementation. Construct with explicit config/theme/time/dirs/services and no argv/env/config/terminal side effects; report each initial-path success/error. Drop/recreate must invalidate old TabIds.
- Keep App the only UI owner. Public `EditorPane` forwards core `KeyInput` unchanged; owned paste/mouse preserve existing behavior. Return Consumed for grammar-owned input including pending/modal/no-ops, NotConsumed only for unowned input. Document press-only expectation.
- `set_focused` is the single authority: losing focus cancels App Space/drag and hides cursor while preserving prompt/mode/overlay; same-tab core pending grammar suspends and resumes; tab change resets it via core-owned operation. Size changes apply on next render, including rendered relayout.
- Expose monotonic-time tick/deadline/redraw and one bounded idle unit; host measures each 8 ms slice after 5 s idle. Audit engine-private timing behind `vim.rs` without hjkl leakage.
- Generate stable pane-generation TabIds, request IDs, ordered event queue and tab snapshots (path/title/dirty/new/changed/active/MRU). Host open resolves canonical identity or nearest existing parent; open_existing rejects missing; startup and command policy are explicit. Options for source line/after-frontmatter/Insert apply only to a newly opened tab. Repeated active focus changes neither event nor MRU.
- Initial/open file requests use the policy service; Task 009 completes policy coverage for all lifecycle I/O. Use only owned public DTOs and extend exact facade/privacy tests.

## Acceptance criteria

- [ ] Public headless host constructs zero and multiple tabs, gets typed partial-open failures, drops/recreates without stale-ID reuse or leaks.
- [ ] Key/paste/mouse/focus/size/tick/idle cases assert exact modes, text, cursor visibility, consumption and deadlines; no pane terminal output occurs.
- [ ] Tab open/focus/metadata/MRU and event-order cases pass; host deduplication and standalone command policy are distinct.
- [ ] Public API exposes no ratatui/crossterm/hjkl signatures or raw mutable App/session escape.

## Validation

`make test`

`make build-examples`

`make bench-check`

## Dependencies

002, 003, 004, 005, 006, 007

## Expected areas of change

`crates/oom-edit/src/lib.rs`, `app.rs`, `event.rs`, `clipboard.rs`, a private pane facade module, `crates/oom-edit-core/src/session.rs`/`vim.rs` for pending reset, public-host/API tests and fixtures.

## Risks / notes

Do not fabricate crossterm events or put hjkl types outside `vim.rs`. Event order and input ownership must be asserted against real dispatch, including prompts and pending prefixes.
