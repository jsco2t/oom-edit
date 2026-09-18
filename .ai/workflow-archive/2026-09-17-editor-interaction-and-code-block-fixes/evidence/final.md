# Final Work-Package Evidence

## Work package

- ID: `2026-09-17-editor-interaction-and-code-block-fixes`
- Title: Editor interaction and code block fixes
- Acceptance round: 3

## Original objective

Make help and palette actions consistent across rendered modes, render tab-indented code correctly, support safe reload and `:tabnew` paths, and provide process-local command history. Acceptance follow-ups restored full palette navigation, added editable ex-command handoff, and made tab indentation correct in source Insert mode as well as rendered Normal.

## Completed tasks

1. Help in Select and palette actions — [001.md](001.md)
2. Render code tabs — [002.md](002.md)
3. Tabnew paths and paste — [003.md](003.md)
4. Reload commands — [004.md](004.md)
5. Session command history — [005.md](005.md)
6. Palette list navigation — [006.md](006.md)
7. Palette ex-command handoff — [007.md](007.md)
8. Source Insert code tabs — [008.md](008.md)

The earlier final reviews remain in [final-round-001.md](final-round-001.md) and [final-round-002.md](final-round-002.md).

## Whole-package review

Reviewed the complete request, approved plan, all eight tasks and their evidence, and the cumulative source diff from baseline `b1044df098eaa76b5d288ed7accb0d613d345c29`. The new Insert-mode projection is confined to `oom-edit-core`, uses the same literal source bytes as editing and saving, and retains exact byte ownership for expanded tab cells. App uses core display geometry only for scrolling tabbed source lines; no-tab behavior and the existing rendered-code path remain intact. The full suite, including rendered code, palette, lifecycle, public API, dependency hygiene, and performance tests, passed.

## High-confidence findings fixed

- Task review removed repeated prefix scans from wrapped source-atom mapping.
- Task review fixed a narrow horizontal viewport where the left clipping marker could cover the cursor immediately after a tab.
- Final integrated review found no further high-confidence defects.

## Final acceptance criteria

1. Normal and every Select shape open help with `?`; enabled App actions remain identifiable and executable. Tasks 001, 006, and 007 cover the final focus and Enter behavior.
2. Rendered Go and other code fences use four-column tab stops with unchanged source bytes and exact provenance. Task 002 covers this.
3. `:tabnew` accepts absolute and launch-directory-relative paths, including spaces and `s/`, and validates pasted ASCII paths atomically. Task 003 covers this.
4. `:e!`, `:reload`, and atomic `:reload-all` follow safe lifecycle handling across file-backed tabs. Task 004 covers this.
5. Up/Down traverse ten process-local command submissions across tabs and restore drafts; Escape does not submit. Task 005 covers this.
6. Up/Down and Tab/BackTab reach every filtered palette row below the initial viewport while retaining visible focus and boundary clamping. Task 006 covers this.
7. App, editable ex, disabled, and read-only rows have distinct visible focus and Enter behavior. Tasks 006 and 007 cover this.
8. Enter on `:wq` opens a prefilled core Command prompt without executing; the second Enter submits, while Esc cancels. Task 007 covers this.
9. Argument-taking ex rows prefill editable literal prefixes without placeholders, then use existing core parsing, lifecycle, and history. Task 007 covers this.
10. In Insert mode, the kitchen-sink Go lines show one- and two-tab indentation at four-column stops. Task 008 covers core frames and drawn TUI cells.
11. Source tabs after spaces, other tabs, and wide Unicode use four-column stops while canonical and saved bytes retain tabs. Task 008 covers this.
12. Insert-mode wrap, nowrap, clipping, cursor, scroll-follow, hit mapping, highlighting, and decorations stay aligned around tabs, including narrow widths. Task 008 covers this.
13. Every task gate and the integrated package gate passed `make check`.

## Final quality gate

| Command | Result |
| --- | --- |
| `make check` with the writable local advisory cache and offline scanner flags recorded in Task 001 evidence | PASS: 7 checks, 0 failures, no warnings |

## Cumulative diff

25 tracked source, test, and snapshot files changed from the baseline; the workflow plan and evidence were added under `.ai/workflow/`. No dependency, lockfile, vendor, or Makefile change. The worktree remains uncommitted.

## Remaining non-blocking concerns

None
