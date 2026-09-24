# Task 001: Complete physical blank-line identity

Delegation: main-only

## Goal

Render a real blank source line between a parser container block and the next
block as a numbered, atom-free content row even when the preceding block span
already includes that blank line.

## Context

The current separator lookup starts at `last_content_source.end`. A list block
or item may include the following blank separator in its parser span, leaving
an empty search gap before the next heading. The renderer then falls back to an
unnumbered synthetic separator, reproducing the missing line 43.

## Scope

### In scope

- Failing core/session/TUI reproduction of list item, physical blank, heading.
- Physical-line lookup immediately before the next block boundary.
- Exact LF/CRLF and Unicode byte ranges.
- Absolute/relative gutters, canonical cursor remapping, and status ruler.
- Adjacent blocks without a blank and multiple-blank one-row normalization.

### Out of scope

- Soft/hard break paragraph segmentation, owned by task 002.
- Continuation gutter glyphs, owned by task 003.
- Displaying every one of multiple consecutive blank lines.

## Implementation requirements

- Add failing regressions before product changes and confirm the list/heading
  separator is synthetic or unnumbered on the baseline.
- Derive the displayed separator from exact physical line boundaries before
  `next_source.start`; do not depend solely on the previous parser span's end.
- Emit a physical blank as `LineKind::Content` with its complete line-ending
  range, empty text, and no atoms.
- Keep truly presentation-only separators synthetic, unnumbered, and atom-free.
- Preserve the existing single displayed separator policy and paragraph-motion
  behavior.
- Commit canonical cursor/status identity through `EditorSession`; do not add a
  TUI-side source-position workaround.

## Acceptance criteria

- [ ] A list item on source line 42, blank line 43, and heading line 44 renders
      line 43 as an empty numbered content row.
- [ ] Moving or remapping onto that row yields canonical line 43, column zero
      and a displayed status position of `43:1`.
- [ ] The row owns the exact LF/CRLF blank-line range and has no display atoms.
- [ ] Absolute and relative gutters show the correct value for the row.
- [ ] Adjacent blocks without a physical blank retain an unnumbered synthetic
      separator; multiple blanks still normalize to one displayed row.
- [ ] Existing top-level, loose-list, paragraph-boundary, and synthetic-row
      behavior remains covered and passing.

## Validation

- `cargo test -p oom-edit-core --offline --locked rendered_blank_line`
- `cargo test -p oom-edit-core --offline --locked loose_list`
- `cargo test -p oom-edit-core --offline --locked rendered_line_number`
- `cargo test -p oom-edit --offline --locked rendered_blank_line`
- `cargo test -p oom-edit --offline --locked rendered_gutter`
- `make check`

## Dependencies

None

## Expected areas of change

- `crates/oom-edit-core/src/rendered/mod.rs`
- `crates/oom-edit-core/src/rendered/tests/blocks.rs`
- `crates/oom-edit-core/tests/session_integration.rs`
- `crates/oom-edit/src/screens/rendered.rs`
- affected rendered snapshots/goldens

## Risks / notes

Parser spans may start after container markers or end after line endings. The
lookup must use UTF-8-safe physical boundaries and must not reinterpret a
nonblank preceding line as a blank separator.
