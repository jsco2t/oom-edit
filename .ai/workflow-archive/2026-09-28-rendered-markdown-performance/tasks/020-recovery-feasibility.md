# Task 020: Recovery instrumentation and feasibility

Delegation: main-only

## Goal

Prove a bounded path for ordinary mixed-prose multi-range Select edits and attribute the Rust-fence peak-RSS increase before changing production architecture.

## Context

The exact 1 MiB line-600 Character Select delete reproduced at 683.5 ms in the handler plus 348.6 ms for its rendered frame. All large Rust-fence memory rows failed the unchanged final gate. Cache ownership is a hypothesis, not yet a measured allocation cause.

## Scope

### In scope

- Add an asserting, discoverable `make bench-acceptance-1mb-prose-cycles` target and a non-asserting record variant for exact public-pane `d`/`c`/undo cycles at line 600 and a second mixed-Markdown location.
- Add runner contract tests to `make test`; require at least 100 verified cycles per operation, correct text/mode/frame, complete samples and strict p99/worst limits.
- Attribute Vim mutation, source parse, Markdown model, selection projection, row publication and frame costs with test-only instrumentation; measure cold and first-edit memory with controlled tree-cache and allocation-lifetime probes.
- Build a bounded test-only feasibility slice for sequential `TextEdit` batching and multi-block row publication, comparing to a full-build oracle.

### Out of scope

- Shipping a partial production optimization or changing memory/latency ceilings.
- A second mutable text owner or stale frame.

## Implementation requirements

- Establish the red current-candidate baseline from a fresh optimized build; retain raw records and fixture/binary identity.
- Keep the public pane and full-layout oracle on the same source text after every mutation and undo.
- Document whether the source parse tree is the incremental edit accelerator, its incremental memory cost, and the cost of not retaining it.
- If a one-transaction multi-range update or memory repair lacks a credible route to the package criteria, stop at `PLAN_CHANGE_REQUIRED` with evidence before tasks 021–024.

## Acceptance criteria

- [ ] The new exact-fixture runner fails on the existing prose latency, and its contract tests reject missing, misclassified, malformed or unverified samples and boundary-equal timings.
- [ ] Timing evidence separates the three repeated derived-cache refreshes, full row rebuild, Vim mutation, and selection projection, with a proposed bounded final-text transaction.
- [ ] Controlled memory evidence identifies or rules out retained injection-tree ownership as the large-fence RSS cause and measures its first-edit benefit.
- [ ] Feasibility comparisons cover exact cells/styles/atoms, line removal, cursor, undo and parser-propagation cases; a written go/no-go decision supports the next task.

## Validation

- `make bench-acceptance-1mb-prose-cycles-record`
- `make test-incremental`
- `make test`
- `make check`

## Dependencies

019

## Expected areas of change

`Makefile`, `scripts/acceptance_1mb_prose_cycles.py`, `scripts/test_acceptance_1mb_prose_cycles.py`, `crates/oom-edit/examples/performance_acceptance_1mb.rs`, `crates/oom-edit-core/src/session/live_document.rs`, `crates/oom-edit-core/src/rendered/`, `crates/oom-edit-core/src/syntax/mod.rs`, `.ai/workflow/evidence/`

## Risks / notes

Test-only ablation must not leak into production behavior or make the gate pass by skipping work. Peak RSS and retained live heap are different measures; report both where feasible.
