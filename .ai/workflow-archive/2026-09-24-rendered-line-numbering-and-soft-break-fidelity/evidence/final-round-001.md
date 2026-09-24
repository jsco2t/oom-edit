# Final Work-Package Evidence

## Work package

- ID: `2026-09-24-rendered-line-numbering-and-soft-break-fidelity`
- Title: `Rendered Line Numbering and Soft-Break Fidelity`

## Original objective

Correct rendered Normal-mode line identity so physical blank and prose lines
remain numbered and navigable, preserve authored Markdown soft and hard line
breaks, and distinguish only width-generated continuation rows with a subtle
`↳` gutter marker.

## Completed tasks

- 001 — Complete physical blank-line identity — `evidence/001.md`
- 002 — Preserve rendered soft and hard breaks — `evidence/002.md`
- 003 — Mark width continuations in the gutter — `evidence/003.md`

## Whole-package review

The cumulative change was reviewed against the original request, approved
plan, every task, task evidence, the architecture boundaries, and the complete
diff from the recorded baseline. Physical-line segmentation and canonical
source ownership remain in `oom-edit-core`; the TUI derives presentation-only
continuation markers from the core layout and line-number projection. No new
dependency, mutable text owner, public API surface, or alternative line-number
model was introduced. The implementation and tests cover the affected
rendering, navigation, selection, status, gutter, theme, and snapshot paths.

## High-confidence findings fixed

- Fixed an overlapping container-range remap discovered during task review by
  preferring exact starts and then the narrowest containing source range.
- Updated the affected confirmation snapshot found by the first full task gate.
- Consolidated gutter row inputs after the full gate exposed Clippy's argument
  count guard, while retaining simultaneous diagnostic and continuation glyphs.

## Final acceptance criteria

- The list/blank/heading regression renders the physical blank separator as a
  numbered empty content row and reports its canonical line and column in the
  status ruler. Covered by core, session, and rendered-screen tests in
  `evidence/001.md`.
- Every prose soft break and both supported hard-break forms start a distinct
  rendered physical row with correct source identity and numbering, including
  nested styles, links, lists, and block quotes. Covered by mapped-inline,
  provenance, and layout tests in `evidence/002.md`.
- Consecutive source statements remain on distinct rendered rows rather than
  being concatenated into a reflowed paragraph. Covered by the supplied-shape
  regression and updated goldens in `evidence/002.md`.
- Width wrapping remains independent within each physical line; only repeated
  wrapped content rows receive `↳`, while physical, synthetic, and padding rows
  retain their respective number or blank behavior. Covered by classification,
  gutter, and snapshot tests in `evidence/003.md`.
- The continuation marker is quieter than line numbers in color themes,
  retains glyph and modifier signals in monochrome, and coexists with the
  diagnostic cell without changing gutter width. Covered by theme and direct
  gutter tests in `evidence/003.md`.
- LF, CRLF, Unicode, resize/remap, navigation, search, Select projection, and
  line-wise operator behavior retain exact ordered UTF-8-safe ranges. Covered
  by core and integration suites in `evidence/001.md` and `evidence/002.md`.
- Code-span line endings still normalize to spaces, while break bytes,
  generated prefixes, separators, and gutter glyphs claim no source atom.
  Covered by provenance and code-span regressions in `evidence/002.md` and
  gutter tests in `evidence/003.md`.
- The pre-fix regressions are now automated, all focused task commands passed,
  and the final repository gate passed without warnings, dependency changes,
  or deferred work. Covered by all task evidence and the final gate below.

## Final quality gate

| Command | Result |
| --- | --- |
| `make check` | PASS |

## Cumulative diff

From baseline `5cf7981417db4bbc68bb13a1bd0e7e94170080fd`, the worktree now:

- recovers exact physical blank-line ranges before parser-owned blocks and
  selects the most specific rendered range during cursor remapping;
- composes inline Markdown as source-backed physical lines before width
  wrapping, preserving soft/hard breaks, styles, links, and provenance;
- adds an accessible themed continuation gutter slot and `↳` projection for
  true wrapped rows while preserving diagnostics and fixed gutter geometry;
- documents the conforming source-editor soft-break policy; and
- adds focused unit/integration regressions plus updated core and TUI goldens.

## Remaining non-blocking concerns

None
