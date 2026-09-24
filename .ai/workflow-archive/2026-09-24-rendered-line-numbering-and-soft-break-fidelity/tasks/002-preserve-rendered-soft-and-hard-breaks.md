# Task 002: Preserve rendered soft and hard breaks

Delegation: main-only

## Goal

Keep author-chosen prose line endings as rendered physical-line boundaries with
exact source identity, then apply width wrapping independently within each
physical line.

## Context

CommonMark/GFM parse consecutive prose lines as one paragraph with inline
`SoftBreak` events and permit renderers to represent a soft break as either a
space or a line ending. Oom-edit currently flattens soft and hard breaks into
spaces before wrapping, erasing source lines and their gutter/cursor identity.

## Scope

### In scope

- Test-first soft-break reproduction matching the supplied lines 3–6 shape.
- Hard-break row boundaries for two-space and backslash forms.
- Private mapped-inline composition that retains break boundaries.
- Per-physical-row source ranges and independent width wrapping.
- Nested emphasis/strong/link content, lists, block quotes, LF/CRLF, Unicode.
- Cursor/status, resize/remap, navigation, search, Select, and operator ranges.
- Correct renderer/model comments and local Markdown policy documentation.

### Out of scope

- Code-span newline normalization, which must remain space-based.
- Plain-text clipboard conversion.
- Table-cell multiline extensions or changes to parser options.
- Gutter continuation glyph styling, owned by task 003.

## Implementation requirements

- Add failing layout/session regressions before changing product code and
  confirm later physical lines are flattened and unnumbered on the baseline.
- Keep `SoftBreak` and `HardBreak` distinguishable through private inline
  composition until rendered row boundaries are created.
- Split line-level source identity only at parser break spans. Do not recover
  boundaries with global substring searches or reconstructed columns.
- Apply `wrap_mapped_line` independently to each physical segment. All width
  continuations for a segment must repeat that segment's exact source range;
  the next physical segment must use a distinct range and receive a number.
- Preserve inline semantic styles, link/image marker placement, generated
  list/quote prefixes, and exact atom provenance across nested breaks.
- Break bytes may belong to the line-level range but must not become visible
  source-backed or generated atoms.
- Preserve code-span line-ending normalization and add an explicit regression.
- Update public contract documentation/API guards only if a public statement
  changes; do not add a second layout DTO without evidence it is necessary.

## Acceptance criteria

- [ ] Each consecutive prose source line begins a distinct rendered row and
      receives its correct source number instead of being concatenated.
- [ ] Soft breaks and both hard-break syntaxes preserve visual row boundaries
      without changing source bytes.
- [ ] Long physical lines still wrap; their continuation rows share the exact
      physical-line range and remain unnumbered.
- [ ] Canonical cursor/status positions, resize remapping, vertical/horizontal
      navigation, search, and Select endpoints track the correct physical line.
- [ ] Character/line/block selections and operators retain exact ordered source
      ranges across breaks without selecting generated prefixes.
- [ ] Nested emphasis, links, lists, block quotes, Unicode, LF, and CRLF retain
      styles and byte-exact provenance.
- [ ] Line endings inside code spans remain normalized spaces and do not create
      physical rendered rows.

## Validation

- `cargo test -p oom-edit-core --offline --locked soft_break`
- `cargo test -p oom-edit-core --offline --locked hard_break`
- `cargo test -p oom-edit-core --offline --locked rendered_line_number`
- `cargo test -p oom-edit-core --offline --locked rendered_navigation`
- `cargo test -p oom-edit-core --offline --locked rendered_select`
- `cargo test -p oom-edit-core --offline --locked random_mode_width_and_motion_mapping_stays_in_bounds`
- `make check`

## Dependencies

- 001

## Expected areas of change

- `docs/markdown-spec.md`
- `crates/oom-edit-core/src/rendered/blocks.rs`
- `crates/oom-edit-core/src/rendered/mod.rs`
- `crates/oom-edit-core/src/rendered/wrap.rs`
- `crates/oom-edit-core/src/rendered/nav.rs`
- `crates/oom-edit-core/src/style.rs`
- `crates/oom-edit-core/src/rendered/tests/blocks.rs`
- `crates/oom-edit-core/tests/session_integration.rs`
- `crates/oom-edit-core/tests/conformance/mod.rs`
- affected rendered goldens

## Risks / notes

Breaks can be nested below inline style/link nodes, so a flat top-level split is
insufficient. The implementation must preserve style scope and append a link's
generated destination marker after its final physical segment.
