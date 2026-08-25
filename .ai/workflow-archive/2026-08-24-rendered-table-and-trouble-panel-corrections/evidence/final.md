# Final Work-Package Evidence

## Work package

- ID: `2026-08-24-rendered-table-and-trouble-panel-corrections`
- Title: Rendered Table and Trouble Panel Corrections

## Original objective

Correct rendered-table word wrapping and source-driven wrapped selection, improve Trouble panel alignment and exclusive cursor ownership, and replace the superseded alternating-row backgrounds with faint, resize-aware, source-less dashed boundaries between adjacent table body rows.

## Completed tasks

- Task 001 — Word-boundary table wrapping — `evidence/001.md`
- Task 002 — Source-driven wrapped-table selection — `evidence/002.md`
- Task 003 — Configurable alternating table rows — `evidence/003.md` (completed historical implementation, superseded and removed by Task 005)
- Task 004 — Trouble panel layout and cursor ownership — `evidence/004.md`
- Task 005 — Replace row backgrounds with dashed boundaries — `evidence/005.md`

## Whole-package review

Reviewed the cumulative product diff from baseline `d70b27b471ba872165056f8c4b26435723fb1cd2`, the approved request and revision-2 plan, all five task specifications, and every task evidence file. The integrated design keeps table wrapping and boundary geometry in core, uses parser-leaf provenance without reconstruction, keeps Vim operation DTOs consumer-owned, and confines terminal styling/cursor ownership to the TUI. Task 005 fully supersedes Task 003's runtime behavior; only the older-config compatibility test retains the removed setting's text. No dependencies, vendored sources, Markdown parsing behavior, or developer commands changed. The pre-existing user-owned workflow-skill modification was left untouched and excluded from review conclusions.

## High-confidence findings fixed

- Preserved exact mapped whitespace expectations by distinguishing visible interior spaces from intentionally consumed wrap separators.
- Kept the expanded selection DTO lint-clean without changing its public semantics or leaking it into `vim.rs`.
- Retained existing nearest-header source metadata for the synthetic table top border while adding nearest-preceding-row metadata for body boundaries.
- Added explicit character/line/block selection and yank coverage after review identified that only character-selection boundary safety had initially been exercised.

## Final acceptance criteria

- [x] Constrained table prose wraps at the last fitting word boundary; only overlong tokens hard-wrap.
  - Evidence: mapped table wrapper unit/integration coverage and the requested `content` sentence regression; Task 001 evidence.
- [x] Wrapped rows remain equal-width and mapped atoms retain exact UTF-8 provenance through styles, repetition, and Unicode.
  - Evidence: width/alignment, CJK/emoji, repeated-text, and byte-exact provenance tests; Task 001 evidence.
- [x] Wrapped-table character selection follows only canonical selected source text and operators use the same exact ranges.
  - Evidence: non-linear projection, independent interval, forward/reverse, resize, yank/delete/change, and neighboring-cell exclusion tests; Task 002 evidence.
- [x] Character, line, and block selection remain deterministic across width changes and outside tables.
  - Evidence: the complete conformance matrix plus Task 005's three-shape boundary/operator regression; Tasks 002 and 005 evidence.
- [x] Alternating-row configuration, roles, theme slots, backgrounds, documentation, and obsolete tests are absent.
  - Evidence: runtime/API/theme/config cleanup, residue search, and old-config ignored/not-serialized regression; Task 005 evidence.
- [x] Tables contain exactly `N-1` faint dashed boundaries, placed only after complete logical body rows.
  - Evidence: zero-through-three-row counting and wrapped-logical-row placement tests; Task 005 evidence.
- [x] Boundaries retain vertical borders, fill allocated interiors with ASCII dashes, match table width, and recompute after resize.
  - Evidence: exact output, equal-width, muted-style, all-theme-tier, and forced 80-to-100-column allocation tests; Task 005 evidence.
- [x] Boundary glyphs are synthetic/source-less and do not alter selected source text, operator payloads, or canonical navigation.
  - Evidence: atom provenance, three selection shapes, yank payload, and two-step navigation regressions; Task 005 evidence.
- [x] Trouble locations, severities, providers, and messages align stably across viewport and terminal states.
  - Evidence: complete-snapshot display-width calculation, scrolling/narrow/progress coverage, and updated snapshot; Task 004 evidence.
- [x] Trouble hides the document cursor over Normal, Select, and Insert and restores it after close or jump.
  - Evidence: App-level cursor visibility and atomic-jump restoration tests; Task 004 evidence.
- [x] Public API guards and intentional goldens/snapshots are current, with a warning-free complete repository gate.
  - Evidence: curated API/dependency guards, updated core/TUI snapshots, and both standard and final `make check` passes.

## Final quality gate

| Command | Result |
| ------- | ------ |
| `make check` | PASS |

## Cumulative diff

From the baseline, the package replaces fixed table-cell chunking with mapped word wrapping, expands rendered selection rows to independent display intervals, makes character selection source-driven, aligns Trouble entries using stable display-cell widths, suppresses the document cursor while a modal owns presentation, and renders muted synthetic dashed boundaries between adjacent table body rows. Focused tests, conformance coverage, API guards, core goldens, and TUI snapshots were updated. No dependency or vendored-source changes were made.

## Remaining non-blocking concerns

None
