# Final Work-Package Evidence

## Work package

- ID: `2026-09-17-editor-interaction-and-code-block-fixes`
- Title: Editor interaction and code block fixes
- Acceptance round: 2

## Original objective

Make help available in Normal and Select, render tab-indented code correctly, support reload commands and safe `:tabnew` paths and paste, and provide ten-entry session command history. The follow-up restores access to every help row and lets concrete ex-command rows open an editable, prefilled Command prompt.

## Completed tasks

1. Help in Select and palette actions — [001.md](001.md)
2. Render code tabs — [002.md](002.md)
3. Tabnew paths and paste — [003.md](003.md)
4. Reload commands — [004.md](004.md)
5. Session command history — [005.md](005.md)
6. Palette list navigation — [006.md](006.md)
7. Palette ex-command handoff — [007.md](007.md)

Round 1 final evidence is preserved in [final-round-001.md](final-round-001.md).

## Whole-package review

Reviewed the request, approved plan, seven tasks and their evidence, and the cumulative diff from baseline `b1044df098eaa76b5d288ed7accb0d613d345c29`. Full-row navigation now keeps the focused row visible in the palette viewport. Focus markers distinguish enabled App actions, editable ex references, read-only guidance, and disabled actions. Ex handoff uses explicit metadata and the core session facade; the first Enter never executes a command or writes history. The original code rendering, path, reload, and history paths remain covered by the full test suite. No dependencies or new command dispatcher were added.

## High-confidence findings fixed

- Restored navigation and scrolling to reference rows below the palette viewport after the round-1 App-only navigation regression.
- Replaced the inert Enter behavior on concrete ex rows with an editable core Command prompt; read-only and disabled rows stay inert.

## Final acceptance criteria

1. Plain `?` opens help in Normal and every Select shape. App actions are clearly marked and execute on Enter; all rows are reachable for reading. Tasks 001, 006, and 007 include registry, palette, snapshot, and App tests.
2. The kitchen-sink Go fence and other fenced or indented code expand tabs at four-column stops with unchanged source bytes, styling, and source mapping. Task 002 includes rendered-layout tests.
3. `:tabnew` accepts launch-directory-relative and absolute paths, including spaces and `s/`, and safely accepts printable ASCII bracketed paste. Task 003 includes core and App tests.
4. `:e!` and `:reload` reload the current file; `:reload-all` preloads all targets before changing any tab. Task 004 covers dirty, missing, unnamed, invalid UTF-8, and unreadable targets.
5. Up/Down traverse the last ten submitted commands across tabs, restore the draft, and leave history untouched on Escape. Task 005 covers process-local history.
6. Up/Down and Tab/BackTab focus every filtered palette row, including beyond 40×12 and 80×24 viewports, and clamp at both ends. Task 006 includes rendered-cell tests.
7. Enabled App actions, editable ex rows, read-only guidance, and disabled actions have distinct focus and Enter behavior in Normal and Select. Tasks 006 and 007 include modal routing tests.
8. A first Enter on `:wq` opens Command mode with `wq` prefilled without saving or quitting; a second Enter submits through core, while Esc cancels. Task 007 includes core and App tests.
9. Argument templates such as `:e {path}` and `:tabnew {path}` prefill literal editable prefixes without placeholders; submission uses the existing parser, lifecycle, and history. Task 007 covers both typed and pasted paths.
10. Every task gate and the integrated package gate passed `make check`.

## Final quality gate

| Command | Result |
| --- | --- |
| `make check` with the writable local advisory cache and offline scanner flags recorded in Task 001 evidence | PASS: 7 checks, 0 failures, no warnings |

## Cumulative diff

21 tracked source, test, and snapshot files changed from the baseline; the workflow plan and evidence were added under `.ai/workflow/`. No dependency, lockfile, vendor, or Makefile changes. The worktree remains uncommitted.

## Remaining non-blocking concerns

None
