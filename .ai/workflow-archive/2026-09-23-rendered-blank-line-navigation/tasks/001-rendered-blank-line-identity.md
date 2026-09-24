# Task 001: Preserve rendered blank-line source identity

Delegation: main-only

## Goal

Make an empty rendered row that represents a physical blank Markdown line own
that line's exact identity, so the gutter and status ruler report the correct
physical line after navigation.

## Context

All inserted separator rows currently inherit `last_content_source` and are
classified synthetic. This correctly protects presentation-only output from
claiming source glyphs, but it also erases the identity of a real blank source
line. The canonical cursor then stays on the preceding block and both the
gutter and status bar are wrong.

## Scope

### In scope

- Test-first reproduction with a paragraph that wraps to at least three rows,
  one physical blank line, and a following block.
- Exact source-gap/physical-line detection between adjacent rendered blocks.
- Empty source-backed blank rows with no display atoms.
- Rendered line-number calculation and canonical cursor mapping for blank rows.
- Absolute and relative TUI gutter output and status-ruler output.
- Entry/remap behavior when the canonical source cursor already occupies the
  blank line.
- Regression coverage for LF, CRLF, Unicode, and adjacent blocks without a
  physical blank separator.

### Out of scope

- Horizontal `h`/Left behavior from the blank row; task 002 owns that change.
- Expanding multiple physical blank lines into additional rendered rows.
- Source Insert-mode behavior or Markdown parser changes.

## Implementation requirements

- Add the failing core and TUI regressions before product changes and confirm
  that they fail for the reported line-number/status mismatch.
- Derive blank ownership from exact adjacent block spans and physical line
  boundaries in the authoritative source text.
- Preserve the current one-row rendered separator policy.
- Keep blank rows atom-free; do not attach source ranges to generated glyphs or
  manufacture an invisible source-backed glyph.
- Keep presentation-only separators synthetic, unnumbered, and associated with
  their existing navigation fallback.
- Commit the blank row through `EditorSession`'s existing rendered cursor path
  so `session.cursor()`, the gutter, and the status bar all observe one
  canonical source position.
- Update comments/public DTO documentation and API guards if their stated
  contracts are affected.

## Acceptance criteria

- [ ] The minimal line-46/47 equivalent has a numbered empty rendered row whose
      line-level range is the physical blank line and whose atom list is empty.
- [ ] Moving down onto the row makes `EditorSession::cursor()` the blank line at
      column zero and remapping from that canonical position returns to it.
- [ ] Absolute and relative gutters render the blank line's correct source
      number/value instead of an empty cell.
- [ ] The full status row reports the blank line at display column one.
- [ ] LF, CRLF, Unicode, and adjacent-no-blank cases preserve exact offsets and
      do not number presentation-only separators.
- [ ] Existing wrapped continuation rows and generated separators remain
      unnumbered and source-atom-free.

## Validation

- `cargo test -p oom-edit-core --offline --locked rendered_blank_line`
- `cargo test -p oom-edit-core --offline --locked rendered_line_number`
- `cargo test -p oom-edit --offline --locked rendered_blank_line`
- `cargo test -p oom-edit --offline --locked rendered_gutter`
- `make check`

## Dependencies

None

## Expected areas of change

- `crates/oom-edit-core/src/rendered/mod.rs`
- `crates/oom-edit-core/src/style.rs`
- `crates/oom-edit-core/src/rendered/tests/blocks.rs`
- `crates/oom-edit-core/src/rendered/nav.rs`
- `crates/oom-edit-core/tests/session_integration.rs`
- `crates/oom-edit/src/screens/rendered.rs`
- `crates/oom-edit/src/app.rs`

## Risks / notes

The line-level range is navigation/selection identity, while atoms are glyph
provenance. A source-backed empty line must not weaken the rule that generated
output has no source-backed atoms.
