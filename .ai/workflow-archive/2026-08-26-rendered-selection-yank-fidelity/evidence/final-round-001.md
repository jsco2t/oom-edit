# Final Work-Package Evidence

## Work package

- ID: `2026-08-26-rendered-selection-yank-fidelity`
- Title: Rendered Selection Yank Fidelity

## Original objective

Make rendered Select `y` preserve exact Markdown formatting, whitespace, and physical newlines, and provide a discoverable alternate action for occasional syntax-free clipboard output.

## Completed tasks

- Task 001 — Preserve Rendered Yank Source — `evidence/001.md`
- Task 002 — Add Plain-Text Yank Binding — `evidence/002.md`

## Whole-package review

The integrated review compared the cumulative implementation with the approved request, plan, task criteria, architecture constraints, and regression risks. Exact source payload construction remains provenance-driven and separate from destructive selection geometry; the private Vim adapter receives a consumer-owned exact yank payload; `Y` changes only the outgoing representation; and the TUI remains a thin format-selecting clipboard host. Registry, palette, snapshots, README, changelog, and tests agree on the two actions.

## High-confidence findings fixed

- Corrected stale regressions that expected source spaces or inline delimiters to be stripped.
- Corrected a partial-boundary test fixture so it represented the full visible construct while selecting only its interior.
- Aligned synthetic-link `y`/`Y` behavior with named and black-hole publication isolation.
- Refined registry wording to describe Markdown as the default without contradicting configured plain-text `y` behavior.

## Final acceptance criteria

- [x] Default rendered character `y` preserves exact canonical Markdown across physical and blank lines, including indentation and trailing spaces.
- [x] Fidelity coverage includes forward/reverse selection, wrapping, Unicode, headings, lists, fences, code spans, links, escapes, entities, tables, and repeated/ambiguous provenance.
- [x] Linewise and blockwise yanks retain their physical-line and rectangular-row register semantics.
- [x] Internal registers receive faithful Markdown and `p`/`P` round-trip multiline source without lossy conversion.
- [x] Delete, change, indent, outdent, selection painting, and mutation ranges remain unchanged.
- [x] Uppercase `Y` is a core-owned, non-destructive, one-shot syntax-free copy that exits Select and publishes once.
- [x] `Y` keeps Markdown-safe registers, preserves register publication rules, handles synthetic links, and uses existing success, failure, and size-limit behavior.
- [x] Plain `y` retains the backward-compatible configured policy with Markdown as its default, while `Y` explicitly publishes plain text.
- [x] README, changelog, registry, palette/help, snapshots, and drift guards consistently expose `y` and `Y`.
- [x] Public API and dependency boundaries are unchanged.

## Final quality gate

| Command | Result |
| ------- | ------ |
| `make check` | PASS |

## Cumulative diff

The package changes the core clipboard source-envelope builder, private session-to-Vim yank DTO path, rendered Select routing, TUI registry and palette projections, focused core/App regressions, two palette goldens, and user documentation. It adds no dependency, public API, source-mode binding, or alternate command registry.

## Remaining non-blocking concerns

None
