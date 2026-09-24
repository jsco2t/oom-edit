# Bug Report Batch — September 23, 2026

## Objective

Resolve all nine reported editing, wrapping, front-matter, spell-check, and
rendered-list defects with regression-first automated coverage, while
preserving the reusable-core/thin-TUI boundary and the existing source
provenance, table-width, and no-stale-diagnostic invariants.

## Current behavior

- Source Insert correctly advances the canonical Vim cursor after Enter, but
  `render_source` obtains no highlighted row for the final empty logical line.
  It therefore leaves the terminal cursor at its `(0, 0)` default even while
  the document grows and the canonical cursor moves to subsequent lines. A
  direct probe with `# foo` reproduced canonical positions `(1, 0)`, `(2, 0)`,
  and `(3, 0)` while every source frame still reported screen position `(0, 0)`.
- Source wrapping uses `wrap_source_line`, an intentionally character-by-
  character hard wrapper. It splits ordinary words, can expose separator
  whitespace at the beginning of a continuation, and always uses the terminal
  text width. Rendered wrapping also uses the complete terminal text width.
  Configuration has only a boolean `editor.wrap`; there is no line-limit
  setting.
- Rendered tables already enforce `TABLE_WIDTH_FLOOR = 80`, and the TUI already
  has horizontal rendered scrolling. The wrapping change must retain the real
  viewport width for cropping and table scrolling while using the configured
  line limit only as the prose/source layout width.
- Rendered `h`/Left and `l`/Right navigation call `horizontal_point`, which
  searches only the current rendered row and clamps at its first/last
  source-backed atom. It cannot cross from the line below a wrapped paragraph
  to the paragraph's final visual row.
- The Markdown block model correctly marks blank-line-separated lists as
  loose. `render_block` explicitly discards the `tight` field, so rendered
  Normal mode removes the inter-item blank separation.
- There is no command or session operation for inserting a default front-
  matter block. App-owned Space commands are sourced from the static command
  registry and are the established place for the requested key combination.
- Spell work starts after 150 ms of input idleness. When a new edit arrives
  while a scan is already dirty or scanning, `SpellState::invalidate` clears
  every published diagnostic, causing unrelated underlines to disappear until
  the replacement scan finishes.

## Proposed implementation

1. Repair source-frame handling for terminal empty logical rows and replace
   ordinary source hard wrapping with a Unicode-width-aware, source-preserving
   word-boundary wrapper. Add `editor.wrap_width`, defaulting to 100 columns,
   and thread it through App-owned source and rendered layout calculations.
   Keep the physical viewport width separate so tables retain their 80-column
   floor and horizontally scroll only when the screen is narrower than their
   rendered width.
2. Make rendered horizontal motion traverse the ordered source-backed display
   atoms across rendered-row boundaries, including wrapped paragraph rows,
   while continuing to skip synthetic decorations and source-less padding.
3. Honor the list model's `tight`/`loose` decision by emitting a source-less
   blank presentation row between sibling items of a loose list, including
   nested cases, without changing tight-list output.
4. Add a core `EditorSession` operation that atomically prepends the uniform
   YAML template `---\ntitle: \"\"\n---\n\n` only when no YAML or TOML front
   matter is already present. Preserve the existing document bytes and
   canonical cursor target, refresh all derived caches through `LiveDocument`,
   and expose the operation through the curated crate-root session facade.
   Register `Space m` (`m` for metadata) as the sole TUI command declaration
   and project it into dispatch, which-key, hints, and the palette.
5. Raise the spell idle delay to five seconds and make repeated locally-safe
   invalidations retain and shift diagnostics that remain provably valid
   outside the edited line. Abort/restart obsolete pending scan work without
   publishing its partial results; continue clearing diagnostics when a full
   context-changing invalidation means they cannot be trusted.

## Architectural decisions

- Text remains owned only by `LiveDocument`/`VimCore`. The empty-line repair is
  a source-frame projection fix; the front-matter insertion is one atomic
  mutation through `LiveDocument`, not a second text copy or App-side rewrite.
- `editor.wrap_width` is a positive display-column limit with a default of 100.
  The effective prose/source layout width is `min(available_text_width,
  wrap_width)`. The available terminal width remains the crop/scroll viewport,
  which is essential for tables wider than narrow terminals.
- Wrapping remains presentation-only. It never inserts or deletes Markdown
  bytes. Source segments preserve exact order, UTF-8 boundaries, styles, and
  concatenation; an individual token wider than the limit may still be hard-
  split because no word boundary exists.
- Horizontal rendered motion operates on existing mapped atoms and never
  manufactures provenance for borders, list markers, blank rows, or padding.
- Loose-list separation is synthetic and source-less. The parser's existing
  `tight` flag is the source of truth; no duplicate blank-line parser is added
  to the renderer.
- The default front-matter command is Normal-mode only and is declared once in
  `command::COMMANDS`. Existing valid or malformed leading front matter counts
  as present and prevents insertion. The default is deterministic and contains
  no clock-derived fields.
- Published spell diagnostics remain conservative: only diagnostics proven
  outside a locally invalidated range survive. Context-changing edits still
  invalidate all potentially stale results.
- No external dependencies are required.

## Work included

1. Trailing empty source lines display the canonical cursor on the correct
   line for new documents without front matter.
2. Source text wraps at word boundaries when a boundary exists, preserves raw
   Markdown exactly, and avoids a continuation that begins with the delimiter
   whitespace from an ordinary prose wrap; overlong indivisible tokens remain
   the only hard-break case.
3. Wrapping defaults to 100 columns and is adjustable with
   `[editor] wrap_width`; configuration parsing, defaults, startup wiring,
   rendering, scrolling, pointer mapping, README documentation, and tests are
   updated together.
4. The existing 80-column table floor and horizontal scrolling on narrower
   screens remain intact under the new wrap limit.
5. Rendered Left/`h` reaches the final source-backed atom of the preceding
   visual row from the start of the following row. Counted and symmetric
   Right/`l` boundary behavior is covered to prevent an asymmetric navigation
   state machine.
6. `Space m` inserts the exact default YAML front-matter template once, and a
   repeat invocation or any pre-existing YAML/TOML block leaves the document
   unchanged with an informative result.
7. Spell scanning starts only after five seconds without input, and unrelated
   published underlines remain visible through repeated ordinary typing while
   the edited range is rescanned.
8. Loose bulleted and ordered lists preserve one blank rendered row between
   sibling items; tight lists and nested marker/indent behavior do not change.
9. Unit, integration, registry-drift, configuration, public-API, rendering,
   event-loop timing, and snapshot/golden coverage is added or updated for the
   changed behavior. README and the Unreleased changelog describe the new
   setting and command.

## Task sequence

1. `tasks/001-source-cursor-and-wrapping.md`
2. `tasks/002-rendered-horizontal-navigation.md`
3. `tasks/003-loose-list-spacing.md`
4. `tasks/004-default-front-matter-command.md`
5. `tasks/005-spell-idle-and-stable-diagnostics.md`

## Quality gate

Both the standard per-task gate and final package gate run `make check`, the
repository's build-system-of-record aggregate. It executes `fmt-check`, Clippy
with `-D warnings`, the workspace build, the complete Rust/Python test suite,
`cargo deny`, `cargo audit`, and bundled-data license validation. Thus one
recorded command supplies evidence for every non-negotiable definition-of-done
check without duplicating the same expensive suite as separate commands.

Task documents also name focused offline Cargo test filters used while
developing each repair. Those are ad-hoc focused checks; `make check` remains
the authoritative gate. No dependency changes are planned, but the aggregate
still runs the supply-chain gates.

## Risks

- Word-boundary source wrapping must preserve exact character/style ordering,
  tabs, wide Unicode scalars, combining suffixes, and cursor/source mappings.
  A visually attractive wrapper that drops delimiter spaces would corrupt
  source projection.
- Confusing configured layout width with actual viewport width could break
  table cropping, horizontal cursor follow, mouse hit-testing, or empty space
  to the right of the configured limit.
- Cross-row horizontal motion must skip source-less rows without jumping to an
  unrelated block or losing count semantics.
- Loose-list padding can accidentally duplicate block-level blank rows or
  inherit list prefixes/provenance if inserted at the wrong rendering layer.
- Prepending front matter must be one undoable edit, preserve the original
  body and cursor target, and leave selections/modes in a valid state.
- Retaining spell diagnostics during pending work is safe only when edit-range
  analysis proves they still describe unchanged text. Broader retention would
  violate stale-diagnostic guarantees.

## Out of scope

- Reflowing or hard-wrapping Markdown on disk.
- User-selectable front-matter templates, TOML as the generated default,
  automatic dates, or schema validation beyond the existing parser.
- Making the five-second spell delay configurable; the report requested a
  longer delay and suggested five or seven seconds, so this package chooses
  five.
- Changing table cell allocation, the 80-column table floor, or table Markdown
  parsing.
- Adding new Markdown constructs, changing the authoritative Markdown spec, or
  refactoring unrelated Vim motions and rendering code.

## Final acceptance criteria

- Starting with an empty document, entering `# foo`, and pressing Enter one or
  more times places both the canonical and terminal source cursor on each newly
  created empty line; the regression is automated without requiring front
  matter.
- At an available width above 100, ordinary prose wraps no later than column
  100 by default and at the configured positive `editor.wrap_width` when
  overridden. Words are not split when a valid boundary exists, raw source
  bytes remain unchanged, and repeated separator input at an ordinary prose
  boundary does not create a spurious leading continuation pad.
- A terminal narrower than 80 columns renders tables at no less than 80
  columns and permits horizontal cursor/viewport movement across their full
  width.
- In rendered Normal and Select, Left/`h` at the first source-backed atom of a
  row moves to the last source-backed atom of the preceding applicable row;
  Right/`l` and counts behave consistently in the opposite direction.
- `Space m` appears through registry-derived command discovery, inserts
  `---\ntitle: \"\"\n---\n\n` before an existing body as one mutation only
  when front matter is absent, preserves the body and logical cursor target,
  and refuses a second insertion or a document already beginning with YAML or
  TOML front matter.
- Spell work does not begin before five seconds have elapsed since the last
  input. During repeated locally-safe typing, diagnostics outside the edited
  line remain published and decorated; completed rescans converge to the same
  diagnostics as a fresh full scan.
- Blank-line-separated list items retain one blank row in rendered output,
  while tight lists, nesting, markers, provenance, and wrapping remain correct.
- README and Unreleased changelog entries describe `editor.wrap_width`, the
  default value, `Space m`, and the behavior changes.
- Every task-specific validation passes, every task records evidence, and the
  final `make check` completes with no warnings, errors, test failures,
  advisory warnings, or license failures.
