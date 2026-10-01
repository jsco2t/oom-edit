# Task 016: Sustained rendered keyboard navigation

Delegation: main-only

## Goal

Retain a regression gate for long Normal-mode keyboard navigation on the actual 1 MiB example, and record the explicit branch-release kitty retest without inventing an unmeasured repair.

## Context

Round 1 timed short wheel reversals and excluded actual terminal output. Task 013 and the exact 63×229 follow-up found no branch-only pane or PTY-output stall in fresh or retained states. The user explicitly retested this branch's release binary in kitty and now sees no hitch. The earlier `oom-edit` command pointed to a clean-main installed binary. Physical kitty presentation is not measured by a PTY flush proxy; the original hitch's cause is unconfirmed.

## Scope

### In scope

- Preserve every key/cursor/frame and test both fresh flat and post-edit retained states.
- Add a focused asserting sustained-navigation target so these cases can be gated independently of unfinished Select and edit latency work.
- Repair only a newly reproduced, measured navigation regression on the shared pane/core or standalone presentation path.

### Out of scope

- Optimizing memory solely for the reported 12 MB difference.
- Suppressing key events, painting stale rows, reducing fidelity, or hiding output behind a loading mode.

## Implementation requirements

- Preserve task-013 phase attribution and raw traces. The burst probe must acknowledge that all queued keys were processed; a frame after the last key was sent is insufficient. If a new failure is measured, select the smallest coherent fix at its actual layer. Do not add a host-specific divergent renderer.
- Compare repeated `j`/`k`, arrows, initial post-open and stop/resume runs with `main` sequentially. Cover both directions and the transition into/out of large fences.
- Validate exact viewport rows, semantic styles, source atoms, cursor, scroll position, status and terminal output; measure input, frame and PTY-flush tails, not just median CPU time.

## Acceptance criteria

- [ ] The earlier hitch is recorded as not currently reproducible in the explicit branch release binary; no unsupported cause or product-code repair is claimed.
- [ ] The final 1,000-step per-process keyboard scenarios meet the plan's pane and PTY-tail gates without skipping keys or displaying stale frames.
- [ ] Source Insert, wheel navigation, standalone and embedded behavior remain functionally unchanged; the focused navigation gate is discoverable and tested.

## Validation

- `make test-pane-public`
- `make bench-acceptance-1mb-navigation`
- `make check`

## Dependencies

014

## Expected areas of change

`crates/oom-edit-core/src/rendered/rows.rs`, `crates/oom-edit-core/src/session.rs`, `crates/oom-edit/src/app.rs`, `crates/oom-edit/src/event.rs`, `crates/oom-edit/src/screens/rendered.rs`, `scripts/`

## Risks / notes

The displayed terminal may lag even if owned-pane frames are fast. The final result must account for both rather than claiming a headless-only win.
