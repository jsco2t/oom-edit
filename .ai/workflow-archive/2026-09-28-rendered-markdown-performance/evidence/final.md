# Final Work-Package Evidence

## Work package

- ID: `2026-09-28-rendered-markdown-performance`
- Title: Rendered Markdown performance
- Approved plan revision: 7; acceptance round: 2

## Original objective

Investigate the large-note stalls reported by an embedding application and deliver a responsive, fully faithful Markdown viewer/editor through the same terminal-free core path for standalone and embedded hosts. The approved revisions prioritize ordinary editing and scrolling over a rigid first-frame budget, require exact Select deletion/change semantics, disallow loading or stale-editable states, and require test-forward memory-leak evidence before accepting a narrow Rust-fence RSS allowance.

## Completed tasks

| Task | Evidence |
| --- | --- |
| 001 — Reproducible performance fixtures and public-pane harness | `evidence/001.md` |
| 002 — Share compiled highlight queries | `evidence/002.md` |
| 003 — Indexed rendered cursor mapping | `evidence/003.md` |
| 004 — Linear large-fence highlighting and source parse reuse | `evidence/004.md` |
| 005 — Preserve layout across metadata-only saves | `evidence/005.md` |
| 006 — Instrument interaction work and exact reference | `evidence/006.md` |
| 007 — Bounded incremental feasibility | `evidence/007.md` |
| 008 — Incremental source analysis transaction | `evidence/008.md` |
| 009 — Retained Markdown model and global dependencies | `evidence/009.md` |
| 010 — Indexed retained layout and viewport | `evidence/010.md` |
| 011 — Public-pane interaction and cold performance | `evidence/011.md` |
| 012 — Standalone and embedded fidelity acceptance | `evidence/012.md` |
| 013 — Reproduce real-fixture interaction and terminal latency | `evidence/013.md` |
| 014 — Bounded Select and large-fence feasibility gate | `evidence/014.md` |
| 015 — Characterwise full-line delete and change semantics | `evidence/015.md` |
| 016 — Sustained rendered keyboard navigation | `evidence/016.md` |
| 017 — Indexed responsive rendered Select projection | `evidence/017.md` |
| 018 — Incremental large-fence code edits | `evidence/018.md` |
| 019 — Full interaction, fidelity and host acceptance | `evidence/019.md` |
| 020 — Recovery instrumentation and feasibility | `evidence/020.md` |
| 021 — Atomic multi-range live mutation | `evidence/021.md` |
| 022 — Retained multi-block and row publication | `evidence/022.md` |
| 023 — Bounded Rust-fence working set and leak stability | `evidence/023.md` |
| 024 — Integrated recovery acceptance | `evidence/024.md` |

## Whole-package review

The primary thread reviewed the approved request, revisions, all task evidence, cumulative diff from `0e26684733551691a49a2ff7111cec9404fdd2d3`, new retained-model/row and harness sources, public API guards, ownership and reload paths, documentation, and task-level raw results. The final code retains one authoritative live-document mutation path and a single fully faithful core projection for standalone and embedded hosts. No terminal dependency entered core, no second editable text owner or loading state was added, and no new external dependency was introduced. Structural/global semantic edits remain a measured wide path rather than a falsely claimed universal <50 ms case.

## High-confidence findings fixed

- During integrated acceptance, obsolete rendered rows were found alive while reload allocated replacement source analysis. Releasing them first reduced completed repeated-reload current RSS from roughly 266–273 MiB to 218–224 MiB. The unchanged three-process, 400-cycle gate then passed twice; `evidence/024-rss-failure.md` preserves the initial failure and diagnosis.
- Final review found `make check` omitted the new RSS runner contract tests although `make test` included them. The check target now runs them, and a contract test guards both inventories.
- Documentation was corrected to distinguish earlier observational reload timing from the later ownership-order change and to disclose cold-timing outliers, finite RSS scope, and PTY versus physical kitty presentation.

## Final acceptance criteria

1. Responsive ordinary 1 MiB edits, scrolling and warmed returns: final `make bench-interactions` passed 2,310 verified public-pane samples. Rendered line edit/undo p99 was at most about 20 ms; source scroll p99 was at most 15.3 ms; rendered scroll p99 was 1.0 ms; warmed source-to-rendered returns were about 1.2 ms p99. First-ever returns were about 186 ms and were gated separately. Raw task record: `evidence/024-interactions.jsonl`.
2. Exact 1 MiB Select and navigation acceptance: final `make bench-acceptance-1mb` passed 60,096 verified pane records plus standalone PTY trials. Navigation p99 remained near 1 ms across prose/Rust/Go before and after retained edits; Select removed the intended 11/14/15 physical lines in those regions. Final `make bench-acceptance-1mb-prose-cycles` passed 400 exact edit/undo cycles with edit p99 8.18–8.49 ms. Task-level Rust/Go 400-cycle edit gate passed p99 18.37–32.04 ms. Raw task records: `evidence/024-acceptance-1mb.jsonl`, `evidence/024-prose-cycles-after-order.jsonl`, `evidence/024-edit-cycles-after-order.jsonl`.
3. Cold and realistic-content envelope: final `make bench-realistic` passed 182 process measurements with 1 MiB mixed worst-of-five 488.2 ms at 100×41 and 491.6 ms at 70×28, plus all unchanged class, heap and scaling limits. The legacy `make bench` contracts also passed. Task-level raw realistic record is `evidence/024-realistic-after-order.jsonl`.
4. Memory and no-growth evidence: only Rust-fence peak RSS uses the approved fixed-baseline +15% cap; every other realistic memory and retained-heap limit is unchanged. Final `make bench-rss-stability` independently passed 24 fresh 400-cycle processes across Rust/Go delete, change, mode and reload. The task-level raw checkpoint record, bounded source-cache tests and ownership review are in `evidence/024-rss-stability-after-order.jsonl`, `evidence/023.md` and `evidence/024-rss-failure.md`. A finite run cannot establish universal leak-freedom, but no monotonically retained editor state was found on the tested paths.
5. Fidelity and both host models: complete `make check`, `make test-public-api`, `make test-embedding-example` and `make test-standalone-host` passed. Differential/property and conformance tests compare current cells, semantic styles, byte provenance, mapping, selection operations, undo, reload and host behavior against the complete builder and existing contracts. The standalone binary uses the same `EditorPane` and `oom-edit-core` path as embedded hosts.
6. No hidden rendering lifecycle: all accepted frames are current-text owned frames. No interstitial loading mode, stale editable layout, second parser dependency or second mutable document was introduced. Global reinterpretation, resize, cold open and extreme 1 MiB fences are separately measured and documented, not included in the ordinary local-edit bound.

## Final quality gate

| Command | Result |
| --- | --- |
| `make check` | PASS; seven stages, including the RSS runner contract test. |
| `make bench` | PASS; legacy release and pane-frame contracts. |
| `make bench-realistic` | PASS; 182 process measurements. |
| `make bench-rss-stability` | PASS; 24 fresh 400-cycle processes. |
| `make bench-interactions` | PASS; 2,310 verified samples. |
| `make bench-acceptance-1mb` | PASS; pane and standalone PTY. |
| `make test-public-api` | PASS. |
| `make test-embedding-example` | PASS. |
| `make test-standalone-host` | PASS. |
| `make bench-acceptance-1mb-prose-cycles` | PASS; 400 exact cycles. |

## Cumulative diff

From baseline `0e26684733551691a49a2ff7111cec9404fdd2d3`, the work adds deterministic realistic and exact kitchen-sink fixtures; public-pane, PTY, interaction and RSS harnesses; bounded source analysis and query reuse; retained Markdown blocks, mapped row chunks and indexes; exact characterwise full-line deletion/change with unchanged yank/copy behavior; reload ownership ordering; differential, property, conformance, API and host-parity tests; Make targets and performance documentation. The worktree is intentionally uncommitted; `final_sha` records the current `HEAD` as required by the workflow, not a new implementation commit.

## Remaining non-blocking concerns

- Two historical five-process runs each had one 1 MiB mixed cold sample at 501–502 ms against the unchanged 500 ms limit; subsequent idle repeats and the final gate passed. The raw misses are retained, not hidden or reclassified.
- The 2 MiB post-warmup RSS gate has a narrow margin in some Rust reload trials. The full gate passed twice after the ownership repair, but finite checkpoints cannot prove absence of every possible leak.
- PTY timing cannot prove when kitty physically presents pixels. The documented manual 63×229 release-build check remains supplementary; the user previously reported no visible hitch with this branch’s release binary.
