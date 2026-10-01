# Task 014: Bounded Select and large-fence feasibility gate

Delegation: main-only

## Goal

Decide whether one retained core projection can meet the user's selection and large-fence edit requirements without stale frames, full-document work on ordinary operations, or fragile parallel state.

## Context

Character Select currently materializes/scans a complete layout. A block over 8 KiB takes the full Markdown-model rebuild path, so either 390 KiB code fence defeats the existing block-level edit strategy. Task 013 supplies the measured costs and baseline.

## Scope

### In scope

- Test-only bounded prototypes for indexed selection and local intra-fence source/model/row updates.
- Differential and same-machine measurements on actual Rust/Go fence function edits and the other user scenarios.
- An explicit go/no-go architecture memo in workflow evidence.

### Out of scope

- Shipping a partial or test-only path as production behavior.
- Adding loading UI, stale editable layout, a second mutable text owner, parser replacement, or a second renderer.

## Implementation requirements

- Prove that a short selection can obtain accurate visible cells/intervals and operation ranges from indexed source/row spans without converting every retained row or scanning all document atoms on each motion.
- Demonstrate a single-undo, register-exact projected deletion through the authoritative Vim buffer without routinely materializing, cloning and replacing the entire 1 MiB text.
- Prove that an interior 10–20-line Rust and Go function edit can update source injections and rendered code rows from changed ranges/line chunks, while preserving exact full-builder output. Test raw strings, multiline comments, Unicode, CRLF, changed line counts, closing/opening fence delimiters, and changes that legitimately propagate far.
- Record bytes parsed, rows rebuilt, full fallbacks, key-to-frame phases, retained heap and RSS; compare to task 013 baseline. Do not infer feasibility only from a synthetic microbenchmark or move cost to the next frame.
- Review state ownership and invalidation complexity against the one-owner core design. If the prototype cannot plausibly meet the round-2 targets with exact fidelity, set `PLAN_CHANGE_REQUIRED`, preserve evidence, and recommend a source-centric viewport alternative or partial rollback for human decision. Do not continue to task 015 on a failed gate.

## Acceptance criteria

- [ ] Differential tests cover cells, styles, UTF-8 byte atoms, cursor/selection, operations and undo against a fresh full build for both large fences and representative non-fence Markdown.
- [ ] The measured short-selection and local-fence work is bounded by the affected region, with no routine full 1 MiB parse/layout and a credible path to the stated p99 targets.
- [ ] A written go/no-go decision names the measured benefits, complexity costs, fallback cases and the recommended production architecture; a failed gate stops the workflow rather than weakening the plan.

## Validation

- `make test-incremental`
- `make bench-acceptance-1mb-record`
- `make check`

## Dependencies

013

## Expected areas of change

`crates/oom-edit-core/src/rendered/`, `crates/oom-edit-core/src/syntax/`, `crates/oom-edit-core/src/session.rs`, `crates/oom-edit-core/tests/`, `.ai/workflow/evidence/`

## Risks / notes

Tree-sitter changed ranges can expand through a raw string or block comment; a correct wide update is allowed but must be explicit. A favorable first edit followed by an expensive deferred frame is not a pass.
