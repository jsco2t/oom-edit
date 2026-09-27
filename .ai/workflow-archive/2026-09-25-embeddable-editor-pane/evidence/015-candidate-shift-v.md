# Task 015 Shift+V candidate verification — incomplete release

Main-only, attempt 1, acceptance round 1, approved revision 5. Candidate:
`6682ea4d6b8c4541aa0626d7dfc39daaef1bfd40` on `oom-prep`, committed locally
with the reviewed 12-file fix/test/documentation/amendment scope. Original
candidates `9c9ad9e24c2f7a1264d5700ccb2d1ddae736d24f` and
`ab30fa474a8cd816e018cbe9c3a5f5577706b2ee` remain in history. Historical
failed performance/Shift+V results are preserved; no acceptance is backfilled.

Approved plan hash:
`c750cc88e70cccb9392b9c74aab93d75da004b86adf29cd74c51cd3a74033a9b`.
Versions remain editor/core 0.6.0 and spell 0.1.0, with exact pins and lockfile.
No dependency, vendor, parser, golden, public API or performance-limit change.
Behavioral red/green and actual main-thread review: `015-shift-v-review.md`.
Explicit narrow user approval: `015-shift-v-amendment.md`.

## Exact-candidate local results

| Command/check | Result/evidence |
| --- | --- |
| make test-shift-v | PASS, core and both translated host paths; `/tmp/oom-edit-task015-shift-v-green.log` |
| make ci | PASS, full normal suite, all seven normal gates, release build and strict 191-case/91-requirement coverage; `/tmp/oom-edit-task015-shift-v-ci.log` |
| make build-examples | PASS, executed by exact-candidate make ci |
| make doc | PASS, warning-fatal docs executed by make ci |
| make bench-check | PASS, complete debug budgets executed by make ci |
| make bench | PASS, complete unchanged release budgets executed by make ci |
| make tui-perf-record BRANCH_ROLE=candidate OUTPUT=.ai/workflow/evidence/perf-release-candidate.tsv TRIALS=5 | PASS, 75/75 rows, all 15 cases × five trials, every revision field equals full candidate SHA; `/tmp/oom-edit-task015-shift-v-perf-record.log` |
| make tui-perf-compare BASELINE=.ai/workflow/evidence/010-baseline-current-host.tsv CANDIDATE=.ai/workflow/evidence/perf-release-candidate.tsv OUTPUT=.ai/workflow/evidence/perf-release-comparison.tsv | PASS, 60/60 metrics, unchanged comparator/limits; `/tmp/oom-edit-task015-shift-v-perf-compare.log` |
| make terminal-guard-pty-test | PASS, 7/7 native Linux cases; `/tmp/oom-edit-task015-shift-v-linux-pty.log` |
| Native Linux README key-table smoke | PASS, actual PTY observations, clipboard emission and exact disk bytes; `015-linux-smoke-fixed.md` |
| make downstream-prepare | PASS, fresh `/tmp/oom-edit-task015-consumer-shift-v`; `/tmp/oom-edit-task015-shift-v-downstream-prepare.log` |
| make downstream-candidate-check | PASS, independently owned manifest/lock/vendor/config, empty Cargo home, offline/locked warning-fatal clippy and real public behavior; `/tmp/oom-edit-task015-shift-v-downstream-check.log` |
| make downstream-negative-check | PASS, 20 actual source-provenance negatives plus real vendored checksum corruption; `/tmp/oom-edit-task015-shift-v-downstream-negative.log` |
| Separate standard make check | PASS, all seven checks, zero failures; `/tmp/oom-edit-task015-shift-v-standard-gate.log` |
| Native macOS CI/PTY/manual smoke | NOT RUN, no native runner or results provided |
| Separate publication authorization / v0.6.0 tag / intended push | NOT GIVEN / NOT CREATED / NOT RUN |
| make downstream-tag-check | NOT RUN, requires separately authorized, published exact candidate tag |

Independent consumer commands use `DOWNSTREAM_DIR=/tmp/oom-edit-task015-consumer-shift-v`
and `DOWNSTREAM_REV=6682ea4d6b8c4541aa0626d7dfc39daaef1bfd40`. Preparation
uses `DOWNSTREAM_SOURCE=/home/duser/Developer/sources/personal/oom-edit` to
seed committed Git objects without exposing/copying authenticated origin.
Actual lock sources for oom-edit, hjkl-buffer, hjkl-engine, tree-sitter-md and
dirs-sys all identify canonical `https://github.com/jsco2t/oom-edit` at the
full candidate SHA; no floating branch or path dependency substitutes for it.

Native environment: Linux x86_64, Rust 1.97.1, AMD Ryzen 7 5700U, eight logical
CPUs. All five candidate trials ran after CI benchmarks ended and before any
consumer compilation. Approved five-trial current-host baseline remains the
unchanged original commit. Earlier candidate measurements were copied and
SHA-256 verified byte-for-byte before active output paths were regenerated;
see archival note in `015-candidate.md`.

Release worst first frame 38.99 ms (<150), 1 MiB layout 158.76 ms (<250), heap
46,869,472 bytes (<64 MiB), read-only 64-note analysis 621.86 µs (<10 ms), retained
capacity 6,572 bytes (<16 KiB), and owned 200×60 frame p95 502.763 µs (<1 ms).
Dirty-diagnostic additional overhead reports 0 µs (<100) because all nine raw
diagnostic durations are lower than their nonzero baselines; these are real
28–31 ms measurements, not missing/zero raw samples. The unchanged oracle's
zero-overhead validity guard and exact-boundary tests pass.

## Outstanding release prerequisites

All fix/candidate local gates pass as of 2026-09-27T14:50:47Z, with no build,
lint, documentation or advisory warnings/errors. The tracked tree is clean and
the frozen revision-5 hash re-verifies exactly. Task 015 cannot be COMPLETE
without native macOS CI/PTY and actual README smoke
on this full SHA, separate subsequent release publication authorization and
post-publication exact-tag consumer verification. The user approved only the
Shift+V repair, not publication or a platform waiver. No tag/push/archive or
package DONE status is asserted by this checkpoint. Tasks 001–014 stay COMPLETE
and immutable; final_sha and completed_at stay unset. Workflow phase is BLOCKED
on the missing external release prerequisites, Task 015 remains IN_PROGRESS,
attempt 1, and final_attempts remains 0. The Shift+V repair itself is complete.

## Historical candidate superseded — 2026-09-27 Linux portability re-check

The user subsequently pulled `45b687c3e80399ddd33efd371629e09d92af6c96` and
requested fresh Linux quality verification. This report remains the historical
6682ea4 checkpoint, including its then-missing macOS evidence; nothing above
is backfilled. Fresh evidence and the user's explicit macOS PASS acceptance
are recorded in `015-linux-portability-verification.md` and
`015-macos-portability-review.md`.

Before regenerating the active performance paths for 45b687c, the 6682ea4
outputs referenced above were preserved byte-for-byte and SHA-256 verified:

- `perf-release-6682ea4-candidate.tsv`:
  `b8c75daa21b22a7be99b2d21930a183367c1cb96ce0d4cde1b06fa94e3c12fd2`.
- `perf-release-6682ea4-comparison.tsv`:
  `cb95defba53f82660aa6acf427374ad7c109f0ef5e0534e3bbf1d4232b5a5630`.

The active `perf-release-candidate.tsv` and `perf-release-comparison.tsv` now
belong to 45b687c, not this historical report.
