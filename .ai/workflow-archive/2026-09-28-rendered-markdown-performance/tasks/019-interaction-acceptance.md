# Task 019: Full interaction, fidelity and host acceptance

Delegation: main-only

## Goal

Validate the integrated delivery against the user's real 1 MiB workflow, round-1 contracts and both supported host models, then document measured behavior and limits.

## Context

The prior final gate passed while the user still experienced sustained keyboard hitches, slow Select and wrong deletion. This task closes that escaped-acceptance gap rather than relying only on generated fixtures.

## Scope

### In scope

- Run the asserting exact-document acceptance suite and existing package gates.
- Review complete rendered/source fidelity, Select operations, standalone/embedded parity and memory.
- Update performance documentation and a reproducible manual verification transcript.

### Out of scope

- Relaxing an acceptance threshold solely to produce a green gate.
- Modifying `oom` or adding a loading/pending UX.

## Implementation requirements

- Compare fresh and post-edit retained runs, immediate navigation after opening, 1,000-key sustained and stop/resume sequences, short and multi-line `v` selection, `d`/`c`/undo, source Insert behavior and both code-fence languages. Capture per-phase tails and terminal-output proxy measurements.
- Compare all affected current frames with a fresh full-builder oracle for cells, semantic styles, mapped atoms, line numbers, links/footnotes, cursor, selection and operation results. Run standalone and split-pane embedded tests on the same core code path.
- Record cold first-frame and memory outcomes without letting them override interaction/fidelity priority. Update `docs/performance.md` with the exact static fixture, measured limits, and the distinction between pane completion and physical terminal presentation.
- Provide a short manual acceptance procedure for human confirmation of perceived smoothness; automated success cannot replace the user's visual report.

## Acceptance criteria

- [ ] All five round-2 package acceptance criteria are met with raw records and no unresolved correctness or UX regression relative to `main`.
- [ ] `make bench-acceptance-1mb` and every pre-existing final quality command pass with no warnings.
- [ ] Core-only, public pane, standalone and embedded paths retain the same functionality and fidelity, including unchanged Select yank/copy behavior.
- [ ] Documentation distinguishes measured promises from global/adversarial fallback costs and supplies a reproducible manual check.

## Validation

- `make bench-acceptance-1mb`
- `make bench-interactions`
- `make test-public-api`
- `make test-embedding-example`
- `make test-standalone-host`
- `make check`

## Dependencies

018

## Expected areas of change

`docs/performance.md`, `Makefile`, `scripts/`, `crates/oom-edit-core/tests/`, `crates/oom-edit/tests/`, `.ai/workflow/evidence/`

## Risks / notes

A PTY flush is not the same as the human's terminal finishing a frame. Report both objective timing and a manual acceptance protocol; if the user still sees hitches, treat that as new acceptance evidence rather than declaring the perception wrong.
