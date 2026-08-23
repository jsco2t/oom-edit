# Rendered-mode and shell usability refinements

## Objective

Resolve all usability findings in the 2026-08-22 review as one integrated change: stable launch presentation, a readable and aligned command palette, compact and unambiguous key help, clipboard-capable rendered link-index rows, tables that reflow to an 80-column floor, cursor-following horizontal navigation below that floor, and configurable mode-specific terminal cursor shapes.

## Current behavior

- The event loop calls `Terminal::draw` on every approximately 50 ms iteration, even when no visible state changed. Each frame is flushed without Crossterm synchronized-update framing, and queued resize events are handled one at a time between draws. A launch-time resize burst or a terminal painting a large frame incrementally can therefore appear as repeated full-screen repaints.
- The command palette uses different hard-coded description widths for executable and reference rows, styles enabled rows with `Style::default()`, and renders every reference or disabled row with the very subdued general-purpose `Muted` semantic style. The supplied screenshot shows the resulting column drift and poor contrast.
- The bottom hint bar renders every registry row independently as `<binding>=<long description>`. Space-prefix commands repeat `Space`, and the Help row's shared description produces the misleading `Space h=help / command palette` text. Search and Command-mode keys are not represented in the quick-bar registry projection.
- The rendered link index is deliberately synthetic and source-less. Its rows have no typed interactive target, so source-backed selection/yank logic cannot act on them. Enter only reports destinations on original inline-link jump targets.
- Table columns are independently sized to their natural content with a 40-cell per-column cap. Total width is not budgeted against the rendered viewport, so ordinary multi-column tables can remain around 120 cells wide.
- `TabEntry.left_col` and horizontal follow logic apply only to non-wrapped source Insert mode. Rendered screens always paint from column zero and have no per-tab horizontal viewport state.
- Configuration controls source wrapping but not cursor shapes. The shell never sends `SetCursorStyle`; rendered modes use only painted semantic cursor/selection carriers, while Insert and Command expose Ratatui's terminal cursor with the terminal's existing shape.

## Proposed implementation

1. Refine the registry metadata used by compact hints, group Space-prefix quick actions into one bracketed chord, add registry-owned standalone `/` search and `:` command hints, and give the palette a single responsive column layout plus explicit theme slots for its surface, readable text, secondary/reference text, and selected row.
2. Attach the existing typed `TargetKind::Link` metadata to each generated link-index row while keeping every generated glyph source-less. Resolve `y` and Enter on a synthetic link-target row inside `EditorSession`, emit the existing owned clipboard effect with the exact destination, and keep OSC 52 handling in the TUI.
3. Pass the renderer-neutral available width into table layout. Cap natural cell widths as today, then deterministically shrink the widest columns until the complete box fits `max(available_width, 80)` display cells, wrapping cell content without damaging source atoms.
4. Add a separate per-tab rendered horizontal offset in `App`. Make rendered scroll-follow adjust it from the core's display-cell cursor column, and make the screen adapter horizontally crop the styled line while leaving the gutter fixed. Apply this to all rendered surfaces so Normal navigation exposes wide table content and Select/Command remain geometrically consistent.
5. Add `[editor].cursor_shapes` (default `true`). Map Normal to a steady block, Insert and Command to a steady bar, and Select to a steady underscore; `false` maps all four modes to a steady block. Keep the semantic cursor/selection carriers as accessibility fallbacks, position the real terminal cursor at the active rendered point when visible, and restore the terminal's user-default cursor shape on every cleanup path.
6. Present frames only after startup or an actual event/timer/idle-work change, coalesce a queued resize burst before repainting, and bracket each terminal paint with Crossterm's synchronized-update commands. Preserve the existing post-read timestamp rule, spell-work budgets, modal routing, and cleanup guarantees.

## Architectural decisions

- `oom-edit-core` remains terminal-free. It owns table layout, rendered navigation, link-index targeting, and the decision to request a clipboard write. Crossterm cursor commands, synchronized updates, terminal event batching, and horizontal screen cropping remain in `oom-edit`.
- Link-index rows remain synthetic: their displayed marker, padding, and destination glyphs receive no source byte ranges. Interactivity reuses the existing project-owned `TargetKind::Link(index)` plus the row's `LineKind::Synthetic`, both attached at layout time; it is never inferred later by matching rendered strings and does not expand the public API.
- Copying a link-index row copies only its destination string, not the generated `[n] ` marker. `y` in Select follows the existing yank convention and returns to Normal; Enter copies without changing the current mode. Non-link source selections keep their current Vim-backed behavior.
- The 80-column table floor is measured in display cells within the renderer-neutral content surface. It is local to table blocks: prose, lists, metadata, and code continue to receive the true viewport width and reflow normally below 80 columns.
- Horizontal state is host-owned and per tab. The core continues to receive only width and to expose renderer-neutral display coordinates; it does not gain terminal geometry or TUI scroll state.
- The command registry remains the sole binding source. Compact hint labels are additional presentation metadata on the same registry rows, not a second command list. Palette alignment is computed by one pure layout function shared by command and reference rows.
- Palette state differences always have a modifier or glyph carrier in addition to color. Dedicated palette UI slots are completed for every built-in theme and capability tier and covered by completeness/contrast tests.
- Cursor-shape configuration is a TUI concern because it controls terminal presentation, not editing behavior. No cursor or Crossterm type crosses the core API.
- Frame synchronization uses the already-vendored Crossterm API and adds no dependency. The end-synchronization and cursor-reset sequences are included in ordinary, panic, and fatal-signal cleanup.
- The existing public `TargetKind`/`LineKind` model is sufficient for link-index interaction. The curated crate-root API is expected to remain unchanged and its compile guards must prove that no implementation or third-party type leaks out.

## Task sequence

1. [Task 001: Clarify command discovery surfaces](tasks/001-command-discovery-surfaces.md)
2. [Task 002: Make rendered link-index rows copyable](tasks/002-copyable-link-index.md)
3. [Task 003: Fit rendered tables to an 80-column floor](tasks/003-responsive-table-floor.md)
4. [Task 004: Follow the rendered cursor horizontally](tasks/004-rendered-horizontal-follow.md)
5. [Task 005: Add configurable mode cursor shapes](tasks/005-mode-cursor-shapes.md)
6. [Task 006: Stabilize terminal frame presentation](tasks/006-stable-frame-presentation.md)

## Quality gate

Every task first runs its focused validation and then all standard commands from `gate.json`: `make fmt-check`, `make lint`, `make build`, `make test`, and `make check`. This deliberately preserves the repository's explicit Definition of Done even though `make check` repeats formatting, lint, build, and test internally and additionally runs deny, audit, and bundled-data license checks. Final acceptance reruns that complete list and adds `make build-release` because the launch report explicitly covers both build profiles.

No dependency change is expected. If implementation unexpectedly requires one, the approved plan no longer covers the supply-chain work; the workflow must stop with `PLAN_CHANGE_REQUIRED` rather than adding it silently.

## Risks

- Table shrinking must count terminal display cells, not bytes or Unicode scalar values. Wide and combining characters can otherwise break box alignment or provenance.
- Horizontal cropping must keep selection, diagnostic, search, cursor, and semantic-style columns aligned after the offset, including at wide-character boundaries.
- Synthetic link-index interaction must not cause generated text to acquire false source ownership or let destructive source operators target a synthetic row.
- Reusing `TargetKind::Link` for original inline links and generated index rows requires an explicit `LineKind::Synthetic` check so Enter keeps its existing informational behavior on the original source-backed target.
- Palette contrast differs by dark, light, 16-color, and monochrome tiers. A readable default-dark screenshot alone is insufficient.
- A redraw-on-change event loop can miss which-key appearance, transient expiry, diagnostics, or resize repaint if its invalidation result is incomplete. Deterministic scheduler tests must cover each source.
- Synchronized-update mode must always be ended after draw errors, panic restoration, and signal restoration so a terminal cannot remain visually frozen.
- Some terminals ignore cursor-shape and synchronized-update escape sequences. The semantic cursor carriers and ordinary Ratatui rendering remain functional fallbacks.

## Out of scope

- Opening links in a browser or adding network access.
- Copying generated link markers, borders, padding, or other source-less decorations as editable Markdown.
- Mouse-driven horizontal scrolling or a persistent horizontal scrollbar.
- User-defined arbitrary cursor-shape strings beyond the requested enable/disable setting and the documented mode defaults.
- Replacing OSC 52, adding a clipboard crate, changing Markdown syntax support, or adding a dependency.
- Redesigning overlays other than the command palette or introducing a second command/binding registry.

## Final acceptance criteria

- [ ] Launching debug or release builds with a document path presents each complete frame atomically, does not repaint unchanged idle frames, and coalesces queued resize bursts before repainting; deterministic terminal/event-loop tests cover the behavior and cleanup.
- [ ] The command palette uses readable theme-owned styles and identical responsive column boundaries for executable, disabled, and reference rows at wide and minimum supported geometries, with selected state distinguishable without relying on color alone.
- [ ] The bottom bar uses one compact description per binding, groups Space chords unambiguously, represents `/` as search and `:` as Command mode, and never partially renders a hint group when width is insufficient.
- [ ] A focused rendered link-index row copies the exact destination through the system clipboard effect on `y` and Enter in Normal and Select, reports clipboard success/failure, and remains entirely source-less in provenance data.
- [ ] Rendered tables whose natural width exceeds the available surface shrink and wrap to at most 80 display cells when the available width is 80 or narrower, while wider surfaces use their available budget and all border rows remain aligned.
- [ ] Moving the rendered cursor horizontally brings clipped table content into view, keeps the cursor within the configured horizontal scroll margin, preserves gutter position and styled overlays, and maintains independent offsets per tab.
- [ ] With default configuration, terminal cursor shapes are block in Normal, bar in Insert and Command, and underscore in Select; setting `editor.cursor_shapes = false` uses block in all four modes, and cleanup restores the user's default cursor shape.
- [ ] Existing source editing, rendered selection/operators, resizing, tab switching, spell work, modal input routing, public API boundaries, source provenance, and terminal restoration continue to pass their regression tests.
- [ ] README configuration guidance and the Unreleased changelog describe the user-visible behavior without legacy mode names.
- [ ] No dependency is added, and every command in `gate.json.final_commands` passes without warnings or errors.
