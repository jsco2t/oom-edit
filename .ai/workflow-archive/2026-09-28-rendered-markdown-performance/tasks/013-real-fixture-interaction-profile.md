# Task 013: Reproduce real-fixture interaction and terminal latency

Delegation: main-only

## Goal

Turn the user's sustained keyboard, Select and code-deletion reports into a reproducible, phase-attributed baseline on the exact 1 MiB example, compared sequentially with `main` on this machine.

## Context

Round 1 measured short, reversing wheel motions on generated Markdown and omitted Select and terminal output. A passing result there does not settle the reported UX regression.

## Scope

### In scope

- Add deterministic public-pane and PTY/standalone measurement scenarios for `examples/kitchen-sink-1mb.md`.
- Record candidate and clean-`main` observations in separate, versioned evidence without changing the user's working tree or note.
- Add runner-contract tests and Make targets for non-asserting records and the eventual asserting gate.

### Out of scope

- Product-algorithm fixes or relaxed performance limits.
- Changes to the integrating `oom` project.

## Implementation requirements

- Measure separate 1,000-consecutive-key Normal-mode sequences per fresh process in both directions, starting in the reported 400–600-row area and separately inside each large fence; include immediate post-open, warmed, stop/resume, `j`/`k`, and arrow keys. Preserve event cadence and record queue/batch timing rather than collapsing a burst into one key.
- Separately exercise 10–20-line character Select motion and `d`/`c` plus undo inside Rust and Go fences, and a prose example. Assert actual text/mode/frame state after each operation.
- Attribute input handler, source/injection work, Markdown model, row/selection projection, owned frame, host draw/write/flush, output bytes, idle work, and RSS. State clearly that PTY flush is a proxy for terminal presentation, not proof of a completed physical display refresh.
- Use an isolated `main` revision sequentially on the same machine; do not overwrite or reset the active worktree. Record revision, fixture hash, pane dimensions, sample cadence, raw tails and reproduction steps. Measure both initial flat and post-edit retained states.
- Add `make bench-acceptance-1mb-record` and `make bench-acceptance-1mb` to `Makefile`; the asserting target may report known failures until fixes, but contract tests must pass. No new dependency without the repository checklist.

## Acceptance criteria

- [ ] Raw candidate/`main` profiles reproduce or explicitly falsify each user-reported behavior and identify which latency phase differs.
- [ ] Existing wheel benchmark's blind spots are documented with actual sample/runner evidence.
- [ ] The exact fixture, each key and operation result, sample count, timing, and source revision are validated by runner tests.
- [ ] The new targets are discoverable in `make help`, while existing performance gates remain unchanged.

## Validation

- `make test`
- `make bench-acceptance-1mb-record`
- `make check`

## Dependencies

None

## Expected areas of change

`Makefile`, `scripts/`, `crates/oom-edit/examples/`, `crates/oom-edit/tests/`, `.ai/workflow/evidence/`

## Risks / notes

Human-visible hitch timing may differ from PTY write completion. Do not call a root cause confirmed solely because one code path looks expensive; tie it to the measured phase and rule out competing causes.
