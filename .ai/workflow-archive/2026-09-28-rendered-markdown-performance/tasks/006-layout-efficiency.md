# Task 006: Instrument interaction work and exact reference

Delegation: main-only

## Goal

Establish a repeatable interaction-first baseline and a full-build correctness
oracle before changing the document-to-view algorithm.

## Context

The previous task-006 pure-layout scope stopped at an approved-plan boundary.
Its profiling evidence remains in `evidence/006-profile.md`; task 006 is
replanned with the user's explicit authorization. At 1 MiB, rendered line
delete is about 644 ms median and source structural edits are 80/216 ms
median/p99. Existing probes distinguish input and frame time but not retained
work or exact incremental-vs-full output.

## Scope

### In scope

- Extend deterministic 144 KiB and 1 MiB fixtures and public-pane interaction
  vectors to cover local source line insert/delete, rendered line delete/undo,
  mode return, source/rendered scroll, save, resize, reload, known/unknown fence
  label changes, front-matter and global-definition edits.
- Add test-only or opt-in core counters/timing for source parse and changed
  ranges, injection/definition work, block-model work, rendered block/row
  reuse, cursor mapping, frame production and retained memory as applicable.
- Build a full-rebuild reference comparison harness for cells, semantic styles,
  source atoms, navigation, cursor/selection and operation results. Keep the
  reference outside production hot paths.
- Record five fresh processes × 20 cycles at top and midpoint on this machine;
  store fixture identity, median/p95/p99/worst, input/frame phases, peak RSS,
  retained layout heap and work counts in evidence.

### Out of scope

- Optimizing parsing/layout, changing public modes or introducing background
  work, placeholder frames or a second text owner.

## Implementation requirements

- Instrumentation must not change editor behavior or pollute release hot paths
  unless explicitly enabled for a measurement run. Do not expose parser or
  renderer third-party types through a public API.
- Every recorded operation must assert that its intended text, mode and
  viewport change occurred; the recorder rejects missing/invalid samples.
- The full-build oracle uses the same authoritative current text but does not
  share incremental derived caches. It compares exact output, not line counts.
- Add any new developer/CI command as a discoverable top-level Make target
  with harness tests.

## Acceptance criteria

- [ ] A fixed same-machine baseline records all named interactions and phase
  distributions at top and midpoint, with 1 MiB rendered delete/undo and
  structural source edit explicitly represented.
- [ ] Tests reject missing cases, stale fixture identity, wrong sample counts,
  malformed timings and unchanged viewport/text when a change is expected.
- [ ] The full-build oracle compares exact cells, styles, source bytes,
  navigation and operation state on representative Unicode/CRLF/fence/table/
  front-matter/link/footnote documents.
- [ ] Opt-in/test counters can identify parse propagation and reused/rebuilt
  blocks/rows without changing the public editor contract.

## Validation

- `make test-realistic-performance`
- `make test-pane-public`
- `make bench-interactions-record TRIALS=5 ITERATIONS=20 OUTPUT=.ai/workflow/evidence/incremental-baseline.jsonl`
- `make bench-realistic-record TRIALS=5 OUTPUT=.ai/workflow/evidence/incremental-cold-baseline.jsonl`

## Dependencies

005

## Expected areas of change

`crates/oom-edit-core/src/syntax/`, `crates/oom-edit-core/src/rendered/`,
`crates/oom-edit-core/src/session.rs`, `crates/oom-edit/examples/performance_realistic.rs`,
`crates/oom-edit/tests/`, `scripts/interaction_performance.py`,
`scripts/test_interaction_performance.py`, `Makefile`

## Risks / notes

Timing counters may perturb the measured path. Keep structural work counters
separate from wall-clock probes and record the instrumentation configuration.
The baseline is evidence, not a passing 50 ms performance result.
