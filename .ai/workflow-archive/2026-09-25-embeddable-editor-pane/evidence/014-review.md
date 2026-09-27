# Task 014 integrated review and regression probes

This is review/probe evidence, not task-completion or release evidence. Task 014
remains attempt 1 under approved revision 4. The frozen hash is
`a9943dfacf2b945ab8517c7b1ee3c559179ff14ab7c3ad4cb18b32f6e19b305e`.

## Coverage and boundary review

The final inventory now resolves 184 cases across 91 requirements; FR-066 is
the sole specification-authorized exclusion. Broad planning rows were checked
against actual assertions, corrected to describe their executable fixture, and
supplemented with explicit cases for missing subscenarios. Public assertions now
cover all four cursor modes/focus states at seven sizes, both tab-bar policies,
two-line empty states, exact three-tab metadata/MRU and index-shift identity,
three initial-path outcomes around an invalid UTF-8 file, compact close and
overwrite choices, separate dictionary roots and real persistence failure.
Prepared-close save faults cover both before replacement and uncertain durability.

Full crate-root export statements, all pane method signatures and eleven new
compile-fail privacy/escape examples guard the facade. Recursive production
guards retain hjkl isolation and exactly the two existing audited signal unsafe
blocks. Existing four-mode/conformance/property/registry/dependency tests remain.
The 28-clause oom PRD reachability table explicitly distinguishes host-owned work.
The complete developer Rust snippet is byte-identical to the independently
exercised consumer and executes in package tests. Both editor facades deny
missing public docs; rustdoc warnings are fatal.

## Test-forward observations

- Initial package/documentation guards were red for missing policy, metadata,
  guide and CI integration; see `/tmp/oom-edit-task014-red.log` and
  `/tmp/oom-edit-task014-doc-red.log`. The reachability guard was separately red
  before its document (`/tmp/oom-edit-task014-package-reachability-red.log`).
- Strict-location mutations reject missing/external/compiled-out cases, ignored
  cases without one exact executing recipe, nonexistent targets and stubbed or
  skipped Python/Rust bodies. Seven mutation tests pass. Review added a red
  single-assertion/skip-alias regression
  (`/tmp/oom-edit-task014-python-guard-red.log`) before fixing the AST oracle;
  the corresponding `-green.log` passes. Meaningful single assertion calls are
  accepted; expectedFailure and aliased skip decorators are rejected.
- New runtime tests characterize unchanged production behavior, not newly
  invented UI labels. The compact oracle was corrected to established Save[y/w]
  and Overwrite[o]; the diagnostic remains a bold bullet, not an invented W.
  README now describes the actual diagnostic glyph. No existing golden changed.
- Task 004's missing historical pre-implementation red log remains disclosed
  in its immutable evidence. Reconstruction uses a separate temporary source
  snapshot `fe866808aa7f1a9fb70225501ab88f86bbdb4398`, then deliberately mutates
  configuration behavior there, never in the working implementation. Removing
  explicit default-theme presence, using the wrong wrap fallback and dropping
  an unrelated TOML value produces three real compiled failures
  (`/tmp/oom-edit-task014-config-reconstructed-red.log`). Skipping authored spell
  enablement produces a fourth real compiled failure
  (`/tmp/oom-edit-task014-config-roundtrip-red.log`). Restoring the source makes
  all four cases pass (`/tmp/oom-edit-task014-config-reconstructed-green.log`).
  Source and test files compare byte-identically with the working tree after
  restoration. These are retrospective regression probes, not a fabricated
  historical log. They use `make test-config-public` and existing dependencies.

## Gate findings repaired

- First integrated CI run rejected a complex function-pointer type in the
  API test. A private query-type alias preserves exact signature checking
  without suppressing warnings. Warning-fatal lint passes.
- Next integrated CI run reached the release dirty-diagnostic benchmark and
  rejected eight zero net-overhead samples as "measured no work". This was a
  measurement-oracle defect: saturating subtraction can yield zero when both
  operations are timed and the diagnostic path is equal or faster. The new
  deterministic case reproduced that rejection in
  `/tmp/oom-edit-task014-overhead-red.log` (ten other core smoke cases passed).
  Checked overhead now requires two nonzero raw timings, records both totals,
  and permits a nonpositive additional duration. Both edited buffers must match
  exact inserted bytes and diagnostic removal/shift assertions still hold.
  Other absolute operations retain their nonzero-work checks. The unchanged
  strict <100 microsecond limit, exact-boundary rejection and lowered-zero
  negative oracle all remain. `make bench-check` is green in
  `/tmp/oom-edit-task014-overhead-green.log`. The subsequent full CI/release
  run passed (`/tmp/oom-edit-task014-ci-complete.log`), including a worst
  additional dirty-diagnostic duration of 94.38 microseconds. The final rerun
  after the API conversion correction below also passes in
  `/tmp/oom-edit-task014-ci-final.log`.
- Boundary review found public standard `From` implementations accepting
  ratatui `Color` and `Modifier` on exported project-owned style DTOs. The
  package guard and compile-fail examples reproduced both escapes before
  implementation (`/tmp/oom-edit-task014-conversion-guard-red.log` and
  `/tmp/oom-edit-task014-conversion-doc-red.log`). These conversions are now
  named crate-private helpers with identical conversion logic. The new guard,
  expanded owned-signature checks and all 29 workspace doctests pass
  (`/tmp/oom-edit-task014-conversion-guard-green.log` and
  `/tmp/oom-edit-task014-conversion-doc-green.log`). This closes the renderer
  leak without changing frames or editor behavior.
- The next integrated rerun rejected a hardcoded color in the new compile-fail
  documentation example (`/tmp/oom-edit-task014-ci-final-boundary.log`). The
  example now constructs a typed renderer default rather than a color literal;
  the existing color-confinement guard is unchanged.

The final `make ci` exits zero: seven normal checks, strict 184-case coverage,
examples, warning-fatal rustdoc, debug performance and all release gates pass.
Representative release results: first frame worst 39.42 ms (<150 ms), 1 MiB
layout worst 155.89 ms with 46,869,472 heap bytes (<250 ms / <64 MiB), dirty
diagnostic additional duration worst 18.78 microseconds (<100), 64-document
analysis worst 645.33 microseconds with 6,572 retained bytes (<10 ms / <16 KiB),
and 200x60 owned-frame conversion p95 510.45 microseconds (<1 ms).

These failures occurred during task-specific review/validation. The separate
standard TaskGate has not yet run; no standard-gate retry is consumed.

## Preserved constraints

No editor runtime behavior, original snapshot, performance threshold, comparator,
approved planning artifact, completed evidence or vendored crate was relaxed.
The public renderer conversion escape was removed rather than broadening the
owned-only API. Original Task 001 baseline evidence is unchanged. The approved
current-host comparison previously passed all 60 metrics, but current integrated
completion still requires every validation command and the full TaskGate.
Native macOS, immutable 0.6.0 candidate and publication checks belong to Task 015;
none is claimed here. No root commit, tag or push has occurred.
