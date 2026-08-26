# Final Work-Package Evidence

## Work package

- ID: `2026-08-25-theme-support-and-gutter-diagnostics`
- Title: Theme Support and Gutter Diagnostics
- Acceptance round: 3
- Approved plan SHA-256: `861c272c87cb5241f500304ffcc8a4138b08678b8a5b59c4a98523a53321611b`

## Original objective

Deliver the complete owned theme catalog, bundled and strict custom themes, independently themed gutters, bounded per-tab diagnostic markers, documentation and licensing guards, and enforceable source/rendered performance contracts. Acceptance rounds 2 and 3 refine the shared gutter to place markers before aligned line numbers and remove one trailing blank cell, yielding exact rows such as `W 73 |Line of text` without changing diagnostic behavior.

## Completed tasks

- Task 001: Performance Harness and Pre-change Baseline — `evidence/001.md`.
- Task 002: Theme Provenance and Canonical Notices — `evidence/002.md`.
- Task 003: Owned Theme Catalog and Built-ins — `evidence/003.md`.
- Task 004: Custom Theme Loading and Startup Integration — `evidence/004.md`.
- Task 005: Gutter Trouble State and Bounded Builder — `evidence/005.md`.
- Task 006: App Idle Scheduling and Per-tab Integration — `evidence/006.md`.
- Task 007: Themed Gutter Rendering and Trouble Markers — `evidence/007.md`.
- Task 008: Integration, Documentation, and Repository Guards — `evidence/008.md`.
- Task 009: Candidate Performance Evidence and Comparison — `evidence/009.md`.
- Task 010: Relocate Gutter Markers Before Line Numbers — `evidence/010.md`.
- Task 011: Reduce Gutter Right Padding — `evidence/011.md`.
- Consolidated prior-round history: `evidence/final-round-001.md` and `evidence/final-round-002.md`.

## Whole-package review

Reviewed the cumulative baseline diff, all eleven task documents and evidence files, the approved plan hash, repository status, and every final gate. The package retains the one-way core/TUI boundary, one private owned theme catalog, canonical diagnostic ownership, bounded per-tab projection, shared pure gutter painter, accessibility carriers, strict startup loading, canonical attribution, and Make-owned performance enforcement. Round 3 is confined to the shared gutter gap and mechanically derived source/rendered geometry; it does not alter marker state, style, public APIs, dependencies, licenses, theme assets, or retained performance evidence.

## High-confidence findings fixed

- Task 001 identified and replaced the rendered layout's repeated source-prefix scan with linear line-number indexing, restoring attainable source and full-rendered startup contracts.
- Task 003 corrected cursor and carrier details exposed while integrating owned theme lowering.
- Task 006 removed a test-only unbounded allocation while preserving bounded idle scheduling.
- Task 010 moved the existing diagnostic marker into the leading alignment cell, preserving gutter width and body origin.
- Task 011 repaired exact cursor, viewport, wrapping, scrolling, provenance, diagnostic-projection, and golden expectations coupled to the intentional one-column body-origin shift.

## Final acceptance criteria

- [x] Tasks 001–004 provide a real TUI performance harness, retained comparable evidence, the focused rendered-layout repair, canonical palette provenance/licenses, one owned eight-theme catalog, and strict deterministic custom-theme startup loading.
- [x] Tasks 005–007 provide compact immutable diagnostic summaries, explicit severity priority, bounded generation-cancelled idle construction, per-tab invalidation/publication, and themed accessible markers in both shared source and rendered gutters.
- [x] Task 008 provides end-to-end integration, CLI/README/performance documentation, canonical `--licenses`, data-license guards, public-API/dependency/color-locality checks, and Make discoverability.
- [x] Task 009 provides source/rendered absolute budgets, the separate full-rendered layout targets, memory and scaling enforcement, five-trial candidate evidence, quiescence and marker-scaling cases, and metadata-checked baseline comparison.
- [x] Task 010 places every `E`/`W`/`I`/`H` marker at gutter column zero before the unchanged aligned number, including absolute, relative, wrapped, synthetic, clipped, scrolled, and monochrome cases.
- [x] Task 011 changes the shared content gap from two cells to one and no renderer-specific adjustment exists.
- [x] Line 73 is exactly `  73 ` unmarked and `W 73 ` for a warning, composing as `  73 |Line of text` and `W 73 |Line of text`.
- [x] The number field retains its width and alignment across absolute, current, and signed hybrid-relative 9/10/999/1000 boundaries, while gutter width and both body origins reduce by exactly one cell.
- [x] Marker glyph, severity precedence, foreground, background, modifier, monochrome carrier, publication, snapshot ownership, and memory behavior remain unchanged.
- [x] Cursor, viewport, wrapping, selection/provenance, horizontal scrolling, continuation/synthetic rows, zero/narrow rectangles, and gutter-background fill remain consistent at the compact width.
- [x] No Cargo dependency, lockfile, vendor source, public facade, terminal-free core boundary, theme asset, license contract, or retained baseline/candidate evidence changed during round 3.

## Final quality gate

| Command | Result |
| --- | --- |
| `make fmt-check` | PASS |
| `make check` | PASS — format, lint, build, full tests, deny, audit, and data licenses |
| `make bench-check` | PASS — spell, core, and TUI debug performance suites |
| `make bench` | PASS — release source, rendered, layout, marker, memory, scaling, and quiescence gates |
| `make tui-perf-compare BASELINE=…/baseline.tsv CANDIDATE=…/candidate.tsv OUTPUT=…/comparison.tsv` | PASS — retained evidence is complete, comparable, and within thresholds |

The final live 1 MiB cold rendered layout measured 176.39 ms worst against the 250 ms target and retained 46,869,472 bytes against the 64 MiB target. All source/rendered first-frame, steady-frame, dense-gutter, idle projection/cancellation, and quiescence cases passed.

## Cumulative diff

From baseline `a1a508344941ed7c8407080767bfa8354474a84f`, the tracked package diff spans 43 files with 4,667 insertions and 709 deletions, plus the approved new workflow files, theme assets, gutter module, performance harness, documentation, and scripts. It delivers the owned theme system, strict theme discovery, canonical notices, themed compact gutters, bounded diagnostic projection, marker-first presentation, and performance automation without dependency or public-API expansion.

## Remaining non-blocking concerns

None
