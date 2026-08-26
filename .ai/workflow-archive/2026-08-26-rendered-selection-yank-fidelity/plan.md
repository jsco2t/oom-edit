# Rendered Selection Yank Fidelity

## Objective

Make the default rendered Select `y` preserve the selected Markdown source faithfully, including physical newlines, blank lines, indentation, ordinary spaces, trailing spaces, and Markdown syntax. Add an explicit, discoverable uppercase `Y` action for the rarer one-shot copy of syntax-free rendered plain text without weakening internal register or put behavior.

## Current behavior

- Rendered Select projects visible characters into ordered UTF-8 source ranges. Character selections spanning rendered rows normally contain multiple ranges because physical newlines, blank lines, block markers, delimiters, and other renderer-hidden bytes do not own visible atoms.
- `clipboard::markdown_for_ranges` expands some fully selected inline constructs, normalizes the ranges, and then calls `concatenate_ranges`, which appends only the selected slices. Every byte between disjoint ranges is discarded. This is the direct cause of newline and whitespace loss.
- The private Vim wrapper independently builds characterwise register payloads by concatenating the same ranges, so the unnamed/system register can lose the same bytes even when the system-clipboard override is later improved.
- Linewise selection already uses an exact physical source range, and blockwise selection intentionally builds independent rectangular rows separated by newlines.
- `ClipboardContent` already carries both Markdown and parsed plain-text forms. App chooses the outgoing form from `[clipboard].copy_format`, whose default is `markdown`.
- The command registry exposes `y` as a core-owned Select binding. Uppercase `Y` is unused in rendered Select. App owns only Space chords, while core owns rendered selection and register semantics.
- Existing tests strongly cover single-line delimiters, entities, multi-backtick code spans, linewise yanks, block payloads, register routing, OSC 52 output, and configured format choice, but do not assert a multiline character selection with source-only gaps.

## Proposed implementation

First repair the core payload boundary. Build the Markdown representation from source provenance rather than from rendered strings: for a character selection, retain the current boundary-aware inline-construct expansion, then copy the complete canonical live-source envelope from the first included byte through the last included byte so every intervening source byte survives. For block selection, apply the same row-local envelope rule and preserve the existing explicit newline between rectangular rows. Keep linewise behavior unchanged.

Carry that faithful Markdown payload into the private Vim selection operation so a yank records the same text in the in-process register and publishes it to the clipboard effect. Keep renderer-projected ranges unchanged for highlighting, delete, change, indent, and outdent; payload fidelity must not broaden mutation ranges or alter rendered provenance.

Then add uppercase `Y` as a core-owned rendered Select binding. It performs the same non-destructive yank and records the same Markdown-safe internal register as `y`, but explicitly publishes `ClipboardContent::plain_text()` to the host for that one operation. This explicit plain-text action bypasses the global output preference for its system copy. Plain `y` retains the existing configured policy, which defaults to exact Markdown; an explicitly configured `plain-text` default remains backward-compatible.

Update the single command registry, help/palette projections, README, configuration guidance, and changelog. Add regression matrices at the core and App boundaries for multiline/blank-line whitespace, indentation and trailing spaces, Markdown block and inline syntax, wrapping, forward/reverse selection, Unicode, all three selection shapes, register prefixes, config interaction, link-index behavior, one-shot `Y`, internal put fidelity, and clipboard errors/size limits.

## Architectural decisions

- `EditorSession` remains the owner of rendered Select grammar and source/layout projection. App receives typed clipboard effects and does not inspect selections or Markdown.
- Source fidelity is derived only from existing byte-exact provenance ranges and the canonical live document. No substring search, rendered-text reconstruction, or terminal-cell scraping is permitted.
- Character yank payload may use a contiguous source envelope, but character delete/change ranges remain the existing projected atom ranges. Copy fidelity must not silently change editing semantics.
- The private, consumer-owned Vim operation DTO carries any exact yank payload needed for register recording. No rendered-layout DTO or third-party type enters `vim.rs`.
- `y` with the default configuration means Markdown-preserving yank/copy. Existing explicit `[clipboard] copy_format = "plain-text"` behavior remains supported to avoid a configuration compatibility break.
- `Y` means one-shot plain-text system copy regardless of `copy_format`; the internal unnamed, numbered, named, or system register still receives Markdown source so `p`/`P` cannot inject lossy rendered text into the document.
- Named and black-hole register rules remain authoritative: a yank that does not currently publish to the system clipboard must not start publishing merely because `Y` was used. Synthetic link destinations remain format-invariant.
- Uppercase input must work for the terminal representations used by crossterm, without treating Ctrl/Alt variants as `Y`.
- `ClipboardContent`, `Effect`, `EditorSession`, and the curated crate-root facades remain project-owned. No public API expansion or new dependency is planned.
- Live text is canonically normalized to LF by the existing document boundary; fidelity means exact bytes from that live representation, not reconstruction of original CRLF encoding.

## Work included

1. Repair multiline character and block Markdown payload construction so all selected source whitespace and formatting are preserved.
2. Make the Vim register payload match the faithful Markdown clipboard representation for yanks without changing mutation geometry.
3. Add the core-owned rendered Select `Y` one-shot plain-text system-copy action.
4. Preserve global `copy_format` compatibility, explicit register semantics, synthetic link behavior, size/error handling, and internal put safety.
5. Update the command registry, palette/help discoverability, README/configuration guidance, changelog, and drift guards.
6. Add unit and integration regression coverage for source fidelity, plain-text output, routing, and terminal input translation.

## Task sequence

1. `tasks/001-preserve-rendered-yank-source.md`
2. `tasks/002-add-plain-text-yank-binding.md`

Tasks execute strictly in order because the alternate action must reuse the corrected dual-format payload and register semantics from Task 001.

## Quality gate

Both the standard and final gate run `make check`. This is the repository's complete local CI gate and includes format checking, Clippy with warnings denied, workspace build, the full test suite, dependency/license policy, RustSec audit, and bundled-data license validation.

No benchmark command is added: the change is event-driven selection copying, not render/startup/idle hot-path work, and the repository has no clipboard-specific performance target. No new developer workflow is introduced, so no new Make target is needed.

## Risks

- A naive contiguous envelope could include boundary syntax that was not selected. The implementation must retain existing boundary-aware construct rules and add partial-link/emphasis/code regression cases.
- Fixing only the system clipboard would leave the internal register lossy. Register contents and subsequent `p`/`P` must be asserted directly.
- Reusing faithful copy ranges for delete/change could broaden destructive edits. Mutation ranges and tests must remain unchanged.
- Wrapped rows are visual, not physical lines. Soft wrapping must preserve source spaces without introducing newline bytes.
- Tables, list decorations, fenced blocks, escaped text, entities, synthetic rows, and reverse selections stress non-linear or source-less rendering and need exact fixtures.
- Uppercase key reporting differs across terminals. Core and crossterm-boundary tests must cover accepted uppercase representations and reject modified unrelated keys.

## Out of scope

- HTML, rich-text, or multi-MIME clipboard formats.
- Reading the desktop clipboard or replacing OSC 52.
- Changing source Normal/Insert Vim yank behavior.
- Changing delete, change, indent, outdent, selection painting, cursor motion, or rendered provenance.
- Removing or renaming the existing `clipboard.copy_format` configuration setting.
- New clipboard dependencies, public selection APIs, or terminal-specific keymaps.

## Final acceptance criteria

- [ ] With default configuration, rendered character Select followed by `y` copies the exact selected canonical Markdown source across physical newlines and blank lines, preserving indentation, internal/trailing spaces, and Markdown syntax byte-for-byte.
- [ ] Character-yank fidelity holds for forward/reverse selection, wrapping, Unicode/grapheme source ranges, headings, lists, fenced/code spans, links, escapes, entities, and tables without claiming synthetic output as source.
- [ ] Linewise `V` yanks preserve exact physical lines and linewise register semantics; blockwise yanks preserve independent rectangular rows and explicit row newlines.
- [ ] The in-process yank register receives the same faithful Markdown payload as the default system copy, and `p`/`P` round-trips multiline selections without collapsing whitespace or formatting.
- [ ] Delete/change/indent/outdent source ranges and behavior are unchanged by the copy repair.
- [ ] Uppercase `Y` in rendered Select performs a non-destructive yank, writes syntax-free rendered plain text to the system clipboard once, exits Select like `y`, and remains discoverable as a core-owned binding.
- [ ] `Y` keeps Markdown in internal registers, respects named/black-hole publication rules, handles synthetic link destinations safely, and reports existing clipboard success, failure, and size-limit outcomes.
- [ ] Plain `y` retains the backward-compatible `copy_format` policy with `markdown` as the default; `Y` explicitly chooses plain text for its one-shot system copy.
- [ ] README, configuration guidance, changelog, command registry, palette/help rows, hint/meta-tests, and exact binding contracts agree on `y` versus `Y` behavior.
- [ ] Public API guards and dependency hygiene remain unchanged, no dependency is added, and `make check` passes.

## Acceptance follow-up — Round 2

### Observed acceptance gap

Copying only source-backed code content works and preserves its whitespace, but selecting the complete rendered fenced-code surface does not reliably include the opening and closing Markdown fence lines. In particular, the language info string on the opening fence and the closing delimiter can be omitted even though the selection visually spans the complete block.

### Root cause and repository evidence

- `render_code_fence` renders the opening fence as a synthetic `▏ <language>` row and the closing fence as a synthetic `▏` row. Their display atoms correctly carry no source ranges because the visible gutter glyphs are renderer-generated rather than literal backticks.
- The block parser nevertheless supplies `render_code_fence` with the exact full block source span and exact content span. The opening and closing source boundaries are therefore known at the parser leaf while layout is built.
- The layout currently retains only `RenderedLineRole::CodeFence` after rendering. It does not retain a private association between the contiguous rendered fence surface and the full Markdown block span.
- Rendered Select consequently projects only code-content atoms. Round 1's source-envelope repair can preserve bytes between selected content atoms, but it cannot extend beyond the first or last selected atom to recover a source-only opening or closing fence.

### Additive fix strategy

Retain renderer-neutral, crate-private fenced-region metadata when each `BlockKind::CodeFence` is rendered. Each region will pair the complete rendered row interval with the exact full source span already owned by that parser leaf. Synthetic gutter atoms remain source-less, and the public `RenderedLayout`/crate-root API remains unchanged.

During yank preparation only, clone the projected selection and expand that yank-specific source geometry when its rendered row interval fully covers a fenced region from opening row through closing row. Use the expanded selection for the exact Markdown clipboard payload and internal yank register, including the empty-fence case. Keep the original rendered selection as the authoritative geometry for painting and every destructive or transforming operator.

A selection confined to code-content rows will not cover both boundary rows and therefore follows the existing Round 1 path unchanged. A complete surface selection copies the exact canonical source block, including the original opening delimiter, language/info marker, body whitespace and newlines, closing delimiter, and selected physical-line ending. The behavior applies in forward and reverse selection directions and composes with the existing `y`/`Y` publication policy: default `y` publishes Markdown, while `Y` publishes syntax-free text but retains the complete Markdown fence in the internal register.

### Architectural decisions

- Fence boundaries are retained from the existing block parser at render time. Do not reconstruct them with substring matching, terminal-cell scraping, or delimiter searches during copy.
- Region metadata is private core state associated with the cached rendered layout. It introduces no public DTO field or crate-root export.
- Generated gutter cells remain source-less. Fence metadata describes block-level selection coverage; it does not assign the displayed `▏` or language label to raw backtick bytes.
- Expansion is yank-only. Delete, change, indent, outdent, block selection, cursor mapping, and the public rendered-selection projection continue to use the original atom-derived ranges.
- Full-surface detection is based on rendered row coverage, so it remains stable for empty bodies, highlighted languages, wrapping, nested container prefixes, and forward/reverse endpoints.
- Existing configured `y`, one-shot `Y`, register publication, clipboard limit/error, and put semantics remain authoritative.

### Work included

1. Preserve private full-source and rendered-row metadata for fenced-code regions as part of layout construction and invalidation.
2. Expand only yank-specific source geometry when a characterwise or linewise rendered selection spans the complete fence surface.
3. Preserve exact content-only copying without adding either fence when the selection remains inside the body.
4. Cover language markers, exact whitespace/newlines, empty and multiline bodies, forward/reverse selections, supported fence delimiters, nested container prefixes, internal put behavior, `Y`, and unchanged non-yank geometry.

### New task sequence

3. `tasks/003-preserve-complete-code-fences.md`

Task 003 is additive to the immutable completed Round 1 tasks.

### Quality gate

`gate.json` remains unchanged. Both the task and final package gate run `make check`, the repository's complete format, lint, build, test, supply-chain, and bundled-data licensing gate. No dependency or developer workflow is introduced.

### Risks

- Treating any contact with a code block as full-block selection would regress content-only copies. Expansion must require coverage of both rendered boundary rows.
- Reusing expanded ranges for destructive operators would silently broaden edits. The expanded selection must exist only in the yank path.
- Empty fences have no content atoms, so tests must prove the fence metadata alone is sufficient to produce a non-empty yank and register payload.
- Nested list or blockquote fences contain structural prefixes in their canonical source. Exact full-block copying must retain those bytes rather than synthesizing a standalone fence.
- Fence metadata must invalidate atomically with the cached layout so edits or width changes cannot leave stale source spans.

### Out of scope

- Changing how fenced code blocks are visually rendered.
- Making generated gutter glyphs source-backed or copying those glyphs.
- Changing content-only copy semantics, blockwise rectangular yanks, source-mode yanks, or Markdown parsing rules.
- Changing destructive selection geometry or adding a dedicated code-block selection command.
- Adding public API, dependencies, clipboard formats, or keybindings.

### Round 2 acceptance criteria

- [ ] A rendered characterwise or linewise selection that spans a complete fenced-code surface copies the exact canonical Markdown block with default `y`, including the opening delimiter, language/info marker, body whitespace and newlines, closing delimiter, and selected final line ending.
- [ ] Complete-fence copying is exact in forward and reverse directions for empty and multiline blocks, supported backtick/tilde fence forms, longer delimiters, syntax-highlighted and unknown languages, and fences nested in lists or blockquotes.
- [ ] Selecting only content within a fenced block copies exactly that content with Round 1 whitespace fidelity and adds neither fence delimiter nor the language marker.
- [ ] Full-fence `y` records the same exact Markdown in the internal linewise or characterwise register, and `p`/`P` round-trips it without loss.
- [ ] Full-fence `Y` publishes syntax-free body text while retaining the exact fenced Markdown in the internal register; configured `y`, named/black-hole/system registers, clipboard limits, and error reporting remain unchanged.
- [ ] Generated gutter atoms remain source-less, and selection painting, cursor mapping, blockwise selection, delete, change, indent, and outdent retain their existing source geometry.
- [ ] No public API or dependency changes are introduced, and `make check` passes.
