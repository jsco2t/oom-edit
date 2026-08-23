# Task 006: Stabilize terminal frame presentation

Delegation: main-only

## Goal

Eliminate launch-time repaint flicker by presenting complete frames atomically, avoiding unchanged idle paints, and collapsing queued resize bursts into one repaint.

## Context

The current loop draws before every poll, flushes each frame directly, and dispatches one event per iteration. Ratatui diffs unchanged buffers, but the shell still requests frames continuously; terminal renderers may visibly tear a large update, and multiple queued resizes can force multiple clear/reflow/draw cycles. This task changes presentation scheduling without changing editing/input ownership.

## Scope

### In scope

- A deterministic redraw decision/result shared by tick, event dispatch, spell idle work, deadlines, and resize handling.
- Initial-frame and changed-state-only painting.
- Bounded draining of ready input with coalescing of consecutive queued Resize events before the next paint.
- Crossterm begin/end synchronized-update framing around every terminal draw, with guaranteed cleanup on errors.
- Terminal guard cleanup hardening and deterministic recording-backend/event-source tests.
- Unreleased changelog entries covering the complete workflow.

### Out of scope

- Changing frame-rate or spell-work performance budgets except to stop unnecessary paint calls.
- Background threads, async runtimes, terminal capability negotiation, or a new dependency.
- Dropping/reordering non-resize input, weakening post-read timestamp sampling, or changing modal routing.
- Benchmarking terminal emulators over the network or adding an automated GUI harness.

## Implementation requirements

- Draw exactly one initial frame, then draw only after visible state may have changed: accepted terminal input, a resize batch, a timer transition (including which-key appearance or transient expiry), or idle spell/diagnostic progress.
- Keep polling at the existing deadline/budget cadence even when no paint is needed. Do not block or starve spell work.
- When a ready event is Resize, retain only the final resize in a consecutive queued burst before dispatch/reflow; preserve the order and individual post-read timestamp sampling of all non-resize events. Bound a batch so sustained input cannot starve presentation indefinitely.
- Wrap every draw in `BeginSynchronizedUpdate`/`EndSynchronizedUpdate` using the existing Crossterm dependency. Attempt the end command even if rendering fails, and have terminal restoration end any in-progress synchronization defensively.
- Keep bracketed paste, mouse capture, alternate screen, cursor-shape, and raw-mode setup/restore centralized and balanced.
- Add deterministic tests that count requested draws, event dispatch order, coalesced resizes, timer/spell invalidation, synchronized command ordering, and error cleanup. Do not rely only on a manual visual claim.
- Add concise entries under `CHANGELOG.md`'s Unreleased section for command UI, copyable link rows, responsive/horizontally navigable tables, cursor configuration, and stable frame presentation.

## Acceptance criteria

- [ ] A scripted launch with a document produces one initial paint; repeated idle polls with no visible change produce no additional paint.
- [ ] Consecutive queued resize events produce one effective resize/reflow and one subsequent paint at the final dimensions.
- [ ] Key, paste, mouse, overlay, which-key deadline, transient expiry, and spell-diagnostic changes still trigger timely paints and preserve their routing/timestamp rules.
- [ ] Every recorded paint is enclosed by one begin/end synchronized-update pair, including the initial frame.
- [ ] A forced draw error still emits end-synchronization and normal terminal restoration emits defensive end-synchronization, cursor reset, bracketed-paste disable, mouse disable, alternate-screen leave, and raw-mode disable behavior as applicable.
- [ ] Existing event-loop budget, interruptibility, resize remap, terminal guard, panic/signal, and modal exclusivity tests pass.
- [ ] The Unreleased changelog accurately summarizes all user-visible workflow changes.

## Validation

- `make test`
- `make build-release`

## Dependencies

- Task 005

## Expected areas of change

- `crates/oom-edit/src/event.rs`
- `crates/oom-edit/src/app.rs`
- `crates/oom-edit/src/lib.rs`
- `crates/oom-edit/src/terminal_guard.rs`
- `CHANGELOG.md`

## Risks / notes

The scheduler must distinguish a timer deadline that merely wakes polling from one that changes visible state. Event coalescing must never consume or reorder a key behind a resize. Synchronized-update cleanup is terminal state, so all error and signal paths need explicit review rather than relying only on RAII Drop.
