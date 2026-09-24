# Task 004: Expand kitchen-sink regression scenarios

Delegation: main-only

## Goal

Make `examples/kitchen-sink.md` a clear manual corpus for the recently repaired
rendering and editing behaviors, correct misleading Markdown examples found by
the audit, add a uniform expected-rendering callout to every top-level section,
and protect the important scenarios with an automated contract.

## Context

The September 23–24 fixes cover source wrapping/cursor projection, rendered
horizontal navigation, loose lists, default front matter, stable spell
diagnostics, physical blank-line identity, preserved soft/hard breaks, and
gutter continuation markers. The current example exercises some of these only
incidentally. It also labels several constructs incorrectly, so a user cannot
reliably tell whether the editor rendered the promised syntax.

## Scope

### In scope

- Focused edits to the existing wrapping, inline-formatting, block-quote, list,
  thematic-break, code, soft-break, spelling, edge-case, and final-summary
  sections of `examples/kitchen-sink.md`.
- One `> **Expected rendering:** ...` blockquote at the top of every top-level
  content section delimited by thematic rules, including the opening and final
  sections.
- Explicit recent-regression scenarios for physical versus wrapped rows,
  physical blank rows, both hard-break forms, nested inline soft breaks,
  loose-list spacing, list/blank/heading boundaries, multiline code spans, and
  separated spelling diagnostics.
- Correct `-`/`+`/`*` marker labels and examples, all three thematic-break
  syntaxes, real Setext headings, an ordered-`)` list, and a tilde fence.
- A focused automated test reading the actual example file and checking both
  exact source syntax and rendered layout behavior at wide and narrow widths.
- Preservation checks for the front matter, language fences, and literal tabs
  already used by the source-viewport regression.

### Out of scope

- Product implementation changes in the renderer, navigation, spell engine,
  commands, configuration, or parser.
- Turning the fixture into an editor-command tutorial or full CommonMark
  conformance corpus.
- Replacing existing small fixtures or the TUI snapshot fixture.
- New dependencies, public APIs, or developer commands.

## Implementation requirements

- Keep examples in their natural topical sections and use concise labels that
  state the expected observable outcome.
- Use the exact `> **Expected rendering:** ...` form for every section callout.
  Keep each callout to one concise paragraph and place it before the section's
  examples. Do not treat front-matter delimiters, fence contents, Setext
  underlines, or individual rules inside the Thematic Breaks demonstration as
  section boundaries.
- Preserve the exact long-line-plus-blank-row scenario used for wrapped-row and
  backward-navigation checks, while making its intent clearer.
- Include literal examples of a plain soft break, `\\` hard break, two-trailing-
  space hard break, and multiline code span. Do not replace invisible syntax
  with descriptive text alone.
- Add at least one loose sibling list and one multi-block list item without
  weakening existing tight/nested-list examples.
- Include a list followed by a physical blank line and a heading so the
  container-span blank-line regression is manually reproducible.
- Correct the currently inaccurate bullet, thematic-break, and Setext samples;
  retain the existing surrounding coverage.
- Add deliberate spelling errors in separated physical lines with an ordinary
  edit-safe line between them. Clearly label them as intentional.
- Test the real example through core layout APIs using unique text anchors and
  computed source lines/ranges, never hard-coded document line numbers.
- Test that the intended top-level section inventory has exactly one uniformly
  labeled expected-rendering callout per section and that each callout itself
  parses/renders as an ordinary block quote.
- Assert the literal trailing spaces and Go tabs from raw bytes. At narrow
  width, assert continuation rows repeat a physical range and are unnumbered;
  at wide width, assert authored soft-break/quote rows have distinct numbered
  ranges and the physical blank/list boundary remains numbered.
- If the new corpus exposes a product defect, stop for a plan change rather
  than modifying product code under this documentation task.

## Acceptance criteria

- [ ] The example visibly and accurately demonstrates every recent static
      Markdown scenario listed in scope.
- [ ] Every top-level thematic-rule-delimited section has exactly one concise,
      consistently formatted expected-rendering callout, with no extra
      callouts created for syntax that merely resembles `---`.
- [ ] The mislabeled bullet markers, repeated thematic-break form, and false
      Setext examples are corrected, with ordered-`)` and tilde-fence coverage
      added.
- [ ] Existing front matter, long lines, nested structures, language fences,
      and intentional Go tab bytes remain present.
- [ ] A focused test reads `examples/kitchen-sink.md` and verifies raw syntax,
      callout completeness, wide-layout physical row identity, narrow-layout
      continuation identity, loose-list separation, and multiline code-span
      normalization.
- [ ] Existing kitchen-sink and neighboring Markdown/rendering tests pass.
- [ ] The full repository quality gate passes without product-code,
      dependency, public-API, or build-workflow changes.

## Validation

- `cargo test -p oom-edit-core --offline --locked example_kitchen_sink_recent_regressions`
- `cargo test -p oom-edit-core --offline --locked kitchen_sink`
- `cargo test -p oom-edit-core --offline --locked soft_break`
- `cargo test -p oom-edit-core --offline --locked hard_break`
- `cargo test -p oom-edit-core --offline --locked loose_list`
- `cargo test -p oom-edit-core --offline --locked setext`
- `cargo test -p oom-edit-core --offline --locked thematic_break`
- `cargo test -p oom-edit-core --offline --locked --test source_viewport`
- `make check`

## Dependencies

- 001
- 002
- 003

## Expected areas of change

- `examples/kitchen-sink.md`
- `crates/oom-edit-core/src/rendered/tests/blocks.rs`

## Risks / notes

The actual example file is intentionally distinct from the smaller
`crates/oom-edit-core/tests/fixtures/kitchen-sink.md` and the TUI's inline
snapshot fixture. The new contract should read the real example explicitly;
do not synchronize or replace those purpose-built fixtures.
