# Task 020 investigation — exact 1 MiB recovery feasibility

All measurements used optimized local builds on the evidence machine and
`examples/kitchen-sink-1mb.md` unless noted. They are diagnostic samples,
not substitute acceptance gates. The test-only probes are discoverable in
`make help`; the temporary memory ablations were restored before validation.

## Red public-pane baseline

`make bench-acceptance-1mb-prose-cycles` ran 100 verified edit-and-undo cycles
per case at 100×41 and failed its strict p99 <50 ms and worst <100 ms limits
on all four cases. The probe checks current text, removed physical lines,
result mode, completed frame, stable edited cells across cycles, and exact
one-step undo. Raw per-cycle input/frame records are
`evidence/020-prose-red.jsonl.gz`.

| Character Select: 15 `j`, then | Median key-to-frame | p99 | Worst |
| --- | ---: | ---: | ---: |
| line 600 prose, `d` | 889.07 ms | 906.01 ms | 1043.53 ms |
| line 600 prose, `c` | 497.35 ms | 507.28 ms | 701.53 ms |
| line 130 list, `d` | 646.28 ms | 663.39 ms | 809.64 ms |
| line 130 list, `c` | 294.12 ms | 309.00 ms | 467.08 ms |

The phase probe reproduced 706.4 ms in the line-600 `d` handler, of which
380.7 ms was highlighter refresh, 264.9 ms Markdown-model refresh, under
1 ms Vim mutation, and about 60 ms other projection/effect work. The three
source edits are byte ranges `18003..18518`, `18524..18551`, and
`18556..18557`. Line 130 has one `3948..4243` range yet takes 448.2 ms in
the handler: source refresh 265.6 ms, model refresh 146.7 ms, Vim under
0.2 ms, and about 36 ms elsewhere. Thus merely batching the three edits
is insufficient: a direct `Highlighter::apply_edit(&edits)` still took
287.5 ms at line 600, and one full Markdown-model build took 99.6 ms.
The existing `PendingProjectionChange::Multiple` drops the retained row
cache, so the following rendered `d` frame takes a separate full-layout path.

During the task's incremental test gate, a saved property seed exposed a
separate source-fidelity error: a word insertion at the start of a paragraph
immediately after `#\n` was classified as block-neutral, leaving the inserted
word with the preceding heading's style. The deterministic regression test
failed before the fix. The block-neutral shortcut now excludes that unseparated
line-start case while keeping proven separated and interior cases; the
focused regression, saved seed, incremental suite and full test suite pass.

## Bounded feasibility slice

The test-only prototypes parse a section-bounded window around each exact
selection, then compare their current-text results to the full builders.
These are proofs for the selected ordinary cases, not a general Markdown
invalidation algorithm.

| Window | Bytes | Source fragment | Markdown blocks | Local block parse | Local row render | Chunk splice |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| line 600 | 982 | 2.54 ms | 10 | 0.06 ms | 4–6 ms | 0.04 ms |
| line 130 | 637 | 2.54 ms | 10 | 0.04 ms | 4–6 ms | 0.04 ms |

The fragment's styled lines equal a fresh full highlighter for every line
in the affected window. The local Markdown blocks equal the full model's
blocks in that window. Independently rendered rows equal the full builder's
cells, semantic styles, source atoms and line numbers. A test-only chunk
splice rebases unchanged suffix rows, handles the changed link count, and
matches the complete layout, fence regions and block boundaries after full
materialization. That full-layout oracle materialization took about 194 ms
for line 600 and 9 ms for line 130; it is **not** included in the 0.04 ms
retained splice and must not be on the ordinary pane frame path. Production
work still must prove safe window boundaries, suffix link-marker/footnote
dependencies, and viewport publication across edits, undo and Unicode.

## Controlled Rust-fence memory ablations

The 1 MiB generated Rust-fence `layout` case with the production source
injection tree cache measured 212.9 MB (203.1 MiB) peak RSS. Temporarily
disabling only cache retention, then rebuilding the same release probes,
measured 180.7 MB (172.3 MiB), a 30.7 MiB reduction; corresponding
256/512 KiB cases fell from 59.9/110.4 MB to 51.4/94.2 MB. This identifies
retained source-language trees as the size-correlated additional memory
owner. It is not a viable repair alone: the exact first Rust function `d`
rose from 20.5 ms key-to-frame with the cache to 708.1 ms without it.

A second temporary ablation consumed, instead of cloning, each highlighted
code line while constructing rendered rows, leaving tree caching enabled.
The 1 MiB RSS fell from 212.9 MB to 200.4 MB, and first Rust `d` stayed at
about 20.5 ms. This nearly reaches, but does not yet pass, the unchanged
198.8 MB (189.6 MiB) ceiling. The smaller Rust-fence rows were likewise
near but slightly over their fixed ceilings. The result gives task 023 a
concrete allocation-lifetime direction; it is not claimed as a passing fix.
Both ablation edits were reverted after measurement.

The directly emitted samples behind that comparison were:

```text
cache on, 256 KiB: LAYOUT ... 142068208 6101 14344736 59932672
cache on, 512 KiB: LAYOUT ... 289698580 12197 28689344 110350336
cache on, 1 MiB:   LAYOUT ... 599770986 24390 57378968 212942848
cache off, 256 KiB: LAYOUT ... 146609854 6101 14344736 51372032
cache off, 512 KiB: LAYOUT ... 293273341 12197 28689344 94167040
cache off, 1 MiB:   LAYOUT ... 598151904 24390 57378968 180711424
consume lines, cache on, 1 MiB: LAYOUT ... 570547315 24390 57378968 200351744
first Rust d, cache on: 19234234 input ns + 1239287 frame ns
first Rust d, cache off: 284379250 input ns + 423674607 frame ns
first Rust d, consume lines/cache on: 19330102 input ns + 1125244 frame ns
```

The `LAYOUT` fields shown are elapsed nanoseconds, row count, retained-layout
heap bytes and peak RSS bytes; omitted prefix fields were the same fixture
identity and width. These are one-process ablation samples, not five-run
acceptance results.

## Go/no-go

Proceed with tasks 021–023 under the approved architecture. The bounded
window and row-splice probes demonstrate a plausible sub-50 ms ordinary
path while preserving complete-builder equality on the two red scenarios.
The direct full-tree batch and no-cache memory alternatives demonstrably
cannot meet the targets. The next tasks must turn the local prototypes into
conservative, proven production invalidation and current-text publication;
they may not assume arbitrary delimiter/reference edits are local or hide a
full-layout conversion behind a good splice number. If those boundary proofs
fail, stop at `PLAN_CHANGE_REQUIRED` rather than weakening acceptance.
