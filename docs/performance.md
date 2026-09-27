# Performance contracts

oom-edit keeps source editing and rendered Markdown performance as separate
contracts. All automated developer workflows run through the top-level
`Makefile`; the commands below use locked offline dependencies.

## Fixed release fixtures

The large-document fixture is deterministic seeded Markdown. Its exact source
size is 1 MiB, its rendered width is 96 text columns (approximately a 100-column
terminal after the gutter), and the TUI viewport is 100 columns by 41 rows with
a 40-row document body. Scaling checks use the same generator and seed at
256 KiB, 512 KiB, and 1 MiB.

Release measurements use optimized test or bench binaries. Steady-state cases
warm code and allocator paths before recording. First-frame cases run exactly
one cold session/layout iteration per process so wall time, process CPU, and
peak RSS describe the same operation. Debug smoke ceilings are deliberately
relaxed and exist to catch catastrophic regressions; release gates own the
exact limits.

Dirty-diagnostic edit overhead retains its strict <100 µs limit. The benchmark
alternates matched edits with and without published diagnostics, verifies exact
diagnostic removal/shift outcomes, and records both nonzero raw timing totals.
Their additional duration is clamped to zero when the diagnostic path is equal
or faster. Zero **overhead** is valid; zero raw work is not. Deterministic smoke
tests reject missing timings, the exact 100 µs boundary and an intentionally
lowered zero budget, without weakening any absolute-operation gate.

## NFR-1: source first frame

Construction plus the first fully highlighted source viewport frame for the
1 MiB fixture must complete in less than 150 ms worst case. This case enters
source Insert mode explicitly; it does not measure the default rendered Normal
surface.

## NFR-4: rendered layout and first frame

The existing 5,000-line rendered-layout checks remain in force. The large
1 MiB fixture adds these limits:

| Measurement | Limit |
| --- | ---: |
| Cold full `RenderedLayout` construction | less than 250 ms worst |
| End-to-end rendered Normal first frame | less than 350 ms worst |
| Heap capacity retained by `RenderedLayout` | at most 64 MiB |
| One-shot process peak RSS on the evidence host | at most 192 MiB |

The retained-heap measurement sums the capacities owned by rendered lines,
text, style spans, source atoms, line numbers, jump targets, link rows, and link
destinations. It is deterministic and portable. Peak RSS includes the session,
temporary block model, final layout, terminal test backend, runtime, and
allocator behavior, so it is also protected by same-host baseline comparison.

For both cold-layout time and retained heap, neither 256 KiB → 512 KiB nor
512 KiB → 1 MiB may grow by more than 2.25 times. This scaling gate rejects
quadratic whole-document work without requiring viewport-lazy rendering or a
different provenance representation.

## Owned pane-frame conversion

Converting an already-rendered 200 × 60 off-screen buffer into owned cells,
including grapheme continuations and resolved styles, must take at most 1 ms
at the 95th percentile in an optimized build. `make bench-pane-frame` runs the
asserting fixture directly; `make bench` includes it alongside the existing
release gates. The limit covers conversion only, not Markdown layout or widget
drawing, which retain their separate budgets above.

## Commands and evidence

The evidence schema uses fixture version `oom-edit-tui-v2` and records these
baseline-comparable TUI cases: `source-render-empty`, `rendered-render-empty`,
`source-first-frame`, `rendered-first-frame`, `edit-frame`,
`source-scroll-frame`, and `idle-quiescent`. The final candidate also records
`source-gutter-sparse`, `rendered-gutter-sparse`,
`source-gutter-dense-5000`, `source-gutter-dense-50000`,
`rendered-gutter-dense-5000`, `rendered-gutter-dense-50000`,
`idle-gutter-project`, and `idle-gutter-cancel`.

The empty, sparse, and dense render cases are warmed steady-state paints. The
dense pairs use the same 50,100-line document and change only the number of
off-screen markers; increasing 5,000 markers to 50,000 may add at most 25% to
median wall and CPU time with a 40-row viewport. A completed snapshot may own
at most 24 bytes per unique marked line plus 4 KiB fixed overhead. Projection,
cancellation, and final quiescence each remain below 1 ms worst in release.
The first-frame cases construct a fresh pane using the unchanged text fixture
and draw through the standalone public host once, while edit and scroll cases
include their mutation or motion boundary. All render cases include owned-frame
production and copying into the host renderer. Repeated unchanged states share
an immutable owned cell snapshot and its renderer projection. App-owned
presentation revisions invalidate that snapshot on input, lifecycle, appearance,
focus and background-work transitions; which-key time and pane dimensions are
also cache inputs. Changed frames still perform complete widget drawing and
conversion, and the conversion-only gate always converts a full buffer. The rendered
first-frame row also records retained layout heap, and the process wrapper
records CPU and peak RSS for every case.

The retained pre-theme baseline predates marker state, so comparison requires
its seven baseline cases and all final candidate cases. Candidate-only cases
receive their absolute, snapshot-memory, and within-candidate off-screen
scaling rows rather than being silently treated as missing baseline data.

Run relaxed debug smoke and deterministic shape checks:

```console
make bench-check
```

Run exact release gates, including source and rendered TUI cases:

```console
make bench
```

For diagnosis, `make bench-first-frame-profile` reports Markdown parser setup,
the full block parse, session construction, source viewport and cold rendered
layout costs using the same fixed 1 MiB fixture. It is not an acceptance gate;
`make bench` still owns all unchanged release limits. `make test-first-frame`
checks the patched grammar's Cargo-profile contract, pre-batching named-tree
and highlighting characterization, structural-edit equivalence, and byte-exact
parser-leaf/wrapping mapping. Optimized Cargo profiles compile both generated Markdown parsers with
the host's optimization settings; level-zero debug profiles keep the upstream
optimization-off directives.

Eligible top-level prose lines reuse the block scanner's existing whole-line
table check as a batched token; the full block tree and complete inline syntax
remain available synchronously. Rendered wrapping borrows styled text and moves
mapped fragments instead of cloning temporary per-character text and source
atoms. Neither optimization changes fixtures, limits or required work.

Record five same-host trials and compare them:

```console
make tui-perf-record BRANCH_ROLE=baseline OUTPUT=/path/to/baseline.tsv TRIALS=5
make tui-perf-record BRANCH_ROLE=candidate OUTPUT=/path/to/candidate.tsv TRIALS=5
make tui-perf-compare BASELINE=/path/to/baseline.tsv CANDIDATE=/path/to/candidate.tsv OUTPUT=/path/to/comparison.tsv
```

The TSV comparator rejects incompatible fixture versions, toolchains, operating
systems, architectures, CPU models, logical CPU counts, memory sizes, missing
cases, duplicate trials, and fewer than five trials. Comparable steady-state
timing and CPU regress only when both 10% and 100 µs are exceeded; RSS and
deterministic heap regress only when both 5% and 1 MiB are exceeded.

If a resumed session has different machine metadata (including visible CPU
count), preserve the original evidence and record a fresh immutable baseline
on the current host. Do not edit metadata to make incompatible runs compare:

```console
make tui-perf-baseline-prepare BASELINE_DIR=/path/to/new-empty-directory BASELINE_REV=<full-baseline-commit>
make tui-perf-record PERF_ROOT=/path/to/new-empty-directory BRANCH_ROLE=baseline OUTPUT=/path/to/fresh-baseline.tsv TRIALS=5
```

The local clone leaves the active worktree untouched. The current recorder can
measure that clone's original executable, retaining original first-frame limit
failures as baseline evidence while still enforcing every candidate limit.
