# Task 004: Follow the rendered cursor horizontally

Delegation: main-only

## Goal

Expose clipped rendered content by horizontally following cursor movement while keeping the gutter and per-tab viewport state stable.

## Context

After Task 003, tables retain an 80-cell layout when the content surface is narrower than 80. The core already exposes rendered cursor columns in display-cell coordinates, but App only maintains a horizontal offset for unwrapped source Insert mode and the rendered screen always starts at column zero.

## Scope

### In scope

- Separate per-tab rendered horizontal offsets.
- Horizontal scroll-follow based on the active rendered cursor column and the existing scroll margin policy.
- Rendered-line horizontal cropping/scrolling with a fixed gutter.
- Consistent behavior in rendered Normal, Select, and Command surfaces.
- Resize, reflow, tabs, cursor visibility, Unicode, style/decorations, and snapshots/tests.

### Out of scope

- Mouse horizontal scrolling, a scrollbar, or explicit new key bindings.
- Moving viewport ownership into core.
- Changing source Insert wrapping or its existing `left_col` behavior.
- Vertical scroll policy changes except where required to keep combined follow atomic.

## Implementation requirements

- Add a rendered offset distinct from source `left_col`; both must remain independent per tab.
- Compute follow in App from `EditorSession::rendered_cursor()` and the renderer-neutral content width. Keep the cursor inside `HSCROLLOFF` where possible, saturate safely at zero, and reset/clamp the offset when the active row or resized layout no longer needs it.
- Pass the offset explicitly into the rendered screen adapter. Crop in display cells while preserving semantic spans, diagnostics, search matches, selection carriers, the active cursor cell, metadata/code surfaces, and source gutter placement.
- Horizontal movement itself remains core-owned (`h`/`l`, arrows, counts, word/edge motions). App only follows the resulting typed cursor effect.
- Resizing or switching tabs must not reuse another tab's offset or leave the active cursor off-screen.
- Cover wide and combining Unicode at crop boundaries so half of a wide glyph is never presented as the active cursor cell.

## Acceptance criteria

- [ ] Moving right through an over-width rendered table advances the horizontal viewport before the cursor leaves the visible text surface; moving left restores earlier columns.
- [ ] The gutter stays fixed and line numbers do not scroll with table content.
- [ ] Selection, search, diagnostic, semantic, cursor-row, and normal-cursor styles remain attached to the correct visible cells after scrolling.
- [ ] A narrow-to-wide resize clamps the rendered offset back to the valid range and keeps the same canonical source content focused.
- [ ] Multiple tabs retain independent rendered horizontal offsets.
- [ ] Ordinary narrow prose and source Insert-mode horizontal/wrapped scrolling remain unchanged.
- [ ] CJK and combining-character regression tests keep the active rendered cursor fully visible at crop boundaries.

## Validation

- `make test`

## Dependencies

- Task 003

## Expected areas of change

- `crates/oom-edit/src/app.rs`
- `crates/oom-edit/src/screens/rendered.rs`
- `crates/oom-edit/src/event.rs`
- `crates/oom-edit/src/snapshot_tests.rs`
- `crates/oom-edit/tests/snapshots/rendered_table.txt`

## Risks / notes

Ratatui scrolling and the core both use display-cell coordinates, but styled spans contain UTF-8 strings. Verify actual backend cells rather than assuming a scalar-index slice matches a display-cell crop.
