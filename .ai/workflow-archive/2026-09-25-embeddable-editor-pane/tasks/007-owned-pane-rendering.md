# Task 007: Pane-local owned rendering

Delegation: main-only

## Goal

Render the complete App UI into a bounded owned cell grid with pane-local cursor, compact overlays and no visual regression in standalone mode.

## Context

`App::render` and some overlays assume the whole terminal; the embedding boundary cannot expose ratatui. The existing screen bodies already accept a `Rect`.

## Scope

### In scope

FR-005 and FR-010–019: owned frame/cell/style/cursor, focus and size behavior, editor line/hint option, tab rule, empty state, pane-relative overlays, compact reachability and frame performance.

### Out of scope

Host tabs/lifecycle protocols and disk watching.

## Implementation requirements

- Add exact goldens and cell/cursor assertions first at 0×0, 19×4, 20×5, 60×20, 80×24, 100×30, 200×60 and nonzero host origin; include CJK, emoji, combining, clipping, every modifier and default/indexed/RGB colors.
- Keep one-row editor line with mode/file/dirty/spell/ruler/transient/active prompt and cursor. Optionally render existing hints/which-key in-line for standalone; embedded mode exports the same content without a second hint row. Support multi-tab/always tab rules and host-supplied empty lines.
- Position palette, help, confirmation, spell and trouble overlays relative to requested bounds. At 20×5 all actions remain operable via compact/scrollable layouts; below that show a safe too-small notice. No out-of-bounds cursor or half wide-glyph artifacts.
- Produce deterministic frames and visible-state-change signal; no pane terminal writes. Add asserting release benchmark for off-screen conversion overhead ≤1 ms at 200×60, preserving existing `docs/performance.md` limits.

## Acceptance criteria

- [ ] Owned frame contains faithful symbols/continuations/styles and pane-local valid cursor/shape or none when unfocused.
- [ ] All UI and overlay cells remain within requested bounds; every compact confirmation option can be selected at 20×5.
- [ ] Standalone 80×24 and existing snapshot outputs remain byte-identical absent approved changes.
- [ ] Repeated rendering with same state/size/focus/time is deterministic; changed-state signal is correct.
- [ ] Release frame-conversion overhead is ≤1 ms at 200×60, `make bench-check` passes, and the known baseline-only first-frame `make bench` failure is documented for Task 014 without relaxing its limit.

## Validation

`make test`

`make bench-check`

`make bench-pane-frame`

## Dependencies

005

## Expected areas of change

`crates/oom-edit/src/app.rs`, `screens/`, `overlay/`, `widgets/`, a private rendering DTO module re-exported at `lib.rs`, snapshots/perf tests, `Makefile` if benchmarks need a new target.

## Risks / notes

Convert host origin exactly once outside the pane. The owned frame must not expose ratatui/crossterm types and must preserve non-color accessibility signals.

The user approved resolving this host's pre-existing v0.5.0 first-frame absolute benchmark failure in Task 014. Task 014 and the final gate still require the complete unchanged `make bench` target to pass.
