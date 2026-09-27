# Task 013: Embedding example and independent consumer

Delegation: main-only

## Goal

Prove that an external host can compose the public pane and that a separately locked/vendored consumer resolves the exact dependency and four patch sources.

## Context

OOM needs a living split-pane reference, and Cargo root patches do not propagate automatically to a consumer. A compilation-only example cannot prove input, hints, lifecycle, rendering or patch provenance.

## Scope

### In scope

FR-111/112/117, split-pane example, headless host tests, downstream fixture/prepare/check targets and provenance assertions.

### Out of scope

oom views/index/sync, release tag publication (Task 015).

## Implementation requirements

- Build a split screen with a stand-in host panel, pane-local drawing, host status row with exported hints/which-key, one intercepted host global key, host-triggered close-all, event drain and shared key translation vectors. Add scripted headless integration assertions for interception, frame/hints, AllClosed and disk notification→ReloadedFromDisk.
- Provide an out-of-workspace Cargo fixture with its own lock/vendor/config and clean Cargo home. Its manifest pins main oom-edit and hjkl-buffer, hjkl-engine, tree-sitter-md and dirs-sys patches to one candidate revision; assert resolved provenance, not mere compilation. Provide make-owned network preparation and offline/locked validation targets.
- Document canonical credential-free origin and the exact candidate revision/patch block. Do not print or copy authenticated remote URLs. Fixture checks construction, input, frame and lifecycle from public API only.
- Add make help entries and CI wiring for fixture validation once candidate artifacts exist; Task 015 runs exact candidate and post-tag checks after commit/authorization.

## Acceptance criteria

- [ ] Example builds and its headless test proves host interception, exported hints, close-all and reload events without private imports.
- [ ] Independent fixture owns lock/vendor and fails when any of four patch provenances is missing/mismatched.
- [ ] Make-owned prepare/check workflow is documented and check runs offline/locked against a candidate revision; final exact-revision execution is captured in Task 015.
- [ ] Shared translation vectors pass in standalone and example with enhancement on/off.

## Validation

`make build-examples`

`make test`

## Dependencies

012

## Expected areas of change

`crates/oom-edit/examples/` or root `examples/`, integration tests, standalone/example translation tests, downstream fixture files, `Makefile`, `.github/workflows/ci.yml`, `README.md`/`CONTRIBUTING.md`.

## Risks / notes

The fixture must not inherit workspace patches or lock silently. Network seeding is distinct from offline validation. Candidate revision and tag checks occur only after the relevant immutable commit exists.
