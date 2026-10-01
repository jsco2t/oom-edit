# Task 024 RSS-stability diagnostic

The first integrated `make bench-rss-stability` run retained all 24 raw process
records in `024-rss-stability.jsonl` and failed the unchanged 2 MiB post-warmup
current-RSS gate in one Go reload process. Its cycle 100/200/300/400 readings
were 283,230,208 / 282,025,984 / 284,467,200 / 285,753,344 bytes. The
cycle-400 difference from cycle 100 was 2,523,136 bytes (2.41 MiB), 425,984
bytes beyond the gate. The other 23 process records passed their respective
checks; the failed process verified restored text, Normal mode, complete frame,
and a drained reload event on every cycle.

The failed series is non-monotonic and the other two Go reload series also
oscillate by several MiB. Task 023's earlier three-process Go reload run passed,
but its readings likewise varied. This is consistent with allocator working-set
variation, not proof of either allocator causation or absence of a small leak.
`LiveDocument::reload` drops the old rendered model, replaces the highlighter
and Vim state, and the session resets its rendered state; cache-entry/byte
bounds and language-change tests passed in Task 023. An isolated Go reload
repeat on the unchanged binary passed, with 273.59 / 270.05 / 271.02 /
271.40 MiB at cycles 100/200/300/400 (`024-go-reload-repeat.jsonl`). This
reinforces the substantial non-monotonic variation between otherwise exact
reload cycles. No threshold or failed sample was changed or discarded.

Review found one narrow ownership-order issue: the old rendered rows remained
live while new source analysis was allocated during reload. The session now
releases those rows before rebuilding source state. The full `make test` suite
passed after this change. The unchanged 24-process RSS gate then passed, with
all six Rust/Go reload processes completing 400 cycles and preserving exact
text, frame and mode. Current reload RSS was about 218–224 MiB, compared with
roughly 266–273 MiB before the change. The largest post-warmup increase was
2,007,040 B (1.91 MiB, Rust reload trial 2); the largest Go reload increase
was 1,409,024 B. The slowest first post-open edit was 46.90 ms. The passing
record is `024-rss-stability-after-order.jsonl`.
