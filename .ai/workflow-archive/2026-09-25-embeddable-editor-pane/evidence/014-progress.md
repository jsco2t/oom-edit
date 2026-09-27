# Task 014 progress — not completion evidence

Captured: 2026-09-27T10:10:30Z. Implementation: main, attempt 1.
Task remains IN_PROGRESS; workflow is PLAN_CHANGE_REQUIRED.

## Verified progress

- Added test-forward package guards; the initial runnable red tests rejected
  missing public documentation policy, canonical metadata, the integration
  guide and incomplete local CI ownership. Logs:
  `/tmp/oom-edit-task014-red.log`, `/tmp/oom-edit-task014-doc-red.log`.
- Documented the public TUI facade under `#![deny(missing_docs)]`; `make doc`
  treats warnings as errors and passed after documentation additions
  (`/tmp/oom-edit-task014-doc-progress.log`).
- Created `DEVELOPER.md` with the exact independently exercised consumer
  snippet; updated AGENTS/CONTRIBUTING/README/CHANGELOG and canonical repository
  metadata. Worktree-local `make ci` now owns coverage, examples, docs and
  performance separately from immutable-revision downstream CI steps.
- Added compiled-case/location validation and negative tests for missing,
  disabled, ignored and unknown workflows. `make drd-coverage-test` passed
  five tests. Coverage locations were provisionally reconciled to existing
  assertions, but the final strict gate is not green and the fixture/oracle
  audit is unfinished. These entries are not an acceptance claim.
- Exact five-trial candidate command completed successfully: 15 cases × 5
  trials, all 75 statuses `pass`, including both previously failing first-frame
  cases. Artifact: `perf-candidate.tsv`; log:
  `/tmp/oom-edit-task014-performance.log`. This is a pre-commit worktree run,
  not exact release-candidate evidence; Task 015 must rerun it.

## Approval boundary

The exact approved command was executed:

`make tui-perf-compare BASELINE=.ai/workflow/evidence/perf-baseline.tsv CANDIDATE=.ai/workflow/evidence/perf-candidate.tsv OUTPUT=.ai/workflow/evidence/perf-comparison.tsv`

It exited 2; log `/tmp/oom-edit-task014-comparison.log` reports
`baseline/candidate mismatch for logical-cpus`. The original baseline records
6 logical CPUs; the current candidate records 8. Rust compiler, OS, architecture,
CPU model, memory and fixture version agree. No comparator limit was changed,
no metadata was rewritten and no comparison success is claimed.

Proposed narrow amendment: change only Tasks 014/015's baseline comparison paths
to the preserved `010-baseline-current-host.tsv`. That artifact is a five-trial
run of the exact original baseline commit
`eafaa6afa796d0258b72a6e5ab6ca5a3699d600a` on the current 8-CPU host. It retains
the original baseline's two first-frame failures (10 failed absolute samples)
instead of disguising them; the candidate must still pass every unchanged
absolute and relative limit. Original Task 001 and all completed evidence remain
immutable. Explicit approval and a recorded plan revision/hash are required.

Frozen plan hash verified unchanged:
`d10dd746bb31ecadc41ab8212c3d775437526ac8801b5304675c3f8bbe606389`.

## Resume checklist after approval

- Apply only the authorized incomplete-task comparison-path amendment, preserve
  planning history/completed tasks and record the new frozen approval/hash.
- Finish fixture/oracle review of every coverage entry. Current strict coverage
  stopped at two planned key-vector locations using the nonexistent name
  `fr_094_shared_terminal_vectors`; actual executable function is
  `fr_094_example_shared_translation_vectors` in `tests/embedding_example.rs`.
- New public compact confirmation characterization ran (26 other lifecycle
  tests passed) but its literal oracle expects `Save [y]`; the existing preserved
  UI says `Save [y/w]`. Correct the test oracle, not the established UI, then
  check every remaining close/overwrite label and distinct outcome. Log:
  `/tmp/oom-edit-task014-lifecycle.log`.
- Run the new public-path prepared-close prerequisite save-fault assertion
  (before-replace and durability boundaries); it is written but not yet run.
- Complete PRD/API reachability checklist and full architecture/API guards;
  review coverage expected outcomes against actual assertions rather than
  merely accepting a function name. Several original planning fixtures group
  more scenarios than their first mapped location; add separate cases/tests
  as needed, without weakening requirements.
- Format, review and run all Task 014 validation plus the entire standard
  `make check`; create final task evidence only after all required results pass.
- Native macOS runner availability is still unanswered for Task 015. Do not
  claim platform verification, commit/tag readiness or publication.

No standard-gate retry has been consumed by these task-local characterization
or comparison failures. No root Git commit, tag or push was performed.
