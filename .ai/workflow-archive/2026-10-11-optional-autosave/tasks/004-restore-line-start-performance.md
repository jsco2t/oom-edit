# Task 004: Restore line-start highlighting performance

Delegation: main-only

## Goal

Preserve correct source ownership for prefix insertions while restoring the established inexpensive line-start character-edit path and unchanged release latency budgets.

## Context

Round 2 rejects the performance regression from task 003. Evidence/round-002-review.md records the baseline and rejected candidate. The original Setext span bug must remain fixed. The vendored Tree-sitter update distinguishes a pure insertion at a node boundary from a content-overlapping replacement; the approved strategy uses an equivalent anchored tree-coordinate edit for the existing narrow block-neutral case.

## Scope

### In scope

Private syntax edit-coordinate handling and lexical proof, deterministic correctness/work regression tests, and validation through existing core/host release benchmarks.

### Out of scope

Autosave redesign, parser/dependency upgrades, public API changes, general bounded-parser expansion, new mutable text ownership, weakened budgets, unrelated spelling changes, commits or archival.

## Implementation requirements

1. Reproduce the saved Setext ownership failure and the rejected line-start work path using retained evidence and current source. Add meaningful deterministic tests before the production repair; tests must show the current blanket veto takes unnecessary document-wide work at byte zero.
2. Implement the plan's equivalent tree-coordinate replacement for admitted ASCII word-only prefix insertions: include a following unchanged real ASCII character in the tree edit so the owning node overlaps it. Apply the original logical TextEdit to text and line indexes. Byte and Point endpoints must describe exactly the same resulting document, including sequential batches. Keep the helper private.
3. Restore the narrow lexical fast path only with the ownership repair. Keep reference/global-semantic exclusions and existing context checks; preserve existing bounded and fence paths, cache limits and unsupported-edit fallbacks. Do not fix output with reconstructed spans, substring matching or post-render column corrections.
4. Extend deterministic differential cases for Setext h1/h2, prose, document byte zero, leading blanks, preceding paragraph/metadata boundaries, multiline titles, inline syntax, Unicode surroundings, reversed deletion and undo/redo. Compare complete StyledLine values after every transition; compare affected spelling exclusion/reference/injection outputs where applicable.
5. Retain the original saved sequence, seed and 150-case property suite. Add sequential-batch boundary coverage and large-document top/middle edit tests. Assert existing parser/reference/injection work counters and cache ownership; safe edits must not trigger whole-document scans. Exercise documents below/at/above the 64 KiB bounded threshold and a 1 MiB fixture with remote viewports.
6. Review surrounding helper comments and update them to explain the actual tree-coordinate operation. Add no dependency, public surface or independent mutation path.
7. Run every validation below and make check. Keep failing release observations. If an unrelated spelling gate miss repeats, stop for the scope decision described in plan.md instead of altering its threshold or changing that subsystem silently.

## Acceptance criteria

- [ ] Original saved/minimized cases and every added transition equal fresh complete styling and relevant derived source data.
- [ ] Equivalent replacement endpoints, sequential coordinate behavior and source ownership are correct; only the intended logical insertion reaches live text.
- [ ] Safe prefix edits preserve the cheap path, caches and work counts at byte zero/top/middle, while unsafe structural cases retain conservative handling.
- [ ] Existing incremental, fence, provenance, autosave and property coverage passes without reducing cases or ignoring tests.
- [ ] Full unchanged make bench and make check pass. No data integrity, crate boundary, public API or dependency change is introduced.

## Validation

- make test-syntax-incremental-regression
- make test-incremental
- make test-first-frame
- make test-pane-autosave
- make bench
- Standard task gate: make check

## Dependencies

003

## Expected areas of change

crates/oom-edit-core/src/syntax/mod.rs; crates/oom-edit-core/src/syntax/incremental_regression.rs; existing related private syntax tests. Makefile only if a distinct new developer validation command is genuinely needed.

## Risks / notes

Do not substitute tree shape equality or one viewport for exact source spans. Anchors must be real unchanged source admitted by the proof. Avoid allocation/scanning proportional to the document merely to choose an anchor. If the specified equivalent edit does not preserve semantics, request a revised design rather than shipping a broader slow fallback. Completed tasks/evidence and rejected raw measurements are immutable.
