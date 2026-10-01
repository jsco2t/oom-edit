# Task 010: Indexed retained layout and viewport

Delegation: main-only

## Goal

Avoid rebuilding and scanning all rendered rows after ordinary local edits,
while keeping a fully current, exact rendered view and navigation.

## Context

`RenderedState` currently drops its single complete layout on edits and
`EditorSession::render_layout` rebuilds every mapped row. The current
`RenderedLayout` includes source atoms, line numbers, fence regions, jump
targets and selection/cursor mapping. Task 009 provides a retained semantic
block model for a width-aware block/row cache.

## Scope

### In scope

- Retain unchanged mapped rows by block and width; rebuild only dirty blocks
  and their dependents, preserving semantic style and source provenance.
- Maintain indexed row counts/source ranges and exact source↔rendered cursor,
  Select endpoint, scroll and jump mapping without an all-row pass for local
  edits or viewport draws.
- Handle width changes by reusing syntax/model data and computing exact
  width-dependent geometry/visible rows without stale editable layout.
- Update `EditorSession`/`RenderedState` through the one mutation transaction;
  preserve the existing curated core and pane APIs where possible.
- Compare every rendered result against the complete reference builder.

### Out of scope

- Loading placeholders, background jobs, changed modes or a separate host
  renderer.

## Implementation requirements

- Normal/Select/Command operations must never use a previous-generation row
  or source mapping. A completed input handler and frame represent current
  authoritative text.
- Synthetic glyphs remain source-less; mapped atoms are attached at parser
  leaves. Preserve exact navigation and selection semantics across row-height
  shifts before the cursor.
- Keep document-wide footnotes, links, jump targets and fence-region metadata
  exact. An edit that affects their global ordering must invalidate accordingly.
- Avoid storing duplicate complete 1 MiB layouts; measure heap/RSS.

## Acceptance criteria

- [ ] Differential tests compare every cell, modifier/style, raw source atom,
  line number, jump target, fence region, cursor and selection endpoint after
  local/global edits, undo, mode transitions and two widths.
- [ ] Counters prove an ordinary 1 MiB line deletion does not rebuild or scan
  all rendered rows; source↔rendered mapping remains exact on generated rows,
  wrapped rows and Unicode/CRLF.
- [ ] Rendered and source wheel scrolling remain correct and bounded on the
  warmed 1 MiB fixture; resize never exposes stale editable geometry.
- [ ] Public `EditorSession::render_layout` and `EditorPane::render` behavior
  remains compatible; any facade change has exact compile-time guard updates.

## Validation

- `make test-incremental`
- `make test-pane-public`
- `make bench-interactions-record TRIALS=5 ITERATIONS=20 OUTPUT=.ai/workflow/evidence/incremental-layout.jsonl`

## Dependencies

009

## Expected areas of change

`crates/oom-edit-core/src/rendered/`,
`crates/oom-edit-core/src/session.rs`,
`crates/oom-edit/src/app.rs`, `crates/oom-edit/tests/pane_public.rs`,
`crates/oom-edit-core/tests/`

## Risks / notes

The public layout API currently returns a reference to a complete layout.
Retained indexing and viewport work must preserve that contract or make a
deliberate, guarded public API change; do not hide an eager materialization
inside `render_layout` that restores the original pause.
