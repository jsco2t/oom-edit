# Task 001: Add environment-free release CI

Delegation: main-only

## Goal

Create and verify a minimal GitHub Actions pipeline that builds the release binary and runs the complete `make check` gate through one Make-owned command for every pull request and every push to `main`.

## Context

The repository currently has no hosted CI workflow, but it already owns release compilation through `make build-release` and its definition-of-done gate through `make check`. Repository policy requires new CI workflows to have an equivalent discoverable Make target, and the project avoids unnecessary dependencies and external infrastructure.

## Scope

### In scope

- Add a `make ci` target that sequentially invokes the existing release-build and full-check targets.
- Add a GitHub Actions workflow for every pull request and for pushes to `main` on a hosted Linux runner.
- Apply least-privilege permissions and immutable action pinning, and install exact versions of the audit tools required by `make check`.
- Add dependency-free automated checks for the workflow contract and register them in both `make test` and the test section of `make check`.
- Add the new CI target to existing developer documentation.

### Out of scope

- Release publication, packaging, signing, or artifact upload.
- Operating-system or architecture matrices.
- Service containers, servers, simulators, emulators, Docker, or network-backed tests.
- Performance benchmark targets and performance-evidence workflows.
- Dependency or vendored-source changes.

## Implementation requirements

- The workflow must call `make ci` and must not duplicate raw Cargo or Python repository commands.
- `make ci` must call `make build-release` before `make check` and must propagate either failure.
- The workflow must trigger for unrestricted `pull_request` events and for `push` events restricted to `main`. It must not run for a push to another branch unless that revision is also being validated through a pull request.
- The workflow must declare read-only contents permission and a finite job timeout.
- Every `uses:` reference must be a full commit SHA with an adjacent human-readable release identifier.
- Install exact versions of `cargo-deny` and `cargo-audit` through a SHA-pinned action that verifies checksums and has fallback installation disabled. Do not use mutable action tags or unversioned tool requests.
- Do not add caching, service containers, simulators, servers, or application infrastructure provisioning. The checked-in `rust-toolchain.toml`, Cargo lockfile, and vendored dependencies remain authoritative.
- The contract test must use only the Python standard library and verify the requested triggers, permission, timeout, action and tool-version pinning, lack of services/infrastructure commands, `make check` integration, and exact Make entry point.
- The contract test must be reachable from both `make test` and `make check`.
- `make help` and `CONTRIBUTING.md` must expose how to run the same CI path locally.

## Acceptance criteria

- [ ] `.github/workflows/` contains a valid, minimal workflow for every pull request and for pushes restricted to `main`.
- [ ] The workflow uses read-only contents permission, a finite timeout, immutable action SHAs, exact audit-tool versions, no service infrastructure, and a single repository verification command: `make ci`.
- [ ] `make ci` runs the existing release binary build before `make check` and returns nonzero on either failure.
- [ ] A dependency-free contract test detects drift in all workflow invariants listed above.
- [ ] The contract test is included in the canonical test and check paths.
- [ ] The new Make target is discoverable through `make help` and documented in `CONTRIBUTING.md`.
- [ ] No Cargo manifest, lockfile, vendored source, or product implementation is changed.

## Validation

- `make ci-workflow-test`
- `make ci`

## Dependencies

None

## Expected areas of change

- `.github/workflows/ci.yml`
- `Makefile`
- `scripts/test_ci_workflow.py`
- `CONTRIBUTING.md`

## Risks / notes

- Keep the workflow sufficiently small that the standard-library contract test can assert its invariants without pretending to be a general YAML parser.
- A push to `main` is the post-merge trigger and will also cover direct pushes to `main`; this is intentional branch protection behavior.
- The hosted job intentionally verifies a Linux release build only. Broader release packaging requires a separate approved work package.
