# Task 001: Performance Harness and Pre-change Baseline

Delegation: main-only

## Goal

Add the private TUI performance measurement/comparison infrastructure, establish separate source and large rendered-document NFRs, repair the localized rendered-layout scaling defect, document the contract, and retain a clean five-trial baseline before any theme catalog, gutter styling, or marker production behavior changes.

## Context

The PRD makes honest pre-change evidence a prerequisite. Existing core/spell benchmarks do not exercise `App::render`, `TestBackend`, TUI idle quiescence, process CPU, or peak RSS. The first five-trial run revealed that the new TUI case measured rendered Normal startup while applying NFR-1's source-frame ceiling. Investigation also found that rendered line-number generation rescans source prefixes per row. The human accepted separate source and rendered targets plus a focused linear-time repair; the failed run remains evidence rather than being discarded.

## Scope

### In scope

- A private/test-performance TUI harness for unchanged source/rendered empty-marker-equivalent rendering, distinct source and rendered first frames, edit-to-frame, scroll, memory stability, and idle quiescence.
- A focused replacement for quadratic rendered line-number projection using one line index or an equivalent linear-time algorithm, with exact-numbering regression tests.
- Deterministic retained-layout and App work/memory counters and fixed 256 KiB, 512 KiB, and 1 MiB fixtures without exposing new public API.
- Versioned TSV v1 model, serializer/parser, validation, median/worst aggregation, threshold comparator, atomic output, and unit tests.
- macOS/Linux `/usr/bin/time` normalization through a make-owned script.
- Make targets `tui-perf-record` and `tui-perf-compare`, plus extensions to `bench-check` and `bench`, all documented by `make help`.
- Repository documentation of the exact fixtures, profiles, timing semantics, ceilings, memory definitions, scaling rule, and make commands.
- Byte-for-byte retention of the failed five-trial v1 run as `evidence/performance/pre-optimization-baseline.tsv`, followed by five post-repair/pre-theme baseline trials written atomically to `evidence/performance/baseline.tsv` with `branch-role=baseline`, bumped fixture semantics, and complete environment metadata.

### Out of scope

- Owned theme/catalog production changes, new palettes, user-theme loading, gutter styling, or diagnostic marker state/rendering.
- Sparse/dense marker candidate cases, which do not exist in the unchanged editor.
- Viewport-lazy layout, provenance atom compaction, or other rendered storage/navigation redesign.
- New dependencies, unsafe allocator hooks, or public benchmark API.

## Implementation requirements

- Keep the harness inside `oom-edit` behind test/performance configuration and invoke it through make-owned commands.
- Reuse deterministic repository fixtures and a warmed, reused `TestBackend`; setup/layout construction stays outside steady-state timed regions, while cold source/rendered first-frame cases intentionally include their documented construction work.
- Keep NFR-1 unchanged as construction plus the first highlighted 1 MiB source frame below 150 ms worst. Measure it through an explicitly source-mode App case rather than inferring surface from the default session mode.
- Extend NFR-4 with a pinned 1 MiB/approximately 100-column large-document contract: cold `render_layout` below 250 ms worst, end-to-end rendered first frame below 350 ms worst, retained layout heap at most 64 MiB, and one-shot peak RSS at most 192 MiB on the evidence host.
- Enforce 256 KiB → 512 KiB → 1 MiB scaling: neither cold layout time nor retained heap may grow by more than 2.25× on either doubling. Retained heap deterministically counts every owned layout vector/string capacity; RSS remains process evidence rather than allocator instrumentation.
- Preserve exact rendered line-number behavior, source provenance, navigation, and layout outputs while removing repeated source-prefix newline scans. Add focused regression and scaling coverage and run the complete core suite.
- Emit exactly the PRD's fixed TSV columns, reject tabs/newlines, use one-based trials, and require at least five complete trials per comparable case.
- Reject schema/toolchain/OS/architecture/CPU/logical-CPU/memory/fixture mismatches before aggregation.
- Implement exact dual timing/CPU thresholds, RSS threshold, NFR ceilings, and typed machine-readable status values with integer boundary tests.
- Preserve ordinary `make test` reliability; timing smoke is serialized and reached through `make bench-check`, while exact release evidence is reached through `make bench`/`tui-perf-record`.
- Bump the fixture version when splitting source/rendered first-frame semantics so the comparator rejects the earlier incompatible rows. Preserve the original failed file under its historical name before writing the new baseline.
- If the repaired pre-theme editor fails either source or rendered absolute gate, grows memory, fails quiescence, produces unstable/incomparable metadata, or cannot complete five trials, stop the workflow and report evidence. Do not weaken thresholds or continue.
- Do not change Cargo dependencies, lockfile, vendor tree, or production theme/gutter/marker behavior.

## Acceptance criteria

- [ ] TSV serializer/parser/comparator tests cover fixed columns/units, invalid values, incomplete/duplicate trials, metadata mismatch, medians/worst values, and every threshold boundary.
- [ ] `make help` exposes the new evidence and comparison workflows, and existing `make bench-check`/`make bench` exercise the appropriate TUI cases in addition to existing suites.
- [ ] Repository performance documentation distinguishes NFR-1 source startup from large-document NFR-4 rendered layout/startup and states all fixture, time, heap, RSS, scaling, and reproduction contracts exactly.
- [ ] Exact rendered line numbers and provenance remain unchanged, while 256 KiB/512 KiB/1 MiB release evidence proves cold-layout time and retained heap meet the 250 ms, 64 MiB, and 2.25× gates.
- [ ] The private harness proves unchanged steady-state rendering does not grow App-owned state and clean idle work becomes quiescent.
- [ ] Source first frame passes 150 ms worst; rendered first frame passes 350 ms worst; the one-shot rendered case passes 192 MiB peak RSS on the baseline host.
- [ ] The original five-trial failed evidence remains byte-for-byte available as `pre-optimization-baseline.tsv`, with its old fixture version/status intact.
- [ ] Five complete baseline trials are retained at `/Users/jason/Developer/sources/personal/notebook/projects/oom-edit/features/04-themes/evidence/performance/baseline.tsv` with the required schema and baseline role.
- [ ] The post-repair/pre-theme baseline passes all source and rendered absolute NFR and benchmark gates and contains no unexplained retry, memory growth, or idle spin.
- [ ] Production theme, gutter, and marker behavior is unchanged and no public API/dependency change is introduced.

## Validation

- `make fmt-check`
- `cargo test -p oom-edit-core --offline --locked`
- `make test`
- `make bench-check`
- `make bench`
- `make tui-perf-record BRANCH_ROLE=baseline OUTPUT=/Users/jason/Developer/sources/personal/notebook/projects/oom-edit/features/04-themes/evidence/performance/baseline.tsv TRIALS=5`

## Dependencies

None

## Expected areas of change

- `crates/oom-edit/src/lib.rs`
- `crates/oom-edit/src/perf_tests.rs`
- `crates/oom-edit-core/src/rendered/mod.rs`
- `crates/oom-edit-core/benches/performance.rs`
- `crates/oom-edit-core/tests/perf_smoke.rs`
- `docs/performance.md`
- `README.md`
- `Makefile`
- `scripts/` performance wrapper/comparator support
- External PRD `evidence/performance/baseline.tsv`

## Risks / notes

The performance repair deliberately precedes the comparison baseline and is part of both baseline and candidate; the retained pre-optimization file prevents this from becoming a silent rebaseline. Baseline evidence must still precede theme/gutter/marker production changes. Writing beside the external PRD may require explicit filesystem authorization. `/usr/bin/time` output differs between macOS and Linux, so normalization and failure diagnostics must be tested without making unit tests depend on live timing.
