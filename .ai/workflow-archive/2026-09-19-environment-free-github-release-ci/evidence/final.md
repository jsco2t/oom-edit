# Final Work-Package Evidence

## Work package

- ID: `2026-09-19-environment-free-github-release-ci`
- Title: `Environment-Free GitHub Release CI`

## Original objective

Add a GitHub Actions pipeline that verifies a release build and runs all tests that need no simulator or server infrastructure. Run it for every pull request and for pushes to `main`, including merged pull-request revisions, using the repository-owned `make check` gate.

## Completed tasks

- [x] 001 — Add environment-free release CI — [`evidence/001.md`](001.md)

## Whole-package review

Reviewed the approved plan, task specification, task evidence, GitHub Actions workflow, Make orchestration, contract tests, documentation, and cumulative repository changes as one integrated package. The workflow trigger scope, release-build ordering, `make check` integration, least-privilege permissions, immutable action pins, exact tool versions, environment-infrastructure exclusions, local discoverability, and canonical test registration all match the approved acceptance criteria. No high-confidence defects remained.

## High-confidence findings fixed

None

## Final acceptance criteria

- [x] The workflow runs for every pull request and for pushes to `main`, while excluding pushes to other branches. Evidence: `.github/workflows/ci.yml` declares an unrestricted `pull_request` trigger and restricts `push.branches` to `main`; the contract test asserts both conditions.
- [x] The workflow is read-only, provisions no service infrastructure, pins actions and Cargo tools, and delegates repository verification to `make ci`. Evidence: the workflow declares `contents: read`, no services or simulator/server setup, full action SHAs, exact cargo-audit and cargo-deny versions, and one repository command; contract tests guard each invariant.
- [x] `make ci` builds the `oom-edit` release binary before running `make check`. Evidence: its two sequential recursive Make recipes invoke `build-release` and then `check`; the final `make ci` run passed.
- [x] Automated workflow-contract coverage detects drift in triggers, permissions, pinning, infrastructure exclusions, `make check` integration, and Make ownership. Evidence: `scripts/test_ci_workflow.py` contains seven dependency-free unit tests and is registered in both canonical test paths.
- [x] The CI target is locally discoverable and documented. Evidence: `make help` exposes `ci`, and `CONTRIBUTING.md` includes `make ci` in its target table.
- [x] Every required quality command passes. Evidence: the final quality gate below completed successfully.

## Final quality gate

| Command | Result |
| ------- | ------ |
| `make fmt-check` | PASS |
| `make check` | PASS |
| `make ci` | PASS |

## Cumulative diff

From baseline `7fbfd7784ca1fa7800dcecb0820f7bffdab81999`, the package adds the GitHub Actions CI workflow and its dependency-free contract test, adds Make-owned `ci` and `ci-workflow-test` entry points, registers contract coverage in the canonical test/check paths, and documents local CI reproduction. It also adds the feature-workflow planning, task, status, and evidence records. No product source, Cargo manifest, lockfile, vendored dependency, or runtime dependency changed.

## Remaining non-blocking concerns

None
