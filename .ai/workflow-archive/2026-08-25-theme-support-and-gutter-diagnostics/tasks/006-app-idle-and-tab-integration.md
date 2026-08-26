# Task 006: App Idle Scheduling and Per-tab Integration

Delegation: main-only

## Goal

Integrate gutter summary ownership, invalidation, publication, bounded idle work, input preemption, and quiescence into App and the existing event loop for every tab.

## Context

Task 005 defines the pure state. Core already invalidates diagnostics synchronously and publishes them through bounded spell work; App must consume the clean publication without scanning or projecting during open, mutation, layout, or render.

## Scope

### In scope

- Completed/pending/generation gutter state in each `TabEntry`.
- Empty initialization on new/open, O(1) invalidation on text mutation, replacement, provider reset, and spell disable.
- Publication detection after the active session becomes diagnostically clean; start pending projection without unbounded work.
- Bounded marker projection as subsequent `App::on_idle_unit` work, input preemption through the existing event loop, atomic publication, and redraw semantics.
- Tab switch/isolation, inactive-tab protection, provider unavailable/disabled behavior, test helpers/counters, and quiescence.
- Existing Trouble overlay refresh behavior preserved independently.

### Out of scope

- Gutter painting or screen geometry changes.
- Core diagnostic API, mutation gateway, provider implementation, or rendered decoration changes.

## Implementation requirements

- Use existing session pending/publication transitions around forwarded input to invalidate only when diagnostics become/remain stale; do not clone document text or add a public core mutation epoch.
- Open/replace and spell disable clear both completed and pending state immediately without building the spell engine or inspecting diagnostics.
- A clean publication starts a pending build, but that same publication step must not project the full diagnostic set. Later units honor the builder budget.
- Any edit/reset/replace/disable generation change discards pending work and prevents stale publication.
- Rendering and viewport changes do not advance provider/build work and do not mutate snapshot/capacity.
- Idle order is host build, active-session scan/publication, then bounded marker projection; once all are clean `on_idle_unit` returns false and requests no redraw.
- Pending terminal input stops the idle drain between units, preserving current scheduler semantics.

## Acceptance criteria

- [ ] New/open/replace/edit/reset/disable tests show immediate empty/invalidation state with zero diagnostic-to-line projections on synchronous paths.
- [ ] Provider scan completes before bounded marker build, partial summaries remain invisible, final publication is atomic, and idle becomes quiescent.
- [ ] Input preemption stops before another marker unit and a canceled generation can never publish.
- [ ] Separate tabs preserve independent empty/pending/completed state; active rendering/work cannot project inactive tabs.
- [ ] Repeated App renders leave provider/build counters and snapshot memory unchanged.
- [ ] Existing Trouble overlay and spell behavior remain correct, with no core/public/dependency change.

## Validation

- `make test`

## Dependencies

- Task 005

## Expected areas of change

- `crates/oom-edit/src/app.rs`
- `crates/oom-edit/src/event.rs`
- `crates/oom-edit/src/gutter.rs`
- App/event/snapshot test helpers

## Risks / notes

This is cross-cutting asynchronous state and cancellation logic. Preserve modal routing and lifecycle target ownership. Bounded loops in tests require explicit maximums and diagnostic failures; do not use sleeps or threads.
