# Task 015: Characterwise full-line delete and change semantics

Delegation: main-only

## Goal

Make rendered Select `d` and `c` remove a physical source line, including its line break, when all of that line's visible rendered text is selected, without changing yank/copy behavior.

## Context

The existing character projection contains source-backed glyph atoms but omits line breaks, so deleting a multi-line selection can leave empty lines. This affects Markdown generally, not just code fences. Lowercase `v` is the reported reproduction; uppercase `V` must remain consistent.

## Scope

### In scope

- Delete/change-specific normalization of selected source spans to physical line boundaries when fully covered.
- Exact tests for `v` and `V` through core, pane and relevant conformance paths.
- Preserve register, clipboard, undo and cursor behavior, including distinct yank/copy semantics.

### Out of scope

- Changing `y`/`Y` or Ctrl-V rectangular-block deletion semantics.
- Treating a single wrapped display row as an entire physical source line.

## Implementation requirements

- Define full coverage using source-backed visible content across all wrapped rows belonging to one physical source line; synthetic borders/padding do not count. A fully covered physical line removes its raw Markdown delimiters and LF/CRLF terminator as appropriate. Handle final lines without a terminator and blank lines enclosed by an otherwise fully selected range without deleting unrelated structural lines.
- Partially selected lines retain unselected text, formatting delimiters and line breaks. Table cells, list prefixes, headings, inline escapes/entities, repeated text and large code fences must not gain false source ownership.
- Apply the rule to Delete and Change before invoking the single `LiveDocument` mutation gateway. The deleted/change register content must match bytes actually removed. Leave Select yank/copy projection, publication and register shape byte-for-byte unchanged.
- Test one undo/redo transaction and exact source plus complete rendered frame after multi-line operations; use the full-builder oracle for provenance and cursor mapping.

## Acceptance criteria

- [ ] `v`-selected whole physical lines in prose, heading, list, table and Rust/Go fences disappear without blank-line remnants after `d` or `c`; `V` behaves consistently.
- [ ] A selected subset of a wrapped physical line does not remove the rest of that line or its terminator.
- [ ] LF, CRLF, Unicode, final-line, blank-line and hidden-markup cases retain exact bytes and one-step undo/redo.
- [ ] All existing `v`/`V`/Ctrl-V yank/copy bytes, clipboard publication and register semantics remain unchanged.

## Validation

- `make test-incremental`
- `make test-pane-public`
- `make check`

## Dependencies

014

## Expected areas of change

`crates/oom-edit-core/src/rendered/nav.rs`, `crates/oom-edit-core/src/session.rs`, `crates/oom-edit-core/src/vim.rs`, `crates/oom-edit-core/tests/`, `crates/oom-edit/tests/`

## Risks / notes

Delete/change widening is a mutation decision, not an excuse to assign source bytes to synthetic glyphs or to alter existing yank output. A code fence is one regression case of a general rule.
