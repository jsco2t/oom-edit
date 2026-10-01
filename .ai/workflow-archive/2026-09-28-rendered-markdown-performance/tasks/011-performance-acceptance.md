# Task 011: Public-pane interaction and cold performance

Delegation: main-only

## Goal

Assert the interaction-first latency envelope and prove the incremental core
does not trade away scroll, cold-open, memory or scaling performance.

## Context

Task 006 establishes a fixed same-machine baseline; tasks 008–010 implement
the production incremental path. The current realistic benchmark gates every
class at 350 ms cold first frame, which conflicts with the user's explicit
priority of smooth editing over cold-frame shaving.

## Scope

### In scope

- Add an asserting `make bench-interactions` target using at least five fresh
  processes × 21 verified cycles, fixed fixture/version IDs, top and midpoint
  positions, input/frame timing and nearest-rank p95/p99/worst output.
- Assert under-50-ms p99 key-to-owned-frame for the named ordinary 1 MiB local
  edits and at least 100 warmed mode returns, with a separate <50 ms worst
  check for those warmed returns. Measure exactly one first-ever
  source-to-rendered return per fresh source-first process as a cold rendered
  transition, requiring <500 ms worst-of-five key-to-owned-frame at top and
  midpoint. Assert under-25-ms p99 source/rendered wheel scroll.
- Update `make bench-realistic` with the revised cold policy: retain ≤350 ms
  for cells already passing after task 005; require <500 ms worst-of-five for
  1 MiB mixed at both pane sizes; permit the 448 KiB table cell only its
  task-006 baseline worst plus 10%. Preserve scaling measurements and an
  explicit memory non-regression gate. Keep the existing fixed-fixture 64 MiB
  layout heap and 192 MiB process-RSS ceilings in `make bench`.
- Record resize, reload, save, global edits, large Rust fences, input/frame
  phases, peak RSS and retained heap. Compare each with task-006 baseline and
  investigate any >10% regression rather than hiding it in an aggregate.
- Add script/fixture contract tests and the new Make target in the same change.

### Out of scope

- Relaxing interaction or fidelity targets to make the benchmark green,
  measuring only a direct core path, or modifying the integrating project.

## Implementation requirements

- Fail the gate on missing/wrong fixture, case, sample count, mode, dimensions,
  intended text/viewport change or malformed timing. Keep raw JSONL records
  available for independent analysis.
- Verify and report the first-ever source-to-rendered frame separately from
  subsequent returns. A later return cannot consume the cold allowance.
  The first cold transition must follow a verified local source edit and
  deliver a complete current-text frame; record open-to-first-render
  separately from transition latency.
- Report percentile and worst observations; do not rely on one favorable run.
- If a cold target conflicts with the proven interaction target, stop for a
  plan-change decision rather than optimizing cold work at the expense of use.
- Do not claim a universal 50 ms or 500 ms bound for adversarial Markdown.

## Acceptance criteria

- [ ] `make bench-interactions` passes every named 1 MiB edit/scroll threshold
  at top and midpoint using at least 100 verified samples per case. The
  source-first return case has five separately reported first-ever cold
  observations under 500 ms worst-of-five and at least 100 warmed returns
  with p99 and worst both under 50 ms at each position.
- [ ] `make bench-realistic` passes the revised cold limits and scaling checks,
  retaining all previously passing 350 ms cells and the 1 MiB prose contract.
- [ ] Resize, reload, save, global edits, extreme fences and memory are
  measured separately; no accepted >10% regression from the fixed task-006
  baseline is hidden or unexplained.
- [ ] Runner contract tests reject incomplete and misleading samples, and
  commands are discoverable through `make help`; they reject cold/warm
  misclassification and fewer than 100 warmed return samples.

## Validation

- `make test-realistic-performance`
- `make bench-interactions`
- `make bench-realistic`
- `make bench`

## Dependencies

010

## Expected areas of change

`Makefile`, `scripts/interaction_performance.py`,
`scripts/test_interaction_performance.py`,
`scripts/realistic_performance.py`,
`scripts/test_realistic_performance.py`,
`crates/oom-edit/examples/performance_realistic.rs`,
`crates/oom-edit/perf/`, `docs/performance.md`

## Risks / notes

Absolute timing is host-sensitive. Record machine/load metadata and retain
structural tests as algorithmic evidence. A contested run should be repeated
under the same conditions; do not weaken an assertion solely for convenience.
The 1 MiB mixed open-to-first-render <500 ms target remains a separate hard
gate; the approved one-time source-to-rendered allowance does not relax it.
