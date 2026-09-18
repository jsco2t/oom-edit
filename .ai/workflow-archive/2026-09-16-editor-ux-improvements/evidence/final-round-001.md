# Final Work-Package Evidence

## Work package

- ID: `2026-09-16-editor-ux-improvements`
- Title: Editor UX improvements

## Original objective

Show theme-aware diagnostic dots and a matching spelling status symbol, tighten gutter and status spacing, make pointer navigation/selection and wheel scrolling usable, and open the command palette with `?` and Space+`?`.

## Completed tasks

- 001 — Severity indicators: [evidence](001.md)
- 002 — Compact terminal spacing: [evidence](002.md)
- 003 — Core pointer position and selection: [evidence](003.md)
- 004 — TUI pointer interaction: [evidence](004.md)
- 005 — Palette question-mark shortcuts: [evidence](005.md)

## Whole-package review

Reviewed the original request, approved plan, all task documents and evidence, cumulative diff from `a9e503b231a9dbdb71949387c98a3dfc7be5fd0c`, updated snapshots, public API boundary, pointer state transitions, registry projections, and the local `hjkl-engine` source delta. The core remains terminal-neutral. No stubs or deferred acceptance work remain.

## High-confidence findings fixed

- A source drag anchor could use an old rendered position after switching to Select changed the text width. The anchor now follows the remapped selection after paint; a narrow relative-number test covers it.
- Command and search prompts could allow mouse events to mutate the document. Pointer input now respects prompt ownership, with focused tests.
- The randomized conformance suite found a pinned engine helper that split a multibyte Unicode line ending on Insert exit. A local source patch converts at character boundaries, and both a deterministic NEL regression and the saved property case pass.
- The palette's TUI reference for `?pattern` was stale once bare `?` became a palette shortcut. The reference was removed; the core's backward-search behavior remains covered for embedding hosts.

## Final acceptance criteria

- Themed warning/error `●` gutter markers and a non-color error distinction: Task 001 tests and evidence.
- Matching themed boxed spelling symbol, neutral zero state, stable count: Task 001 tests and evidence.
- One blank rightmost status cell in ruler, prompts, and hints at narrow widths: Task 002 exact-cell tests and evidence.
- Compact gutter with one content gap, stable marker geometry, relative numbers: Task 002 geometry tests and snapshots.
- Source-backed click mapping after scroll and in Insert with wrap, horizontal offset, and Unicode: Tasks 003–004 integration and App tests.
- Character Select drag with exact source ranges, retained selection on release, modal isolation: Tasks 003–004 tests and final-review prompt test.
- Wheel movement updates viewport and cursor through actual rows and mode changes: Task 004 boundary and mode-change tests.
- Bare `?`, Space+`?`, Space+`h`, `/`, and registry-derived hints/palette: Task 005 routing, projection, and snapshot tests.
- Full gate after each task and integrated package: task evidence and final `make check` below.

## Final quality gate

| Command | Result |
| --- | --- |
| `make check` | PASS — fmt-check, lint, build, full test suite, deny, audit, data-license-check |

`git diff --check` and `rustfmt --check patches/hjkl-engine/src/rope_util.rs` also pass.

## Cumulative diff

From baseline `a9e503b231a9dbdb71949387c98a3dfc7be5fd0c`: core pointer facade and provenance-aware mapping; App gesture/wheel routing; severity and status theme presentation; compact layout; registry-backed `?` aliases and snapshots; a documented, exact-version local `hjkl-engine` patch replacing its generated vendor copy. `Cargo.lock` and `vendor/` were regenerated offline. No third-party package or version was added.

## Remaining non-blocking concerns

None
