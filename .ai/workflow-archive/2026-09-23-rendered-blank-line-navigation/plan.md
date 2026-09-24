# Rendered Blank-Line and Wrapped Navigation Fixes

## Objective

Make a rendered Normal-mode blank row retain the physical source identity of
the blank Markdown line it represents, so its gutter and status ruler are
correct, and make backward horizontal motion from that row land on the final
source-backed character of the preceding wrapped paragraph.

## Current behavior

The renderer inserts an empty row between top-level blocks through
`RenderedLayoutBuilder::add_synthetic_blank`. That helper always replaces its
candidate span with `last_content_source`, so a separator corresponding to a
real source blank line inherits the preceding paragraph's complete source
range. `rendered_line_numbers` also suppresses every synthetic row. When
vertical motion enters the row, `canonical_source_offset_for_row` therefore
falls back to the prior paragraph's start: the gutter has no number and the
status ruler remains on the previous physical line.

Horizontal motion has a related but separate boundary error. It first maps an
atom-free row through `source_backed_point`, which selects the first atom of the
nearest preceding rendered row, and then applies the requested backward step.
For a paragraph wrapped across several rendered rows, that extra step lands at
the end of the penultimate row rather than at the end of the paragraph.

The existing focused gutter and horizontal-navigation tests pass because they
assert that generated/wrapped rows are unnumbered and test cross-row movement
only from source-backed atoms; neither covers a real source blank row.

## Proposed implementation

1. Add failing core and TUI regressions for a narrow, multi-row paragraph
   followed by a physical blank line and another block. Assert the rendered
   row identity, gutter number, canonical cursor `(blank line, column 0)`, and
   status ruler before changing implementation.
2. During rendered block composition, inspect only the exact source gap between
   adjacent parser-owned block spans. When the displayed separator corresponds
   to a physical blank source line, emit an empty content row with that blank
   line's exact byte range and no atoms. Keep purely generated separators
   synthetic and source-atom-free. Preserve the renderer's existing single-row
   normalization between blocks.
3. Update horizontal traversal so an atom-free starting row is treated as a
   directional boundary: one backward step selects the last source-backed atom
   before the row, and one forward step selects the first after it. Subsequent
   counted steps continue through adjacent source-backed atoms. Source-backed
   starting rows retain their current behavior.
4. Add regressions for both `h` and Left, counted movement, Unicode/source
   offsets, generated synthetic rows, Select synchronization, and document
   boundaries, then run all impacted suites and the complete repository gate.

## Architectural decisions

- Blank-row source identity is owned by `oom-edit-core`; the TUI continues to
  project `RenderedLayout.line_numbers` and `EditorSession::cursor()` without a
  second line-number or cursor model.
- A physical blank line may own an empty rendered content row and its line-level
  source range, but it owns no display atom. Generated glyphs and presentation-
  only separator rows remain source-less at the atom level.
- Blank-line ownership is derived from adjacent parser spans and physical line
  boundaries, never from substring matching or post-render column correction.
- The existing policy of one displayed separator row between top-level blocks
  remains unchanged; this package corrects that row's identity rather than
  expanding or collapsing additional layout rows.
- Horizontal navigation remains in the core `nav` grammar. Both character keys
  and terminal-arrow translations continue through the same `KeyInput` path.
- No new public type, dependency, mutable text owner, or TUI-side editing state
  is introduced. If public DTO documentation must change to describe a blank
  content row, the curated facade and API guards will be reviewed in the same
  task.

## Work included

- Correct physical line ownership for displayed blank rows between Markdown
  blocks.
- Correct absolute and relative gutter projections for such rows.
- Correct the canonical cursor and status ruler after vertical movement onto a
  blank row.
- Correct backward `h`/Left movement from an atom-free blank row following a
  wrapped paragraph.
- Preserve forward/backward counts, boundary clamping, generated separator
  behavior, and rendered Select endpoint synchronization.
- Test forward at the core layout/session and TUI presentation layers.

## Task sequence

1. `tasks/001-rendered-blank-line-identity.md` — give physical blank rows exact
   source identity and verify gutter/status behavior.
2. `tasks/002-blank-row-horizontal-navigation.md` — make horizontal traversal
   enter adjacent content from the correct directional edge.

## Quality gate

The repository build system defines `make check` as the complete local CI gate:
format checking, clippy with warnings denied, workspace build, all Python/Rust
tests and doctests, dependency policy, RustSec audit, and bundled-data license
validation. It is required after each task and again for final acceptance.
Focused commands in each task run the new regressions and neighboring rendered
layout, gutter, status, navigation, and Select suites before that full gate.

No separate generic type-check command is invented because Rust compilation and
type checking are already covered by the repository's `make check` build,
clippy, and test stages.

## Risks

- Parser block spans may include line endings differently across Markdown
  constructs. Blank ownership must be determined from verified physical line
  boundaries and tested around headings, paragraphs, rules, Unicode, CRLF, and
  adjacent blocks without a source blank.
- Reclassifying an empty row as source-backed can affect entry mapping, line
  selections, resize remapping, and relative gutters. Tests must verify these
  consumers while keeping the row atom-free.
- Directional entry from an atom-free row must not change pointer hit-testing or
  the nearest-source fallback used by unrelated operations.
- The worktree already contains the completed prior package and a user-owned
  `examples/kitchen-sink.md` edit. Implementation must preserve those changes;
  the new regressions will use minimal deterministic text rather than rewriting
  the example.

## Out of scope

- Changing source Insert-mode gutter/cursor behavior.
- Showing more than the existing normalized one separator row for multiple
  consecutive blank Markdown lines.
- Changing Markdown parsing, word motions, vertical desired-column rules,
  wrapping width policy, or rendered styling.
- Modifying the user's `examples/kitchen-sink.md` fixture content.
- Adding dependencies or new configuration.

## Final acceptance criteria

- A rendered blank row backed by physical source line 47 displays gutter number
  47 in absolute mode and the correct relative value in relative mode.
- Moving onto that row sets the canonical session cursor to source line 47,
  column 1 for display, and the status ruler reports `47:1`.
- The blank row has exact line-level source ownership but no source-backed or
  generated display glyph atoms.
- From that row, one `h` or Left lands on the final source-backed character of
  the last rendered row of source line 46, even when line 46 wraps across three
  or more rows.
- Counted backward motion continues from that final character without skipping
  or duplicating atoms; boundaries and Select endpoints remain correct.
- Presentation-only synthetic rows remain unnumbered and do not claim source
  atoms.
- Every new regression fails against the pre-fix behavior and passes after the
  implementation.
- Every task-specific command and the final `make check` pass with no warnings,
  errors, dependency changes, or deferred work.
