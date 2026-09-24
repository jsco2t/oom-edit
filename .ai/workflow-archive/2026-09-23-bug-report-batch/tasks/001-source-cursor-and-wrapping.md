# Task 001: Repair source cursor projection and configurable wrapping

Delegation: main-only

## Goal

Make source Insert display trailing empty cursor lines correctly and wrap
ordinary prose at a configurable 100-column default without splitting words or
breaking exact source/style/cursor mappings.

## Context

The canonical Vim cursor already advances after Enter, but the source frame
does not materialize a final empty highlighted row and paints the cursor at the
frame default. Source wrapping is currently an exact hard wrapper at terminal
width. The new line limit must affect source and rendered prose while leaving
the actual viewport width available for table scrolling and pointer geometry.

## Scope

### In scope

- Regression tests reproducing the `# foo` plus repeated Enter failure.
- A source-frame row for the canonical trailing empty logical line, with
  correct line number, source offset, screen cursor, and scrolling behavior.
- Unicode/display-width-aware word-boundary wrapping that preserves exact
  source character and style order and hard-splits only indivisible overlong
  tokens.
- Ordinary wrap-delimiter handling that avoids a spurious leading continuation
  pad after repeated spaces without mutating source.
- `[editor] wrap_width`, default 100, positive-value validation/fallback,
  serialization/round-trip tests, startup wiring, and App layout calculations.
- Effective layout width for both source and rendered prose, with actual
  viewport width retained for crop, pointer, horizontal-scroll, and the
  existing 80-column table floor.
- README and changelog coverage for the setting and repaired behavior.

### Out of scope

- Cross-row rendered `h`/Left navigation.
- Markdown list looseness, front-matter insertion, or spell scheduling.
- Any on-disk reflow of source text.

## Implementation requirements

- Add failing tests before changing implementation.
- Keep all wrapping calculations in display columns and preserve UTF-8-safe
  source mappings, tabs, wide scalars, combining suffixes, and span styles.
- Concatenating source-wrap segments must reproduce the original displayed
  source exactly; no Markdown byte may be inserted, removed, or normalized.
- Use 100 as the default configured limit. Invalid zero/out-of-range config
  must not panic or silently create a zero-width editor; use the project's
  existing load-with-defaults warning/fallback posture.
- Do not replace actual viewport width with the configured layout limit. Tables
  must remain at least 80 columns and horizontally scroll on narrower screens.
- Update relevant source-frame, scrolling, resize, pointer, config, README,
  changelog, snapshot, and golden tests in the same task.
- Add no external dependency.

## Acceptance criteria

- [ ] Repeated Enter after typing `# foo` in a document without front matter advances both canonical and painted cursors across terminal empty lines.
- [ ] Source prose wraps at word boundaries at 100 columns by default and at a configured positive override, without changing document bytes.
- [ ] Ordinary repeated-space input at a prose wrap boundary does not leave a spurious leading continuation pad; overlong indivisible tokens remain the documented hard-break exception.
- [ ] Source spans/cursor mappings remain correct for ASCII, tabs, wide Unicode, combining suffixes, and exact-width boundaries.
- [ ] Rendered prose uses the same configured line limit, while a sub-80-column viewport still renders and horizontally scrolls an 80-column-minimum table.
- [ ] Configuration round trips, partial configuration defaults to 100, and README/changelog documentation is current.

## Validation

- `cargo test -p oom-edit-core --offline --locked source_frame_tracks_cursor_on_trailing_empty_lines`
- `cargo test -p oom-edit-core --offline --locked source_wrap_`
- `cargo test -p oom-edit --offline --locked wrap_width`
- `cargo test -p oom-edit --offline --locked rendered_table`
- `make check`

## Dependencies

None

## Expected areas of change

- `crates/oom-edit-core/src/rendered/wrap.rs`
- `crates/oom-edit-core/src/session.rs`
- `crates/oom-edit/src/config.rs`
- `crates/oom-edit/src/app.rs`
- `crates/oom-edit/src/screens/editor.rs`
- `crates/oom-edit/src/screens/rendered.rs`
- related core/TUI tests, snapshots, and goldens
- `README.md`
- `CHANGELOG.md`

## Risks / notes

The configured layout width and physical terminal viewport are deliberately
different inputs. Collapsing them into one field would regress table scrolling
or pointer mapping. Preserve provenance at existing parser/highlighter leaves;
do not reconstruct it after wrapping.
