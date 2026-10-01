# Task 016 investigation — not acceptance evidence

The reported physical kitty scrolling hitch is not yet reproduced by the
application-side or PTY-output probes. This note does not mark task 016 complete.

## Reproduction identity

- Exact fixture: `examples/kitchen-sink-1mb.md`, SHA-256
  `1a7676c0af7b6c82480a6e0b60b56748b0cbd028ffef9808b6422d5dacb302e7`.
- User's kitty geometry: 63 rows × 229 columns; held arrow keys. Both
  `TERM=xterm-kitty` and legacy arrow sequences were exercised. The standalone
  startup path does not enable kitty's enhanced repeat-key protocol.
- The user's `oom-edit` command resolves through `~/.local/bin/oom-edit` to a
  separate, clean checkout at `0e26684733551691a49a2ff7111cec9404fdd2d3`
  (`main`), not to this worktree's release binary. The installed binary was
  not changed.
- After running the explicit branch release binary in kitty, the user reported:
  "I'm not seeing any visual hitches or pauses when using the binary that's
  built from our branch." This is manual presentation evidence, not a PTY
  measurement. The user approved retaining the regression gate without a
  speculative repair.

## Measurements

- Release, 63×229: 1,000 serial arrows at fresh open, row 500, and inside
  large fences had approximately 5–6 ms p99 key-to-PTY-flush. Per-key emitted
  byte hashes matched clean main in the same scenarios. The fresh and retained
  comparisons in `016-*-fresh-held-hash.jsonl.gz`, `016-*-kitty-term.jsonl.gz`,
  and `016-*-retained-kitty.jsonl.gz` show no branch-only pause. The post-edit
  retained p99 was 8.91 ms candidate versus 11.77 ms main.
- Corrected end-of-queue acknowledgment: fresh 600-arrow release bursts at
  5 ms cadence emitted 601 frames and exactly 6,027,795 total bytes on both
  versions (`016-*-acked-fresh.jsonl.gz`). The Go fence 1,000-arrow bursts
  emitted 1,001 frames and exactly 7,766,304 bytes on both versions; all 1,000
  corresponding serial-frame hashes matched (`016-*-acked-go.jsonl.gz`).
- The user's installed `~/.local/bin/oom-edit` binary was run through the same
  600-arrow fresh probe. Its 601 frames, 6,027,795 output bytes, and all 600
  serial-frame hashes matched the branch release binary
  (`016-installed-main-acked-fresh.jsonl.gz`).
- Debug, 63×229: both versions cost about 33 ms median and 40–41 ms p99 per
  serial arrow; all 600 corresponding serial output hashes matched
  (`016-*-debug-fresh.jsonl.gz`). The corrected burst probe acknowledges all
  queued input; it emits fewer frames because the event loop combines draws
  under load (`016-*-debug-acked.jsonl.gz`). Main's debug cold first frame was
  35.9 s, candidate's 3.4 s; these are not release navigation measurements.
- A PTY flush-receipt timestamp cannot measure when physical kitty presents
  pixels. The current environment has no kitty executable or process.

## Probe corrections

- Added exact geometry, terminal name, fresh-open and post-edit states, cadence,
  per-key output hashes, and an optional long first-frame timeout.
- A burst now appends an ordered quit command and waits for successful exit.
  The prior fixed two-second drain could end while many debug-build keys were
  still queued. The acknowledgment is tested and recorded in raw rows.
- Region verification now reads a visible gutter line, because ratatui may
  update only changed status-line cells. A split `19933:1` status previously
  looked like a stale `1:1` to the regular-expression probe. The correction
  has a contract test.
- `make acceptance-1mb-perf-test` passes (12 tests). `make check` passes its
  seven gates after these diagnostic changes.

## Decision

The original task required a demonstrated branch-only cause and a
red-before-green regression test. Neither can be supplied honestly from these
traces or the explicit branch-binary kitty retest. The user approved revising
task 016 to retain strengthened navigation gates and mark the originally
reported hitch as unconfirmed, then proceed to Select and edit latency work.
