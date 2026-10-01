# Task 006 profiling and plan-change evidence

The approved `make bench-realistic` gate was run after tasks 001–005. Five cold-process trials per first-frame cell showed:

| Fixture | Pane | Median | Worst | 350 ms gate |
| --- | --- | ---: | ---: | --- |
| mixed 1 MiB | 100×41 | 558.2 ms | 560.0 ms | fail |
| mixed 1 MiB | 70×28 | 559.0 ms | 562.8 ms | fail |
| tables 448 KiB | 100×41 | 346.1 ms | 355.5 ms | fail |

All other fixed first-frame cells passed in that run. The 256→512→1024 KiB full-layout samples scaled near 2× for every class; the mixed series was 62.7/128.7/259.3 ms. The mixed 1 MiB layout has 24,960 rows, 53.4 MB retained layout heap, and about 188 MB peak process RSS in the task-004 release record.

A temporary timing probe was applied, run once on the same release fixture, then fully removed. It measured the mixed 1 MiB path at 100×41:

| Component | Time |
| --- | ---: |
| Core source Markdown tree-sitter parse during open | 218.7 ms |
| Source injection discovery/query preparation during open | 43.3 ms |
| Complete highlighter construction | 290.9 ms |
| Public-pane open phase | 297.9 ms |
| Pulldown block-model construction during render | 69.4 ms |
| Rendered layout construction | 153.3 ms |
| Complete public-pane render phase | 258.3 ms |
| Open-to-owned-first-frame total | 560.1 ms |

The temporary probe left no source changes; `make fmt-check` passed afterward. The source Markdown parse is a distinct fidelity path from pulldown rendered modeling. Task 006 only authorizes local pure-layout work. To reach 350 ms without changing the 1 MiB fixture, that work would have to remove about 210 ms from 222.7 ms of block-model plus layout construction, leaving roughly 12 ms to parse and materialize 24,960 fully source-mapped rows. That is not a credible local optimization, and it would risk the required visible-cell, style, and provenance equivalence.

An architectural change is required to overlap or stage source parsing with full rendered preparation while preserving exact first-frame output and synchronous mutation correctness. The approved plan explicitly makes such an expansion a human plan-change decision. The fixture and 350 ms budget were not weakened. No task-006 product optimization was retained.
