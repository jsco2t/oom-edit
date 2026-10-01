# Task 018: Incremental large-fence code edits

Delegation: main-only

## Goal

Make ordinary local edits and 10–20-line function deletion/change inside the 390 KiB Rust and Go fences responsive while preserving immediately faithful source and rendered output.

## Context

The current retained model's 8 KiB block cap and newline-change rules force a full-model rebuild for this common acceptance fixture. A cached source-injection parse is discarded when its fence is edited. Task 014 must first demonstrate a bounded design.

## Scope

### In scope

- Integrate the proven line/sub-block update and parse-invalidation strategy into the one `LiveDocument` transaction and retained row projection.
- Preserve full-layout public API and complete-builder oracle.
- Measure both Rust and Go fence edits, delete/change/undo and propagation cases.

### Out of scope

- Pretending a changed fence delimiter or unclosed multiline syntax always has a local effect.
- A second mutable document, parser replacement without separate approval, or incomplete temporary styling.

## Implementation requirements

- Represent unchanged interior code lines/rows as retained source-mapped material rather than reparsing and reallocating the whole fence after every local code edit. Update source injection trees/captures using exact sequential `TextEdit` coordinates and propagate through the true changed range; rebuild wider when necessary, with an explicit counter/reason.
- Keep language query reuse, front matter, fences, Markdown reference/link/footer numbering, navigation targets, source-byte atoms, physical line counts and width-dependent rows exact. A changed line count rebases subsequent offsets and row indices before control returns.
- Avoid routine whole-buffer materialize/clone/replace for a local projected edit, and do not create a second mutable text owner. Change `vim.rs` only through its private wrapper and preserve undo/register semantics. Keep all core output terminal-independent and shared by standalone/embedded hosts.
- Differential/property tests cover local edits near fence beginning/middle/end, full-function `d`/`c`, undo/redo, Rust raw strings, Go block comments, multibyte text, CRLF, delimiter edits and repeated edits; verify complete frame and source highlighting after every operation.

## Acceptance criteria

- [ ] Routine local edits within either large fence do not take the full 1 MiB model/layout or full-fence syntax parse path; counters demonstrate bounded changed work.
- [ ] At least 100 verified 10–20-line Rust and Go function `d`/`c` cycles each meet p99 <50 ms through `EditorPane`, including the complete frame in the correct resulting mode (rendered Normal for `d`, source Insert for `c`), with exact text, rows, provenance and one-step undo.
- [ ] Legitimately propagating or delimiter edits use a measured correct wider path and never publish stale cells, styles or source ranges.
- [ ] Existing realistic cold, ordinary edit, memory, direct-core, standalone and embedded tests remain green.

## Validation

- `make test-incremental`
- `make bench-acceptance-1mb-record`
- `make bench-interactions`
- `make check`

## Dependencies

017

## Expected areas of change

`crates/oom-edit-core/src/session/live_document.rs`, `crates/oom-edit-core/src/syntax/mod.rs`, `crates/oom-edit-core/src/rendered/retained.rs`, `crates/oom-edit-core/src/rendered/rows.rs`, `crates/oom-edit-core/src/session.rs`, `crates/oom-edit-core/tests/`

## Risks / notes

This is the highest-risk task. If a correct implementation cannot meet the target without a materially different architecture, stop at `PLAN_CHANGE_REQUIRED` with the evidence; do not raise the 8 KiB cap or weaken the interaction gate to claim success.
