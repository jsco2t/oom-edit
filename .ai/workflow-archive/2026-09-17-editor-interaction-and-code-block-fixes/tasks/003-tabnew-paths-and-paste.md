# Task 003: Tabnew paths and paste

Delegation: main-only

## Goal

Make `:tabnew` reliably accept typed or pasted file paths, including absolute and launch-directory-relative paths.

## Context

The ex parser searches path text for substitute syntax, truncates at whitespace, and Command mode rejects bracketed paste.

## Scope

### In scope

- Ex command parsing and path argument extraction in core.
- Command-mode bracketed-paste input for ASCII paths.
- App capture of the launch directory and resolution for `:tabnew`.
- Tests for parsing, sanitation, terminal event routing, and opening paths.

### Out of scope

- Shell interpolation, glob expansion, general clipboard reads, and other file commands beyond regression protection.

## Implementation requirements

- Recognize substitute syntax only at a valid command prefix with its supported optional range; do not inspect path arguments for `s/`.
- Consume the full literal path remainder, including spaces, for path-bearing commands; keep existing `:s`, `:w`, and `:e` behavior covered by tests.
- Let Command mode receive `Event::Paste` through a core facade method. Trim surrounding ASCII whitespace/clipboard line endings; accept printable ASCII path characters; reject internal newline, other control characters, and non-ASCII pasted text without partial insertion.
- Keep overlay and confirmation input exclusive: paste while either is open must not reach the underlying command buffer or document.
- Resolve relative `:tabnew` paths against a launch directory captured at startup and injected for tests. Preserve absolute paths and file-new-buffer semantics where the path does not exist.

## Acceptance criteria

- [ ] `:tabnew ./examples/kitchen-sink.md` is parsed as tabnew and opens the file from the launch directory, without a substitution-range error.
- [ ] Absolute paths and paths containing spaces or `s/` open their intended targets.
- [ ] Pasted printable ASCII path text works in Command mode; invalid paste leaves the command buffer, document, and tab count unchanged and reports an error.
- [ ] Existing substitute, save, and edit command parsing regressions remain green.

## Validation

- `make test`

## Dependencies

None

## Expected areas of change

- `crates/oom-edit-core/src/session.rs`
- `crates/oom-edit/src/app.rs`
- `crates/oom-edit/src/lib.rs`

## Risks / notes

Treat path text literally; do not execute shell syntax. Do not derive relative paths from the active document directory.
