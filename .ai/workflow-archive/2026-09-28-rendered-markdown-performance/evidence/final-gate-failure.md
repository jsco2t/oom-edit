# Final-gate failure — round 2

The package is not complete. The final gate did not pass within its two-attempt
limit; no thresholds were raised or disabled.

## Attempt 1

- `make check`: PASS, all seven stages.
- `make bench`: FAIL in the unchanged spell-diagnostic overhead case. One of
  eight paired samples measured 1.05 ms additional time against the existing
  100 µs worst limit; the other seven were near zero. No spell code changed
  in round 2. This was treated as a suspected scheduling outlier, not a
  reason to change the budget.

## Attempt 2

- `make check`: PASS, all seven stages, including the interaction and exact
  1 MiB runner contracts now wired into its test stage.
- `make bench`: PASS at all unchanged limits; spell-diagnostic overhead
  measured 35.80 µs worst against 100 µs.
- `make bench-realistic`: FAIL only the Rust-fence peak-RSS comparisons at
  128 KiB (both pane sizes) and 256, 512 and 1024 KiB complete-layout sizes.
  Cold-frame time, other content classes, layout scaling and retained-heap
  checks passed. Observed Rust-fence peak RSS was about 36.2/35.9 MiB for the
  128 KiB pane cases and 57.2, 105.3 and 203.1 MiB for the larger layout
  cases. The fixed baseline-plus-10% ceilings are about 35.1/34.9, 54.1,
  99.2 and 189.6 MiB respectively. The failure grows with fence size, so it
  should not be dismissed as the earlier timing outlier.

## Investigation and scope

The new `Highlighter::highlight_fence_span` path calls
`source_injection_tree` during a rendered cold layout, and that method keeps
the code-language parse tree in the bounded `source_parse_cache`. This is a
plausible explanation for the size-correlated extra RSS, but it is an
inference, not a measured allocation attribution. Retaining the tree is also
what lets an immediate ordinary large-fence edit reuse its source parse and
meet the approved interaction budget. Disabling the cache or raising the
memory ceiling without measuring both consequences would trade one accepted
property for another. The approved plan prohibits an unreviewed >10%
realistic-fixture RSS regression.

No subsequent final commands were run after this failure. The code-fence
edit-cycle, exact navigation/Select, actual-geometry PTY, API, standalone,
embedded and standard quality gates have their separate passing task evidence,
but they do not override this red final gate.
