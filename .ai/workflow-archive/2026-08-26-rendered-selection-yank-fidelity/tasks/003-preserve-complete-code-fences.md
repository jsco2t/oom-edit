# Task 003: Preserve Complete Code Fences

Delegation: main-only

## Goal

Make a rendered selection that spans an entire fenced-code surface yank the exact full Markdown block while leaving content-only copying and all non-yank selection geometry unchanged.

## Context

Round 1 made source-backed rendered yanks preserve exact whitespace and Markdown, but fence header and closing rows are synthetic gutter rows with no source-backed atoms. The renderer knows the complete `BlockKind::CodeFence` source span while building those rows, yet that block-level provenance is not retained for yank preparation. This task adds the missing private provenance and uses it only when the selection covers the complete rendered fence surface.

## Scope

### In scope

- Retain crate-private rendered-row and full-source spans for each fenced-code region in the cached layout state.
- Expand yank-specific characterwise and linewise geometry for completely covered fence regions.
- Preserve exact opening delimiters, info/language markers, body bytes, closing delimiters, and selected line endings.
- Preserve content-only yank behavior and the established `y`/`Y` clipboard/register split.
- Add focused renderer, session, Vim/register, and App regressions where needed.

### Out of scope

- Visual changes to code-block gutters or syntax highlighting.
- Source ownership for generated gutter atoms.
- Changes to blockwise rectangular yanks, source-mode yanks, destructive operators, cursor motion, or selection painting.
- A new command, keybinding, public API, dependency, or clipboard format.

## Implementation requirements

- Capture every fence region directly from the existing `BlockKind::CodeFence` parser leaf while rendering; do not recover fences later through substring or delimiter searches.
- Keep the region metadata private to `oom-edit-core` and invalidate it with the matching rendered-layout cache.
- Pair each region's exact full canonical source span with its complete rendered row interval, including synthetic header and closing rows.
- Before a yank, derive an expanded copy of the selection only when its rendered endpoints cover both boundary rows of a fence region. Support forward and reverse endpoints and multiple fully covered regions.
- Use the expanded copy for `ClipboardContent` Markdown construction and the private Vim yank DTO so empty fenced blocks also record and publish a payload.
- Do not replace the authoritative selection used by delete, change, indent, outdent, block operations, painting, cursor movement, or public selection queries.
- Leave content-only selections on the current source-envelope path so whitespace is preserved without introducing fence bytes.
- Retain default/configured `y`, one-shot plain-text `Y`, linewise/characterwise register types, named/black-hole/system publication rules, clipboard size/error behavior, and Markdown-safe internal put semantics.
- Add no dependency and expose no new public type, field, or signature.

## Acceptance criteria

- [ ] Whole-surface default `y` copies exact fenced Markdown, including language/info text and both delimiters.
- [ ] Whole-surface fidelity holds forward and backward for empty/multiline bodies, body indentation/trailing spaces/newlines, supported backtick and tilde forms, longer delimiters, known/unknown languages, and nested list/blockquote source.
- [ ] Content-only characterwise and linewise yanks retain exact body whitespace and omit fence/info bytes.
- [ ] Whole-surface `y` and `Y` keep exact fenced Markdown in the internal register; `y` publishes according to configuration and `Y` publishes syntax-free body text.
- [ ] Empty whole fences produce a real clipboard/register yank rather than being rejected as an empty atom selection.
- [ ] Blockwise yanks and delete/change/indent/outdent ranges remain unchanged.
- [ ] Synthetic gutter atoms remain source-less and public API/dependency guards remain green.
- [ ] The full repository quality gate passes.

## Validation

- `make test`
- `make check`

## Dependencies

- Task 001
- Task 002

## Expected areas of change

- `crates/oom-edit-core/src/rendered/mod.rs`
- `crates/oom-edit-core/src/session.rs`
- `crates/oom-edit-core/src/clipboard.rs`
- `crates/oom-edit-core/tests/session_integration.rs`
- `crates/oom-edit/src/app.rs`
- focused renderer/navigation tests as required

## Risks / notes

- A fence nested under a container must copy its actual canonical prefix bytes; never synthesize a standalone code block.
- A selection touching only one boundary is partial and must not gain the opposite delimiter.
- Expanded source geometry is safe only for non-destructive yank handling and must not leak into other operators.
