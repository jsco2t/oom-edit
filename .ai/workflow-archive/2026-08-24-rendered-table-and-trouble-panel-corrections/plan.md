# Rendered Table and Trouble Panel Corrections

## Objective

Correct the reported rendered-table wrapping, selection, and row-readability
problems and polish the Trouble/Diagnostics modal so its columns align and it
does not leave the document cursor visible. Replace the completed alternating
row-background feature with faint, source-less dashed boundaries between body
rows. Preserve exact source provenance, the core/TUI dependency boundary,
configuration compatibility, and all four public modes.

## Current behavior

Tasks 001 and 002 have corrected table word wrapping and source-driven wrapped
selection. Tasks 003 and 004 added alternating row backgrounds and corrected
Trouble layout/cursor ownership respectively.

The alternating background implementation now adds a core line role, public
DTO surface, TUI theme slot, App runtime plumbing, and a user setting. The user
has found the background approach has too many drawbacks and explicitly
superseded it. The current table renderer emits a solid header/body separator
but no visual boundary between adjacent body rows, which is especially hard to
scan when a logical row wraps across multiple visual lines.

## Proposed implementation

First, replace the table-only fixed chunker with direct mapped word wrapping.
It will partition `MappedFragment` values without reconstructing provenance,
prefer the last whitespace boundary that fits, trim only the consumed wrap
separator, and hard-wrap only a token that cannot fit in an empty cell line.

Second, make character-selection display projection source-driven. Canonical
selected source ranges will be projected onto every matching rendered atom,
including non-linear wrapped-table layout. The selection row DTO will represent
all independent display intervals required on a visual row, the TUI will paint
each interval, and the private Vim operation DTO will remain consumer-owned in
`session.rs`/`vim.rs`. Line and block semantics will remain unchanged.

Third, remove the alternating-row feature completely: its core role, public API
guard entries, App/runtime plumbing, configuration and README entry, theme
slot, render composition, tests, and snapshot expectations. Replace it inside
the table builder with one synthetic boundary line between each pair of body
rows. Each column interior is filled across its full allocated width with ASCII
`-` glyphs, contained by the existing vertical table borders. Dash fragments
use the existing renderer-neutral muted semantic style while border glyphs
retain the ordinary table style. Because the boundary is built from the same
current `col_widths` as the data rows, layout reconstruction on terminal resize
recomputes its geometry automatically.

Finally, compute stable Trouble column widths from its complete entry snapshot
and format every visible row with those widths. Suppress the document screen's
cursor request whenever a modal overlay owns presentation; cursor-bearing
overlays remain responsible for explicitly positioning their own cursor.

## Architectural decisions

- Mutable text ownership and all editing mutations remain unchanged.
- Table wrapping operates directly on mapped fragments. It must not recover
  provenance with substring matching or post-layout column correction.
- Synthetic table borders, padding, dashed row boundaries, separators, and
  consumed wrap whitespace remain source-less. Source atoms retain exact UTF-8
  byte ranges.
- Selection rendering is derived from canonical source ranges and may contain
  multiple independent display intervals on one rendered row. The public
  selection DTO and crate-root API guard are updated deliberately if its shape
  changes; rendered DTOs do not cross into `vim.rs`.
- Core owns dashed row-boundary placement and width because both derive from
  table layout. It emits ASCII dash glyphs with `SemanticStyle::Muted`, never
  terminal colors or ratatui types.
- Exactly one dashed boundary follows each body row except the last, after all
  wrapped continuation lines. The header retains its existing solid separator;
  one-row tables gain no dashed boundary.
- Each boundary keeps the table's vertical edge and inter-column borders and
  fills every column's complete interior allocation with dashes. It neither
  extends beyond the table nor claims source provenance.
- The superseded `editor.alternating_table_rows` setting is removed rather than
  retained as an ignored compatibility flag; serde's existing unknown-field
  behavior keeps older config files loadable without preserving dead runtime
  behavior.
- Modal cursor ownership is enforced at the App/screen presentation boundary;
  Trouble does not manufacture a fake cursor because its row highlight is the
  complete selection signal.
- No external dependency or new developer command is required.

## Work included

1. Word-boundary wrapping for rendered table cells, with direct source-map
   preservation and focused coverage for whitespace, overlong tokens, Unicode,
   repeated text, alignment, and equal-width table rows.
2. Source-driven character-selection projection across wrapped table cells,
   including forward/reverse selections, independent visual intervals, exact
   operator ranges, resize stability, and UTF-8 safety.
3. Removal of the superseded configurable alternating body-row surfaces and
   replacement with faint, source-less dashed boundaries between adjacent body
   rows, including wrapped rows, resize-driven geometry, API/config/theme
   cleanup, selection/navigation safety, and snapshots.
4. Stable Trouble columns and exclusive modal cursor ownership, with narrow
   terminal, scrolling, Normal/Select/Insert background, and snapshot coverage.

All changed behavior receives automated tests in the same task that changes
it.

## Task sequence

1. `tasks/001-word-boundary-table-wrapping.md`
2. `tasks/002-source-driven-wrapped-table-selection.md`
3. `tasks/003-configurable-alternating-table-rows.md`
4. `tasks/004-trouble-panel-layout-and-cursor.md`
5. `tasks/005-replace-row-backgrounds-with-dashed-boundaries.md`

Task 005 supersedes Task 003's final behavior. Tasks 001–004 remain completed
historical steps; only Task 005 executes for plan revision 2.

## Quality gate

Both the standard per-task gate and final package gate run `make check`, the
repository's local CI target. It performs format checking, Clippy with warnings
denied, a warning-free workspace build, the full isolated-config workspace test
suite, cargo-deny, cargo-audit, and bundled-data license validation. This is the
complete repository-defined completion gate, so separate generic type-check or
lint commands are not invented. Task-specific `make test` runs provide focused
behavioral feedback before the complete gate. Dependency-only re-vendoring is
omitted because this plan adds no dependency; if a dependency became necessary,
that would materially change the frozen plan.

## Risks

- Table visual order differs from Markdown source order once cells wrap. A
  screen-rectangle shortcut can reintroduce selection leakage, especially when
  selections cross cells or run backward.
- Whitespace at a wrap boundary has real source ownership even when it is not
  displayed. Tests must distinguish intentionally hidden separator whitespace
  from lost selectable content.
- Wide glyphs and zero-width suffixes can violate width or byte-boundary
  assumptions unless display groups remain intact.
- Expanding selection row geometry can affect block/line selections and public
  API guards; regression tests must cover every selection shape.
- A boundary must be inserted once per logical row, not once per wrapped visual
  continuation, and must remain distinct from the solid header/body separator.
- Synthetic boundary rows can perturb rendered navigation, line numbering,
  selection projection, or nearest-source behavior if their line kind and atom
  provenance are wrong.
- Width tests must force different column allocations on resize; tables whose
  natural width already fits would otherwise make a resize regression vacuous.
- Cursor visibility is frame-global in ratatui. Suppressing the document cursor
  must not prevent a cursor-owning overlay from setting its own position.
- `.agents/skills/feature-workflow/SKILL.md` was already modified before this
  workflow started. It is user-owned and must remain untouched and excluded
  from implementation review conclusions.

## Out of scope

- Changing Markdown table parsing or the authoritative Markdown specification.
- Changing source Insert-mode hard wrapping or the runtime `:set wrap` grammar.
- Adding arbitrary user-authored color values or a new theme file format.
- Redesigning table borders, column allocation, alignment semantics, or the
  40-column natural cell cap beyond what word wrapping requires.
- Adding multi-cursor support, mouse interaction, Trouble sorting/filtering, or
  new diagnostic providers.
- Adding a configuration toggle or runtime command for dashed row boundaries.
- Changing the solid header/body separator, outer borders, cell padding,
  column allocation policy, or the 40-column natural cell cap.
- Adding dependencies or modifying vendored sources.
- Modifying the pre-existing user change in
  `.agents/skills/feature-workflow/SKILL.md`.

## Final acceptance criteria

- Ordinary prose in a constrained rendered table cell wraps at the last word
  boundary that fits; only a token wider than the cell is hard-wrapped.
- Every wrapped table line stays within its allocated display width, table rows
  remain equal width, and source-backed atoms retain exact valid UTF-8 ranges
  through repeated text, Unicode, inline styles, and wrapping.
- Character Select within a wrapped table cell highlights only the selected
  source-backed text across its visual continuation lines, never candidate text
  from another cell or logical row; forward and reverse selections and their
  yank/delete projections operate on the same exact source ranges.
- Character, line, and block selections remain deterministic across layout
  width changes and preserve their existing semantics outside tables.
- The alternating-row feature is absent: no `alternating_table_rows` setting,
  App/render setting, alternate-row core role, theme slot, background styling,
  documentation, or obsolete tests remain.
- A table with two or more body rows contains exactly one faint dashed boundary
  between each adjacent pair. A wrapped logical row receives no internal
  boundary and exactly one after its final continuation; a zero- or one-body-row
  table receives none.
- Every dashed boundary retains vertical edge and inter-column borders, fills
  each allocated column interior completely with ASCII `-` glyphs, matches the
  table's display width, and is recomputed when a resize changes column widths.
- Boundary glyphs and their layout cells are synthetic/source-less, use the
  existing muted semantic style, cannot become selected source text, and do not
  alter exact character/line/block operator ranges or rendered navigation.
- Trouble entries with different location widths display aligned severity,
  provider, and message columns without breaking narrow-terminal scrolling or
  progress/footer states.
- While Trouble is open over Normal, Select, or Insert, the terminal cursor is
  hidden and the selected-row carrier remains visible; closing or jumping from
  Trouble restores the normal screen's cursor behavior.
- Public API guards and relevant golden/snapshot fixtures reflect removal of
  the superseded role and addition of dashed boundaries, and `make check`
  passes with no warnings or errors.
