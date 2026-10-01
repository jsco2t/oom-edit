# Task 017: Indexed responsive rendered Select projection

Delegation: main-only

## Goal

Make short character/line/block Select motions and visible highlighting responsive in 1 MiB documents without converting or scanning the entire rendered layout on every key.

## Context

The current character path materializes a complete layout when entering Select and rescans all atoms for ranges and all lines for visible selection every motion/frame. The problem is observable on both `main` and this candidate.

## Scope

### In scope

- Integrate the task-014 indexed selection design in `oom-edit-core`.
- Keep complete public selection queries exact while allowing viewport consumers to request only visible projection.
- Test all three Select shapes, motions, operations and host parity.

### Out of scope

- A second Select dispatcher or renderer-specific selection model in `oom-edit`.
- Stale/incomplete selection styling during a key sequence.

## Implementation requirements

- Keep anchor/active canonical source positions and one operation projection in core. Use indexed row/source spans to compute ranges for the selected region and visible row intervals; do not perform an all-layout scan for a short 10–20-line selection merely because the document is 1 MiB.
- Retain exact source-backed atoms, synthetic/source-less glyph behavior, wrap/table/list/prose/fence semantics and the unchanged yank/copy contract. If public API additions are necessary, export owned DTOs deliberately and update compile-time API/privacy guards.
- Include performance scaling tests at multiple document sizes and repeated motion/undo sequences in a large-fence and mixed-Markdown region. Verify every rendered frame against the full-layout reference.

## Acceptance criteria

- [ ] Short `v`, `V` and Ctrl-V motion produces exact current selection cells/source spans and never requires routine whole-layout materialization.
- [ ] At least 100 verified per-language short character-Select motions meet p99 <50 ms key-to-owned-frame on the 1 MiB example.
- [ ] Yank/copy, `d`, `c`, undo, cursor mapping and standalone/embedded parity remain exact; selected-region work counters exclude all-document scans for short selections.

## Validation

- `make test-incremental`
- `make test-pane-public`
- `make bench-acceptance-1mb-record`
- `make check`

## Dependencies

015

## Expected areas of change

`crates/oom-edit-core/src/rendered/nav.rs`, `crates/oom-edit-core/src/rendered/rows.rs`, `crates/oom-edit-core/src/session.rs`, `crates/oom-edit-core/src/style.rs`, `crates/oom-edit/src/screens/rendered.rs`, `crates/oom-edit-core/tests/public_api.rs`

## Risks / notes

A caller explicitly asking for a complete million-byte selection may still incur work proportional to that selection. The requirement is to avoid work proportional to the entire document for a short selection and on each viewport frame.
