# Final Work-Package Evidence

## Work package

- ID: `2026-09-24-rendered-line-numbering-and-soft-break-fidelity`
- Title: `Rendered Line Numbering and Soft-Break Fidelity`

## Original objective

Correct rendered Normal-mode source-line identity so physical blank and prose
lines remain numbered and navigable, preserve authored Markdown soft and hard
line breaks, distinguish only width-generated continuation rows with a subtle
`↳` gutter marker, and make `examples/kitchen-sink.md` an explicit manual and
automated regression corpus with consistent expected-rendering callouts.

## Completed tasks

- 001 — Complete physical blank-line identity — `evidence/001.md`
- 002 — Preserve rendered soft and hard breaks — `evidence/002.md`
- 003 — Mark width continuations in the gutter — `evidence/003.md`
- 004 — Expand kitchen-sink regression scenarios — `evidence/004.md`

## Whole-package review

The cumulative change was reviewed against the original request, both approved
acceptance rounds, all task documents and evidence, repository architecture,
and the complete diff from the recorded baseline. Physical-line segmentation,
source provenance, navigation identity, and line numbers remain core-owned;
the TUI derives only presentation styling and the continuation glyph. The
follow-up changes use the real example document as a source-and-rendering
contract without altering product behavior. No dependency, second text owner,
public API expansion, parser option, or parallel numbering model was added.

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
- Expanded the opening callout to document the existing front-matter and
  `Space m` behavior.

## Final acceptance criteria

- The list/blank/heading regression renders its physical separator as a
  numbered empty row and reports its canonical line and column. Core, session,
  rendered-screen, and kitchen-sink tests cover the behavior.
- Soft breaks and both supported hard-break forms start distinct rendered rows
  with correct source identity and numbering, including nested styles, links,
  lists, and quotes. Width wrapping remains independent within each row.
- Only repeated width-continuation rows receive `↳`; physical rows retain
  numbers and synthetic/padding rows remain blank. The glyph is subdued,
  accessible without color, and does not displace diagnostics.
- LF, CRLF, Unicode, resize/remap, navigation, search, Select projections, and
  line-wise operators retain exact ordered UTF-8-safe ranges.
- Code-span line endings still normalize to spaces, while break bytes and all
  generated prefixes, separators, and gutter glyphs claim no source atom.
- `examples/kitchen-sink.md` now explicitly demonstrates the repaired wrapped
  row, blank row, soft/hard break, quote, loose-list, list-to-heading,
  multiline-code-span, and separated-spelling-diagnostic scenarios.
- The opening demonstration and all 21 top-level named sections each contain
  exactly one concise `> **Expected rendering:** ...` callout.
- The example uses accurate `*`, `+`, and `-` list markers, all three thematic
  rule syntaxes, real Setext underlines, ordered `)` markers, and a tilde fence,
  while preserving front matter, long lines, nested content, language fences,
  and literal Go tabs.
- The focused corpus contract reads the actual example and verifies raw syntax,
  wide physical-row behavior, narrow continuations, numbered blanks,
  loose-list separation, quote rows, and multiline-code-span normalization.
- All focused commands and both task-level and final `make check` gates passed
  without warnings, dependency changes, deferred work, or product expansion in
  the follow-up round.

## Final quality gate

| Command | Result |
| --- | --- |
| `make check` | PASS |

## Cumulative diff

From baseline `5cf7981417db4bbc68bb13a1bd0e7e94170080fd`, the worktree now:

- recovers exact physical blank-line ranges before parser-owned blocks and
  selects the most specific rendered range during cursor remapping;
- composes inline Markdown into source-backed physical rows before width
  wrapping, retaining soft/hard breaks, styles, links, and provenance;
- adds an accessible themed `↳` gutter projection for true width continuations
  while preserving diagnostics and fixed geometry;
- documents oom-edit's conforming source-editor soft-break policy;
- expands the kitchen-sink corpus with accurate Markdown variants and uniform
  expected-rendering callouts; and
- adds focused unit/integration contracts plus updated core and TUI goldens.

## Remaining non-blocking concerns

None
