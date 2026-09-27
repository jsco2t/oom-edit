# Task 015 candidate checkpoint

Historical revision-4 checkpoint. The approved revision-5 Shift+V repair is
recorded in `015-shift-v-amendment.md` and `015-shift-v-review.md`. Current
candidate is `6682ea4d6b8c4541aa0626d7dfc39daaef1bfd40`; its verification
checkpoint is `015-candidate-shift-v.md`. The macOS handoff below must use this
new full SHA. Older candidates and failed smoke/benchmark evidence are retained.

This is progress evidence, not task or release acceptance.

- Main-only, attempt 1, approved revision 4; approved hash remains
  `a9943dfacf2b945ab8517c7b1ee3c559179ff14ab7c3ad4cb18b32f6e19b305e`.
- Active local candidate: `ab30fa474a8cd816e018cbe9c3a5f5577706b2ee`.
  Initial candidate: `9c9ad9e24c2f7a1264d5700ccb2d1ddae736d24f`, committed
  2026-09-27 13:37 UTC on `oom-prep`. Editor/core and exact path/lock entries
  are 0.6.0; spell remains 0.1.0. Third-party vendor bytes and notices are
  unchanged. No tag or push occurred.
- `make test-release-versions` was red before the version bump
  (`/tmp/oom-edit-task015-versions-red.log`), then both dependency-guard suites
  passed (`/tmp/oom-edit-task015-versions-green.log`).
- Cumulative reviewed scope: 130 approved feature/plan files, 56,889 insertions
  and 34,947 deletions, including the previously reviewed generated parser.
  No unrelated dirty file, new stub, whitespace defect or credential-bearing
  source file was found. Runtime workflow status/evidence remain ignored and
  are not part of the candidate source. Publication is not implied by this
  local commit.
- Exact-revision CI, performance, native Linux PTY/manual smoke and independent
  consumer verification are next. Native macOS runner access remains unanswered.
  Separate publication authority and exact-tag consumption are still mandatory.

Environment: native Linux x86_64, Rust 1.97.1 (8bab26f4f 2026-07-14), AMD Ryzen 7
5700U, 8 logical CPUs. No macOS or published-release result is claimed.

## Independent consumer preparation

`make downstream-prepare DOWNSTREAM_DIR=/tmp/oom-edit-task015-consumer
DOWNSTREAM_REV=9c9ad9e24c2f7a1264d5700ccb2d1ddae736d24f
DOWNSTREAM_SOURCE=/home/duser/Developer/sources/personal/oom-edit` passes. Git
objects were seeded from the committed local source without reading or copying
its authenticated remote URL. The independent consumer owns its manifest,
lockfile, vendor/config and candidate identity. Offline checking is pending.
That directory belongs to the initial candidate; it must not claim verification
of the active corrected revision.

## Candidate performance finding and correction

The initial candidate's full CI passed all normal gates, coverage, documentation
and debug checks, then failed release dirty-diagnostic overhead: worst 105.36
microseconds against strict <100. The failed run is retained at
`/tmp/oom-edit-task015-candidate-ci.log`; it is not release acceptance.

Paste materialized nearly 48,000 preceding lines to compute its byte offset in
the unchanged fixture. Front-matter refresh also copied another complete Vim
string after synchronously updating the highlighter text. Indexed-offset and
actual copy-count regression probes were red on the original implementation
(`/tmp/oom-edit-task015-paste-red.log` and `-paste-cache-red.log`). Paste now
uses the rope's line-byte index; front matter borrows the refreshed derived
text. The same gateway remains the only mutation owner.

`make test-paste-work` passes all three cases
(`/tmp/oom-edit-task015-paste-green.log`): exact text, metadata and cursor for
YAML, TOML, malformed metadata, CRLF, Unicode and 10,000 prior rows. The existing
navigation bound is strengthened from two full materializations to one.
Warning-fatal `make lint` passes. Benchmark fixtures, samples, measurement and
comparison implementation and every limit are unchanged. The correction is a
separate seven-file commit, preserving the original candidate and completed
Task 014 evidence. Coverage now has 186 cases. Acceptance must run again on the
active SHA.

## Native macOS handoff required

Use a native Mac checkout containing the exact full candidate SHA above; verify
`git rev-parse HEAD` and a clean tracked worktree first. Run `make ci` and
`make terminal-guard-pty-test`, recording OS/architecture, Rust version, exit
statuses and complete logs. Run `make run-isolated ARGS=/tmp/oom-edit-smoke.md`
with a disposable Markdown paragraph and manually exercise every row of the
README key table: Insert/Escape, character/line/block Select, Markdown/plain
clipboard emission, Command, help/palette, save, front-matter insertion and
safe quit. Record actual visible modes/palette and exact saved file bytes;
OSC 52 emission is best effort, not a desktop clipboard acknowledgement.

No Mac runner is currently provided. This prerequisite cannot be substituted
with Linux tests or cross-compilation. Publication requires a subsequent,
separate explicit authorization; the local `v0.6.0` tag does not exist.
