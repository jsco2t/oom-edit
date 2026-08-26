# Task 001: Preserve Rendered Yank Source

Delegation: main-only

## Goal

Make rendered Select yanks preserve exact multiline Markdown and whitespace in both clipboard content and internal Vim registers without changing selection painting or destructive operator ranges.

## Context

Character selection stores only source-backed atom ranges. Clipboard and Vim payload builders currently concatenate those disjoint slices, dropping intervening newlines, blank lines, indentation, block syntax, and other source-only bytes. The system clipboard override and register payload must be repaired together.

## Scope

### In scope

- Character, line, and block Markdown payload construction from source provenance.
- Boundary-aware inline formatting inclusion and exact intervening live-source bytes.
- Faithful private Vim register payloads for rendered yanks.
- Default/system/named/black-hole register behavior and put round trips.
- Focused clipboard, Vim-wrapper, and session integration regression tests.

### Out of scope

- New key bindings or user documentation, owned by Task 002.
- Plain-text policy/config changes.
- Selection painting, cursor/motion behavior, or mutation geometry changes.
- Source-mode Vim operations.

## Implementation requirements

- Derive copy text from existing UTF-8 byte provenance plus the canonical live document; never reconstruct from rendered strings or use substring matching.
- For character selections, preserve every source byte between the first and last included boundary after applying the existing full-construct boundary rules.
- Do not add unselected opening/closing syntax at partial selection boundaries.
- For block selections, preserve row-local source bytes and the existing explicit newline between rectangular rows; preserve empty/padded row semantics.
- Keep linewise selection as exact physical source lines with linewise register metadata.
- Carry the faithful Markdown payload through a private consumer-owned Vim operation model so yank recording and clipboard effects agree.
- Keep delete/change/indent/outdent ranges identical to the pre-change projection; an exact yank payload must not broaden edits.
- Preserve named/system/unnamed/black-hole register routing and emit no duplicate clipboard effect.

## Acceptance criteria

- [ ] A character selection spanning ordinary lines and blank lines yanks exact `\n` separators, indentation, ordinary spaces, trailing spaces, and Markdown markers.
- [ ] Forward and reverse selections produce identical exact payloads; wrapped visual rows do not introduce newlines and retain the original source spaces.
- [ ] Headings, lists, links, emphasis, multi-backtick code spans, fenced blocks, escapes, entities, tables, Unicode, and repeated/ambiguous text have byte-exact regression coverage.
- [ ] Partial inline selections retain the existing rule that unselected boundary delimiters are not added.
- [ ] Line and block selection behavior remains exact for shape-specific source and register semantics.
- [ ] Unnamed/system register contents match the faithful Markdown payload and a subsequent put round-trips multiline formatting and whitespace.
- [ ] Named and black-hole yanks retain their existing publication and register isolation rules.
- [ ] Delete/change/indent/outdent regression tests prove mutation ranges and results are unchanged.

## Validation

- `make test`

## Dependencies

None

## Expected areas of change

- `crates/oom-edit-core/src/clipboard.rs`
- `crates/oom-edit-core/src/session.rs`
- `crates/oom-edit-core/src/vim.rs`
- `crates/oom-edit-core/tests/session_integration.rs`

## Risks / notes

The key risk is conflating a faithful copy envelope with destructive selection ranges. Keep the copy/register payload explicit and leave renderer-projected mutation ranges intact.
