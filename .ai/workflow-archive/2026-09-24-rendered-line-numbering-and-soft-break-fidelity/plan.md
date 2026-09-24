# Rendered Line Numbering and Soft-Break Fidelity

## Objective

Make rendered Normal and Select layouts retain every displayed physical source
line as an independently numbered and navigable row, preserve Markdown soft and
hard line breaks instead of reflowing them into one artificial paragraph, and
mark only width-created continuation rows with a subtle accessible gutter
glyph.

## Current behavior

`pulldown-cmark` correctly parses consecutive nonblank prose lines as one
paragraph containing `SoftBreak` events. The block model retains those events,
but converts each soft break to a mapped space. `render_paragraph` then flattens
the entire paragraph to one `MappedLine`, applies width wrapping, and gives
every resulting row the paragraph's complete source range. Consequently:

- author-chosen physical line endings disappear into reflowed prose;
- `rendered_line_numbers` can number only the first row for the paragraph-wide
  source span;
- cursor/status and line-selection identity cannot distinguish the later
  physical lines from width-created continuations.

Hard breaks currently follow the same flattening path and become two visible
spaces instead of forcing a rendered row boundary. By contrast, line endings
inside code spans are intentionally normalized to spaces and must remain so.

The recently added blank-line ownership logic searches only the gap beginning
at the preceding rendered line's source-range end. Some parser container spans,
notably a list before a heading, include the following blank separator. The
search interval is then empty, so the renderer emits an unnumbered synthetic
separator instead of the real physical blank line (the reported missing line
43).

The TUI gutter currently represents every `None` line-number row as an empty
cell, so width-created continuations are visually indistinguishable from
synthetic presentation rows.

## Proposed implementation

1. Add failing core/session/TUI regressions for a list item followed by a
   physical blank line and heading. Resolve the separator from the physical
   line immediately before the next parser-owned block boundary, rather than
   trusting the preceding block's possibly inclusive end span. Preserve exact
   LF/CRLF ranges and the existing one-row separator normalization.
2. Replace paragraph-wide inline flattening with a private mapped-line
   composition path that treats parser `SoftBreak` and `HardBreak` leaves as
   structural row boundaries. Split each row's line-level source range at the
   parser-provided break bytes, then apply width wrapping independently to each
   physical row. Keep every visible atom attached to its original UTF-8 source
   range; line-ending bytes remain line-level identity and never become a
   generated visible atom.
3. In the TUI, render `↳` in the line-number field for an unnumbered content
   row that repeats the immediately preceding physical source range. Add a
   dedicated continuation-gutter theme slot whose colored tiers are quieter
   than ordinary line numbers and whose glyph plus dim modifier preserves the
   signal in monochrome. Keep diagnostic markers in their existing leading
   gutter cell.
4. Update renderer contract documentation and affected goldens/snapshots, then
   run focused parsing, provenance, navigation, selection, gutter, theme, and
   snapshot suites followed by the complete repository gate.

## Architectural decisions

- The repository's `docs/markdown-spec.md` remains authoritative. It defines a
  soft break as a plain newline within a paragraph and already permits it to
  render as a space or line ending.
- CommonMark 0.31.2 and GFM both explicitly allow a conforming renderer to
  render a soft break as either a space or a line ending. Oom-edit will choose
  the permitted line-ending form because it is a source-oriented editor:
  <https://spec.commonmark.org/0.31.2/#soft-line-breaks> and
  <https://github.github.com/gfm/#soft-line-breaks>.
- This is therefore a UX/canonical-position defect rather than a parser
  conformance defect for soft breaks. Hard breaks do require a visual boundary
  and will use the same structural path.
- `oom-edit-core` owns physical-line segmentation, exact source ranges,
  wrapping, navigation identity, and line numbers. The TUI only chooses the
  gutter glyph and theme style from core-provided row identity.
- `RenderedLayout.line_numbers` remains the single line-number source of truth.
  A physical row start is `Some(source line)`; a width continuation is an
  unnumbered content row repeating the preceding exact source range; a
  generated separator remains synthetic and unnumbered. No second mutable
  line-number model is added.
- Soft/hard break bytes provide line-level identity but produce no display
  atom. Generated list/quote prefixes and the gutter marker remain source-less.
- Inline styles and link/image metadata must survive breaks, including breaks
  nested inside emphasis, strong text, links, list items, and block quotes.
- Line endings inside code spans retain the specification's space
  normalization and do not create rendered rows.
- Plain-text clipboard policy, Insert-mode source rendering, Markdown parsing
  options, wrap-width configuration, and source bytes do not change.
- `↳` is a one-cell, non-color continuation signal. Synthetic rows, physical
  blank rows, padding rows, and the first row of a source line never receive
  it.

## Work included

- Recover physical blank-line identity when the preceding parser block span
  consumes the separator.
- Preserve soft and hard prose line boundaries in rendered Normal/Select.
- Give each displayed physical prose line exact line-level source ownership,
  correct absolute/relative numbering, canonical cursor/status identity, and
  stable resize/selection behavior.
- Keep width wrapping within each physical source line and mark its additional
  display rows with an accessible, subdued gutter continuation glyph.
- Cover LF, CRLF, Unicode, nested inline styling, lists, block quotes, code
  spans, synthetic rows, diagnostic gutters, themes, and snapshots.

## Task sequence

1. `tasks/001-complete-physical-blank-line-identity.md` — recover numbered
   physical blank separators even when parser container spans include them.
2. `tasks/002-preserve-rendered-soft-and-hard-breaks.md` — retain physical
   prose line boundaries and their canonical source identity before wrapping.
3. `tasks/003-mark-width-continuations-in-the-gutter.md` — render and theme the
   `↳` marker only for true width-created continuation rows.

## Quality gate

The repository build system defines `make check` as the complete local CI gate:
format checking, clippy with warnings denied, workspace build, all Rust/Python
tests and doctests, dependency policy, RustSec audit, and bundled-data license
validation. It is the standard gate after every task and the final package
gate. Each task also runs narrow test filters for the changed layout,
provenance, navigation, gutter, theme, and snapshot paths before the full gate.

No separate generic type-check command is introduced because Rust compilation
and type checking are already enforced by the build, clippy, and test stages of
`make check`.

## Risks

- Inline breaks may occur inside nested emphasis, links, images, list items,
  and block quotes. Splitting after style/provenance has been discarded would
  corrupt spans or link markers; boundaries must remain explicit during mapped
  inline composition.
- Parser block spans and line-ending ranges differ around LF, CRLF, container
  markers, and trailing blank lines. Separator and row ranges must be computed
  from verified physical boundaries, not substring matching.
- More rendered rows affect cursor remapping, desired columns, line/character
  selection, yank/delete ranges, search, viewport follow, and snapshots.
- The gutter marker must not overwrite the diagnostic marker column, change
  gutter width, appear on synthetic rows, or become color-only in accessible
  themes.
- Preserving physical prose lines changes the previous deliberate reflow
  appearance. Width wrapping must still work independently within long source
  lines, and source bytes must remain untouched.

## Out of scope

- Changing Insert-mode layout or line numbering.
- Adding a configuration switch for soft-break rendering.
- Treating every physical newline inside code spans, HTML tags, tables, or
  literal/code blocks as a prose soft break.
- Changing plain-text clipboard conversion.
- Displaying more than one normalized separator for multiple consecutive blank
  source lines.
- Changing wrap width, Markdown parser options, or adding dependencies.

## Final acceptance criteria

- The reported list/blank/heading shape renders the physical blank line 43 as
  a numbered empty content row; its canonical cursor and status ruler report
  line 43, column 1.
- Every prose `SoftBreak` and `HardBreak` produces a new rendered physical-line
  start with the correct absolute/relative gutter number and canonical source
  line, including nested styled content, lists, and block quotes.
- Consecutive source statements like the reported lines 3–6 remain on distinct
  rendered rows and are not concatenated into an artificial paragraph.
- Width wrapping still occurs within each physical line. Only additional rows
  created by that wrapping display `↳`; physical rows display numbers, while
  synthetic/padding rows remain blank.
- The continuation marker is visually quieter than ordinary line numbers in
  color themes, remains distinguishable by glyph/modifier in monochrome, and
  coexists with diagnostic markers without changing gutter width.
- LF, CRLF, Unicode, resize/remap, navigation, search, Select projections, and
  line-wise operators retain exact, ordered, UTF-8-safe source ranges.
- Code-span line endings continue to normalize to spaces, and no generated
  prefix, separator, or gutter glyph claims a source byte.
- Every new regression fails against the pre-fix behavior and passes after the
  implementation; every task-specific command and final `make check` pass with
  no warnings, errors, dependency changes, or deferred work.

## Acceptance follow-up — Round 2: Kitchen-sink regression coverage

### Observed acceptance gap

The rendering fixes are automated in focused unit and integration fixtures, but
the user-facing `examples/kitchen-sink.md` does not yet present a deliberate,
self-explanatory scenario for every recent behavior. The document contains a
long wrapped line followed by a blank line, consecutive block-quote lines, rich
front matter, and one intentional misspelling, so it already exercises parts of
the September 23–24 repairs. Its soft-break section, however, still describes a
renderer-dependent outcome rather than oom-edit's newly selected line-ending
policy, and it does not explicitly cover:

- both hard-break syntaxes;
- the multiline code-span normalization exception;
- loose-list spacing and the list/blank/heading boundary that lost a line
  number;
- multiple separated spelling diagnostics around ordinary editable text; or
- the distinction between physical source rows and width continuations.

The broader Markdown-spec audit also found objectively misleading examples:

- the three unordered-marker groups are mislabeled and never use `+`;
- the thematic-break section repeats `---` instead of demonstrating `---`,
  `***`, and `___`; and
- the Setext section uses ATX `#`/`##` headings rather than underline syntax.

The authoritative spec additionally identifies useful uncovered variants:
ordered-list `)` delimiters and tilde-fenced code blocks.

The document also lacks a consistent statement of expected behavior at the
start of each thematic-rule-delimited section, which makes it harder to use as
a manual verification artifact even when the syntax itself is present.

### Additive implementation strategy

1. Add exactly one consistently formatted blockquote callout at the top of
   every top-level content section delimited by `---`:
   `> **Expected rendering:** ...`. Each callout will concisely describe the
   visible Normal-mode structure and any important interactive expectation for
   that section. The initial heading demonstration and final section receive
   the same treatment. The three thematic rules demonstrated inside the
   Thematic Breaks section do not create three additional content sections.
2. Refine the existing wrapping and paragraph sections so their labels state
   the expected Normal-mode result: every authored prose row remains distinct,
   a long physical row may produce marked width continuations, and the physical
   blank row after it remains numbered and navigable.
3. Add compact examples for plain soft breaks, backslash and two-space hard
   breaks, styled/link content crossing a soft break, and a multiline code span
   that remains one normalized rendered row.
4. Add list examples that distinguish tight from loose lists, include a
   multi-block item, and place a heading after a list-owned physical blank line.
   Add a small, explicit spell-check scenario with independent misspellings on
   either side of ordinary editable text.
5. Correct the malformed/mislabeled marker, thematic-break, and Setext examples,
   and add the compact ordered-`)` and tilde-fence variants identified by the
   spec audit. Update the final coverage summary to match the actual document.
6. Add a focused contract test that reads the real
   `examples/kitchen-sink.md`, validates the key source forms, and renders it at
   wide and narrow widths to verify physical-line numbering, continuation
   identity, blank-row identity, loose-list separation, quote-line fidelity,
   and the code-span exception. The contract also verifies that every intended
   top-level section has exactly one expected-rendering callout in the uniform
   form. Keep existing focused behavior tests and run the complete repository
   gate.

### Architecture and test decisions

- This round changes the example corpus and its tests, not renderer behavior.
  Any newly discovered product defect requires a plan change rather than an
  undocumented implementation expansion.
- Examples remain organized under their natural Markdown construct sections;
  the document should be useful for human exploration rather than read like an
  internal bug ledger.
- Expected-rendering callouts use ordinary CommonMark block quotes with a
  strongly emphasized label. This syntax is supported by the project and
  remains understandable in both Insert and rendered modes without relying on
  a nonstandard alert extension.
- “Section” means a top-level content region separated by the document's
  thematic rules. Front-matter delimiters, fenced-code contents, Setext
  underlines, and the three rule examples inside the Thematic Breaks section
  are not independent sections requiring nested callouts.
- Assertions use unique scenario text and source/layout properties rather than
  fixed absolute line numbers, so adding unrelated examples later does not make
  the contract brittle.
- Physical lines are checked through `RenderedLayout.line_numbers` and exact
  source ranges. Width continuations are checked as unnumbered content rows
  repeating the physical line's range. TUI glyph/theme behavior remains covered
  by completed task 003 and is not duplicated in this corpus contract.
- No dependency, public API, parser option, source mutation, or Make target is
  added.

### Round 2 task sequence

4. `tasks/004-expand-kitchen-sink-regression-scenarios.md` — correct and expand
   the user-facing Markdown corpus, add durable scenario assertions, and run
   focused plus complete validation.

### Round 2 risks

- Literal two-space hard breaks and tabs are invisible source features. Tests
  must assert their exact bytes so ordinary editing cannot silently convert the
  examples into a different construct.
- A large example-file rewrite would make manual comparison noisy. Changes
  should stay localized to existing topical sections plus only the small new
  sections needed for recent interactive behavior.
- Repeated callout block quotes add many rendered rows and could obscure the
  syntax examples if verbose. Each callout should remain one concise paragraph
  and use the exact shared label.
- Some recent fixes are stateful rather than representable by static Markdown
  alone, notably typing onto trailing empty lines, the five-second spell idle
  timer, and invoking `Space m` on a document without front matter. The corpus
  can supply representative content, but existing integration tests remain the
  authoritative behavioral verification for those interactions.

### Round 2 out of scope

- Further renderer, navigation, spell engine, command, or parser changes.
- Duplicating every CommonMark conformance example or every programming
  language grammar in the user-facing document.
- Replacing the smaller deterministic test fixtures or the TUI's separate
  inline snapshot fixture with this 600-plus-line document.
- Adding prose instructions for every editor command; this remains a Markdown
  rendering/editing corpus rather than a user manual.

### Round 2 acceptance criteria

- `examples/kitchen-sink.md` contains clear examples for the recently repaired
  wrapped-line/blank-row navigation, preserved prose and quote line endings,
  both hard-break syntaxes, loose-list spacing, list-to-heading blank identity,
  multiline code-span normalization, and separated spelling diagnostics.
- Every top-level thematic-rule-delimited content section, including the
  opening heading demonstration and final section, begins with exactly one
  `> **Expected rendering:** ...` callout that accurately describes its visible
  outcome; rule examples inside the Thematic Breaks section are not
  misclassified as separate sections.
- The example's unordered markers, thematic-break variants, and Setext headings
  are syntactically correct and accurately labeled; ordered-`)` and tilde-fence
  variants are also represented.
- The existing front matter, long lines, nested constructs, code-language
  corpus, and intentional Go tabs remain intact.
- A focused automated contract reads the actual example file and verifies its
  callout completeness, key raw source forms, and rendered wide/narrow layout
  properties without fixed absolute line-number assumptions.
- Existing kitchen-sink, soft/hard-break, loose-list, Setext, thematic-break,
  and source-viewport tests pass, followed by a warning-free `make check`.
- No product behavior, dependency, public API, or build workflow changes are
  introduced unless the package returns for an explicitly approved plan change.

## Acceptance follow-up — Round 3: Visible default-theme continuation marker

### Observed acceptance failure and root cause

The supplied default-dark screenshot shows correctly detected width
continuations: the spelling-diagnostic dot repeats on each continuation row,
which means the core row identity and TUI continuation classification are
working. The `↳` glyph itself is not visible.

Repository inspection confirms this is a theme-composition defect rather than
a character-layering defect. `render_gutter` writes `↳` into the expected
number-field cell and the existing buffer test verifies the symbol. However,
`add_surface_slots` derives `UiSlot::GutterContinuation` from `UiSlot::Border`.
In default-dark TrueColor, the border foreground and gutter background are both
`#2f343e`; after the background style is patched onto the marker, its foreground
equals its background. The additional `DIM` modifier makes no difference to an
already invisible glyph.

The escaped validation gap is that existing tests assert symbol placement,
modifier presence, and inequality from the ordinary gutter style, but do not
assert contrast against the actual gutter background after style composition.

### Additive implementation strategy

1. Add a failing rendered-screen regression for default-dark TrueColor that
   inspects the actual `↳` buffer cell after gutter composition. Verify the
   symbol remains in the number field, its effective foreground differs from
   its effective background, and a simultaneous diagnostic marker remains in
   the leading gutter cell.
2. Derive the continuation foreground from an existing visible gutter role
   rather than the border role whose color may equal the gutter surface. Keep
   the dedicated continuation slot and its `DIM | ITALIC` non-color carriers so
   the marker remains quieter than ordinary line numbers and accessible in
   monochrome.
3. Strengthen theme-contract tests across every built-in and test-loaded custom
   theme and capability tier. Colored palettes must not resolve a continuation
   foreground equal to the gutter background; monochrome and accessible paths
   must retain the glyph/modifier contract without introducing color.
4. Run the focused gutter and theme suites, then the complete repository gate.

### Architecture and test decisions

- This correction is TUI presentation-only. Core layout, source provenance,
  continuation classification, line numbers, navigation, and document text do
  not change.
- Continue using `UiSlot::GutterContinuation` as the semantic theme boundary;
  do not hardcode a color in `render_gutter` or add a second rendering path.
- Reuse the theme's existing gutter foreground as the visible color basis and
  use modifiers to make the marker quieter. This keeps built-in and custom
  theme behavior coherent without extending the user theme schema.
- Test the fully composed buffer-cell style in addition to palette rows. A
  palette-level inequality from ordinary numbers is insufficient because a
  foreground may still equal the inherited gutter background.
- No dependency, public API, core API, configuration field, or Make target is
  added.

### Round 3 task sequence

5. `tasks/005-make-continuation-markers-visible.md` — repair continuation
   marker theme contrast and add composited-style regression coverage.

### Round 3 risks

- ANSI color tiers do not expose RGB contrast ratios. The durable invariant is
  that the marker foreground differs from the gutter background while `DIM`
  keeps it subordinate to the ordinary number style.
- Ratatui style patching can inherit background and modifiers from multiple
  layers. Tests must inspect the final buffer cell, not only the standalone
  theme slot.
- A diagnostic and continuation marker can occupy adjacent cells on the same
  row. The fix must not move, overwrite, or recolor the diagnostic cell.

### Round 3 out of scope

- Changing the `↳` glyph, gutter width, or continuation classification.
- Altering line-number, cursor/status, wrapping, Markdown, or source behavior.
- Adding a continuation-style configuration option or extending custom theme
  files.
- Retuning unrelated border, gutter, diagnostic, or document colors.

### Round 3 acceptance criteria

- Default-dark TrueColor renders a visible `↳`: the final marker cell contains
  the glyph and has an effective foreground distinct from its gutter
  background.
- Every colored built-in and test-loaded custom theme tier keeps continuation
  foreground distinct from the gutter background after theme lowering.
- Continuation markers remain visually quieter than ordinary line numbers via
  the dedicated slot and non-color modifiers; monochrome and accessible modes
  remain color-free and distinguishable.
- Diagnostic markers and continuation markers coexist in their existing cells
  without changing gutter width or document alignment.
- Focused gutter/theme regressions and the full warning-free `make check` pass
  without core, dependency, public-API, configuration, or build-workflow
  changes.
