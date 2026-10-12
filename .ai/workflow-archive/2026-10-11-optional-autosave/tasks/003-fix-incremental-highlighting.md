# Task 003: Fix incremental highlighting mismatch

Delegation: main-only

## Goal

Repair the saved core incremental/fresh source-highlighting discrepancy and restore the package quality gate.

## Context

The user explicitly approved this scope extension after final make check exposed a pre-existing core defect. See evidence/final-attempt-001.md for seed and failure.

## Scope

### In scope

Reproduction/minimization, the responsible core parser or injection invalidation decision, deterministic regression and persisted seed, focused validation and full original gates.

### Out of scope

Other core features, dependency upgrades, unrelated defects, public API expansion, weakened tests, global fresh-parse fallback, commits or archival.

## Implementation requirements

1. Replay the saved case and isolate its failing edit and parser/injection path before changing production behavior.
2. Preserve complete incremental/fresh semantic span equality after every edit, including byte coordinates and styles; add a readable regression and keep the persisted seed.
3. Repair the responsible path narrowly, preserve existing bounded/incremental cases and core terminal independence.
4. Register any new focused validation command in Makefile.
5. Rerun task-specific checks, make check and every original final gate command. Preserve completed task evidence and the failed final-attempt evidence.

## Acceptance criteria

- [ ] The original case and deterministic minimized regression pass against complete fresh highlighting after each edit.
- [ ] Root cause and repair are documented with evidence, without global fallback, dependency or public API change.
- [ ] Focused syntax/performance-path/autosave checks and make check pass without warnings or weakened tests.

## Validation

- make test-syntax-incremental-regression
- make test-first-frame
- make test-pane-autosave
- Standard task gate: make check

## Dependencies

001, 002

## Expected areas of change

crates/oom-edit-core/src/syntax/mod.rs; private syntax regression test module if helpful; proptest-regressions/syntax/mod.txt; Makefile.

## Risks / notes

Keep invalidation conservative only where the edit can change grammar ownership. Do not discard the random seed or reduce test cases to obtain a passing gate.
