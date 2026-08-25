# Final Work-Package Evidence

## Work package

- ID: `2026-08-25-markdown-clipboard-copy-policy`
- Title: `Markdown Clipboard Copy Policy`
- Acceptance round: 2

## Original objective

Make outgoing clipboard copy preserve exact Markdown source by default while
offering a persistent `plain-text` preference, without changing internal Vim
register, put, edit, undo, or terminal transport behavior. Round-two acceptance
additionally requires exact preservation of multi-backtick code spans whose
rendered payload itself contains literal backticks.

## Completed tasks

- 001 — Build dual-format clipboard projection — `evidence/001.md`
- 002 — Wire clipboard format configuration — `evidence/002.md`
- 003 — Fix multi-backtick code-span copy — `evidence/003.md`

Round-one final evidence is preserved at `evidence/final-round-001.md`.

## Whole-package review

The integrated review traced clipboard data from parser-leaf provenance through
rendered selection, the typed dual-format core effect, App preference selection,
and the injected OSC 52 sink. Markdown interpretation remains core-owned;
configuration and final representation choice remain TUI-owned. The round-two
fix narrows code-leaf alignment to parser-confirmed payload bytes and does not
alter selection ranges or the Vim operation projection. No remaining
high-confidence correctness, architecture, security, concurrency, data-integrity,
or test-coverage finding was identified.

## High-confidence findings fixed

- Block plain text now comes from selected rendered atoms, avoiding malformed
  per-row Markdown parsing across wrapped constructs.
- Startup configuration is grouped in the owned `AppStartupOptions` boundary,
  resolving the constructor-arity lint without hiding it.
- Multi-backtick code payloads no longer assign visible atoms to opening
  delimiter bytes, so complete Markdown reconstruction restores the full opener.
- CRLF inside code spans maps both parser-rendered spaces to the exact CRLF token
  instead of falling back to the whole code span.

## Final acceptance criteria

- [x] Default copy preserves the exact original Markdown example and all
      backticks; App/core integration tests assert the hardcoded payload.
- [x] Complete inline code, emphasis, strong, strikethrough, link, image,
      nested, escaped, and entity constructs have byte-exact dual-format
      coverage.
- [x] Partial selections remain bounded and do not add unmatched delimiters,
      destinations, or unselected content.
- [x] Character, line, and block behavior remains covered across wrapping,
      tables, repeated content, multiline text, UTF-8, and synthetic output.
- [x] `copy_format = "plain-text"` selects the rendered syntax-free field while
      absent/default configuration selects Markdown.
- [x] Register shapes, explicit/default system yanks, puts, deletes, changes,
      undo, empty selections, and source-mode non-emission remain green.
- [x] URL-only synthetic copies remain format-invariant and renderer-generated
      borders, padding, prefixes, and link markers do not enter selection data.
- [x] Configuration defaulting, validation, round-trip behavior, README, and
      changelog coverage accurately describe the external-only setting.
- [x] App emits exactly one configured representation; sink feedback and the
      100 KiB limit apply to the selected bytes.
- [x] The curated public facade exposes only the project-owned clipboard DTO;
      core remains terminal-independent with no new dependency or text owner.
- [x] The exact kitchen-sink multi-backtick fragment now round-trips byte-for-byte
      in Markdown and emits the literal inner backticks without outer delimiters
      in plain text.
- [x] One-, two-, and longer-backtick source ownership excludes outer delimiter
      runs, including leading, trailing, and repeated literal-backtick payloads.
- [x] Multi-backtick partial, line, and block selection regressions pass without
      changing edit/register semantics.
- [x] LF, CRLF, table escaped-pipe, Unicode, wrapping, and repeated-text
      provenance tests pass.
- [x] Both App format preferences pass the exact escaped-backtick sink test.
- [x] The final `make check` completed with all seven gates passing.

## Final quality gate

| Command      | Result                    |
| ------------ | ------------------------- |
| `make check` | PASS — 7 passed, 0 failed |

## Cumulative diff

Relative to baseline `6bf7c7aee91601e2d9cad201d0db3777e47561b5`, the
worktree adds the core dual-format clipboard projection, Markdown-default App
configuration, documentation, complete regression coverage, and the
multi-backtick provenance correction. The same worktree also contains the
separately requested feature-workflow acceptance-round enhancement; that
concurrent tooling change was preserved and is not part of the clipboard runtime
behavior.

## Remaining non-blocking concerns

None
