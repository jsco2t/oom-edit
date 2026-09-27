# Task 015 — exact-candidate Linux portability verification

Candidate: `45b687c3e80399ddd33efd371629e09d92af6c96` on branch `oom-prep`.
Verification completed 2026-09-27, Linux x86_64, Rust 1.97.1,
AMD Ryzen 7 5700U, eight logical CPUs. This is a review/verification checkpoint,
not task or release completion. No source changes, commits, tags or pushes.
Tracked worktree is clean. Tasks 001–014 remain complete and unchanged.

The feature-workflow approved-plan helper returns the unchanged revision-5 hash
`c750cc88e70cccb9392b9c74aab93d75da004b86adf29cd74c51cd3a74033a9b`.
Main-thread nine-perspective local review: 0 Critical / 0 Important findings
at confidence threshold 80; see `015-macos-portability-review.md`.

## Repository-native checks

All listed executed commands exited 0. The full CI invocation executed the
canonical make check stage; no separate standalone make check is claimed here.

| Command/check | Result and evidence |
| --- | --- |
| `make ci` | PASS; `/tmp/oom-edit-task015-macos-portability-linux-ci.log` |
| `make check`, invoked by CI | PASS; fmt-check, lint, build, full test, deny, audit and data-license-check all report PASS |
| Release build, strict coverage, examples, docs, debug and release budgets, invoked by CI | PASS; 191 compiled cases across 91 requirements; `make build-examples`, `make doc`, `make bench-check` and `make bench` all completed |
| `make terminal-guard-pty-test` | PASS; four Python harness regression cases and seven native Linux Rust cases; `/tmp/oom-edit-task015-macos-portability-linux-pty.log` |
| `TMPDIR=/tmp/oom-edit-macos-paths.bvlmDc-alias make test-pane-public test-pane-lifecycle test-pane-disk test-pane-bindings test-standalone-host` | PASS; actual symlinked temp-root fixture, all invoked public-pane, lifecycle, disk, bindings, snapshots and standalone parity cases pass; `/tmp/oom-edit-task015-macos-portability-symlink-root.log` |
| Native interactive README smoke | PASS; actual Shift+V, other modes, both yanks, palette, save, metadata and clean quit; `015-linux-portability-smoke.md` |
| `git diff --check 6682ea4d6b8c4541aa0626d7dfc39daaef1bfd40..45b687c3e80399ddd33efd371629e09d92af6c96` | PASS; no whitespace errors |

No build, lint, documentation or advisory warnings/errors were found in the
gate logs. No assertions, goldens, thresholds, dependency versions, parser
bytes or production APIs were changed. Darwin-only native tests cannot execute
on Linux; macOS acceptance below is a separate user-supplied disposition.

## Performance

`make tui-perf-record BRANCH_ROLE=candidate OUTPUT=.ai/workflow/evidence/perf-release-candidate.tsv TRIALS=5`
passed: 75/75 rows, all 15 cases × five trials, every revision field identifies
the exact candidate. Trials ran alone after CI/PTY and before downstream
compilation. Log: `/tmp/oom-edit-task015-macos-portability-perf-record.log`.

`make tui-perf-compare BASELINE=.ai/workflow/evidence/010-baseline-current-host.tsv CANDIDATE=.ai/workflow/evidence/perf-release-candidate.tsv OUTPUT=.ai/workflow/evidence/perf-release-comparison.tsv`
passed: 60/60 metrics, unchanged comparator and limits, against the original
baseline revision measured on this same host. Log:
`/tmp/oom-edit-task015-macos-portability-perf-compare.log`.

Release first-frame worst 39.37 ms (<150), 1 MiB rendered layout worst 159.85 ms
(<250), heap 46,869,472 bytes (<64 MiB), 64-note read-only analysis worst
602.87 µs (<10 ms), retained capacity 6,572 bytes (<16 KiB), and owned 200×60
frame conversion p95 562.563 µs (<1 ms). Dirty-diagnostic additional overhead
is 0 µs (<100): all nine diagnostic durations were lower than nonzero baseline
durations. Raw samples and the unchanged validity oracle remain in the CI log.

Active output SHA-256:

- Candidate TSV: `ee99cf1a53994120235f4366a0426527f7d6e588e41003cd9b49d183e5e21b45`.
- Comparison TSV: `998b8b8f3e970740dee8825faeee207cda6d25523740666ffb407aa353b80b94`.

Previous 6682ea4 outputs were preserved byte-for-byte before regeneration as
`perf-release-6682ea4-candidate.tsv` and `perf-release-6682ea4-comparison.tsv`;
their verified hashes are recorded in `015-candidate-shift-v.md`.

## Independent downstream candidate

Fresh directory: `/tmp/oom-edit-task015-consumer-macos-portability`.
All three commands below passed:

```sh
make downstream-prepare DOWNSTREAM_DIR=/tmp/oom-edit-task015-consumer-macos-portability DOWNSTREAM_REV=45b687c3e80399ddd33efd371629e09d92af6c96 DOWNSTREAM_SOURCE=/home/duser/Developer/sources/personal/oom-edit
make downstream-candidate-check DOWNSTREAM_DIR=/tmp/oom-edit-task015-consumer-macos-portability DOWNSTREAM_REV=45b687c3e80399ddd33efd371629e09d92af6c96
make downstream-negative-check DOWNSTREAM_DIR=/tmp/oom-edit-task015-consumer-macos-portability DOWNSTREAM_REV=45b687c3e80399ddd33efd371629e09d92af6c96
```

The consumer owns its manifest, lockfile, vendor tree and source configuration;
empty Cargo home, offline/locked warning-fatal clippy, public behavior and all
five main/patched source provenance checks passed. Negative verification
rejected 20 actual source-provenance violations and vendored checksum corruption.
Preparation used committed local Git objects and credential-free canonical
identity, not a path dependency or floating branch. Logs:

- `/tmp/oom-edit-task015-macos-portability-downstream-prepare.log`
- `/tmp/oom-edit-task015-macos-portability-downstream-check.log`
- `/tmp/oom-edit-task015-macos-portability-downstream-negative.log`

## macOS disposition and remaining release boundary

macOS gate: PASS — explicitly accepted by the user, who said
"No - I don't have that info. Please consider the gate as passing."
No platform details, command statuses, native logs or independent smoke
observations were supplied. No claim of native macOS execution by this agent.
See the full question/disposition context in `015-macos-portability-review.md`.

Linux quality re-check is complete and green. Task 015 remains IN_PROGRESS,
attempt 1; workflow remains BLOCKED only on separate publication authority and
the ensuing exact-tag consumer verification. `v0.6.0` does not exist locally.
No tag, push or downstream-tag-check was performed without authorization.
Final integrated package review/gate and archival remain pending the normal
workflow after all task criteria are met; final_attempts is still 0 and
final_sha/completed_at remain unset. This checkpoint does not declare DONE.

## Historical candidate superseded by main squash merge

The user subsequently reported the changes merged to main. Its release target
is `87d5b48f766eb35f30c2136e23d2a4448329713b`; both revisions have the identical
tree `e8f2d6b86ff5aae444f18844273b6e587a54e983`. This report remains the
historical 45b687c checkpoint and does not claim execution on the new SHA.

Before regenerating active performance paths, the outputs above were preserved
byte-for-byte, with matching hashes, as `perf-release-45b687c-candidate.tsv`
(`ee99cf1a53994120235f4366a0426527f7d6e588e41003cd9b49d183e5e21b45`) and
`perf-release-45b687c-comparison.tsv`
(`998b8b8f3e970740dee8825faeee207cda6d25523740666ffb407aa353b80b94`).
Active performance paths will then belong to the main release target.
