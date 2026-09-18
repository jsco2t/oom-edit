# Task 008: Source Insert code tabs

Delegation: main-only

## Goal

Show literal source tabs at four-column tab stops in Insert mode without changing document bytes, while keeping the source viewport's cursor, wrapping, scrolling, styles, and hit mapping consistent.

## Context

Task 002 fixed rendered Markdown code blocks. Insert mode uses `EditorSession::render_source_with_atoms` and still passes raw tabs through geometry that assigns them zero display width. The kitchen-sink Go fence therefore loses indentation after switching modes.

## Scope

### In scope

- Source-view display projection and geometry in `oom-edit-core`.
- Source wrap, nowrap, cursor, scroll-follow, decorations, and viewport cell-to-source mapping around tabs.
- Headless core and TUI regression tests, including the Go fixture.

### Out of scope

- Changing stored source text, save behavior, Markdown parsing, or the completed rendered-code path.
- Configurable tab width or code formatting.

## Implementation requirements

- Expand source-view tabs to the next four-column stop from the start of each logical line before display-width, wrap, and clipping decisions. Keep canonical text and highlighter input unchanged.
- Map every cell of an expanded tab to that tab's exact source byte; preserve byte ranges for later characters and keep clipping indicators source-less.
- Use consistent tab-aware geometry in `render_source`, `visual_row_info`, and `source_offset_at_viewport_cell`; preserve the source-character contract of `Viewport.left_col` and the existing behavior for lines without tabs.
- Make App source scroll-follow use core-owned display geometry so the cursor remains visible after tabs. Update curated public API guards if a new facade method is required.
- Preserve highlighting, search, and diagnostic decorations through expansion. Keep the TUI presentation thin and verify actual drawn cells and cursor placement.
- Add focused tests for the actual kitchen-sink Go lines, generic mixed and consecutive tabs, wide Unicode, wrap and nowrap at narrow widths, clipping, cursor and cell mapping, and unchanged document bytes.

## Acceptance criteria

- [ ] Insert mode displays one- and two-tab Go indentation at four-column stops in the source frame and TUI cells; rendered Normal remains correct.
- [ ] Generic source tabs expand correctly after mixed spaces and wide characters without changing canonical or saved bytes.
- [ ] Wrapped rows, unwrapped horizontal windows, cursor and scroll-follow, and click/hit offsets agree on the same tab-expanded cell positions.
- [ ] Highlight spans and source decorations remain aligned; generated clipping cells claim no source byte.
- [ ] Existing no-tab source viewport behavior and earlier work-package tests remain green.

## Validation

- `make test`

## Dependencies

- 002

## Expected areas of change

- `crates/oom-edit-core/src/session.rs`
- `crates/oom-edit-core/src/rendered/wrap.rs` or a private core source-view geometry module
- `crates/oom-edit-core/src/style.rs`
- `crates/oom-edit-core/tests/source_viewport.rs`
- `crates/oom-edit-core/tests/public_api.rs` if the facade grows
- `crates/oom-edit/src/app.rs`
- `crates/oom-edit/src/screens/editor.rs` tests

## Risks / notes

One tab byte owns multiple display cells. A display-only expansion must keep those cells tied to the original byte through wrapping, clipping, pointer lookup, and decoration projection; a TUI-only replacement would leave the core's viewport geometry incorrect.
