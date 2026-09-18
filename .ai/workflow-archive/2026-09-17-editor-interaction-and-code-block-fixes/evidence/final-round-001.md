# Final Work-Package Evidence

## Work package

- ID: `2026-09-17-editor-interaction-and-code-block-fixes`
- Title: Editor interaction and code block fixes

## Original objective

Fix Select-mode help and actionable palette focus; render tab-indented Go and other code blocks correctly; support single and all-tab reloads; accept sanitized pasted and launch-directory-relative `:tabnew` paths; and provide ten-entry, process-local ex-command history.

## Completed tasks

1. Help in Select and palette actions — [001.md](001.md)
2. Render code tabs — [002.md](002.md)
3. Tabnew paths and paste — [003.md](003.md)
4. Reload commands — [004.md](004.md)
5. Session command history — [005.md](005.md)

## Whole-package review

Reviewed the original request, approved plan, every task and task evidence file, and the cumulative diff from baseline `b1044df098eaa76b5d288ed7accb0d613d345c29`. Input stays in the core command grammar; App owns modal routing, launch-directory resolution, and lifecycle I/O. Code tab expansion preserves source spans. Reloads preflight every target before changing tabs. Command history is shared only in memory. No dependencies, persistent history, or unrelated behavior were added.

## High-confidence findings fixed

- Added a deterministic existing-but-unreadable target case to the atomic `:reload-all` regression test. It verifies the first dirty tab remains unchanged when a later path is a directory.

## Final acceptance criteria

1. `?` opens help in Normal and all Select shapes; enabled palette commands alone receive actionable focus and Enter dispatches the selected action. Registry, App, palette, and snapshots cover this.
2. The kitchen-sink Go fence and generic fenced/indented code expand tabs at four-column stops with style and source-byte mapping intact. Core rendered-layout tests cover this.
3. `:tabnew` resolves absolute and launch-directory-relative paths, including `s/` and spaces. Paste accepts sanitized printable ASCII and rejects control, multiline, and non-ASCII input atomically. Core and App tests cover this.
4. `:e!` and `:reload` reload the current file; `:reload-all` atomically refreshes file-backed tabs without changing their order or active index. Dirty, missing, unnamed, invalid UTF-8, and unreadable targets are covered.
5. Up/Down traverse the last ten process-local commands across tabs, restore the draft, retain duplicate submissions, and leave history untouched on Escape or blank Enter. Core and App tests cover this.
6. All five task gates and the final `make check` gate passed.

## Final quality gate

| Command | Result |
| --- | --- |
| `make check` with a writable local copy of the existing advisory databases and Makefile offline scanner flags | PASS: 7 checks, 0 failures, no warnings |

## Cumulative diff

19 tracked source/test/snapshot files changed; workflow plan and evidence were added under `.ai/workflow/`. No dependency, lockfile, vendor, or Makefile changes. The worktree remains uncommitted.

## Remaining non-blocking concerns

None
