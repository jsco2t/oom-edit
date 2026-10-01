# Performance contracts

oom-edit keeps source editing and rendered Markdown performance as separate
contracts. All automated developer workflows run through the top-level
`Makefile`; the commands below use locked offline dependencies.

## Fixed release fixtures

The legacy large-document fixture is deterministic seeded Markdown. Its exact source
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

## Realistic Markdown and interaction gates

`make bench-realistic` measures one complete cold public-pane frame per fresh
process, including pane construction, open, rendered layout, drawing, and host
cell copy. It uses five processes per case at both 100×41 and 70×28, verifies
fixed fixture hashes and frame state, and checks peak process RSS against the
same-machine pre-incremental baseline plus 10%, except for a narrowly scoped
+15% cap on Rust-fence peak RSS. Retained-layout heap remains at +10% for
every class, including Rust fences. The fixture version is
`oom-edit-realistic-v1`; the authoritative hash inventory is in
`scripts/realistic_performance.py`. The 1 MiB mixed fixture ID is
`oom-edit-realistic-v1:mixed:1048576:a2669d94455fc82d`.

| Cold rendered content | Limit, worst of five | Measured worst, 100×41 / 70×28 |
| --- | ---: | ---: |
| 1 MiB prose | <350 ms | 188.1 / 187.8 ms |
| 1 MiB mixed | <500 ms | 493.0 / 491.9 ms |
| 144 KiB mixed | <350 ms | 110.5 / 109.3 ms |
| 128 KiB single Rust fence | <350 ms | 140.0 / 139.1 ms |
| 128 KiB many fences | <350 ms | 154.6 / 156.9 ms |
| 512 KiB lists | <350 ms | 314.6 / 315.6 ms |
| 448 KiB tables | <391.05 ms | 315.7 / 318.3 ms |

The mixed-note allowance does not change the legacy 1 MiB fixture's 350 ms
limit above. The table allowance is its fixed pre-incremental worst plus 10%;
all other previously passing realistic cells retain 350 ms. The same gate
checks 256/512/1024 KiB full-layout time growth at at most 2.25× per doubling
and retained heap / process RSS against those fixed class-specific limits.
The mixed cell is close to its cold limit: two separate five-process runs
contained one 501–502 ms sample, while their idle repeats passed and a
20-process cold diagnostic stayed below 492 ms. Those failed raw records are
retained in the workflow evidence; no threshold was raised for them.
For the 1 MiB mixed layout, the measured retained heap is 50.9 MiB and peak
RSS about 180 MiB. The separate legacy 64 MiB/192 MiB ceilings above apply
to that legacy fixture, not to every Markdown shape or to transient edits.

The Rust-fence RSS allowance preserves the retained source-injection parse
tree. Removing that tree saved about 30.7 MiB in a controlled 1 MiB ablation,
but made the first post-open Rust delete take about 708 ms. With highlighted
code lines consumed instead of cloned, the five-process worst Rust-fence peak
RSS measurements were:

| Rust-fence case | Fixed baseline peak RSS | Five-process candidate worst | Ratio |
| --- | ---: | ---: | ---: |
| 128 KiB cold, 100×41 | 33,419,264 B | 36,954,112 B | 110.58% |
| 256 KiB layout | 51,515,392 B | 57,032,704 B | 110.71% |
| 512 KiB layout | 94,531,584 B | 104,480,768 B | 110.52% |
| 1024 KiB layout | 180,727,808 B | 200,314,880 B | 110.84% |

This allowance is a working-set trade-off, not permission for RSS to grow
indefinitely. The 128 KiB cold case is also measured at 70×28; the same +15%
rule applies there.

`make bench-rss-stability` separately measures current Linux `VmRSS` after
completed, text-restored public-pane frames at cycles 100, 200, 300 and 400.
It exercises Rust and Go delete/change/undo, source-to-rendered transitions,
and disk reloads in three fresh processes per scenario on the exact 1 MiB
kitchen-sink fixture. Timing-sensitive edit processes run serially; reload
processes may run concurrently because they have no timing assertion. No later
checkpoint may exceed cycle 100 by more than
2 MiB. The edit cases also assert that the first post-open key-to-owned-frame
edit is below 50 ms. The core source-injection cache has an eight-entry and
1 MiB source-byte limit; tests check its ownership and invalidation across
edits and fence-language changes. A finite RSS test and source-byte bound
cannot prove universal leak-freedom or put an exact byte cap on tree-sitter's
internal allocations; they check the observed long-running paths and rule
out monotonically retained cache entries there.
In the 24-process acceptance run, the largest post-warmup increase was
2,007,040 B (1.91 MiB, Rust reload), and the largest Go reload increase was
1,409,024 B (1.34 MiB). The slowest first post-open edit was 46.90 ms
(Go change). The reload checkpoints below are current RSS in MiB, not peak
RSS, from three fresh processes per language:

| Reload trial | Cycle 100 | Cycle 200 | Cycle 300 | Cycle 400 |
| --- | ---: | ---: | ---: | ---: |
| Rust 1 | 221.05 | 220.13 | 219.13 | 220.05 |
| Rust 2 | 218.59 | 220.50 | 218.58 | 220.50 |
| Rust 3 | 222.25 | 220.55 | 220.39 | 222.12 |
| Go 1 | 219.75 | 221.10 | 218.26 | 221.10 |
| Go 2 | 221.57 | 221.18 | 219.35 | 222.18 |
| Go 3 | 223.67 | 219.75 | 221.33 | 221.34 |

An earlier 24-process run missed the 2 MiB gate in one Go reload process by
425,984 B. Review found that obsolete rendered rows were still live while
reload allocated replacement source analysis. Releasing those rows first
reduced the repeated-reload working set from roughly 266–273 MiB to
218–224 MiB, and the unchanged gate then passed. Both the failed and passing
raw records are retained. The remaining 1.91 MiB margin is narrow, so repeat
measurements and the exact 100-cycle p99 checks remain part of acceptance.

`make bench-interactions` is the asserting public-pane key-to-owned-frame gate
on the 1 MiB mixed fixture. It runs five fresh processes and 21 verified
cycles per case, reports input and frame phases separately, and uses
nearest-rank p95/p99 plus worst timings. Local source typing, source line
insert/delete, and rendered line delete/undo must be <50 ms p99 at both top
and midpoint. Source and rendered wheel scroll must be <25 ms p99 and change
the viewport. One five-process same-machine record, taking the maximum across
top/midpoint and operation directions, measured:

| 1 MiB ordinary interaction | Maximum p99 | Maximum worst |
| --- | ---: | ---: |
| Source character type/delete | 6.7 ms | 6.7 ms |
| Source line insert/delete | 11.6 ms | 11.6 ms |
| Rendered line delete/undo | 22.6 ms | 22.7 ms |
| Source wheel scroll | 14.6 ms | 16.8 ms |
| Rendered wheel scroll | 1.0 ms | 1.0 ms |

Every operation verifies its intended text, mode, dimensions and/or viewport
effect before a sample is accepted.

The first-ever source-to-rendered return after a verified source edit is a
separate cold projection: five fresh top and midpoint observations must each
be <500 ms key-to-frame. The measured worst was 181.5 ms at top and 192.8 ms
at midpoint. The following 100 returns per position are **warmed** operations
and must pass both p99 and worst <50 ms; their observed worst was 1.2 and
1.3 ms. No later return receives the cold allowance. Source open time is not
included in that transition number; the complete cold open-to-rendered frame
is the independent `make bench-realistic` measurement above. Neither path
uses a loading frame or stale editable layout.

`make bench-interactions-record` retains the wider observational inventory
at both 144 KiB and 1 MiB: front matter, references, fence-language changes,
structural edits, resize, reload and save in addition to the gated cases.
`TRIALS`, `ITERATIONS`, `CASE`, `SIZE`, and `OUTPUT` select and preserve raw
JSONL observations. On an earlier 1 MiB record, structural/front-matter/
reference insertions took roughly 217–226 ms worst, resize 123 ms, unchanged
save 4.9 ms, and externally changed reload 482 ms. Reload peak RSS was
197.9 MiB after releasing the obsolete retained block model before building
replacement source caches, versus 200.5 MiB in the fixed pre-incremental
record. That observational reload trace predates the later row-release
ordering change; its time and peak RSS have not been remeasured as a fresh
five-process distribution. These globally invalidating operations are measured
and disclosed;
they are **not** covered by the <50 ms local-edit guarantee.

## Exact 1 MiB kitchen-sink interaction acceptance

`examples/kitchen-sink-1mb.md` is a separate, authored-pattern fixture with
large Rust and Go fences. Its exact size is 1,048,722 bytes and its SHA-256 is
`1a7676c0af7b6c82480a6e0b60b56748b0cbd028ffef9808b6422d5dacb302e7`.
`make bench-acceptance-1mb` runs five fresh-process public-pane trials at
100×41, checking 1,000 consecutive keys in each of six navigation scenarios
before and after a retained edit, Select motion and edit results, plus
standalone PTY output. The key-to-owned-frame limit is p99 <25 ms with no
sample ≥50 ms for navigation; Select motion is p99 <50 ms. The <50 ms
multi-line `d`/`c` edit budget applies to the **Rust and Go fence** scenarios,
not to every Markdown construct. Prose edit timing is always recorded and
its line-removal result is checked, but it is not hidden inside a passing
code-fence latency claim. `make bench-acceptance-1mb-edit-cycles` additionally
checks 100 delete/change-and-undo cycles per fence language with exact text
and completed-frame assertions.

On the evidence host, the latest 400-cycle fence gate measured p99 20.26 ms
for Rust `d`, 32.04 ms for Rust `c`, 18.37 ms for Go `d`, and 29.12 ms for Go
`c` (key to completed owned frame). Five-trial 1,000-key navigation runs
stayed near 1 ms p99 through both fences. A separate 1 MiB mixed-note gate
measured warmed source-to-rendered returns at 1.2–1.6 ms p99 and cold first
returns at 183–192 ms. These are synchronous, fully faithful frames; there
is no pending/loading state.

The exact-fixture public-pane cold frame reached at most 431.65 ms across
the five-trial scenario runs. Navigation-only process peak RSS was at most
191.93 MiB, versus 211.72 MiB for clean `main`. The multi-range prose Select
edit reached 246.51 MiB versus 223.27 MiB on `main`, a roughly 23 MiB
higher process peak; the Rust/Go fence Select edits stayed near 194 MiB on
this branch versus about 223 MiB on `main`. These are process high-water
marks, not a claim that the same amount remains live after the operation.

The recovered 15-line Character Select paths now retain unaffected source,
model, and rendered rows. The 100-cycle before/after values below include
both the input handler and the completed owned frame on the same exact 1 MiB
fixture. The earlier broad-rebuild pause is no longer the current behavior:

| Select `v`, 15 `j`, then | Pre-recovery branch p99 / worst | Current p99 / worst |
| --- | ---: | ---: |
| line 600 prose, `d` | 906.01 / 1043.53 ms | 8.26 / 8.48 ms |
| line 600 prose, `c` | 507.28 / 701.53 ms | 8.45 / 9.41 ms |
| line 130 list, `d` | 663.39 / 809.64 ms | 8.43 / 10.02 ms |
| line 130 list, `c` | 309.00 / 467.08 ms | 7.89 / 8.79 ms |

The same gate verifies exact deleted text, physical line removal, frame state
and one-step undo on every cycle; undo p99 was 9.13–9.90 ms in the latest
run. The clean `main` baseline for the older prose selection removed only one
line rather than the expected eleven. Delimiter or document-wide semantic
changes may still take a wider path, so these local-edit results are not a
universal <50 ms bound.

The public-pane timer ends when an immutable owned frame is ready. The PTY
trace measures receipt of terminal output through a pseudoterminal and records
bytes and frame gaps; neither measures when kitty physically presents pixels.
On the final 63×229 `TERM=xterm-kitty` five-trial record, serial-arrow p99
across the six regions was at most 5.79 ms and the largest active burst-frame
gap was 76.18 ms. The acknowledged burst can coalesce draws while still
processing every queued key. The raw trace is
`.ai/workflow/evidence/019-pty-63x229.jsonl.gz`; the exact public-pane and
edit-cycle traces are alongside it in that workflow evidence directory.
For the final human check, run `make build-release`, open kitty at the actual
window geometry (63×229 was reported during the investigation), and launch
`target/release/oom-edit examples/kitchen-sink-1mb.md`. Immediately after
opening, hold Up/Down in Normal near line 500 and inside each large fence;
repeat after stopping briefly. In Character Select (`v`), move over 15 lines
with arrows or `j`/`k`, then test `d`, `c`, undo, and unchanged `y`/copy in
prose and both fences. Confirm every cursor/selection step paints, fully
selected physical lines disappear with their line breaks, undo restores exact
text, and any remaining pause is reported with mode, location and geometry.
This visual check complements, rather than being inferred from, the pane and
PTY timing gates.

An extreme single 1 MiB Rust fence took about 577 ms to build its full
layout; the 1 MiB many-fence case took about 565 ms. These are scaling and
memory observations, not a promise that every possible Markdown edit or cold
open meets 50 or 500 ms. The figures above were recorded on Linux x86_64,
AMD Ryzen 7 5700U, Rust 1.97.1, with eight logical CPUs online. Absolute
time is host/load-sensitive; fixture identity, raw JSONL, exact differential
tests and source-mapping property tests guard against a favorable timing run
masking a fidelity regression.

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
remain available synchronously. The core retains unchanged source analysis,
Markdown blocks, mapped rows and row/source indexes across local edits, while
the same full builder remains a differential oracle. Rendered composition
borrows parser-leaf text until final owned-row materialization, and wrapping
uses mapped fragments for authoritative style and source provenance. All
standalone and embedded hosts receive the same complete core projection.

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
