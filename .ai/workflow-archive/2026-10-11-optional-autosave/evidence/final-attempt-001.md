# Final gate attempt 001 — blocked by an existing core defect

## Completed feature review

Both approved tasks are COMPLETE, with evidence in `001.md` and `002.md`.
The integrated source review found no blocking autosave defect. The approved
plan hash remains `147ae2c1e83c719f090bd76305ef24bcfbede3fc30af8a7cb59c0a84e1e4a643`.

## Final command result

`make check` failed: 6 gate components passed and 1 failed.

| Component | Result |
| --- | --- |
| fmt-check | PASS |
| lint | PASS |
| build | PASS |
| full tests | FAIL — core incremental highlighting property test |
| deny | PASS |
| audit | PASS |
| data-license-check | PASS |

The remaining final commands have not run on this attempt. Earlier task
validation is successful, but this package has not passed final acceptance.

## Failure evidence

- Test: `syntax::tests::proptest_with_backward_and_newlines`.
- Location: `crates/oom-edit-core/src/syntax/mod.rs:5523`.
- Assertion: incremental versus fresh highlighting, line 0 / span 0 start
  column, observed `3` versus `0`.
- The core unit suite reported 527 passed, 1 failed, 12 ignored; the failure
  occurred after 49 generated successes.
- Proptest saved seed
  `a6891b578d5a9dae7ffec3db672f51eb2f3aa7c5b7132fdca8de7b4dd8144049`
  to `crates/oom-edit-core/proptest-regressions/syntax/mod.txt`. Retain that
  automatically generated regression instead of discarding the failure.
- `git diff 1960ade52a394a7c23fe400fdbb395283834ab72 -- crates/oom-edit-core/src`
  is empty. The isolated core test exercises incremental/fresh syntax analysis
  and does not import or execute the autosave feature.

Both task gates previously passed. The final randomized suite exposed an
existing source-analysis defect; retrying with a different random case or
removing its persisted seed would not resolve it.

## Required scope decision

The approved plan permits core changes only for a genuine defect needed for
autosave correctness and explicitly excludes unrelated fixes. Repairing this
incremental-highlighter mismatch needs an additional core bug-fix scope and
regression coverage. Feature Workflow requires `PLAN_CHANGE_REQUIRED` and
human approval when a required repair materially changes the approved plan.

Requested next action: expand this package to investigate and repair the core
incremental-highlighting mismatch, retain its regression, and rerun all final
quality commands without weakening the gate. No unrelated source fix has
been made, and the package is not marked DONE or archived.
