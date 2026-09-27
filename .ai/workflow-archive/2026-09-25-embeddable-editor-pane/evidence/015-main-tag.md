# Task 015 — verified main target and local release tag

Release target: `87d5b48f766eb35f30c2136e23d2a4448329713b`, main.
Local annotated tag: `v0.6.0`, object `9dfd4deeb546ce5336ea8a627b1c5011e2a4f71e`.
Created 2026-09-27, after fresh exact-SHA Linux verification. No source changes,
branch pushes, tag publication, force updates or archival were performed.

## User direction and identity

The user said: "We only tag changes that exist on main. I will push the changes
to main, then let you create the tag." They subsequently confirmed:
"Change is merged to main." Local tag creation received the command approval;
that approval explicitly stated that no refs would be pushed.

Local HEAD/main/origin-main and GitHub's main all identified the release target.
The remote was checked twice with credential-free canonical `git ls-remote`;
the tag did not exist there before creation. The old reviewed 45b687c commit is
not an ancestor of this squash merge. Both have identical tree
`e8f2d6b86ff5aae444f18844273b6e587a54e983`, and `git diff --exit-code` between
them passes. The tag targets main itself, not the unmerged branch revision.
Editor/core remain 0.6.0 with exact pins; no lock/vendor/golden/limit changes.

The unchanged approved revision-5 plan hash re-verifies as
`c750cc88e70cccb9392b9c74aab93d75da004b86adf29cd74c51cd3a74033a9b`.
Prior main-thread source review applies to identical contents. Historical
review, red/green, native smoke and previous candidate evidence are preserved.

## Fresh Linux validation at the main SHA

Linux x86_64, Rust 1.97.1, AMD Ryzen 7 5700U, eight logical CPUs.
All executed make commands below exited 0, without gate warnings/errors.

| Command/check | Result and log |
| --- | --- |
| `make ci` | PASS; `/tmp/oom-edit-task015-main-ci.log` |
| Canonical `make check`, invoked by CI | PASS: fmt, lint, build, full test, deny, audit, data-license checks |
| CI release/coverage/examples/docs/debug/release stages | PASS: 191 cases / 91 requirements; build-examples, doc, bench-check, bench all completed |
| `make terminal-guard-pty-test` | PASS: four Python and seven native Rust cases; `/tmp/oom-edit-task015-main-linux-pty.log` |
| Actual README-key smoke | PASS; `015-main-smoke.md`, including Shift+V, OSC 52 emissions, exact disk bytes and exit 0 |
| Five-trial performance record | PASS: 75/75 rows, 15 cases × five, every revision is the exact main SHA; `/tmp/oom-edit-task015-main-perf-record.log` |
| Same-host baseline comparison | PASS: 60/60 metrics, unchanged limits/comparator; `/tmp/oom-edit-task015-main-perf-compare.log` |
| Independent candidate preparation/check | PASS: offline/locked consumer, warning-fatal clippy, behavior and all five source provenances; `/tmp/oom-edit-task015-main-downstream-prepare.log`, `/tmp/oom-edit-task015-main-downstream-check.log` |
| Independent negative check | PASS: 20 source-provenance negatives and actual isolated checksum corruption; `/tmp/oom-edit-task015-main-downstream-negative.log` |
| Cumulative `git diff --check` | PASS |

Performance commands use the approved active paths:

```sh
make tui-perf-record BRANCH_ROLE=candidate OUTPUT=.ai/workflow/evidence/perf-release-candidate.tsv TRIALS=5
make tui-perf-compare BASELINE=.ai/workflow/evidence/010-baseline-current-host.tsv CANDIDATE=.ai/workflow/evidence/perf-release-candidate.tsv OUTPUT=.ai/workflow/evidence/perf-release-comparison.tsv
```

Trials ran alone after CI/PTy/smoke and before consumer compilation.
Candidate TSV SHA-256:
`43f13d479688b0a7a86ebea6f59800613e77f9809c0bdf2f512bfcf78670a49d`.
Comparison TSV SHA-256:
`bc50ca8190301f2f6590569c36789e3c87dbc7072299345847a1144634264286`.
Old 45b687c outputs were preserved byte-for-byte under revision-specific names;
see the archival note in `015-linux-portability-verification.md`.

Release first-frame worst 40.00 ms (<150), 1 MiB layout worst 157.27 ms (<250),
heap 46,869,472 bytes (<64 MiB), read-only analysis worst 606.86 µs (<10 ms),
retained capacity 6,572 bytes (<16 KiB), owned-frame p95 538.596 µs (<1 ms).
Dirty-diagnostic overhead is 0 µs (<100); nonzero raw paired durations remain
in the log and the unchanged validity oracle passes.

Consumer commands use `DOWNSTREAM_DIR=/tmp/oom-edit-task015-consumer-main` and
`DOWNSTREAM_REV=87d5b48f766eb35f30c2136e23d2a4448329713b`; preparation uses
`DOWNSTREAM_SOURCE=/home/duser/Developer/sources/personal/oom-edit` to seed
immutable local Git objects without reading/copying authenticated origin.
Each consumer owns its manifest/lock/vendor/config and checks with empty Cargo
home; no path dependency or floating branch substitutes for the target SHA.

macOS acceptance remains PASS by the user's explicit disposition for the
identical reviewed contents. No native macOS execution on the new squash SHA,
platform details or logs are fabricated; see `015-macos-portability-review.md`.

## Local tag creation and remaining authority boundary

Executed, exit 0:

```sh
git tag -a v0.6.0 87d5b48f766eb35f30c2136e23d2a4448329713b -m 'oom-edit 0.6.0: embeddable editor pane and standalone parity'
```

Verified annotated object type `tag`, peeled target exactly equal to main,
successful ancestry check and clean tracked worktree. No existing tag was
overwritten. Publication is not inferred: the local tag has not been pushed.
Separate authority to push only `refs/tags/v0.6.0` and ensuing published-tag
downstream verification remain pending. Task 015 remains IN_PROGRESS, attempt
1, workflow BLOCKED on that boundary; final integrated review/gate remains
pending task completion. No DONE transition or automatic archival.
