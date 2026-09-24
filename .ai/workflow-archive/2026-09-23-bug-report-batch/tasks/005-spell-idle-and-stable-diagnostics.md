# Task 005: Delay spell work and retain valid decorations

Delegation: main-only

## Goal

Wait five seconds after the latest input before spell work and prevent
unrelated misspelling underlines from disappearing during repeated ordinary
typing.

## Context

The event loop currently gates all idle spell work at 150 ms. `SpellState`
properly removes/updates diagnostics for the first local edit, but if another
edit arrives before the replacement scan is clean it clears every diagnostic
and restarts a full scan. This is the visible underline flicker in the report.

## Scope

### In scope

- A five-second post-input idle threshold with boundary-exact deterministic
  event-loop tests.
- Retention and byte shifting of published diagnostics proven outside repeated
  locally-safe invalidation ranges.
- Cancellation/restart of obsolete exclusion/token scan cursors without
  publishing partial old-generation results.
- Continued full clearing for edits whose Markdown/exclusion context makes
  existing diagnostics potentially stale.
- Tests that source and rendered diagnostic decorations remain visible outside
  the edited line and converge to a fresh full scan after idle work completes.
- Changelog documentation.

### Out of scope

- A configurable debounce duration.
- New dictionaries, spelling policy, suggestions, or tokenizer rules.
- Background threads or a new diagnostics publication owner.

## Implementation requirements

- Add failing timing and diagnostic-publication tests first.
- Use the event timestamp already sampled after input; do not call the clock
  inside pure scheduling/state functions.
- Before five seconds of idleness, no engine-build, scan, or gutter-projection
  unit may run. At exactly the threshold, work may begin if existing deadline
  and slice-safety rules allow it.
- A diagnostic may remain published only if its source text/range is still
  provably valid after the edit. Full/context-changing invalidations remain
  conservative and may clear affected/all diagnostics.
- Pending scan results from pre-edit text must never be merged after a newer
  edit; final results must equal a fresh session scan.
- Preserve bounded cooperative work, performance counters, Trouble/gutter
  publication state, and zero-dependency architecture.

## Acceptance criteria

- [ ] Spell work performs zero units before five seconds since the latest input and begins at/after the exact threshold under normal deadline conditions.
- [ ] Every new input resets the full five-second delay.
- [ ] Repeated locally-safe edits on one line retain and correctly shift published diagnostics on unaffected lines, so their source and rendered decorations do not blink off.
- [ ] Diagnostics intersecting the edited line are removed until rescanned, and context-changing edits still clear any result that cannot be proven current.
- [ ] A completed restarted scan is identical to a fresh full scan, with no obsolete partial result publication.
- [ ] Existing bounded idle slices, input precedence, gutter/Trouble behavior, and performance gates continue to pass.

## Validation

- `cargo test -p oom-edit --offline --locked spell_idle_delay`
- `cargo test -p oom-edit --offline --locked timeout_requires_full_idle_delay`
- `cargo test -p oom-edit-core --offline --locked repeated_local_spell_invalidation`
- `cargo test -p oom-edit-core --offline --locked spell_session`
- `make check`

## Dependencies

- 004

## Expected areas of change

- `crates/oom-edit/src/event.rs`
- `crates/oom-edit/src/app.rs`
- `crates/oom-edit-core/src/spell/state.rs`
- `crates/oom-edit-core/tests/spell_session.rs`
- relevant performance/event-loop tests
- `CHANGELOG.md`

## Risks / notes

The key safety distinction is between retaining a diagnostic that still points
to unchanged text and retaining one merely for visual continuity. Only the
former is allowed. If invalidation cannot prove locality, correctness requires
temporary removal even though that specific edit may still cause a limited
visual change.
