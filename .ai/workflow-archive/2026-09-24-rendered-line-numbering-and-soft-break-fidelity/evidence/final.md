# Final Work-Package Evidence

## Work package

- ID: `2026-09-24-rendered-line-numbering-and-soft-break-fidelity`
- Title: `Rendered Line Numbering and Soft-Break Fidelity`
- Acceptance rounds: 3

## Original objective

Correct rendered Normal-mode source-line identity so physical blank and prose
lines remain numbered and navigable, preserve authored Markdown soft and hard
line breaks, distinguish only width-generated continuation rows with a subtle
and visible `↳` gutter marker, and make `examples/kitchen-sink.md` an explicit
manual and automated regression corpus with consistent expected-rendering
callouts.

## Completed tasks

- 001 — Complete physical blank-line identity — `evidence/001.md`
- 002 — Preserve rendered soft and hard breaks — `evidence/002.md`
- 003 — Mark width continuations in the gutter — `evidence/003.md`
- 004 — Expand kitchen-sink regression scenarios — `evidence/004.md`
- 005 — Make continuation markers visible — `evidence/005.md`

## Whole-package review

The cumulative change was reviewed against the original request, all three
approved acceptance rounds, every task document and evidence record,
repository architecture, and the complete diff from the recorded baseline.
Physical-line segmentation, source provenance, navigation identity, and line
numbers remain core-owned; the TUI derives only presentation styling and the
continuation glyph. The example document serves as a source-and-rendering
contract. No dependency, second text owner, public API expansion, parser
option, or parallel numbering model was added.

## High-confidence findings fixed

- Fixed an overlapping container-range remap by preferring exact source starts
  and then the narrowest containing range.
- Updated the affected confirmation snapshot found by the first full task gate.
- Consolidated gutter-row inputs after Clippy exposed the argument-count guard,
  retaining simultaneous diagnostic and continuation glyphs.
- Corrected the kitchen-sink contract's initial loose-list blank classification
  to the established atom-free, numbered `Content` representation.
- Refined the ordered-list callout so it describes accepted `)` source syntax
  without promising preservation of that delimiter in rendered output.
- Expanded the opening callout to document existing front-matter and `Space m`
  behavior.
- Reproduced the default-dark visibility failure as equal continuation
  foreground and gutter-background RGB values, then derived the marker from a
  visible gutter text role.
- Extended the theme matrix to catch and fix the equivalent default-light
  Color16 collision with a current-line gutter fallback.
- Added the missing custom-theme lowering coverage alongside every built-in
  theme and presentation tier.

## Final acceptance criteria

- Physical blank lines render with their source line number and report their
  canonical cursor line and column.
- Soft breaks and both supported hard-break forms start distinct rendered rows
  with correct source identity and numbering, including nested styles, links,
  lists, and quotes. Width wrapping remains independent within each row.
- Only width-continuation rows receive `↳`; physical rows retain numbers and
  synthetic or padding rows remain blank.
- The continuation marker is visible against colored gutter backgrounds in
  every built-in tier and test-loaded custom themes, remains dim and italic,
  and stays color-free in monochrome and accessible tiers.
- Diagnostic and continuation glyphs coexist without changing gutter width or
  content alignment.
- LF, CRLF, Unicode, resize/remap, navigation, search, Select projections, and
  line-wise operators retain exact ordered UTF-8-safe ranges.
- Code-span line endings still normalize to spaces, while break bytes and all
  generated prefixes, separators, and gutter glyphs claim no source atom.
- `examples/kitchen-sink.md` explicitly demonstrates the repaired wrapped row,
  blank row, soft/hard break, quote, loose-list, list-to-heading,
  multiline-code-span, and separated-spelling-diagnostic scenarios.
- The opening demonstration and all 21 top-level named sections each contain
  exactly one concise `> **Expected rendering:** ...` callout.
- Focused core, session, renderer, theme, snapshot, and corpus tests cover the
  changed behavior, and the full package gate passes without warnings.

## Final quality gate

| Command | Result |
| --- | --- |
| `make check` | PASS — 7 stages passed, 0 failed |

## Cumulative diff

From baseline `5cf7981417db4bbc68bb13a1bd0e7e94170080fd`, the worktree now:

- recovers exact physical blank-line ranges before parser-owned blocks and
  selects the most specific rendered range during cursor remapping;
- composes inline Markdown into source-backed physical rows before width
  wrapping, retaining soft/hard breaks, styles, links, and provenance;
- adds an accessible themed `↳` gutter projection for true width continuations
  while preserving diagnostics and fixed geometry;
- guarantees colored continuation markers contrast with the gutter surface
  across built-in and custom themes;
- documents oom-edit's conforming source-editor soft-break policy;
- expands the kitchen-sink corpus with accurate Markdown variants and uniform
  expected-rendering callouts; and
- adds focused unit/integration contracts plus updated core and TUI goldens.

## Remaining non-blocking concerns

None
