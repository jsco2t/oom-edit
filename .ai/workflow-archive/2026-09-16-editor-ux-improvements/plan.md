# Editor UX improvements

## Objective

Make spelling diagnostics clearer and theme-aware, tighten the gutter and status-row spacing, make pointer navigation and selection usable, and open the command palette with `?` in Normal mode and Space+`?` wherever Space+`h` works.

## Current behavior

- `gutter::marker_style` renders severity letters (`E`, `W`, `I`, `H`), while the gutter renderer already gets warning/error colors from theme UI slots. The status bar builds the boxed `🅂` and count as one dimmed span, so the symbol cannot carry the gutter color independently.
- `status_bar::gutter_width` reserves a three-digit minimum, a sign/marker column, and one trailing cell. The same width is used by source and rendered screens, but the extra minimum width makes short documents wider than needed. The status ruler reaches the last terminal column.
- App handles only wheel up/down. Other mouse events are dropped. Wheel scrolling changes the viewport while leaving the canonical and rendered cursors at their old locations. The next scroll-follow transition can jump the view back.
- Rendered layout atoms already pair display-cell columns with exact source byte ranges; synthetic glyphs have no source range. `EditorSession::jump_to_offset` moves the canonical cursor, but there is no session facade for direct rendered-point positioning or a drag gesture. Source Insert uses a separate viewport with wrapping, horizontal offset, and skipped visual rows.
- In the TUI, Space+`h` dispatches `AppCommand::Help`, which opens the command palette. Bare `?` reaches core's backward rendered-search prompt. The core conformance suite covers that backward-search behavior, so the TUI binding must be explicit and tested without accidentally changing unrelated core search semantics.

## Proposed implementation

1. Replace gutter severity letters with the single-cell `●` glyph. Keep severity-specific theme colors and use a distinct modifier for error so severity is not conveyed by color alone. Build the spelling status as separately styled symbol and count spans, deriving the symbol role from the highest displayed spelling severity; use a neutral style for zero issues.
2. Make the gutter width depend on actual line-number and relative-sign needs, keeping one terminal-cell gap after the final digit and a stable marker position. Remove any redundant presentation padding at the gutter/document seam without removing intentional Markdown indentation. Reserve one terminal cell at the right edge of the status row for ruler, hints, and prompts, including narrow-terminal cases.
3. Add narrow terminal-neutral `EditorSession` operations for positioning the cursor at a rendered display point, mapping a source viewport point to a source cursor, and beginning/updating a character selection from rendered points. Reuse existing source atoms and selection projection. Define deterministic nearest-source behavior for synthetic cells, wide glyphs, end-of-line, and out-of-bounds coordinates. Extend the curated core public API guards and focused mapping tests.
4. Translate crossterm pointer coordinates once in App using the same body, gutter, scroll, wrap, and horizontal-offset geometry as rendering. A primary-button press moves the cursor; drag establishes a character Select anchor and updates the endpoint; release retains the selection. Mouse wheel movement shifts both viewport and cursor by the actual visible row delta, retaining the cursor's screen-relative row when possible. Clear gesture state on release, modal takeover, tab change, and other interrupted gestures. Preserve modal exclusivity. Exercise these flows with App event tests and rendered/source fixtures.
5. Declare palette shortcut metadata in the single command registry. Route bare `?` in Normal mode and Space+`?` in the existing rendered Space contexts to `AppCommand::Help`; keep Space+`h` working. Update the palette, quick hints, which-key continuations, snapshots, and binding completeness tests from the registry projections. Keep `/` search available.

## Architectural decisions

- The core remains free of crossterm, ratatui, screen rectangles, mouse-event types, and theme colors. App converts screen positions to renderer-neutral points and viewport inputs; the core owns source mapping and selection state.
- Pointer movement uses the existing `EditorSession` facade and its live document, rendered cursor, and selection state. It must not create a second mutable text owner or mutate derived caches independently.
- Rendered pointer mapping uses parser-attached source atoms. Synthetic output has no invented source ownership; nearest-source fallback is explicit and deterministic.
- New gesture state is one closed enum, not separate anchor/drag booleans. Overlays and confirmations exclusively own input.
- The spelling indicator and gutter use the same theme slots at each capability tier. Error gets a non-color modifier difference from warning. No direct colors enter core.
- Registry rows remain the source of truth for the new palette shortcuts. A direct Normal-mode `?` is an explicit TUI command shortcut; the core's generic backward search remains available to non-TUI hosts and continues to be covered by its conformance suite.
- No new dependency, developer command, or make target is expected.

## Work included

1. Severity dot in the gutter, themed status spelling glyph, and tests for warning/error/zero/monochrome behavior.
2. Compact gutter geometry and one-cell status right padding, with exact-cell tests and updated snapshots.
3. Core cursor and character-selection operations for rendered and source viewport positions, including UTF-8 and synthetic content tests.
4. TUI click, drag, and wheel behavior in Normal, Select, and Insert, including scroll-follow and modal regression tests.
5. Bare `?` and Space+`?` palette shortcuts, with registry and visual projection tests.

## Task sequence

1. [001 — Severity indicators](tasks/001-severity-indicators.md)
2. [002 — Compact terminal spacing](tasks/002-compact-terminal-spacing.md)
3. [003 — Core pointer position and selection](tasks/003-core-pointer-position-selection.md)
4. [004 — TUI pointer interaction](tasks/004-tui-pointer-interaction.md)
5. [005 — Palette question-mark shortcuts](tasks/005-palette-question-shortcuts.md)

## Quality gate

Each task runs `make test` as its task-specific behavior validation and `make check` as the standard gate. The package repeats `make check` after integrated review. `make check` is the repository's full CI gate: it runs formatting, Clippy with warnings denied, build, the full test suite, license/advisory checks, and bundled-data license verification. The existing Makefile supplies every needed workflow; no validation class is omitted.

## Risks

- A terminal cell is the smallest reliable horizontal unit. For a three-digit number with a visible diagnostic, the marker, digits, and one-cell content gap already require five cells. Compaction can remove unused minimum-width space for shorter line counts and redundant seam padding, but cannot make that five-cell case narrower without hiding information or removing the requested gap.
- The rendered surface may contain synthetic rows and source-less glyphs, tables, wrapped text, wide Unicode, and horizontal scrolling. Hit testing must select an exact source-backed atom or a documented nearest-source position without fabricating byte ownership.
- Source Insert wrapping and horizontal scroll use different coordinates from rendered Normal. A click after wheel scrolling and a drag that changes mode must keep the currently viewed content visible.
- The new TUI `?` shortcut intentionally supersedes backward search in Normal mode. `/` remains the visible search entry point; core's backward-search conformance is unchanged for embedders.
- Snapshot changes can be broad because gutter and right-edge layout affect many screens; accept only changes explained by the new geometry.

## Out of scope

- New editor modes, alternate selection shapes for dragging, multi-click word/line selection, clipboard-on-selection, pointer interaction with tabs or overlay controls, and configurable mouse scroll speed.
- A new spelling provider or changed diagnostic classification. Existing warning/error severity is only presented more clearly.
- New dependencies or terminal escape protocols.

## Final acceptance criteria

- Warning and error gutter diagnostics display `●` at a stable position in source and rendered views, with theme-selected severity colors and a non-color error distinction. No severity letter remains as the gutter marker.
- The boxed spelling status glyph uses the matching active-theme severity role when diagnostics exist, stays legible at every supported color tier, and displays a neutral state for zero issues; its count and positioning remain correct.
- The bottom status row has one blank terminal cell at the right edge for the ruler, command/search prompts, and ordinary hints. Narrow widths render without overflow or panic.
- The gutter uses only the space needed for the visible number/sign and marker, and exactly one cell separates its last digit from unindented document text. Relative numbers and diagnostics do not shift document text when the marker appears.
- Clicking a visible document position after wheel scrolling moves the cursor to the corresponding source-backed content; clicks in source Insert also move the cursor correctly with wrap, horizontal offset, and Unicode.
- Dragging from a document position creates or updates a rendered character selection, switches to Select mode, preserves the selected source ranges, and leaves the selection visible on release. Modal screens do not pass mouse input to the document.
- Wheel scrolling moves the cursor with the viewport through the actual scrolled rows in rendered and source views. Changing mode after a long pointer scroll does not snap back to the old cursor location.
- Bare `?` in Normal and Space+`?` in rendered Space contexts open the same command palette as Space+`h`; `/` still starts search, and registry-derived help/hints accurately expose the shortcuts.
- `make check` passes after each task and for the integrated package.

## Acceptance follow-up — Round 2

### Observed failure and cause

The follow-up screenshot shows the `●` marker pressed against the left window edge and line number. Task 002 reserved exactly one marker cell and one trailing content-gap cell. In `status_bar::gutter_width`, a three-digit absolute gutter is therefore five cells (`1 + 3 + 1`); `render_gutter` paints the marker in cell zero. Both source and rendered text begin at that width, and App uses it for pointer hit testing.

Terminal layout has integral character cells, so adding literal blank cells to both sides of `●` would increase the gutter width. The user clarified that the existing trailing cell is necessary visual separation between the number and content, superseding the earlier round-two no-gap proposal. For this round, use a visually smaller centered circular bullet, `•`, whose ink has side space inside its one terminal cell. This explicitly supersedes the round-one requirement to use the exact `●` code point in the gutter while preserving a circular severity marker. The separate status spelling symbol remains unchanged.

### Fix strategy and architecture

1. Change the gutter marker glyph to `•`, preserving the existing theme-derived severity roles and non-color error modifier. Keep the marker in the leading single cell; do not add whole-cell spacers around it. Check the result visually in the same terminal presentation as the supplied screenshot because font metrics determine the exact side bearing.
2. Retain `GUTTER_CONTENT_GAP = 1` and the existing width calculation. A three-digit absolute gutter stays five cells: marker, three digits, one separator. Keep the number, separator, and document text at their current columns whether or not a diagnostic is present.
3. Extend exact-cell tests to lock down the smaller marker's position, themed styles, and retained separator in source and rendered views. Update only snapshots affected by the glyph. Existing viewport, cursor, and pointer geometry must remain unchanged.

The core remains terminal-neutral. Only TUI presentation and its tests change. No new dependency or build command is needed. `gate.json` remains `make check` for both task and final gates.

### Task sequence

6. [006 — Gutter visual spacing follow-up](tasks/006-gutter-visual-spacing.md)

### Risks and boundaries

- Exact pixel padding depends on the user's terminal font and cannot be guaranteed by ratatui. The one-cell `•` supplies intrinsic side space in common monospaced fonts; the visual result must also be inspected against the reference screenshot.
- Retaining the separator means a marked three-digit gutter cannot be narrower than five cells without hiding the marker or a digit. This is the deliberate tradeoff for a clear gutter/document boundary.
- This round changes only the gutter marker's presentation. It does not change gutter geometry, diagnostics, theme colors, spelling status, mouse behavior, or keyboard bindings.

### Round 2 acceptance criteria

- The circular trouble marker has visible space to its left and right in a representative terminal rendering, and is themed exactly as the prior warning/error marker.
- A three-digit absolute gutter remains five cells: one marker cell, three number cells, and one blank separator. Unindented document text starts after that separator. Marked and unmarked lines use the same body start.
- Signed relative labels, number-width transitions, source and rendered views, narrow windows, cursor placement, and pointer click positions remain unchanged.
- Updated exact-cell tests and snapshots pass; `make check` passes for the task and whole package.
