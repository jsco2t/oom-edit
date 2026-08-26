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
