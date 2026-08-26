# Task 009: Candidate Performance Evidence and Comparison

Delegation: main-only

## Goal

Complete the TUI performance matrix for marker states, record five final candidate trials, generate the same-host comparison, and resolve every in-scope regression before final package review.

## Context

Task 001 retained the post-repair/pre-theme baseline and the separate source/rendered contracts. The implemented feature must now satisfy both absolute NFR families, rendered-layout and marker viewport scaling, retained memory, quiescence, and same-machine relative thresholds without unexplained retries or rebaselining.

## Scope

### In scope

- Extend the private harness with source/rendered empty, sparse-visible, dense-document, 5,000/50,000 off-screen scaling, idle projection, cancellation, snapshot memory, and final quiescence cases.
- Debug deterministic work/scaling smoke and exact release wall/CPU/RSS/memory gates.
- Re-run the 256 KiB/512 KiB/1 MiB cold rendered-layout time/heap scaling matrix and the one-shot rendered first-frame RSS gate after all feature changes.
- Five candidate trials at the external PRD evidence path using the baseline's toolchain/machine/fixture/profile.
- Generated median/worst comparison TSV with all thresholds, aggregates, deltas, and machine-readable pass/fail status.
- Diagnose and fix high-confidence performance regressions within the approved architecture; rerun complete candidate evidence after any fix.

### Out of scope

- Changing requirements/thresholds, discarding baseline rows, silent retries, rebaselining, or architecture changes outside the approved plan.
- New profiling dependencies or unsafe allocator instrumentation.

## Implementation requirements

- Construct sessions, themes, layouts, and snapshots outside timed steady-state regions; warm and reuse `TestBackend`.
- Each case measures at least 250 ms of work and reports wall average/worst, process CPU/frame, peak RSS, and snapshot payload/capacity.
- Empty comparable source/rendered medians fail only when regression exceeds both 10% and 100 µs; RSS fails only when exceeding both 5% and 1 MiB.
- Source first frame remains below 150 ms worst; cold 1 MiB rendered layout remains below 250 ms worst; end-to-end rendered first frame remains below 350 ms worst; edit-to-frame and source scroll remain below 50 ms worst.
- Retained 1 MiB `RenderedLayout` heap remains at or below 64 MiB, one-shot peak RSS remains at or below 192 MiB on the evidence host, and cold-layout time/heap grow by no more than 2.25× per 256 KiB → 512 KiB → 1 MiB doubling.
- Completed snapshot memory stays at or below 24 bytes per unique marked line plus 4 KiB fixed overhead and repeated rendering does not grow it.
- Increasing off-screen marked lines from 5,000 to 50,000 with 40 visible rows stays within 25% median wall and CPU growth.
- Comparator must reject any baseline/candidate metadata mismatch or incomplete five-trial evidence before generating a passing comparison.
- Evidence writes are atomic. Never overwrite good evidence with a partial/interrupted run.

## Acceptance criteria

- [ ] `make bench-check` covers relaxed debug TUI smoke plus deterministic visible-row, memory-shape, cancellation, and quiescence invariants.
- [ ] `make bench` covers all exact release TUI cases and existing core/spell benchmarks with no warning or failure.
- [ ] The final large-document rendered matrix passes the 250 ms cold-layout, 350 ms rendered-first-frame, 64 MiB heap, 192 MiB peak-RSS, and 2.25× per-doubling gates without changing fixture semantics.
- [ ] Five complete candidate trials exist at `/Users/jason/Developer/sources/personal/notebook/projects/oom-edit/features/04-themes/evidence/performance/candidate.tsv` and match baseline environment metadata.
- [ ] `/Users/jason/Developer/sources/personal/notebook/projects/oom-edit/features/04-themes/evidence/performance/comparison.tsv` contains all comparable/new case thresholds and passes every relative, absolute, scaling, memory, and quiescence condition.
- [ ] No unexplained retry, discarded row, weakened threshold, or silent rebaseline occurred; any repaired regression is documented in task evidence.
- [ ] Final performance implementation remains private, dependency-free, make-owned, portable across shipped macOS/Linux targets, and leaves public/core boundaries unchanged.

## Validation

- `make bench-check`
- `make bench`
- `make tui-perf-record BRANCH_ROLE=candidate OUTPUT=/Users/jason/Developer/sources/personal/notebook/projects/oom-edit/features/04-themes/evidence/performance/candidate.tsv TRIALS=5`
- `make tui-perf-compare BASELINE=/Users/jason/Developer/sources/personal/notebook/projects/oom-edit/features/04-themes/evidence/performance/baseline.tsv CANDIDATE=/Users/jason/Developer/sources/personal/notebook/projects/oom-edit/features/04-themes/evidence/performance/candidate.tsv OUTPUT=/Users/jason/Developer/sources/personal/notebook/projects/oom-edit/features/04-themes/evidence/performance/comparison.tsv`

## Dependencies

- Task 008

## Expected areas of change

- `crates/oom-edit/src/perf_tests.rs`
- Make-owned performance script/comparator support
- `Makefile`
- External `evidence/performance/candidate.tsv`
- External `evidence/performance/comparison.tsv`

## Risks / notes

Evidence must be produced on the same host/toolchain as Task 001 with concurrent project tests stopped. A threshold failure must be optimized within approved scope or transition to `PLAN_CHANGE_REQUIRED`; it cannot be waived. External evidence writes may require explicit filesystem authorization.
