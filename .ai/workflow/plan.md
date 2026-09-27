# Embeddable Editor Pane and Standalone Parity

## Objective

Deliver the full embeddable `oom-edit` pane specified by the 2026-09-25 DRD, its linked implementation contracts, and its automated test plan. Another terminal application must construct, drive, render, observe and dispose of the editor through the curated `oom-edit` crate-root API. The standalone binary must use that same pane implementation while preserving its current appearance, command behavior and performance, except for the DRD's explicit disk-watching, missing-file protection and safer theme-persistence changes. Update project documentation, including `AGENTS.md`, and add `DEVELOPER.md` as a tested integration guide for another Rust project. Prepare version 0.6.0 and complete release verification; tag/push only after the separate release authorization required by the DRD.

Source of truth: `/home/duser/Developer/sources/personal/notebook/projects/oom/v1/oom-edit-drd.md` v1.1, `documents/implementation-contracts.md`, and `documents/automated-test-plan.md` beside it. The source documents remain outside this repository and are not modified by this work package.

## Current behavior

At baseline `eafaa6afa796d0258b72a6e5ab6ca5a3699d600a` (v0.5.0), `crates/oom-edit/src/lib.rs` exports only `Args`, `ParseOutcome`, and `run`. `run` loads process configuration/environment, builds one `App`, installs the terminal guard and enters the private event loop. `App` owns tabs, overlays and the `execute_lifecycle` dispatcher; rendering and resize handling assume a whole-terminal frame. `Document` owns file identity/serialization and tracks length/mtime, but save does not reject a disappeared previously-backed file. Reload reconstructs a session and loses position/undo. Config presence is separately tracked, and theme cycling rewrites a full config. The terminal guard uses process-global hooks and unconditional mouse/paste escapes. The existing core and TUI have substantial conformance, snapshots, dependency guards and performance gates, but no public pane host test.

The root `Makefile` provides `make check` (format, clippy, warning-free build, full tests, deny, audit, data-license check), `make ci` (release build and check), `make build-examples`, `make doc`, `make bench-check`, `make bench`, and baseline/candidate TUI performance recording/comparison. CI currently runs `make ci` on Ubuntu. `docs/performance.md` defines absolute release budgets and same-host comparison rules. The workspace is vendored/offline and the surveyed worktree was clean.

## Proposed implementation

Follow the DRD's sequence. First capture unchanged standalone behavior, public-host expectations and a same-machine performance baseline; create the clause-level test inventory. Add versioned disk state and lightweight analysis to the core. Export owned configuration, theme, style, notices and guard services. Build pane-local owned-frame rendering, then the `EditorPane` host protocol around the existing `App`, with stable IDs, policy-checked file I/O, typed events, focus/input/time and prepared lifecycle. Migrate the standalone loop through the public pane before adding disk polling and safe-point prompts. Consolidate bindings and export hints. Finish the embedded example, independent consumer, architecture guards, docs and performance suite. Finally version and verify the release candidate.

For every implementation slice: register exact cases/fixtures/expected outcomes before code, demonstrate behavioral red evidence after the API scaffolding (or capture a runnable unchanged characterization before pure extraction), make them green, review the slice, then run its task validation and the complete `make check` gate. Capture compact red/green and baseline/candidate evidence in task evidence. Existing snapshots, conformance and perf limits remain fixed unless a change is one of the explicitly allowed DRD behavior changes, and any golden change needs a reviewed explanation.

Task 007 validation amendment, approved by the user's 2026-09-25 answer: its release performance command is the owned-frame-specific `make bench-pane-frame`. `make bench-check` and `make check` remain required. The pre-existing v0.5.0 first-frame absolute failure is recorded in Task 007 evidence but is not accepted or waived: Task 014 must bring the complete unchanged `make bench` gate and same-host candidate limits green before package acceptance. No threshold or final command changes.

Revision 3 sequencing amendment, explicitly authorized by the user's 2026-09-27 answer "resolve it now please": supersede only the preceding first-frame-fix scheduling. Task 010 first diagnoses and fixes the known source/rendered first-frame absolute failures, adds exact regression coverage and runs unchanged `make bench`, `make bench-check` and `make check` before standalone migration. Task 014 retains integrated re-verification and same-host comparison. Completed tasks/evidence and all acceptance limits remain unchanged. This is the specific amendment the user selected at the recorded approval boundary, not approval of any additional plan change.

Revision 4 input amendment, explicitly authorized by the user's 2026-09-27 answer
"yes - approved": Tasks 014/015 use the preserved
`evidence/010-baseline-current-host.tsv` for their comparison commands because
the Task 001 measurement's 6-CPU host metadata does not match the current 8-CPU
host. The replacement is five trials of the exact unchanged baseline commit,
including its original absolute first-frame failures. Task 001 evidence and all
completed tasks remain immutable. No comparator logic, performance limit,
requirement, gate or publication authority changes.

Revision 5 Shift+V amendment, explicitly authorized on 2026-09-27 by the user's
answer "you are approved to fix the Shift+V bug you found": Task 015 fixes the
pre-existing terminal-reported Shift+V line-Select routing in rendered Normal
and Select. Add test-forward core and real terminal-translation/public-host
regressions covering entry, shape switching, cancellation and modifier
boundaries. Preserve input modifiers and core grammar ownership. Commit a new
candidate and rerun unchanged exact-SHA gates and same-host performance checks.
Only this bug is an additional exception to strict standalone parity; completed
tasks/evidence, source goldens, thresholds, native macOS verification and
separate release publication authority remain unchanged.

## Architectural decisions

- Keep `EditorSession` the only editing facade, `LiveDocument` the only mutable text/cache owner, `Document` the file/version owner, `App` the only UI state owner and `App::execute_lifecycle` the only lifecycle executor. Do not expose App, a mutable session escape, third-party public types, or a second dispatcher.
- Preserve the four public editor modes, current standalone command policies (`:tabnew` duplicates, dirty `:qa` refuses, no `:wqa`) and standalone inline hints/tab rule. Embedded host policies are explicit and use the same implementation; closing its last tab leaves an empty pane.
- Public input uses core `KeyInput` directly with owned paste/mouse DTOs; consumption means grammar ownership. One focus setter controls cursor, pending App chords, input routing and safe reload. Host time is injected; idle slice runtime is measured by the host.
- Public output is an owned cell grid with explicit default/indexed/RGB color, modifiers, grapheme/continuation and pane-local cursor/shape. Convert from internal ratatui off-screen rendering and clip at the requested dimensions. Keep the editor line in the pane; export hints and which-key for embedded host chrome.
- Give tabs pane-generation-scoped IDs and requests correlated IDs. Use captured IDs for continuations. Prepared close/retarget tokens are single-use; disk suspension begins only after prerequisite saves. All I/O routes through a typed `FileAccessPolicy`, including force and direct editor commands. Keep filesystem race limitations accurately documented.
- Distinguish metadata observations from content-validated opaque disk versions. Bind all overwrite/reload/recreation acknowledgements to the shown version. Preserve buffers on missing/error; reload through the atomic mutation gateway, preserving mode/undo when normalized content is identical and source cursor/viewport where possible otherwise.
- Keep process config/env/terminal setup in hosts. Public config carries theme-slot presence and preserves the existing schema. Standalone theme persistence edits the latest valid TOML semantically and atomically with expected-version protection; host sinks own appearance and dictionary persistence. No direct pane stdout/stderr.
- Maintain registry-derived App commands/hints/palette, metadata-only core references, exact public API/privacy guards, dependency direction and the single audited unsafe signal module. Use existing vendored crates; if sha2 is promoted to direct core dependency, complete the documented supply-chain review, lock/vendor and deny/audit gates.
- New developer and CI workflows receive `make` targets and matching CI use. Preserve exact existing performance budgets and add the DRD's ≤1 ms owned-frame overhead at 200×60, ≤1 metadata probe per backed tab per 2 s, public-host parity and same-host baseline/candidate measurements.

## Work included

| Task | DRD deliverables | Requirement groups and test focus |
| --- | --- | --- |
| 001 | D-012, D-015 baseline | Clause inventory for every included EDIT-FR/NFR, baseline command/frame/lifecycle/keyboard traces and same-host performance; red/green evidence rules and coverage meta-tests. |
| 002 | D-001 | FR-050–053, FR-116: opaque disk versions, missing-file protection, reload and retarget; byte-exact fault matrix. |
| 003 | D-017 | FR-115: read-only Markdown analysis and parser-equivalence fixtures. |
| 004 | D-005 | FR-060–067 except DRD-excluded FR-066: public config, validation, distinct paths/sinks, semantic atomic theme persistence. |
| 005 | D-006, D-009 | FR-070–075, FR-102: theme catalog/styles, explicit selection/env helper, live config, byte-exact notices. |
| 006 | D-008 | FR-090–094: one active configurable terminal guard, restore and key-translation vectors. |
| 007 | D-003 | FR-005, FR-010–019: owned frame, pane-relative/compact rendering, non-color signals and frame budget. |
| 008 | D-002 part 1 | FR-001–004/006, FR-020–036/100: pane construction, input/focus/time, stable tabs and typed events. |
| 009 | D-002 part 2 | FR-034/035/040–043/057/101/113/114: policy-checked I/O and prepared lifecycle/retarget transactions. |
| 010 | D-010 | FR-110/117: standalone host migration and full behavior/performance parity. |
| 011 | D-004 | FR-054–056/116: watcher, safe-point reload and version-bound prompts in both hosts. |
| 012 | D-007 | FR-080–085: single binding registry, dispatch ownership, hints, which-key and status exports. |
| 013 | D-011, D-016 | FR-111/112/117: split-pane example, headless host, independent offline/locked consumer with all four patch provenance assertions. |
| 014 | D-012, D-013, D-015 final; user documentation | Final API/privacy/coverage guards, `AGENTS.md` and `DEVELOPER.md` integration guidance, dependency evidence, all benchmark budgets and baseline comparison. |
| 015 | D-014 | Version 0.6.0 candidate, complete platform/release gate, separately authorized tag/push and exact-tag downstream verification. |

All Must and Should clauses in the DRD are included. The DRD itself explicitly excludes Nice FR-066 (preserving comments/formatting during `Space t`); semantic preservation of every unrelated TOML value is mandatory. The DRD's oom PRD cross-check is an API reachability check against its listed editor-dependent requirements, not implementation of oom's views, vault, index or sync.

## Task sequence

1. `tasks/001-characterization-and-coverage.md`
2. `tasks/002-core-disk-state.md`
3. `tasks/003-read-only-analysis.md`
4. `tasks/004-public-config-and-persistence.md`
5. `tasks/005-themes-styles-and-notices.md`
6. `tasks/006-terminal-guard.md`
7. `tasks/007-owned-pane-rendering.md`
8. `tasks/008-pane-host-protocol.md`
9. `tasks/009-prepared-lifecycle-and-policy.md`
10. `tasks/010-standalone-migration.md`
11. `tasks/011-disk-watcher.md`
12. `tasks/012-bindings-and-hints.md`
13. `tasks/013-example-and-downstream.md`
14. `tasks/014-docs-guards-and-performance.md`
15. `tasks/015-release-candidate.md`

## Quality gate

Every task runs `make check` in `gate.json`; this is the repository's full normal completion gate and includes formatting, clippy with `-D warnings`, warning-free build, the full test suite, license/advisory audits and bundled-data checks. Task-specific validation runs focused tests or relevant existing make targets; new test/coverage/downstream/platform workflows must first be added as discoverable make targets. The final gate uses existing commands `make ci`, `make build-examples`, `make doc`, `make bench-check`, `make bench`. `make ci` must be extended to include strict coverage and other worktree-local checks. The independent downstream candidate check runs through its own make target against the committed candidate and through CI's corresponding make invocation, avoiding a dependency on an uncommitted or nonexistent tag. Same-machine `make tui-perf-record`/`make tui-perf-compare` with five trials, the candidate downstream target and platform PTY/smoke targets are separately recorded acceptance evidence once added.

Task 007's approved exception to its local validation sequence is limited to the known baseline first-frame failure. It does not alter `gate.json`, Task 014's exact `make bench` requirement, or any package-level performance acceptance criterion.

There is no separate type checker in this Rust repository: `make check` already compiles every target through clippy/build/tests. No generic lint/test command is invented. `make deny` and `make audit` are mandatory even when no dependency changes because they are already part of `make check`.

## Risks

- Lifecycle confirmation, host transactions and disk prompts may conflict. Closed request states, stable captured targets and exact ordered-event tests guard against data loss or double outcomes.
- A new owned-grid conversion can lose wide/combining glyphs or add frame latency. Compare exact cells/cursors and measure conversion separately against the ≤1 ms release budget, plus existing first-frame/RSS gates.
- File checks based on stat alone miss equal-metadata replacements. Notifications/action validation hash content; version-bound decisions and injected failures assert retained bytes/buffers. A noncooperating writer remains a documented residual race.
- Moving event-loop responsibilities can change key filtering, resize, idle work, cursor escapes or redraw cadence. Characterization before migration and public-host replay afterward must match baseline.
- Guard changes interact with process-wide hooks/signals. Subprocess/PTY tests, partial-acquisition fault seams and no-new-unsafe guard are required.
- `make deny`/`make audit`, candidate dependency preparation, a macOS smoke runner and tag push may require network or external facilities. These are acceptance prerequisites; absence cannot be represented as a passing gate. The DRD explicitly requires separate release publication authority after candidate review.

## Out of scope

Implementing oom itself; wiki links/backlinks; OS file watchers; host commands inside oom-edit's palette/which-key; runtime standalone config reload; guaranteed multi-pane-process support; cross-program portable compare-and-swap; crates.io publication. Nice FR-066 comment/format preservation is the DRD's explicit exclusion, while unknown/unrelated TOML values must survive. No existing command, rendering semantic, theme design or Markdown parsing behavior changes beyond the DRD's explicit exceptions and the approved revision 5 Shift+V repair.

## Final acceptance criteria

1. All included DRD Must/Should FR clauses and NFR-001–010 map to named executable tests with exact assertions, fixtures and make targets; the coverage meta-test rejects missing, unknown, ignored and unjustified excluded cases, and every implementation task records behavioral red/green evidence.
2. A separate consumer can construct zero or multiple tabs, feed owned input, render a bounded owned frame, inspect tabs/status/hints/bindings/styles, perform prepared close/retarget and observe ordered correlated events, using only public crate-root APIs and no terminal dependencies in core.
3. Host file policy and token suspension cover every I/O entry point; stale IDs/versions and invalid dimensions return typed outcomes without panic or data loss. The complete disk fault matrix preserves retained text/undo and exact on-disk bytes, including missing files and committed-but-durability-uncertain saves.
4. Public-host traces and standalone terminal snapshots for four modes, commands, tabs, paste/mouse, prompts, overlays, search, spell, clipboard and config match v0.5.0 baseline except the explicit DRD changes. Standalone keeps its old tab/quit policies, editor line, hints, cursor behavior, key filtering and redraw cadence.
5. Existing release performance limits in `docs/performance.md` all pass unchanged. Five same-host baseline/candidate trials show no disallowed wall/CPU/RSS/heap regression; the 200×60 owned-frame overhead is ≤1 ms in release, metadata polling is ≤1 probe per backed tab per 2 s, and public-path input/redraw/idle/analysis budgets are measured and pass.
6. `make ci`, `make build-examples`, `make doc`, `make bench-check`, `make bench`, the new coverage/downstream/platform targets, data-license/notices checks, and native Linux/macOS automated/PTY and README-key-table smoke checks pass with recorded evidence. All make targets are discoverable and used by CI as applicable.
7. The independent consumer with its own lock/vendor tree and clean Cargo home builds/drives the candidate offline/locked and asserts the main revision plus all four patch provenances. `AGENTS.md`, `README.md`, `CONTRIBUTING.md`, `CHANGELOG.md` and a new `DEVELOPER.md` accurately document the architecture, canonical origin, exact tested revision, full external Rust integration procedure, configuration/public API and intentional disk/persistence changes; integration commands and code snippets are exercised against the example/fixture.
8. Editor/core report version 0.6.0; after explicit release authorization, the exact candidate is tagged/pushed and the tag is independently verified through downstream consumption. The work package is DONE only when this release acceptance is satisfied; it is not archived automatically.
