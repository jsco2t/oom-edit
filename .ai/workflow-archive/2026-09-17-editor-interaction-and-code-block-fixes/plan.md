# Editor interaction and code block fixes

## Objective

Make help access and palette action selection consistent in rendered Normal and Select modes; render tab-indented Go and other code fences correctly; add reliable reload commands; make `:tabnew` accept pasted, relative, and absolute paths; and provide the last ten ex commands through Up and Down for the lifetime of one editor process.

## Current behavior

- The command registry gives direct `?` help only to Normal. Select has `Space h` and `Space ?`, so plain `?` reaches the core. The palette begins on its first row, which is a non-executable reference; navigation can also highlight disabled commands and references as if Enter would execute them.
- The kitchen-sink Go fence contains literal tabs. Rendered fence text is converted to mapped display fragments without tab expansion; the width code treats a tab as zero columns. Indentation is therefore lost. The Markdown specification calls for four-column tab stops.
- The ex parser searches the entire command string for `s/`. A valid `:tabnew ./examples/kitchen-sink.md` contains that byte pair inside its path and is misclassified as a substitute. The parser also truncates path arguments at whitespace.
- Bracketed paste is routed only to `EditorSession::insert_paste`, which accepts Insert mode. Command mode rejects it.
- `:e!` emits an `OpenRequested` effect with an empty path. The App replacement path uses that empty value rather than the active file path. There is no `:reload` or `:reload-all`.
- `EditorSession` owns a command buffer but no command history. Command-mode Up and Down are ignored.

## Proposed implementation

1. Extend the registry's direct help alias to Select. Make palette selection and its actionable highlight derive from enabled executable registry rows; keep non-executable reference information visibly distinct. Verify opening and Enter behavior in Normal and every Select shape.
2. Expand tabs in rendered code content to the next four-column stop before text reaches display-width computation, preserving source-byte provenance for every resulting display cell. Apply the same rule to fenced and indented code and to all languages, without changing stored source text or syntax style roles.
3. Parse ex command names only at the command prefix, with a narrowly recognized optional substitute range. Treat the remainder of a path-bearing command as one literal path. Route bracketed paste in Command mode into a core command-input method that accepts printable ASCII path text after trimming clipboard line endings and rejects embedded controls and multiline commands. Resolve relative `:tabnew` paths against a launch-directory value captured at startup; pass absolute paths through unchanged.
4. Introduce typed core reload effects and an App lifecycle action. Make `:e!` and `:reload` target the current file on disk; make `:reload-all` reload every open file-backed tab, preserving tab order and active index. Preload all targets before applying `:reload-all`, so an unreadable, missing, unnamed, or invalid UTF-8 target leaves every tab untouched and reports an error. Explicit reload discards dirty edits only when every target can be reloaded. Keep normal `:e {path}` dirty protection.
5. Keep a bounded, process-local ex history shared by all tabs through a core-owned history type. Successful Enter submissions, including commands that return an error, enter history; empty submissions and Escape do not. Repeated submissions remain separate entries. Up moves newest to oldest through at most ten entries; Down moves toward the saved draft and restores it at the end. Editing a recalled command makes it an editable draft. Replacements and new tabs retain the shared history; a new process starts empty.

## Architectural decisions

- `EditorSession` remains the ex grammar and command-input owner. App only translates terminal events, captures the launch directory, and executes typed file lifecycle effects.
- `App::execute_lifecycle` remains the sole owner of all reload and tab-open I/O orchestration. Reload requests capture their target indices before any work begins. No component keeps another mutable document-text copy.
- Code tab expansion belongs in `oom-edit-core`'s rendered code path. Expanded spaces carry the source span of their original tab; the fence gutter remains synthetic and source-less. Source-mode text and on-disk bytes remain literal tabs.
- The static command registry remains the source of help bindings and context availability. Palette reference rows remain metadata, not executable commands.
- History uses no persistent config or external dependency. Any new core API is deliberately re-exported through the crate root and added to public API compile guards.
- Path text is literal, with no shell expansion or command interpolation. Pasted ASCII control characters cannot create another command or a hidden path component.

## Work included

1. Direct `?` help in all rendered Select shapes and accurate actionable palette highlight, with registry, palette, and App regression tests.
2. Four-column tab expansion for rendered code, including the kitchen-sink Go fixture, mixed tabs and spaces, styling, and byte-exact rendered-to-source mapping tests.
3. Ex parser fix, `:tabnew` paste, sanitization, and launch-directory path resolution, with core and App tests for relative, absolute, spaced, invalid, and `s/`-containing paths.
4. `:e!`, `:reload`, and `:reload-all` lifecycle behavior, with tests for dirty tabs, multiple tabs, missing/unnamed files, errors, and all-or-nothing replacement.
5. Ten-entry session history, Up/Down draft navigation, Escape, command edits, and cross-tab lifetime tests.

## Task sequence

1. [001-help-in-select-and-palette-actions.md](tasks/001-help-in-select-and-palette-actions.md)
2. [002-render-code-tabs.md](tasks/002-render-code-tabs.md)
3. [003-tabnew-paths-and-paste.md](tasks/003-tabnew-paths-and-paste.md)
4. [004-reload-commands.md](tasks/004-reload-commands.md)
5. [005-session-command-history.md](tasks/005-session-command-history.md)

## Quality gate

Every task runs `make test` for behavior verification, then the repository's complete `make check` gate. `make check` includes formatting, Clippy with warnings denied, build, all tests, dependency license/advisory checks, and bundled-data license checks. The same gate runs after integrated review. No new dependencies or developer commands are planned, so no vendor or Makefile change is expected. If a dependency does change, the repository's dependency checklist, vendoring, `make deny`, and `make audit` also apply.

## Risks

- Tab expansion creates multiple display columns for one source byte. Cursor, selection, and wrap mappings must retain that byte ownership without inventing source positions.
- Reload is destructive by request. The all-tab operation must not partially replace sessions after one target fails and must never reinterpret a missing file as a new empty buffer.
- Ex path parsing must keep `:s` and supported range forms working while ignoring path substrings that resemble substitute syntax.
- Shared history must remain process-local across tab replacement without introducing a second command grammar or persistent state.

## Out of scope

- General clipboard reading or OSC 52 clipboard query support; the terminal's bracketed `Event::Paste` is the input path.
- Shell-style path expansion, globbing, or command completion.
- Persisted command history or a new search-history feature.
- Formatting or rewriting the source code inside fences.

## Final acceptance criteria

1. Plain `?` opens help in Normal and character, line, and block Select; the initial and navigated actionable highlight always corresponds to an enabled command for that mode, and Enter executes that command.
2. The Go fence in `examples/kitchen-sink.md` visibly retains its nested indentation in rendered mode; a tab advances to the next four-column stop in code for any language, and source bytes and byte mappings remain correct.
3. `:tabnew ./examples/kitchen-sink.md` and an absolute path open the intended file from the launch directory, including when the path contains `s/`; pasted printable ASCII path text works, while control, multiline, and non-ASCII pasted input is rejected without opening a tab or changing document text.
4. `:e!` and `:reload` reload the active file. `:reload-all` refreshes every file-backed tab while retaining tab order and active tab; any invalid target leaves all tabs unchanged and produces an error.
5. Command-mode Up/Down traverse no more than the last ten submitted commands across tabs, restore the draft on Down past newest, and Escape closes command mode; history is empty in a fresh process and never written to disk.
6. `make check` passes after each task and for the integrated package.

## Acceptance follow-up — Round 2

### Observed failures and findings

- The palette's `PaletteState::action_positions` includes only enabled App commands. `handle_key` uses that list for Up/Down and Tab/BackTab, so selection cannot reach the Vim reference rows below the last App command. Rendering scrolls from the selected index, leaving lower rows inaccessible. The round-1 focus test explicitly enshrined this behavior.
- `PaletteRow::Reference` combines key guidance and ex-command entries. `Overlay::selected_command` returns only `AppCommand`; App currently closes the palette and reports “reference entry” when Enter is pressed on anything else. Core owns ex entry and execution, but has no terminal-neutral method for opening Command mode with a prefilled command.

### Correction to round-1 intent

Round-1 acceptance criterion 1 required the navigated highlight to stay on enabled App commands. This follow-up supersedes that navigation restriction: every filtered row must be reachable for reading. The palette must still distinguish App actions, editable ex-command handoffs, disabled actions, and read-only guidance by text/glyph as well as styling. The original Normal/Select `?` help behavior remains required.

### Additive fix strategy and architecture

1. Make palette navigation move through every filtered row, clamp at the first and last row, and keep the focused row visible in the list viewport. Preserve filtering and reset focus to a useful matching row. Give a focused read-only row a visible focus glyph distinct from the markers for executable App commands and ex-command handoffs; disabled App commands remain visibly disabled. Enter on read-only or disabled rows leaves the panel open without acting.
2. Give concrete ex-command rows explicit, single-command prefill metadata at their existing sources: `BindingRole::CoreEx` in `command::COMMANDS` and ex entries in `VIM_REFERENCE`. Keep key/motion references read-only. For argument templates, omit placeholder text and prefill only the literal command prefix (for example `:e {path}` → `e ` and `:tabnew {path}` → `tabnew `). A row that displays multiple alternative commands remains read-only unless represented as separate concrete rows. Never infer a prefill by parsing display prose at runtime.
3. Add a curated, terminal-neutral `EditorSession` facade operation to enter Command mode with validated prefill text. App closes the palette, asks the active core session to enter Command mode, and renders the prompt. The first Enter only performs this handoff; the next Enter submits through the existing ex dispatcher and history. Esc cancels without submitting. The method works from rendered Normal and Select without editing document text or bypassing core ex ownership.
4. Replace the palette's App-only selected-command query with a closed semantic selection result for App execution, core ex prefill, or read-only/disabled focus. Keep App command routing in the existing registry and lifecycle path. No string-based App ex dispatcher is added.

### New task sequence

6. [006-palette-list-navigation.md](tasks/006-palette-list-navigation.md) — restore full-list navigation and scrolling with accurate focus affordances.
7. [007-palette-ex-command-handoff.md](tasks/007-palette-ex-command-handoff.md) — add explicit ex prefill metadata and a core Command-mode handoff.

### Tests and quality gate

- Headless palette tests cover Up/Down and Tab/BackTab across App, disabled, key-reference, and ex rows; filtered and empty lists; first/last clamping; and a focused row beyond a 40×12 and 80×24 viewport. App tests cover modal exclusivity and mode-specific focus.
- Core and App tests cover `:wq` handoff, first-Enter non-execution, second-Enter execution, editable path prefixes, Escape cancellation, history on submission only, and transition from Normal and Select. Registry/reference completeness tests cover every ex prefill and prevent a displayed placeholder or combined command from being submitted as literal text.
- Each new task runs `make test` and the unchanged standard `make check` gate. The integrated package runs `make check` again. No dependency or new developer command is needed.

### Round-specific risks and boundaries

- The list is longer than the modal height; focus and viewport offset must be tested together so a selected row is visible rather than merely addressable in state.
- Ex handoff must not execute on the palette Enter or consume the next key twice. Destructive commands such as `:q!` remain editable prompts before submission.
- The palette may describe commands that need arguments. It must never prefill placeholder braces or a composite display string as an executable command.
- Mouse-wheel/click navigation, command completion, and executing ex commands directly from the palette are outside this round.

### Round-2 acceptance criteria

1. Up/Down and Tab/BackTab can focus every filtered help row, including rows below the initial viewport; the focused row stays visible at floor and larger terminal sizes, and navigation clamps at both ends.
2. Focus and Enter behavior are distinguishable for enabled App actions, core ex commands, disabled App actions, and read-only guidance in Normal and Select contexts. Read-only and disabled rows never execute or close the palette on Enter.
3. Selecting a concrete ex row such as `:wq` and pressing Enter closes the palette and opens Command mode with `wq` prefilled, without saving or quitting yet. A second Enter executes through core; Esc cancels.
4. Argument-taking ex rows open an editable prompt with the literal prefix and no placeholder text. Editing and submitting it uses existing core parsing, effects, lifecycle safety, and session history.
5. The previously completed round's behavior remains covered; `make check` passes for both new tasks and the integrated package.

## Acceptance follow-up — Round 3

### Observed failure and root cause

- The Go fence in `examples/kitchen-sink.md` contains one- and two-tab indentation. Task 002 expanded tabs only in the rendered Markdown code path (`rendered::mapped_code_from_styled`), so rendered Normal now looks correct.
- Source Insert uses a separate path: `EditorSession::render_source_with_atoms` sends highlighted source through `wrap_source_line` or `horizontal_window`, then the TUI renders `SourceFrame.lines` directly. `wrap_source_line`, `source_atoms`, and the TUI cursor width calculation treat a tab as zero columns; horizontal windows and App scroll-follow use source-character positions. The raw tab therefore reaches presentation without a stable visible width, and cursor, wrapping, scrolling, and cell-to-source mapping can disagree around it.

### Correction to earlier intent

The earlier statement that source-mode text remains literal tabs applies to canonical document text, editing, and saved bytes. It does not require a literal tab control character in the display frame. This round adds display-only source tab expansion in Insert mode while preserving the original source bytes and syntax highlighting.

### Additive fix strategy and architecture

1. In `oom-edit-core`, project source lines into display cells with four-column tab stops measured from the start of each logical source line. A tab may occupy one to four cells, including after ordinary spaces and wide Unicode characters. Preserve its exact one-byte source ownership for hit testing and decorations; keep generated clipping indicators source-less. Do not rewrite `LiveDocument`, highlighter input, or file contents.
2. Use one tab-aware source-view geometry for wrap and nowrap display rows, cursor placement, wrapped-row counts, viewport cell-to-source lookup, and source decorations. Keep syntax spans attached to the correct visible text after expansion. Do not use a separate TUI-only string replacement that would leave core geometry based on zero-width tabs.
3. Preserve the documented source-character meaning of `Viewport.left_col` and existing no-tab behavior. Make horizontal clipping count display cells after expanding tabs, and update App scroll-follow to use core-provided source display-column geometry so a cursor following tabs stays visible. If this adds a curated core facade method, extend the public API compile guard in the same change.
4. Keep the TUI a thin renderer of the core's tab-expanded `SourceFrame`; check physical terminal cells and cursor placement with a headless TestBackend test. The rendered Normal path from Task 002 remains unchanged.

### New task sequence

8. [008-source-insert-code-tabs.md](tasks/008-source-insert-code-tabs.md) — render source-view tabs at four-column stops with consistent viewport geometry and provenance.

### Tests and quality gate

- Core source-view tests use the actual Go fence lines and generic tabbed source. Cover leading, consecutive, and mixed spaces/tabs; wide characters; wrap and nowrap; horizontal clipping; cursor at and after tabs; viewport cell-to-source lookup within expanded tabs; style and decoration alignment; and unchanged document bytes after rendering or editing.
- A TUI TestBackend test verifies visible one-tab and two-tab Go indentation in Insert mode and that the terminal cursor matches the displayed source position. Existing rendered code tests stay green.
- Task 008 runs `make test`, then the existing `make check` gate. The integrated package runs `make check` again. No dependency or new developer command is needed.

### Round-specific risks and boundaries

- Several display cells map to one source byte. Wrapping or clipping must never fabricate source offsets or split a tab into differently owned bytes.
- `Viewport.left_col` currently counts source characters. Retaining that contract avoids an unannounced public API behavior change; the visible width and cursor must still be computed in display cells.
- Search and diagnostic decorations must follow the expanded display columns. The fix applies to tabs anywhere in source Insert view, not only Go fences; it does not alter Markdown parsing or save-time formatting.

### Round-3 acceptance criteria

1. In source Insert mode, the Go fence in `examples/kitchen-sink.md` shows one- and two-tab nested indentation at four-column tab stops, matching rendered Normal for those code lines.
2. Tabs in any source line, including mixed spaces, consecutive tabs, and wide Unicode neighbors, display at the next four-column stop; literal document and saved bytes remain unchanged.
3. Wrap and nowrap, horizontal scrolling, cursor placement, click/hit mapping, highlighting, and source decorations remain aligned around expanded tabs, including at narrow viewport widths.
4. Earlier round behavior remains covered, and Task 008 plus the integrated package each pass `make check`.
