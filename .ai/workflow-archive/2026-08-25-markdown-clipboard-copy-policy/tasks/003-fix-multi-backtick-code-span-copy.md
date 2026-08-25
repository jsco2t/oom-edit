# Task 003: Fix multi-backtick code-span copy

Delegation: main-only

## Goal

Preserve complete multi-backtick inline-code source during rendered clipboard
copy, including literal backticks inside the payload, without changing editing
or plain-text semantics.

## Context

Round-two acceptance testing reproduced a character-selection failure on the
escaped-backtick span in `examples/kitchen-sink.md`. The parser supplies the
complete code-span token, but the rendered leaf's greedy alignment can map a
visible leading backtick to an opening delimiter byte. The copy projection then
cannot recognize and restore the omitted opening delimiter run.

## Scope

### In scope

- Code-span parser-leaf provenance for delimiter runs and literal backticks.
- Markdown/plain-text clipboard projection regressions for the reported case.
- Complete and partial rendered character selections, block selection where the
  construct participates, and unchanged linewise copying.
- App integration coverage for both configured representations.

### Out of scope

- Configuration schema or defaults.
- Clipboard transport, size limits, or desktop clipboard reads.
- General Markdown parser replacement or unrelated rendering changes.
- Changes to Vim register, put, delete, change, or undo behavior.

## Implementation requirements

- Use the parser-provided code-span token boundary to exclude matched opening
  and closing delimiter runs from visible payload alignment.
- Attach each visible code display group to its exact payload source bytes at
  leaf construction time. Do not use global substring matching, candidate
  searches, or post-hoc clipboard correction.
- Preserve CommonMark code-span normalization already implemented for spaces,
  newlines/CRLF, and table-cell escaped pipes.
- Handle delimiter runs of different lengths and payloads that begin or end with
  literal backticks, contain repeated backticks, Unicode, or repeated text.
- Keep complete-selection Markdown reconstruction exact and partial selection
  bounded to selected content with no unmatched delimiters.
- Keep `RenderedSelection.source_ranges`, `ProjectedSelection`, register shapes,
  destructive operators, puts, and undo unchanged.
- Verify the App continues to send the Markdown field by default and the
  syntax-free rendered payload for `copy_format = "plain-text"`.
- Add no dependency and expose no new public API.

## Acceptance criteria

- [ ] The exact kitchen-sink escaped-backtick fragment has a byte-exact rendered
      character-copy regression: Markdown retains both two-backtick delimiter
      runs and adjacent spaces; plain text contains only the rendered payload.
- [ ] Parser-leaf tests prove source ownership excludes outer delimiters for
      one-, two-, and longer-backtick spans, including payload-leading,
      payload-trailing, and repeated literal backticks.
- [ ] Multiline/CRLF normalization, table escaped pipes, Unicode, wrapping, and
      repeated content retain correct monotonic byte provenance.
- [ ] Partial character selections add no delimiters or unselected content;
      linewise copy remains exact; applicable block-copy behavior is covered.
- [ ] The App's Markdown and plain-text configuration paths emit the correct
      representation for this exact edge case.
- [ ] Existing editing/register/put/undo and clipboard regressions remain green.
- [ ] No dependency or public API change is introduced.

## Validation

- `make test`

## Dependencies

- 001
- 002

## Expected areas of change

- `crates/oom-edit-core/src/rendered/blocks.rs`
- `crates/oom-edit-core/src/rendered/tests/blocks.rs`
- `crates/oom-edit-core/src/clipboard.rs`
- `crates/oom-edit-core/tests/session_integration.rs`
- `crates/oom-edit/src/app.rs`

## Risks / notes

The code-span event range intentionally contains delimiter bytes, while rendered
atoms must own only visible payload bytes. Space trimming and newline
normalization can make rendered groups differ from raw byte groups, so the fix
must retain local, monotonic alignment within the payload window rather than
assuming a one-byte-to-one-glyph mapping.
