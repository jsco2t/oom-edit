# Task 005: Enforce baseline performance acceptance

Delegation: main-only

## Goal

Make substantial public-pane regressions fail acceptance even when broad absolute budgets pass, and prove the integrated repaired package preserves the established baseline performance.

## Context

The rejected candidate passes all public-pane absolute cases despite 75–81% slower character editing. Existing native TUI comparison detects another manifestation. Round 2 retains both absolute and relative checks; the original package baseline is 1960ade52a394a7c23fe400fdbb395283834ab72. Task 004 restores the source path. This task supplies a narrow missing comparison gate and complete package verification.

## Scope

### In scope

A small stdlib-only public-pane JSONL comparator, make targets and meaningful contract tests, performance documentation, clean baseline verification, fresh serial measurements, comparison evidence and integrated review.

### Out of scope

Performance threshold increases, changed fixture geometry/inventory, fabricated or discarded timings, general benchmarking infrastructure, autosave I/O redesign, dependencies, unrelated features, publication and archival.

## Implementation requirements

1. Add scripts/interaction_performance_compare.py using existing interaction_performance.py as the authoritative case/fixture/cycle/absolute-gate source. Keep record and gate interfaces compatible and avoid duplicated case lists. Expose BASELINE, CANDIDATE and OUTPUT through make interaction-perf-compare.
2. Require the full existing 1 MiB, 100x41 gate inventory: five trials, 21 cycles and correct steps/modes/identity per case. Reject missing/duplicate/mismatched trial-cycle-step tuples, wrong phase labels, incompatible fixture geometry, nonpositive/incoherent times and incomplete first/warmed returns. Both sides must meet the existing absolute gate.
3. Compare each case/step/phase group separately. For median input_ns, render_ns and total_ns, fail growth greater than both 10% and 100,000 ns. For median peak_rss_bytes, fail growth greater than both 5% and 1,048,576 bytes. Match the established TUI comparator's numerical allowances; use both thresholds together. Preserve raw precision; report p99/worst observations without dropping cold or tail data. Emit complete machine-readable comparison output before returning nonzero on a regression.
4. Add scripts/test_interaction_performance_compare.py and make interaction-perf-compare-test. Test unchanged/faster results, meaningful regressions for each metric, equality/just-over threshold boundaries, malformed/incomplete/reclassified evidence, output preservation and nonzero CLI status. Include these tests in make test so make check runs them. Retained rejected raw data must fail the new comparison on the observed top/middle character paths; document that real-data negative check in evidence.
5. Document the relative comparison command, requirements and numerical allowances in docs/performance.md. Preserve all previous absolute budgets and measurements as history. No benchmark fixture, case count, warmup, sample count or threshold may be reduced to pass.
6. Verify /tmp/oom-autosave-perf-baseline-20261012 is clean and exactly baseline_sha. If absent, run make tui-perf-baseline-prepare BASELINE_DIR=/tmp/oom-autosave-perf-baseline-20261012 BASELINE_REV=1960ade52a394a7c23fe400fdbb395283834ab72. Never reset or clean an unexpected checkout; stop on a conflicting baseline path. Keep it ready for the explicit final gate commands.
7. Run the strengthened final gate from gate.json after integrated review. Timing-sensitive builds/benchmarks run serially on one host/toolchain. Record fresh five-trial TUI and 2,310-sample public-pane data for both versions; keep cold/warmed returns distinct. Validate provenance, including the candidate's uncommitted source identity. Preserve rejected data and all failed attempts; record why any rerun was necessary.
8. Map every Round 2 and original package criterion to actual evidence. Review cumulative source changes, all autosave safety tests and restored syntax behavior in the primary thread. Close the P1 review finding only when unchanged release gates and both relative comparisons pass. Do not label a passing broad limit as proof of restored baseline latency.

## Acceptance criteria

- [ ] Comparator uses the existing inventory, validates both complete datasets, applies specified thresholds exactly and rejects real rejected-candidate slowdown.
- [ ] Contract tests cover correctness, evidence integrity and CLI failure/output behavior; make help/test/check expose and run the new workflow.
- [ ] Existing benchmark interfaces, absolute limits, fixture identities and sample counts remain unchanged; performance docs match implemented tooling.
- [ ] Clean original baseline and candidate evidence share compatible metadata/provenance. Fresh TUI and interaction comparisons contain no failed row.
- [ ] All original and strengthened final gate commands pass, with full evidence and no omitted/reclassified failure.
- [ ] Correct source ownership, inexpensive line-start editing and complete optional autosave behavior remain intact; the review finding is closed using measurements.

## Validation

- make interaction-perf-compare-test
- make realistic-perf-test
- make tui-perf-test
- make test-pane-autosave
- make test-package-guards
- Standard task gate: make check
- Whole-package final validation: every exact command in gate.json, in listed order. Baseline preparation above is a prerequisite when the checkout is absent.

## Dependencies

004

## Expected areas of change

scripts/interaction_performance_compare.py; scripts/test_interaction_performance_compare.py; scripts/interaction_performance.py only for narrowly necessary reuse; Makefile; docs/performance.md; new Round 2 evidence. Product syntax repairs belong to task 004; high-confidence in-scope corrections remain subject to the approved plan.

## Risks / notes

Repeated absolute budget misses or incompatible metadata are failures, not permission to select samples. The separate baseline spelling miss is recorded in plan.md and cannot be silently ignored if the final candidate reproduces it. Benchmark normal edit-path performance separately from synchronous durability I/O; enabled saves intentionally perform real atomic filesystem work. Keep immutable Round 1 and rejected-candidate evidence untouched.
