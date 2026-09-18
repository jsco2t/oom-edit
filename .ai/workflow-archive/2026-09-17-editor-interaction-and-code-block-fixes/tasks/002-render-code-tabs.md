# Task 002: Render code tabs

Delegation: main-only

## Goal

Preserve literal tab indentation visually in Go and other rendered code blocks while retaining exact source provenance.

## Context

The kitchen-sink Go fence uses tabs. The current rendered mapping treats each tab as a zero-width character, so the TUI loses indentation.

## Scope

### In scope

- Rendered fenced and indented code text in `oom-edit-core`.
- Four-column tab stops relative to code content, including mixed tabs/spaces.
- Source-byte mapping, syntax styles, and regression tests against the Go fixture.

### Out of scope

- Reformatting or saving source code and changing the Markdown parser's block recognition.

## Implementation requirements

- Expand each tab to the next four-column stop before width, wrap, and ratatui presentation; keep the canonical text unchanged.
- Map all display cells produced by one tab to that tab's exact UTF-8 source byte span. Do not assign source bytes to the synthetic fence gutter.
- Apply to known and unknown fence languages and indented code. Preserve syntax-highlight spans around tabs and subsequent tokens.
- Test leading, interior, consecutive, and mixed tabs; verify nested Go indentation and rendered/source navigation at the expanded area.

## Acceptance criteria

- [ ] The kitchen-sink Go fence renders one and two tabs as distinct four- and eight-column indentation levels.
- [ ] Any code fence language and indented code use four-column tab stops without changing document bytes.
- [ ] Style and byte-exact source provenance remain correct for text after a tab and for the tab's expanded display cells.
- [ ] Focused core regressions cover wrapping or narrow widths where tab expansion affects layout.

## Validation

- `make test`

## Dependencies

None

## Expected areas of change

- `crates/oom-edit-core/src/rendered/mod.rs`
- `crates/oom-edit-core/src/rendered/wrap.rs`
- `crates/oom-edit-core/src/rendered/tests/blocks.rs`
- Rendered goldens or TUI snapshots if affected

## Risks / notes

One source byte may own several display columns; selection and cursor mapping must stay deterministic.
