# Environment-Free GitHub Release CI

## Objective

Add a GitHub Actions pipeline that builds the `oom-edit` binary with the release profile and runs the repository's complete `make check` gate for every pull request and for pushes to `main`, including the push produced by merging a pull request. Keep the hosted workflow aligned with locally discoverable Make targets and require no simulator, service container, server, or other application infrastructure.

## Current behavior

The repository has no committed GitHub Actions workflows. It already has the two required underlying operations:

- `make build-release` builds the `oom-edit` release binary with locked, vendored dependencies and Cargo offline mode.
- `make test` is documented as the complete test suite. It runs the feature-workflow Python tests, TUI performance-tooling Python tests, and the full Rust workspace test suite under an isolated XDG configuration directory.
- `make check` is the repository's definition-of-done gate. In addition to `make test`, it runs formatting, linting, a debug workspace build, dependency policy checks, RustSec auditing, and bundled-data license verification.

Repository inspection found no test group that requires a simulator, emulator, service container, network server, or separately provisioned application infrastructure. Release and debug performance benchmarks are exposed as separate `make bench` and `make bench-check` gates rather than as part of the canonical `make test` suite.

## Proposed implementation

1. Add a discoverable `make ci` target that runs `make build-release` followed by `make check`. This keeps the GitHub workflow and local reproduction on one build-system-owned command while making the full repository gate explicit.
2. Add a minimal GitHub Actions workflow for every pull request and for pushes to `main`. A merged pull request creates a push to `main`, so this covers both requested lifecycle points without running on unrelated branch pushes. The job will run on a GitHub-hosted Linux runner, use read-only repository permissions, and check out the source with an immutable action revision.
3. Install the exact `cargo-deny` and `cargo-audit` versions needed by `make check` through a SHA-pinned installer action with checksum verification and fallback installation disabled. GitHub's current Ubuntu runner includes Rust but does not include both required Cargo tools. The repository's `rust-toolchain.toml` remains the source of truth for Rust 1.97.1 and its components.
4. Invoke only the Make-owned `make ci` target for repository build and verification commands.
5. Add a dependency-free workflow contract test and register it in `make test`. The test will guard trigger coverage, least-privilege permission, immutable action and tool-version pinning, absence of service infrastructure, and exclusive use of the Make-owned CI entry point.
6. Document `make ci` in the existing developer target table.

## Architectural decisions

- The Makefile remains the single source of truth for CI commands. GitHub Actions orchestrates checkout and calls `make ci`; it does not duplicate Cargo or Python command lines.
- The CI target uses the existing `build-release` and `check` targets instead of creating a second definition of either operation. `make check` owns the canonical test suite and all definition-of-done checks.
- The workflow uses the pinned toolchain declared in `rust-toolchain.toml` and vendored Cargo sources. No Rust dependency, cache action, server, simulator, or service container is added.
- Checkout and tool-installer actions are pinned to full commit SHAs to reduce mutable-tag supply-chain risk. Human-readable release tags are retained in comments for maintainability, installed Cargo tool versions are exact, checksum verification remains enabled, and installer fallback is disabled.
- `pull_request` is intentionally unrestricted so pull requests into any base branch are validated. `push` is restricted to `main`; this reruns verification on the merged revision and also protects any direct push to `main`.
- The workflow does not upload or publish the release binary. The request is to verify that the release build succeeds, not to create a release distribution.
- Contract coverage uses Python's standard library and textual invariants tailored to this small static workflow, avoiding a YAML-parser dependency.

## Work included

- A GitHub Actions workflow triggered by every pull request and by pushes to `main`.
- A Make-owned environment-free CI target.
- Release-profile compilation of the `oom-edit` binary.
- The complete canonical `make check` gate, including formatting, linting, debug build, Python and Rust tests, supply-chain audits, and data-license verification.
- Version-pinned installation of the two Cargo tools required by `make check` but absent from the hosted runner.
- Automated drift checks for the workflow contract.
- Developer documentation for local CI reproduction.

## Task sequence

1. `tasks/001-add-environment-free-release-ci.md` — implement the Make target, workflow, contract tests, and documentation.

## Quality gate

The standard and final gate run:

- `make fmt-check` to satisfy the repository's explicit formatting requirement.
- `make check` to run the complete definition-of-done gate: formatting, linting, debug workspace build, full tests, dependency policy, RustSec audit, and bundled-data license verification.
- `make ci` to execute the exact release-build and `make check` path that GitHub Actions will invoke.

No separate YAML linter exists in the repository, so one is not invented or added as a dependency. The focused standard-library contract test provides deterministic coverage of the required workflow structure and is also reached through `make test` and `make check`.

## Risks

- GitHub-hosted runner images are mutable and do not currently provide both Cargo audit tools required by `make check`. The workflow installs exact tool versions using a checksum-verifying, SHA-pinned action and otherwise relies on the repository-pinned Rust toolchain and vendored dependencies.
- Makefile test orchestration is currently represented both by the `test` target dependencies and by the summarized `check` recipe. Registering the new contract test must update both paths so `make test` and `make check` cannot drift.
- YAML has syntax and key-coercion edge cases. The workflow will stay deliberately small, and the contract test will assert the exact structural lines required by this design without adding a parser dependency.
- Running the full `make check` gate after a release build duplicates some compilation work but keeps the requested behavior explicit and locally reproducible. Caching is intentionally deferred to avoid introducing mutable state before CI runtime data justifies it.

## Out of scope

- Publishing releases, creating GitHub Releases, signing binaries, packaging, or artifact upload.
- Cross-platform or cross-architecture release matrices.
- Simulator-, emulator-, server-, Docker-, or service-container-backed tests.
- Exact performance benchmarking through `make bench`, debug performance gating through `make bench-check`, and baseline/candidate performance evidence comparison. These are separate timing-sensitive workflows, not part of the canonical `make test` target.
- Dependency updates, vendoring changes, or new third-party libraries.
- Runs for pushes to non-`main` branches. Those revisions are validated when represented by a pull request; the post-merge rerun is limited to `main`.

## Final acceptance criteria

- A committed GitHub Actions workflow runs for every pull request and for pushes to `main`, including merged pull-request revisions, while ignoring pushes to other branches.
- The workflow has read-only repository permissions, provisions no service infrastructure, pins every referenced action and required Cargo tool version, and invokes repository verification through `make ci` rather than raw Cargo or Python commands.
- `make ci` performs a successful release build of the `oom-edit` binary and then runs `make check`.
- The workflow contract has automated coverage that fails if required triggers, permissions, action or tool pinning, infrastructure exclusions, `make check` integration, or the Make-owned entry point drift.
- `make help` exposes the CI target and `CONTRIBUTING.md` documents it for local reproduction.
- `make fmt-check`, `make check`, and `make ci` all pass.
