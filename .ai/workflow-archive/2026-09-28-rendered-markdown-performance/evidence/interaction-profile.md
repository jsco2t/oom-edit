# Responsiveness-first interaction profile

## Measurement

The release public-pane probe opened the deterministic mixed Markdown fixture at
100×41, warmed one complete frame, then measured input handling and owned-frame
production separately. Five fresh processes each ran 20 edit/scroll cycles per
case, yielding 100 samples per step. The recorder uses nearest-rank p95/p99 and
keeps the worst sample visible. The full observations are in
`interaction-baseline.jsonl`; matching middle-of-document observations are in
`interaction-mid.jsonl`.

| Interaction | 144 KiB median / p99 | 1 MiB median / p99 | Dominant 1 MiB phase |
| --- | ---: | ---: | --- |
| Source word-character insert | 5.1 / 5.3 ms | 10.4 / 11.1 ms | Input and source frame |
| Source structural insert | 12.9 / 34.5 ms | 79.8 / 216.2 ms | Input |
| Source wheel scroll | 15.8 / 16.0 ms | 16.6 / 17.1 ms | Input, including viewport projection |
| Rendered wheel scroll | 0.9 / 1.1 ms | 1.2 / 1.8 ms | Frame |
| Rendered line delete | 88.3 / 90.3 ms | 644.0 / 660.8 ms | Input (381 ms median) and frame (263 ms median) |
| Rendered delete undo | 72.6 / 74.6 ms | 521.2 / 535.9 ms | Input and frame |
| Return from Insert to rendered | 29.3 / 35.8 ms | 205.3 / 256.8 ms | Frame |
| Width change | 30.4 / 32.8 ms | 216.2 / 232.9 ms | Resize handler |
| External reload | 106.3 / 107.4 ms | 516.8 / 524.9 ms | Reload handler |

The delete is a verified rendered line Select-delete, not Normal-mode `x` (which
does not edit). The wheel cases verify that the pane-local display actually
changes. The source insert/delete and rendered delete/undo pairs verify text
mutation and restoration. Reload timing excludes the external file write but
includes the pane notification, tick, and next owned frame.

The same 1 MiB cases at a heading/body position after the document midpoint
showed similar results: structural insert 78.3 ms median / 216.8 ms p99,
rendered delete 642.6 / 667.6 ms, and return from Insert 204.9 / 255.9 ms.
The stalls are not specific to edits near the beginning.

## Measured causes and limits of narrow fixes

A temporary release timing probe was applied to `Highlighter::apply_edit`, run
for one cycle per case, and removed. On a 1 MiB structural insert, the
incremental Markdown tree-sitter parse took 176.5 ms and the subsequent
reference-label/injection work took 32.7 ms. A rendered line delete spent
219.2 ms parsing and 29.9 ms afterward. A block-neutral word edit skipped
the block parse and spent 4.9 ms on the later work. These are one-shot phase
samples, not percentile estimates.

The renderer currently rebuilds a width-dependent full-document block model
and mapped layout after text edits or width changes. The earlier cold profile
measured those pure phases at 69.4 and 153.3 ms respectively. The new
interaction samples confirm that this cost is visible during return to
rendered mode, rendered edits, and resize.

The all-row gutter-continuation pass is real, but complete rendered wheel
scrolling at 1 MiB remained below 2.1 ms worst in these samples. Optimizing
that pass now would not address the observed stalls. Incremental injection
rediscovery could save a few milliseconds on neutral edits or some tens of
milliseconds after structural edits, but the measured parser and full layout
costs would remain. No product hot-path optimization is justified solely by
this profile.

## Architecture implications

- A renderer/source-parser split aimed only at cold first frame does not solve
  the measured editing, mode-switch, resize, or reload stalls.
- Background full-layout preparation could remove the approximately 200–270 ms
  foreground render phase. It would not remove the 80–217 ms structural source
  edit stalls or the larger synchronous input phase of rendered deletion.
- Moving source parsing off-thread would change the current synchronous-cache
  and fully highlighted visible-source contracts. Exact styling cannot be
  published for the new text until parsing completes; any pending-state policy
  must be explicit and approved rather than treated as an invisible speedup.
- Incremental/viewport rendered layout would avoid some full rebuilds but has
  higher risk because the current model owns document-wide source-mapped rows,
  footnotes, link indices, jump targets, and cursor mapping.

The appropriate next decision is about interaction semantics, not a 350 ms
cold-start target: is a brief explicit preparation state on rare large-note
structural edits/mode transitions acceptable, or must every edit and rendered
transition remain immediately fully faithful? The latter requires deeper
incremental parsing/layout work. No background or partial layout was
implemented or selected by this profile.
