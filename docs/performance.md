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
The first-frame cases construct a fresh App and draw once, while edit and
scroll cases include their mutation or motion boundary. The rendered
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
