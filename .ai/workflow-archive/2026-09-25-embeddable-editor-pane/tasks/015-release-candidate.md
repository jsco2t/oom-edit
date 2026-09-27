# Task 015: Version 0.6.0 release candidate and authorized tag

Delegation: main-only

## Goal

Version, commit and validate the exact 0.6.0 candidate, then publish and verify its tag only under separate explicit release authorization.

## Context

The DRD defines a tagged release as Goal 5 but expressly separates implementation planning/approval from release publishing authority. A downstream fixture must validate the immutable candidate before any tag exists.

## Scope

### In scope

D-014 and final D-016: editor/core version bump, exact candidate commit, all gates, native platform/PTY and README smoke evidence, independent candidate and post-tag consumption, authorized tag/push.

### Out of scope

Crates.io publication, binary archive publication, oom release or automatic workflow archival.

## Implementation requirements

- Bump editor and core to 0.6.0 with exact intra-workspace pins/lock/vendor consistency; review cumulative diff and commit candidate without unrelated user changes. Record immutable revision.
- Run final commands on the exact candidate: `make ci`, examples, docs, debug/release benchmarks, strict coverage, Linux/macOS automated PTY and manual README key-table smoke. Record platform/toolchain/results; do not claim a missing platform passed.
- Run five same-host baseline/candidate comparison on the committed candidate. Run make-owned independent consumer preparation (network only there) and offline/locked candidate check with a clean Cargo home, asserting main and all four patch revisions.
- Request/receive separate explicit release authorization before creating/pushing `v0.6.0`; never infer it from `$feature-workflow approve`. After authorization, tag exactly the gated commit, push only intended refs, and run independent exact-tag consumer verification.
- If external runner/network/authorization is unavailable, report the precise blocker and preserve the candidate; do not mark release/DONE complete.

## Acceptance criteria

- [ ] Editor/core versions, exact pins, lock/vendor and notices match 0.6.0 candidate commit.
- [ ] All final quality, coverage, benchmark, parity, platform and README smoke gates pass on the exact candidate with evidence and no disallowed regression.
- [ ] Independent consumer builds offline/locked from the immutable candidate with correct main/four-patch provenance and exercises public behavior.
- [ ] After separate explicit authorization, tag `v0.6.0` points to the gated commit, intended push succeeds and exact-tag downstream verification passes.

## Validation

`make ci`

`make build-examples`

`make doc`

`make bench-check`

`make bench`

`make tui-perf-record BRANCH_ROLE=candidate OUTPUT=.ai/workflow/evidence/perf-release-candidate.tsv TRIALS=5`

`make tui-perf-compare BASELINE=.ai/workflow/evidence/010-baseline-current-host.tsv CANDIDATE=.ai/workflow/evidence/perf-release-candidate.tsv OUTPUT=.ai/workflow/evidence/perf-release-comparison.tsv`

`make downstream-candidate-check`

`make downstream-tag-check`

## Dependencies

014

## Expected areas of change

`crates/oom-edit/Cargo.toml`, `crates/oom-edit-core/Cargo.toml`, `Cargo.lock`, `CHANGELOG.md`, downstream fixture/docs, `Makefile`, CI, release evidence.

## Risks / notes

The two downstream targets are introduced in Task 013. The post-tag target runs only after the separately authorized tag exists; no candidate gate may require a tag it is meant to approve. The workflow stays active until that release requirement is met.

Revision 4 input amendment explicitly approved on 2026-09-27: only the comparison
baseline path above changes to Task 010's preserved current-host measurement of
the unchanged original baseline commit. Original Task 001 evidence and all
performance thresholds remain unchanged.

Revision 5 Shift+V amendment explicitly approved on 2026-09-27: fix only the
pre-existing terminal-reported Shift+V line-Select routing in rendered Normal
and Select. Add behavioral red/green regressions for entry, switching,
cancellation and modifier boundaries through core and translated public-host
input. Preserve modifier translation, grammar ownership and completed evidence.
Review and commit a new candidate, then rerun all unchanged exact-SHA local
gates, performance comparison and independent consumption. No native platform
requirement, performance threshold, golden or publication authority is waived.
