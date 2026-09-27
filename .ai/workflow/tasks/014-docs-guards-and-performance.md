# Task 014: Integrated documentation, guards and performance proof

Delegation: main-only

## Goal

Close all DRD clauses with exact public API/privacy/coverage guards, documentation and end-to-end performance evidence before release.

## Context

Earlier slices own their functional tests, but the complete pane requires a final integrated API and regression review against the baseline and all dependent oom PRD editor needs.

## Scope

### In scope

D-012/D-013/D-015 final; FR-001/005/118 and NFR-001–010 integrated checks, documentation, same-host comparison and package gate wiring.

### Out of scope

Release tag publication or implementation of oom itself.

## Implementation requirements

- Resolve every planned manifest case to a named executable assertion with fixture/oracle/make target and red/green evidence. Add strict `make coverage-check` that rejects missing/unknown/ignored/unjustified exclusions and any planned case. Extend public API compile/compile-fail, dependency/hjkl isolation, exact modes/commands, registry uniqueness and unsafe-scope guards.
- Update `AGENTS.md` facade/host architecture rule, `CONTRIBUTING.md` data flow, `README.md` embedding and intentional disk/persistence behavior, `CHANGELOG.md`, `docs/dependencies.md` as needed and canonical repository metadata. Create `DEVELOPER.md` as a complete tested guide for another Rust project's integration: exact dependency/four-patch manifest and provenance, lock/vendor setup, pane construction/services/config/theme, owned input/frame translation, focus/tick/idle loop, event draining, prepared lifecycle/disk coordination, notices/guard, and runnable example/check commands. Ensure public `#![deny(missing_docs)]` and `make doc` warning-free.
- Run exact `make bench`, owned-frame ≤1 ms release case, 50-tab poll counters, analysis allocations and full public-path input/redraw/idle cases. Record five same-host candidate TUI trials and compare with Task 001 baseline using existing make targets. Keep existing absolute budgets and comparator thresholds unchanged.
- Resolve the pre-existing first-frame absolute-limit failure observed in Task 007 before declaring any package-level performance gate green. The user's decision moved this work to Task 014; it did not waive the limit.
- Sequencing supersession approved on 2026-09-27: the user subsequently requested "resolve it now please" at Task 010's benchmark approval boundary. Task 010 now implements and validates the first-frame fix before host migration; this task retains full integrated re-verification with the unchanged absolute and relative limits.
- Review standalone trace/frame differences clause by clause; update no golden except explicitly approved behavior. Walk oom PRD's listed editor-dependent requirements against public example/API. Add strict coverage and worktree-local checks to `make ci`; wire the independent committed-revision downstream target into CI separately, retaining make/dev parity without requiring a nonexistent candidate revision during local implementation.

## Acceptance criteria

- [ ] Strict coverage meta-test has no planned/missing/ignored cases and detects manifest mutations; every included clause has meaningful executable assertions and red/green evidence.
- [ ] Exact public facade/privacy and architecture guards pass; no new unsafe code or forbidden dependency direction exists.
- [ ] Existing and new absolute performance budgets pass; five-trial same-host comparison shows no disallowed wall/CPU/RSS/heap regression.
- [ ] `AGENTS.md`, `DEVELOPER.md`, other documentation, notices, source metadata and oom-editor API reachability checklist are complete and accurate; integration snippets run against the example/fixture and `make doc` is warning-free.
- [ ] `make ci` and CI include the new required make-owned acceptance workflows; committed-revision and tag checks have no pre-commit/pre-tag circularity.

## Validation

`make ci`

`make build-examples`

`make doc`

`make bench-check`

`make bench`

`make tui-perf-record BRANCH_ROLE=candidate OUTPUT=.ai/workflow/evidence/perf-candidate.tsv TRIALS=5`

`make tui-perf-compare BASELINE=.ai/workflow/evidence/010-baseline-current-host.tsv CANDIDATE=.ai/workflow/evidence/perf-candidate.tsv OUTPUT=.ai/workflow/evidence/perf-comparison.tsv`

## Dependencies

013

## Expected areas of change

`AGENTS.md`, `DEVELOPER.md`, `README.md`, `CONTRIBUTING.md`, `CHANGELOG.md`, `docs/`, `Cargo.toml`, API/dependency/coverage/performance tests, `Makefile`, `.github/workflows/ci.yml`, workflow evidence.

## Risks / notes

Do not lower existing budgets or weaken snapshots to pass. Candidate TSV before the immutable release commit is provisional; Task 015 reruns exact-revision evidence and consumer checks.

Revision 4 input amendment explicitly approved on 2026-09-27: only the comparison
baseline path above changes to Task 010's preserved current-host measurement of
the unchanged original baseline commit. Original Task 001 evidence and all
performance thresholds remain unchanged.
