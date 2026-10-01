# Task 007: Bounded incremental feasibility

Delegation: main-only

## Goal

Prove, with a bounded test-only vertical slice, that exact current-text
source/rendered output after ordinary local edits can avoid whole-document
reprocessing and plausibly meet the 50 ms p99 public-pane goal.

## Context

Tree-sitter already receives an edited old tree but took 176.5 ms in a 1 MiB
structural-edit phase sample. The rendered builder independently reconstructs
the entire pulldown block model and mapped layout. A cache alone is not a
demonstrated fix. This task is an explicit feasibility gate before production
architecture changes.

## Scope

### In scope

- Measure Tree-sitter changed byte/line ranges and pulldown/model boundaries
  for top/midpoint line delete/undo, local structural source edits, fence
  language changes and globally disruptive delimiter/definition edits.
- Prototype bounded source analysis and retained block/row rebuilding using
  one authoritative text snapshot and no extra mutable text owner. Prove the
  boundary-expansion rule with the full-build oracle rather than assuming local
  parsing is safe in nested lists, fences or reference-link contexts.
- Compare exact cells, styles, atoms, cursor/selection, jump targets and
  operation results with fresh full builds after every prototype edit.
- Measure at least 100 release samples per ordinary 1 MiB candidate operation
  and report parse/model/layout/total phases, propagation radius and memory.
- Record a go/no-go finding in task evidence before production integration.

### Out of scope

- Shipping a partial fast path that silently falls back for ordinary edits,
  replacing the public editor path with a prototype, loading UI, async
  highlighting or adding a parser dependency.

## Implementation requirements

- The prototype lives in private core/test or performance infrastructure and
  is not a new public facade. It must be removed or integrated by the later
  production tasks; no stub or second supported renderer remains at final
  acceptance.
- Introduce the `make test-incremental` target for the new differential and
  property suite, and include it in `make help` and the repository's normal
  test path so later tasks cannot silently lose coverage.
- Test top/midpoint positions at 144 KiB and 1 MiB, two widths, Unicode and
  CRLF. Distinguish ordinary local edits from intentionally global edits.
- If the prototype cannot match the oracle or demonstrate a credible path to
  under-50-ms p99 key-to-frame on named ordinary edits, set
  `PLAN_CHANGE_REQUIRED` with concrete evidence and stop before task 008.

## Acceptance criteria

- [ ] The prototype's ordinary local edit path reprocesses only a measured,
  bounded region and matches the full-build oracle on all specified outputs.
- [ ] The evidence includes changed-range distributions, block/row reuse,
  parse/model/layout phase timings, p99 candidate totals and peak memory for
  the fixed fixtures; the projected public-pane total can plausibly fit under
  50 ms, or the workflow stops with a failed feasibility report.
- [ ] Global delimiter/definition cases expand invalidation as necessary and
  remain exact; they are not incorrectly classified as local.
- [ ] No public API or production behavior changes as a shortcut for passing
  the feasibility gate.

## Validation

- `make test-incremental`
- `make test-realistic-performance`
- `make bench-interactions-record TRIALS=5 ITERATIONS=20 OUTPUT=.ai/workflow/evidence/incremental-feasibility.jsonl`

## Dependencies

006

## Expected areas of change

`crates/oom-edit-core/src/syntax/`, `crates/oom-edit-core/src/rendered/`,
`crates/oom-edit-core/src/session.rs`, `crates/oom-edit-core/tests/`,
`crates/oom-edit/examples/performance_realistic.rs`, `Makefile`

## Risks / notes

CommonMark constructs can make a local byte edit globally significant.
Tree-sitter changed ranges and pulldown block boundaries are evidence, not a
complete semantic invalidation proof. Reject an unsafe local-only shortcut.
