# Final Work-Package Evidence

## Work package

- ID: `2026-08-25-theme-support-and-gutter-diagnostics`
- Title: Theme Support and Gutter Diagnostics
- Acceptance round: 2
- Approved plan SHA-256: `7c72c0b61a8fd7e0cf78cc63c613facb53e3edb618dd626b556b659fcb9f7a40`

## Original objective

Deliver the complete owned theme catalog, bundled/custom themes, independently themed gutters, bounded per-tab diagnostic markers, documentation/licensing guards, and source/rendered performance contracts. Round 2 additionally corrects the accepted marker geometry from `number + marker + separator` to `marker + aligned number + separator` without changing gutter or document layout.

## Completed tasks

- Tasks 001–009: round-1 feature delivery — evidence in `evidence/001.md` through `evidence/009.md`; consolidated history in `evidence/final-round-001.md`.
- Task 010: Relocate Gutter Markers Before Line Numbers — `evidence/010.md`.

## Whole-package review

Reviewed the cumulative baseline diff and all ten task/evidence pairs for original and follow-up requirement coverage, private/core boundaries, theme and diagnostic ownership, lifecycle invalidation, renderer geometry, accessibility carriers, parsing/startup behavior, licensing, Make discoverability, performance enforcement, and unintended scope changes. The round-2 implementation remains in the existing shared private gutter painter and preserves every formatter, viewport, cursor, body-origin, state, and memory boundary established in round 1.

## High-confidence findings fixed

- The shared painter selected the first trailing content-gap cell as `marker_column`, producing `  73W|Line of text`. It now replaces the first existing alignment cell, producing the requested `W 73 |Line of text` ordering while leaving all subsequent coordinates unchanged.
- The clipped-width contract needed explicit zero-width coverage. A regression assertion now proves zero width is a no-op and one/two-column gutters remain bounded and marker-first.

## Final acceptance criteria

- [x] Normal-width warning geometry is exactly marker-first: tests assert marked `W 73  ` and unmarked `  73  ` gutter rows with identical total width and body origin.
- [x] `E`, `W`, `I`, and `H` render at column zero in source and rendered modes with exact TrueColor, ANSI-16, Monochrome, background, and modifier styling.
- [x] Absolute/current/hybrid-relative line numbers retain signs, digits, alignment, and separator cells across 9/10/999/1000 boundaries.
- [x] Wrapped/synthetic rows remain marker-free, and rendered horizontal scrolling does not move markers away from their numbered source rows.
- [x] Zero-, one-, two-, and normal-width gutter cases are safe, bounded, and deterministic.
- [x] Gutter width, source/rendered body origin, cursor mapping, viewport calculations, diagnostic publication/invalidation, repeated-render quiescence, and retained memory remain unchanged.
- [x] README states that severity glyphs precede aligned line numbers.
- [x] All round-1 catalog, custom-theme, attribution, diagnostic-state, performance, dependency, public-API, and licensing acceptance criteria remain satisfied.

## Final quality gate

| Command | Result |
| --- | --- |
| `make fmt-check` | PASS |
| `make check` | PASS — format, lint, build, full tests, deny, audit, and data licenses |
| `make bench-check` | PASS — spell, core, and TUI debug performance suites |
| `make bench` | PASS — exact release source, rendered, layout, marker, memory, and quiescence gates |
| `make tui-perf-compare BASELINE=…/baseline.tsv CANDIDATE=…/candidate.tsv OUTPUT=…/comparison.tsv` | PASS — all preserved evidence rows |

Final live performance highlights: 1 MiB source first frame 109.77 ms worst against 150 ms; 1 MiB cold rendered layout 172.65 ms worst against 250 ms; retained rendered layout 46,869,472 bytes against 64 MiB. Every live TUI marker case passed.

## Cumulative diff

From baseline `a1a508344941ed7c8407080767bfa8354474a84f`, the package adds the owned theme system and assets, canonical notices/compliance, strict custom-theme loading, themed source/rendered gutters, bounded per-tab diagnostic projection, leading accessible gutter markers, documentation, and Make-owned performance infrastructure/evidence enforcement. Cargo dependencies, `Cargo.lock`, vendor sources, public core facade, and terminal-free core direction remain unchanged.

## Remaining non-blocking concerns

None
