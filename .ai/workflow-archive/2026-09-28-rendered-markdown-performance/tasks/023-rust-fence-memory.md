# Task 023: Bounded Rust-fence working set and leak stability

Delegation: main-only

## Goal

Keep every large Rust-fence peak-RSS row under the narrowly revised baseline-plus-15% ceiling, prove that repeated use does not grow the working set, and preserve responsive edits and faithful highlighting.

## Context

Task 020 attributed about 30.7 MiB at 1 MiB to retained source-injection trees, which are needed to avoid a roughly 708 ms first Rust edit. A narrow line-consumption change put all four Rust RSS rows at about 110.6–110.8% of their fixed pre-incremental baselines in five fresh processes. Revision 7 authorizes a Rust-only +15% cap, conditional on an asserting leak-stability gate. See `evidence/023-rss-decision.md`.

## Scope

### In scope

- Retain the smallest measured line-consumption reduction and remove unsuccessful exploratory complexity.
- Apply the hard-baseline +15% RSS comparison only to Rust-fence cases; preserve every other threshold.
- Add an asserting exact-fixture RSS-stability target covering repeated edit/undo, source↔rendered transitions and reload, with ownership/cache-bound tests.
- Compare first-render, first edit, repeated use and both source/rendered highlighting before and after.

### Out of scope

- Raising/omitting non-Rust RSS or retained-heap limits, simply deferring the parse to the first user edit, dropping Rust/Go fidelity, or adding a dependency without separate plan approval.

## Implementation requirements

- Use task 020's allocation evidence and the five-process `023-rss-candidate.jsonl` record; document why cache eviction trades memory for interaction latency.
- Add a `make bench-rss-stability` target and runner-contract tests wired into `make test`. Sample current `VmRSS` at cycles 100, 200, 300 and 400 after completed frames/restored text, in three fresh processes per Rust/Go edit, mode-transition and reload scenario. Assert no later checkpoint exceeds cycle 100 by 2 MiB. Include core cache entry/byte-bound tests and ownership review; do not call a finite RSS sample proof of universal leak-freedom.
- Ensure the retained parse tree's cache invalidation remains exact across edit, undo, reload and language changes.
- Retain one canonical live text owner and the same core path for standalone/embedded hosts.

## Acceptance criteria

- [ ] `make bench-realistic` passes the Rust-only baseline-plus-15% peak-RSS rows and all other unchanged memory, cold-time, layout-scaling and heap limits in five fresh processes per case. The baseline bytes and fixture identities are unchanged.
- [ ] `make bench-rss-stability` passes all fixed-checkpoint scenarios without post-warmup RSS growth above 2 MiB, with exact document/mode/frame and bounded source-cache ownership.
- [ ] The 100-cycle Rust/Go delete/change gate and prose gate still pass; each first post-open Rust and Go edit is measured below 50 ms key-to-owned-frame and does not inherit hidden deferred parse cost.
- [ ] Source and rendered syntax styles, exact source spans, cursor/selection, undo and reload remain equal to fresh reference builds.
- [ ] Evidence identifies the retained tree and temporary-overlap ownership, with raw five-process RSS and stability samples and an honest limit on what finite leak tests establish.

## Validation

- `make bench-realistic`
- `make bench-rss-stability`
- `make bench-acceptance-1mb-edit-cycles`
- `make bench-acceptance-1mb-prose-cycles`
- `make test-incremental`
- `make check`

## Dependencies

022

## Expected areas of change

`crates/oom-edit-core/src/syntax/mod.rs`, `crates/oom-edit-core/src/rendered/`, `crates/oom-edit-core/src/session.rs`, `crates/oom-edit-core/tests/`, `crates/oom-edit/examples/`, `scripts/realistic_performance.py`, `scripts/`, `Makefile`, `docs/performance.md`

## Risks / notes

Peak RSS is a process high-water mark; it cannot establish leak-freedom. The separate stability probe must use current RSS after quiescent complete frames, and ownership/cache tests must rule out monotonically retained entries. If that stability gate or first-edit requirement fails, diagnose the actual growth/cost rather than relaxing the threshold.
