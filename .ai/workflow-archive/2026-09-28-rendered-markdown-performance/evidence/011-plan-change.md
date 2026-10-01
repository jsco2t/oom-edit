# Task 011 plan-change finding

Task 011 is not complete. The approved revision-2 targets conflict with the
measured source-first path after the retained incremental implementation.
No benchmark threshold or editing contract was relaxed.

## Same-machine measurements

- The final task-010 five-process public-pane record
  (`incremental-layout.jsonl`) measures ordinary rendered line delete/undo
  p99 at 23.7/23.9 ms at the top and 17.8/17.9 ms at midpoint. Source local
  line insert/delete is 10.5/9.6 ms at the top and 12.0/11.1 ms at midpoint;
  warmed rendered wheel scrolling is approximately 1 ms p99. The first
  source-first return from Insert to a complete rendered frame remains
  227–228 ms p99. This is the first construction of the fully mapped rendered
  projection, not a routine local-edit rebuild.
- Five fresh cold trials per pane size (`011-cold-initial.jsonl`) measure the
  1 MiB mixed first rendered frame at 529.2–536.0 ms for 100×41 and
  528.3–534.0 ms for 70×28. Both fail the approved <500 ms worst-of-five
  criterion. The other recorded class/size cells remain within their relevant
  task-011 limits, including the 448 KiB table's task-006 baseline plus 10%.
- The existing `make bench` release gate passes, including the fixed source
  first-frame and full-layout/memory benchmarks. Its fixed 1 MiB source first
  frame is about 40 ms; the fixed complete rendered layout alone is about
  138 ms. Eagerly moving that layout into source open would therefore risk
  the existing <150 ms source-first contract, while moving it onto the next
  ordinary source edit would miss the <50 ms interaction target.

## Decision needed

The current no-loading, no-stale-frame, fully faithful, synchronous architecture
meets the main ordinary edit and scroll targets but not the first-ever
source-to-rendered return target. Making a first rendered projection free at
that transition requires preparing it earlier, making it substantially faster,
or changing the rendering lifecycle. The first option competes with source
first-frame/edit budgets; the lifecycle alternatives are outside this approved
plan. A local cold optimization of roughly 36 ms may address the <500 ms
mixed first frame independently, but would not resolve the 228 ms first
source-to-rendered transition.

The human decision is whether the **first-ever** source-to-rendered transition
may be budgeted as a cold first render while retaining <50 ms for subsequent
ordinary edit/mode-return cycles, or whether a different architecture and its
source-first/interaction budgets should be approved. Until that decision, task
011 remains in progress, task 012 has not started, and no cold-work trade was
made against interaction responsiveness.
