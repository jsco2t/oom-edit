# Final Work-Package Evidence

## Work package

- ID: `2026-09-28-rendered-markdown-performance`
- Title: Rendered Markdown performance
- Active approved plan: revision 3, acceptance round 1

## Original objective

Investigate the same-machine large-note stalls, fix verified bottlenecks, and preserve a first-class standalone editor and equally faithful embeddable core/pane. The later approved priority was prompt ordinary editing and scrolling with no loading/interstitial state, even if the first faithful render takes longer.

## Completed tasks

| Task | Result |
| --- | --- |
| 001 — Reproducible performance fixtures and public-pane harness | [Evidence](001.md) |
| 002 — Share compiled highlight queries | [Evidence](002.md) |
| 003 — Indexed rendered cursor mapping | [Evidence](003.md) |
| 004 — Linear large-fence highlighting and source parse reuse | [Evidence](004.md) |
| 005 — Preserve layout across metadata-only saves | [Evidence](005.md) |
| 006 — Instrument interaction work and exact reference | [Evidence](006.md) |
| 007 — Bounded incremental feasibility | [Evidence](007.md) |
| 008 — Incremental source analysis transaction | [Evidence](008.md) |
| 009 — Retained Markdown model and global dependencies | [Evidence](009.md) |
| 010 — Indexed retained layout and viewport | [Evidence](010.md) |
| 011 — Public-pane interaction and cold performance | [Evidence](011.md) |
| 012 — Standalone and embedded fidelity acceptance | [Evidence](012.md) |

## Whole-package review

The initial report was reproduced and qualified: repeated code-fence query compilation, cursor fallback scanning, large-fence span work and needless save invalidation were real; cursor cost depended on the mapped position. The scratch two-fix result alone did not meet the realistic mixed-note cold envelope or ordinary 1 MiB interaction priority. The delivered core retains one authoritative `LiveDocument` text owner and synchronously publishes current-text source analysis, Markdown blocks, mapped rows and indexes. The complete renderer remains a differential oracle; no loading state, stale editable frame, background job, second mutable text owner, new dependency or host-specific rendering shortcut was added. Standalone and embedded hosts share the same core session/pane behavior.

The cumulative review checked the original request and all active revision-2/3 criteria, exact style/source-atom/cursor/selection tests, host input and lifecycle parity, API/privacy guards, dependency direction, cache bounds and invalidation, cleanup of obsolete feasibility code, malformed runner data, the worktree diff from the baseline SHA and untracked production/test files. No unresolved high-confidence defect remains.

## High-confidence findings fixed

- Reload temporarily held an obsolete retained block model alongside replacement source caches, raising peak RSS to 243.5 MiB. Releasing it inside the reload mutation transaction lowered measured RSS to 197.9 MiB, below the 200.5 MiB baseline, without changing reload behavior.
- The interaction parser accepted a zero RSS sample. It now rejects missing memory data, with a contract test, so unavailable platform measurements fail closed.
- The test-only feasibility prototype and its obsolete benchmark commands were removed after production retained-path tests and the independent full-build oracle covered the same behavior.

## Final acceptance criteria

1. The 1 MiB mixed public-pane interaction gate passed 2,310 verified samples: ordinary local source typing and line edits, rendered delete/undo and source/rendered scrolling all met their top/midpoint p99 limits. The final run's rendered delete/undo p99 was at most 23.0 ms; source scroll p99 at most 15.1 ms. No pending or stale editable frame was used.
2. Five first-ever source-to-rendered returns per position remained below 500 ms (final run 182.5 ms top and 185.0 ms midpoint worst). The subsequent 100 returns per position had p99 about 1.2–1.3 ms and worst at most 1.7 ms, below 50 ms. The separate cold open-to-rendered gate passed below 500 ms at both widths.
3. The final 182-process realistic gate measured 1 MiB mixed first frames of 488.9 ms worst at 100×41 and 491.2 ms at 70×28. Previously passing 350 ms cells, the fixed 448 KiB table allowance, layout-growth limits and memory non-regression checks all passed; the legacy release fixture still passed its original 64 MiB/192 MiB budget in `make bench`.
4. Differential/property suites compare retained and fresh full layouts, highlighting, byte-mapped atoms, semantic styles, cursor/selection operations, global constructs, Unicode, CRLF and random edit/undo sequences. Counters prove bounded ordinary source/model/row rebuilding and indexed viewport reads. Global invalidation takes the documented wider path.
5. Direct core, public pane, standalone terminal adapter and split-pane host tests exercise the shared edit vector and preserve four modes, owned cells/modifiers, source cursor, save, external reload, tab switching and no-data-loss protections. API/privacy/dependency guards and public documentation pass.
6. The complete seven-command final gate below passed without warnings. No parser or dependency family was added, and `docs/performance.md` records fixture identity, thresholds, measured distributions and the global-edit caveat.

## Final quality gate

| Command | Result |
| --- | --- |
| `make check` | PASS — format, lint, build, full tests, deny, audit, data licenses |
| `make bench` | PASS — existing release budgets |
| `make bench-realistic` | PASS — 182 process measurements |
| `make bench-interactions` | PASS — 2,310 verified samples |
| `make test-public-api` | PASS |
| `make test-embedding-example` | PASS |
| `make test-standalone-host` | PASS |

Additional task validation passed `make test-incremental`, `make build-examples`, `make coverage-check` (191 cases/91 requirements), `make doc`, and the runner contract tests.

## Cumulative diff

From baseline `0e26684733551691a49a2ff7111cec9404fdd2d3`, the working tree contains the core source/parser/query/cache, retained block/row, viewport/cursor and save/reload changes; public pane/standalone/split-host adapters and parity tests; deterministic fixture/benchmark scripts and Make targets; performance documentation; and the workflow plan/evidence. Cargo dependency files are unchanged. The worktree has not been committed or archived by this workflow.

## Remaining non-blocking concerns

The 1 MiB mixed cold frame has a narrow same-machine margin below 500 ms. Globally invalidating edits and reload can still take hundreds of milliseconds, and an extreme 1 MiB Rust fence takes roughly 600 ms for full layout. These are explicitly measured and documented; there is no universal 50/500 ms promise for adversarial Markdown.
