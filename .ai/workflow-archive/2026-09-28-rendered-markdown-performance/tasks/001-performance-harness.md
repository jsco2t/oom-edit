# Task 001: Reproducible performance fixtures and public-pane harness

Delegation: main-only

## Goal

Make the reported content-sensitive delays reproducible through the supported public pane API on this machine.

## Context

The existing release gate uses repeated prose and misses code-heavy, list and table notes. The external scratch directory cannot be a permanent test dependency.

## Scope

### In scope

- Add deterministic, versioned Rust fixtures for prose, mixed Markdown, small fences, one Rust fence, lists and tables, including the sizes in the plan.
- Add a cold subprocess runner using only public `EditorPane` inputs and owned frames; include host cell copy, both pane sizes, five trials, phase times, RSS, layout heap and fixture identity.
- Add `make bench-realistic` and `make test-realistic-performance`, with fixture-shape and runner-contract tests in `make test`.
- Record baseline results as task evidence; the release performance target is expected to fail before optimizations.

### Out of scope

- Product-path optimizations, external vault content, or changing existing performance budgets.

## Implementation requirements

- Use a fresh process for each cold sample and time construct + open + first complete rendered frame + host cell copy. Keep sample generation outside the timed region.
- Include source/Insert and rendered/Normal modes, cursor positions with and without source atoms, and edit/save/width/reload trigger modes for later comparison.
- Assert exact source byte size, block/fence counts, fixture version/hash and frame dimensions; reject zero-work or silently missing samples.
- Keep runner configuration and invocation behind Make targets; no new dependency.

## Acceptance criteria

- [ ] Fixed fixtures cover every named content class and acceptance size without relying on external files.
- [ ] Runner reports cold public-pane phase times and process memory for both viewport sizes, with five independent samples per gate cell.
- [ ] Shape and runner tests pass; the known baseline failure is captured as evidence without weakening the final target.

## Validation

- `make test-realistic-performance`
- `make test-pane-public`

## Dependencies

None

## Expected areas of change

`crates/oom-edit/perf/`, `crates/oom-edit/tests/`, `scripts/`, `Makefile`, `docs/performance.md`

## Risks / notes

Wall time varies with machine load. Preserve raw per-trial observations and report the worst; functional tests must not depend on a wall-time threshold.
