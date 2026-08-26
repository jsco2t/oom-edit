# Task 007: Themed Gutter Rendering and Trouble Markers

Delegation: main-only

## Goal

Render the independent full-rectangle gutter theme and immutable Trouble markers correctly in both source Insert and rendered modes.

## Context

The theme catalog now supplies body/gutter roles and App owns completed marker snapshots. The shared renderer must compose backgrounds, numbers, current-line state, and fixed severity glyphs without diagnostic work or source-provenance errors.

## Scope

### In scope

- Pass theme/tier and immutable snapshot into the shared gutter renderer from both screen adapters.
- Paint document body surfaces where configured and fill the entire gutter rectangle with its background.
- Compose ordinary/current line-number foreground/modifiers and `E`/`W`/`I`/`H` marker foreground/modifier while preserving background.
- Reserve one existing separator cell for markers without changing digit/sign geometry.
- Source/rendered wrapping, synthetic rows, relative/multi-digit line numbers, minimum widths, horizontal scroll, below-document rows, and Monochrome behavior.
- Exact cell and integration tests, including real async publication then edit clearing.

### Out of scope

- Diagnostic scanning/building in render, changes to rendered inline decoration projection, or new gutter geometry.
- User-controlled glyphs/modifiers.

## Implementation requirements

- `render_gutter` stays the only painter for both modes and is pure over mode/cursor/visible line numbers/theme/tier/immutable snapshot/area.
- Fill every gutter cell first, including gap, continuation, synthetic, clipped, and padding rows.
- Marker lookup occurs only for `Some(source_line)` rows and only the first numbered row owns a wrapped line marker; `None` rows remain blank markers.
- Current/ordinary number styles and marker styles must not erase gutter background. The active-line non-color carrier remains distinct.
- The new document-body style must not change legacy default terminal background but must reach new bundled/custom theme cells.
- Render tests assert symbols, foregrounds, backgrounds, and modifiers directly; do not rely on symbol-only goldens for color behavior.
- No renderer receives `EditorSession::diagnostics`, provider, mutable marker state, or source text solely for markers.

## Acceptance criteria

- [ ] Every gutter cell in both modes carries the selected gutter background for color themes, including blank/wrapped/synthetic/below-document cells and the separator gap.
- [ ] Ordinary/current line numbers and all four severity glyphs compose exact foreground/modifier/background styles at TrueColor, ANSI-16, and Monochrome.
- [ ] Markers appear once on the numbered physical source row, never on continuations/synthetic rows, and remain fixed across rendered horizontal scroll and viewport changes.
- [ ] Relative numbers, signs/digits at 9/10/999/1000 boundaries, narrow/clipped areas, and active-line behavior remain geometrically correct.
- [ ] A real published diagnostic path renders markers in source and rendered modes and an edit hides them before any rebuild.
- [ ] Render work/counters remain read-only and viewport-bounded; existing snapshots/tests remain correct except intentional focused marker layout updates.

## Validation

- `make test`
- `make bench-check`

## Dependencies

- Task 006

## Expected areas of change

- `crates/oom-edit/src/screens/editor.rs`
- `crates/oom-edit/src/screens/rendered.rs`
- `crates/oom-edit/src/app.rs`
- `crates/oom-edit/src/theme.rs`
- `crates/oom-edit/src/gutter.rs`
- Focused screen/App/snapshot tests

## Risks / notes

Ratatui style patch order can silently clear backgrounds or semantic foregrounds. Inspect exact cells around the gutter/body boundary and marker gap. Synthetic output must never acquire source ownership.
