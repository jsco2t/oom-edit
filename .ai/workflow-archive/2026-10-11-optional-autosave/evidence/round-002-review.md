# Acceptance failure — Round 2

## Context

User rejected the introduced performance regression after local reviewomatic review of the uncommitted autosave package and approved syntax repair. One important/P1 finding met the 80-confidence threshold; no other code or documentation findings met it. No source changes occurred during that review.

Baseline is package HEAD 1960ade52a394a7c23fe400fdbb395283834ab72. Candidate is the completed Round 1 working tree. Measurements ran serially with release, offline/locked builds on the same Linux x86_64 AMD Ryzen 7 5700U host and rustc 1.97.1. Autosave switches were disabled in these native benchmarks.

## Observed failure

| Metric | Baseline | Rejected candidate |
| --- | ---: | ---: |
| Core 1 MiB edit-at-start average | 579.47 microseconds | 11.28 ms |
| Core 1 MiB edit-at-start worst | 878.75 microseconds | 25.04 ms |
| TUI edit-frame median wall/frame | 5.958 ms | 25.523 ms |
| TUI edit-frame median CPU/frame | 7.944 ms | 45.633 ms |
| TUI edit-frame median process peak RSS | 23.73 MiB | 28.87 MiB |
| Mixed 1 MiB top character insert median total | 5.683 ms | 10.002 ms |
| Mixed 1 MiB top character delete median total | 5.694 ms | 9.977 ms |
| Mixed 1 MiB middle character insert median total | 6.538 ms | 11.859 ms |
| Mixed 1 MiB middle character delete median total | 6.568 ms | 11.798 ms |

Candidate make bench fails the unchanged 10 ms worst edit budget. Native five-trial TUI comparison fails wall, CPU and RSS for edit-frame; other 57 comparisons pass. Both versions pass all 75 absolute TUI observations. Both versions also pass 2,310 mixed-document public-pane observations; character medians nevertheless grow 75–81%. Remaining interaction median deltas range from -5.3% to +3.4%.

Candidate owned-frame conversion p95 is 726.655 microseconds, within 1 ms. Baseline make bench completes its core benchmark but later fails the unchanged spelling-engine 4 KiB build step: 2.113484 ms against 2 ms. Preserve this separate baseline failure; it does not excuse the introduced edit regression or permit a weaker gate.

## Implementation evidence

Task 003's new zero-width line-start rejection in edit_is_provably_block_neutral changes the operation's execution path. bounded_source_region refuses edits below byte four and edits touching front matter/global semantics. Highlighter::apply_edit then reparses and may scan references, clear retained source parse cache and rediscover injections. Even other line starts can move from the cheap block-neutral route to more expensive bounded processing. Tree::edit shifting an owning node past inserted text is a real correctness defect; undoing the fix without an ownership proof is unacceptable.

The authoritative Markdown spec supports both Setext heading levels and context-dependent paragraph reinterpretation. Current deterministic tests compare complete StyledLine results through insertion, reversed-range undo and redo, including Unicode, multiline titles and a persisted generated sequence. These must remain intact.

## Retained measurements

The adjacent round-002-rejected-* TSV and JSONL files preserve the original baseline/rejected-candidate raw measurements and comparisons. They are historical acceptance evidence and must not be overwritten by fresh repaired-candidate runs.

Original detailed reports remain at /tmp/oom-autosave-reviewomatic-report.md and /tmp/oom-autosave-performance-report.md. Original benchmark logs remain at /tmp/oom-autosave-perf-bench.log and /tmp/oom-autosave-perf-baseline-bench.log. This summary retains the material facts without embedding full build logs.
