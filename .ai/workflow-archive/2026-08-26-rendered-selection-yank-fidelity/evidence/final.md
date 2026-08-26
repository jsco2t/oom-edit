# Final Work-Package Evidence

## Work package

- ID: `2026-08-26-rendered-selection-yank-fidelity`
- Title: Rendered Selection Yank Fidelity

## Original objective

Make rendered Select yanks preserve exact Markdown formatting, whitespace, and physical newlines; provide a discoverable one-shot syntax-free clipboard action; and, in the second acceptance round, make selecting a complete rendered fenced-code surface copy the exact opening fence, info marker, body, closing fence, and line ending while leaving content-only code copying unchanged.

## Completed tasks

- Task 001 — Preserve Rendered Yank Source — `evidence/001.md`
- Task 002 — Add Plain-Text Yank Binding — `evidence/002.md`
- Task 003 — Preserve Complete Code Fences — `evidence/003.md`

## Whole-package review

The integrated review compared both acceptance rounds with the frozen request, plan, task criteria, architecture constraints, task evidence, and cumulative source diff. Exact source payload construction remains provenance-driven and separate from destructive selection geometry. The renderer retains private parser-leaf fence regions without assigning source to synthetic gutter atoms; the session expands only a cloned yank projection; the private Vim adapter receives an exact consumer-owned payload; and App remains a thin clipboard-format host. Default/configured `y`, one-shot `Y`, internal registers, puts, registry/help, documentation, and tests remain consistent across both rounds.

## High-confidence findings fixed

- Corrected the original source-range concatenation path so intervening newlines, blank lines, whitespace, and Markdown bytes survive both clipboard and internal-register yanks.
- Aligned synthetic-link `y`/`Y` behavior with named and black-hole register publication isolation and corrected stale lossy-copy expectations.
- Preserved source-less synthetic fence endpoints across layout rebuilds by remapping them with retained rendered-line identity.
- Strengthened the complete-fence regression matrix for reverse empty fences, nested-list and unknown-language source, absent final newlines, multiple fully covered fences, and exact `p`/`P` register insertion.

## Final acceptance criteria

- [x] Default rendered character `y` preserves exact canonical Markdown across physical and blank lines, including indentation, internal/trailing spaces, and syntax.
  - Evidence: Task 001 source-envelope, register, and multiline integration regressions.
- [x] Character-yank fidelity holds forward/reverse across wrapping, Unicode, headings, lists, fences, code spans, links, escapes, entities, tables, and repeated provenance without claiming synthetic output.
  - Evidence: Task 001 matrices plus renderer provenance and dependency-hygiene guards.
- [x] Linewise and blockwise yanks retain physical-line, register-type, rectangular-row, and explicit-row-newline semantics.
  - Evidence: Task 001 line/block matrices and the full selection conformance suite.
- [x] Internal registers receive faithful Markdown and `p`/`P` preserve the source payload without lossy conversion.
  - Evidence: Task 001 multiline put regressions and Task 003 exact complete-fence `p`/`P` insertion.
- [x] Delete, change, indent, outdent, painting, and mutation geometry remain unchanged.
  - Evidence: Task 001 operator matrices and Task 003's complete-fence delete-geometry regression.
- [x] Uppercase `Y` is a core-owned, non-destructive, one-shot syntax-free system copy that exits Select and remains discoverable.
  - Evidence: Task 002 routing, input, registry, palette, snapshot, README, and changelog coverage.
- [x] `Y` keeps Markdown-safe registers, preserves named/black-hole/system publication, handles synthetic links, and retains clipboard success, failure, and size-limit behavior.
  - Evidence: Task 002 core/App publication and error matrices plus Task 003 complete-fence register regression.
- [x] Plain `y` retains the configured policy with Markdown as the default, while `Y` explicitly publishes plain text.
  - Evidence: Task 002 configuration matrices and Task 003 App whole-fence policy matrix.
- [x] README, configuration guidance, changelog, registry, palette/help rows, snapshots, and drift guards consistently expose `y` and `Y`.
  - Evidence: Task 002 documentation, registry, projection, and golden updates.
- [x] Public API and dependency boundaries are unchanged and no dependency was added.
  - Evidence: public API, dependency hygiene, deny, audit, and unchanged manifest/lockfile checks.
- [x] A complete rendered characterwise or linewise fence selection copies the exact opening delimiter, info marker, body bytes, closing delimiter, and selected final line ending with default `y`.
  - Evidence: Task 003 complete-fence source matrix and App clipboard-policy matrix.
- [x] Complete-fence fidelity holds forward/reverse for empty and multiline bodies, backtick/tilde and longer delimiters, known/unknown languages, nested lists/blockquotes, multiple fences, and absent/present final newlines.
  - Evidence: Task 003 empty, source-matrix, multiple-region, and private renderer-region regressions.
- [x] Content-only characterwise and linewise code yanks preserve exact body whitespace while omitting fences and language markers.
  - Evidence: `rendered_code_fence_content_only_yanks_do_not_add_delimiters`.
- [x] Complete-fence `y` and `Y` keep exact Markdown in internal registers; `Y` publishes syntax-free body text; configured and prefixed register behavior remains intact.
  - Evidence: Task 003 exact `p`/`P`, App policy, and existing register-publication matrices.
- [x] Empty complete fences produce real clipboard/register yanks.
  - Evidence: forward/reverse characterwise and linewise empty-fence regression.
- [x] Synthetic gutter atoms remain source-less, private fence metadata invalidates with layout, resize preserves boundary endpoints, and non-yank geometry remains unchanged.
  - Evidence: renderer-region, layout-rebuild, content-only, blockwise, and delete-geometry regressions.
- [x] The complete repository quality gate passes.
  - Evidence: final `make check` passed all seven checks.

## Final quality gate

| Command | Result |
| ------- | ------ |
| `make check` | PASS |

## Cumulative diff

From baseline `50d6e763698d69d7d5b26e405b37d7a817bfc5eb`, the package repairs core source-envelope and internal-register yank fidelity; adds the core-owned one-shot plain-text `Y`; updates the TUI registry, palette, snapshots, README, and changelog; retains private parser-leaf fenced-code provenance with the rendered layout; expands only whole-fence yank projections; and adds focused renderer, core session, Vim/register, App, resize, and operator regressions. It adds no dependency, public API, clipboard format, or source-mode binding.

## Remaining non-blocking concerns

None
