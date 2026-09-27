# Task 015 exact candidate verification — incomplete release

Historical checkpoint, superseded by the approved Shift+V amendment and
`015-candidate-shift-v.md`. The original measurements referenced below were
preserved byte-for-byte as `perf-release-ab30fa4-candidate.tsv` (SHA-256
`90387f9d48a6323b35500770d2c09c43f038746ab7e4b0312cb5320fffd9e922`) and
`perf-release-ab30fa4-comparison.tsv` (SHA-256
`5731b689ae41a9c34a15cc918d83b2b4ead7b08e27f332b133b645a651cb6d7a`)
before the active output paths were regenerated for the new candidate.

Main-only, attempt 1, approved revision 4. Active candidate:
`ab30fa474a8cd816e018cbe9c3a5f5577706b2ee`. Initial candidate
`9c9ad9e24c2f7a1264d5700ccb2d1ddae736d24f` and its failed performance run are
preserved, not amended. Correction and actual red/green evidence are in
`015-progress.md`. Neither this document nor green local CI completes Task 015.

Editor/core versions and exact path/lock pins are 0.6.0, spell is 0.1.0.
Third-party vendor bytes and complete notice surfaces are unchanged. Tracked
worktree is clean. Approved plan hash remains
`a9943dfacf2b945ab8517c7b1ee3c559179ff14ab7c3ad4cb18b32f6e19b305e`.

## Exact-revision results

| Command/check | Result |
| --- | --- |
| `make ci` | PASS — `/tmp/oom-edit-task015-candidate-ci-final.log`; all seven normal checks and strict coverage |
| `make build-examples` | PASS — exact child command in candidate CI |
| `make doc` | PASS — exact child command, warnings fatal |
| `make bench-check` | PASS — exact child command, debug core/TUI |
| `make bench` | PASS — exact child command, all unchanged release limits |
| `make tui-perf-record BRANCH_ROLE=candidate OUTPUT=.ai/workflow/evidence/perf-release-candidate.tsv TRIALS=5` | PASS — 75/75 rows; every revision field is active SHA |
| `make tui-perf-compare BASELINE=.ai/workflow/evidence/010-baseline-current-host.tsv CANDIDATE=.ai/workflow/evidence/perf-release-candidate.tsv OUTPUT=.ai/workflow/evidence/perf-release-comparison.tsv` | PASS — 60/60 metrics |
| `make terminal-guard-pty-test` | PASS — seven native Linux cases |
| Manual README key-table smoke | FAIL — pre-existing Shift-V cannot enter line Select; other rows pass; `015-linux-smoke.md` |
| Native macOS automated/PTY/manual verification | NOT RUN — no Mac runner supplied |
| `make downstream-prepare` with directory/SHA/local source below | PASS |
| `make downstream-candidate-check` with directory/SHA below | PASS — independent empty Cargo home, offline/locked metadata, warning-fatal Clippy and public behavior test |
| `make downstream-negative-check` with directory/SHA below | PASS — 20 actual-source negatives and real vendored checksum corruption |
| Authorized release tag/push and `make downstream-tag-check` | NOT RUN — neither publication authorization nor tag exists |

Independent consumer arguments: `DOWNSTREAM_DIR=/tmp/oom-edit-task015-consumer-b`,
`DOWNSTREAM_REV=ab30fa474a8cd816e018cbe9c3a5f5577706b2ee`; preparation additionally
uses `DOWNSTREAM_SOURCE=/home/duser/Developer/sources/personal/oom-edit`.
Actual own lockfile and resolved metadata assert the canonical credential-free
Git origin and the same full SHA for `oom-edit` 0.6.0, `hjkl-buffer` 0.39.0,
`hjkl-engine` 0.39.0, `tree-sitter-md` 0.5.3 and `dirs-sys` 0.5.0. Consumer
sources resolve exclusively inside its own vendor tree, not this workspace.
Raw logs: `/tmp/oom-edit-task015-downstream-prepare-final.log`,
`/tmp/oom-edit-task015-downstream-check.log`,
`/tmp/oom-edit-task015-downstream-negative.log`.

Native environment: Linux x86_64, Rust 1.97.1 (8bab26f4f 2026-07-14), Ryzen 7
5700U, eight logical CPUs. Release measurements: first-frame worst 40.02 ms
(<150), 1 MiB layout worst 160.08 ms / 46,869,472 heap bytes (<250 ms / <64 MiB),
dirty-diagnostic additional duration worst 41.13 microseconds (<100), 64-document
analysis worst 639.89 microseconds / 6,572 retained bytes (<10 ms / <16 KiB),
owned 200x60 frame conversion p95 509.051 microseconds (<1 ms). Matched 16-edit
raw totals are now approximately 28–31 ms rather than the initial candidate's
248–252 ms, with identical fixtures/sampling/limits.

## Required decision and remaining authority

The native manual failure is confirmed in original baseline source. Fixing
Shift-V would change the plan's strict standalone-parity baseline; an explicit
narrow amendment was requested asynchronously, but no answer has arrived.
The workflow stops at `PLAN_CHANGE_REQUIRED`, preserving all 001–014 completion
evidence and the immutable candidate. Task 015 stays IN_PROGRESS, not COMPLETE.

After amendment approval: update only the affected approved plan/task boundary,
implement/test the actual terminal-reported Shift-V transition (including its
shared-vector coverage gap), create a new immutable candidate and repeat every
candidate gate. Native Mac verification must still be supplied. Only after all
candidate checks pass may separate release publication approval be requested;
then tag/push exactly the gated SHA and independently verify its exact tag.
No standard Task 015 or final package gate is claimed complete before those
prerequisites. No requirement, timing limit or failed smoke result is waived.

The existing authenticated origin exposed an embedded token in an earlier
diagnostic tool output; the user was warned to rotate it before publication or
sharing logs. No credential value is copied into evidence or consumer files,
and no remote configuration, tag or push was changed.
