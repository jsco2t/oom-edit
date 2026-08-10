# oom-edit

oom-edit is a console/TUI markdown editor written in Rust with exactly four public modes: rendered Normal, source Insert, rendered character/line/block Select, and Command. It includes tree-sitter-highlighted Markdown source, rich YAML/TOML front matter, and fenced code blocks. The editing core ships as the reusable `oom-edit-core` crate — embeddable in other applications with zero terminal dependencies; the `oom-edit` binary is a thin ratatui shell over it.

**License:** MIT

## Planning docs (authoritative)

Engineering plans live outside this repo in the project notebook. Read these when scoping work — do not duplicate them here.

- **Project index:** `$HOME/Developer/sources/personal/notebook/projects/oom-edit/index.md`
- **Plan & specification:** `…/oom-edit/plan.md` — requirement IDs (FR-x.y, NFR-x, V-xx, VW-x, VN-x, VP-x) are referenced throughout the tasks.
- **Architecture:** `…/oom-edit/architecture.md` — crate/module structure, public API contracts, seams.
- **Tasks:** `…/oom-edit/tasks/index.md` — the ordered implementation plan. Work tasks in order; update the tracking table as you go.
- **Markdown Spec** Located in the repo at `docs/markdown-spec.md`.

The plan and architecture documents are the source of truth for scope and structure. If implementation reality contradicts them, stop and resolve the contradiction explicitly — don't silently diverge.

## Architecture posture

- **Reusable core, thin UI.** All editing logic, highlighting, markdown modeling, and rendered Markdown live in `oom-edit-core`, which must never depend on ratatui, crossterm, or any terminal/IO-event crate. The TUI is a presentation layer. Anything a second application would want belongs in the core.
- **One wrapper for the engine.** The `hjkl` crates are pinned pre-1.0 and fast-moving; every `hjkl_*` type is confined to `crates/oom-edit-core/src/vim.rs`. No hjkl type appears in any other module or any public signature. Upgrades touch that one file plus the conformance suite.
- **Renderer-agnostic styling.** The core emits semantic style slots (`SemanticStyle`); colors exist only in the TUI's theme module. No signal is ever color-only — a modifier or text glyph always carries it (accessibility is a requirement, not a theme).
- **Static everything.** Tree-sitter grammars are statically linked, vendored crates. No runtime grammar loading, no plugins, no network features of any kind.

## Engineering principles

1. **Test-forward.** Every Must-Have requirement has automated tests; the modal surface is guaranteed by a conformance suite with meta-tests that make silent coverage loss impossible. Property-based tests for incremental-highlight equivalence and rendered↔source position mapping are first-class deliverables, not nice-to-haves.
2. **Simplicity over cleverness.** Prefer obvious code, narrow abstractions, and well-trodden patterns. Don't introduce generality for hypothetical future requirements. Three similar lines beats a premature abstraction.
3. **Limited external dependencies.** Only add external dependencies when it is very clear they solve a major gap in functionality. For minor features it's worth evaluating writing the code directly in the repo (vs adding N more dependencies). Worked examples already decided at planning time: hand-rolled CLI args (no clap), hand-rolled snapshot harness (no insta), hand-rolled OSC 52 + base64 (no clipboard crates), hand-rolled fuzzy matcher.
4. **Registry as single source of truth (TUI).** App commands are declared once; the hint bar, which-key, command palette, and dispatch are projections of that registry, with drift-prevention meta-tests. Nothing user-facing is hand-maintained in two places.
5. **Pure build + thin render.** Layout/content computations are pure functions over injected inputs (width, clock, state) and unit-testable headlessly; ratatui adapters stay thin. Clocks are always passed in as parameters.
6. **No data loss.** Saves are atomic (write temp + fsync + rename); the live file is never truncated. External modifications are detected before overwrite. A crash or panic never corrupts the file being edited and never leaves the terminal in raw mode.

## Supply chain rules (non-negotiable)

- **Minimize external dependencies — actively.** Every new direct dependency drags transitives, expands the license-audit and security-review surface, increases the vendored tree, and adds versions that go stale. **Before adding a dep, evaluate whether the functionality could be written directly in the repo.** Hand-rolling a small well-specified algorithm is almost always preferable to importing a feature-rich library that solves a hundred problems we don't have. If a dep is genuinely needed, prefer the narrowest crate that solves only our problem.
- **Permissive licenses only:** MIT, Apache-2.0, BSD-2/3-Clause, ISC, Zlib, Unlicense, CC0-1.0, Unicode-3.0, Unicode-DFS-2016.
- **Forbidden:** GPL-2.0, GPL-3.0, LGPL-2.1/3.0, AGPL-3.0, SSPL, Commons Clause, anything copyleft. **GPL-with-linking-exception is also forbidden** (e.g., libgit2 — the ambiguity isn't worth it; `git2` / `libgit2-sys` are banned in `deny.toml`).
- **Pinned exact versions.** `Cargo.lock` is the source of truth and is committed. The `hjkl` and tree-sitter crate families are pinned exact (`=x.y.z`) — pre-1.0 churn and grammar-ABI compatibility make range pins dangerous here.
- **Vendored dependency tree.** All crates vendored at `vendor/` via `cargo vendor`. Builds run `--offline` / `CARGO_NET_OFFLINE=true`.
- **Off-target vendored sources are unavoidable; budget for them.** `Cargo.lock` is target-agnostic by design, and cargo's source replacement validates the entire lockfile against `vendor/`, so Windows/Android/wasm sources of platform-conditional transitives land on disk even though rustc never compiles them for our triples. `deny.toml`'s `targets = [...]` keeps license/advisory checks scoped to the four shipped triples (macOS aarch64/x86_64, Linux x86_64/aarch64). This is one more reason "minimize dependencies" is non-negotiable.
- **No build-script networking.** Dependencies' `build.rs` must not fetch anything or shell out to undocumented executables. (Tree-sitter grammar crates compiling their _vendored_ C sources via `cc` is compliant — they fetch nothing.)
- **Dependency-add/upgrade checklist** (documented in the PR/commit description): license check, upstream maintenance signal, popularity baseline, diff review of vendored sources, **and an honest assessment of "could we hand-roll this instead?"** Record the outcome in `docs/dependencies.md`.
- **Enforcement:** `cargo deny` for license + advisory + banned crates; `cargo audit` for RustSec advisories. Both are CI gates via `make deny` / `make audit`.

## Definition of done (every change, non-negotiable)

A change is complete only when **all** of the following hold — no exceptions, no partial credit:

1. The code is written — no stubs, no `todo!()`, no "wire up later".
2. Tests covering the new/changed behavior are written **in the same change**.
3. `make fmt-check` passes.
4. `make lint` passes (clippy with `-D warnings`).
5. `make build` passes with **NO** `warnings` or `errors` (all build issues regardless of origin must be resolved)
6. **All** tests pass — `make test`, the entire suite, not just the new tests.
7. If dependencies changed: `make deny` and `make audit` pass and `vendor/` + `Cargo.lock` are updated in the same change.

`make check` runs the core gate (fmt-check + lint + build + test + deny + audit). If `make check` is red, the change is not done — regardless of how done it feels.

## No deferred work

**Never defer, skip, or stub out work without explicitly asking the user first.** If a task's acceptance criteria cannot be fully met — because a dependency is missing, an API doesn't exist yet, an upstream crate behaves differently than planned — stop and ask. Do not silently mark items as "wiring-pending", "deferred to follow-up", "lands later", or "simplified version". If implementing the full requirement requires adding a dependency, writing more code, or solving a harder problem, do that work rather than shipping a stub. If it is genuinely impossible, say so up front as the first thing communicated, not buried in a report footnote.

## Build system

**`make` is the build system of record. If you can't do it via a make target, it doesn't exist.** Every developer-facing workflow — build, test, lint, format, vendor, supply-chain audit, docs, benchmarks — has a target in the top-level `Makefile`. Developer workstations and CI run identical commands by going through `make`; raw `cargo` invocations are reserved for ad-hoc exploration. Run `make` (or `make help`) to list every target.

The canonical targets (all cargo invocations use `--offline --locked` except `vendor`/`toolchain`):

| Target                        | What it does                                                     |
| ----------------------------- | ---------------------------------------------------------------- |
| `make toolchain`              | One-time dev bootstrap: Rust via rustup, cargo-deny, cargo-audit |
| `make build`                  | `cargo build --workspace`                                        |
| `make test`                   | Full test suite                                                  |
| `make test-update-snapshots`  | Re-run with `OOM_UPDATE_SNAPSHOTS=1` to (re)write golden files   |
| `make test-all`               | Tests + example builds                                           |
| `make fmt` / `make fmt-check` | Auto-format / format check (CI gate)                             |
| `make lint` / `make lint-fix` | `cargo clippy -- -D warnings` (CI gate) / apply safe suggestions |
| `make check`                  | fmt-check + lint + build + test + deny + audit — **the local CI gate** |
| `make deny` / `make audit`    | License/ban/advisory checks (CI gates)                           |
| `make doc`                    | `cargo doc --no-deps`                                            |
| `make vendor`                 | Re-vendor deps (the only target that needs network)              |
| `make bench`                  | Criterion benchmarks (NFR performance budgets)                   |
| `make run ARGS=...`           | Run the editor                                                   |
| `make clean`                  | Remove build artifacts                                           |

### Keeping the Makefile up to date — a project rule

**When new functionality introduces a new developer or CI command, add a corresponding `make` target in the same change.** New lint passes, test categories (benchmarks, fuzzers, property suites), code-gen steps, script harnesses, release/packaging steps — all of them. Two reasons:

1. **Discoverability.** Every workflow lives in `make help`; no command exists only in someone's shell history, a CI YAML, or a commit message.
2. **CI / dev parity.** CI invokes `make`, so anything CI does is reproducible locally with the same target name. If a PR adds a `run:` shell command to a CI workflow, the equivalent `make` target must land in the same PR.

If a target's command grows long or sprouts cases, prefer flag variables (`CLIPPY_FLAGS`, etc.) over inlining.

## Git commit messages

Never include `Claude-Session:` lines, session URLs, or any Claude/AI attribution metadata in commit messages. Commit messages describe the change — they are not a place for tool provenance.

## Shell command style

Prefer running commands as separate Bash tool calls rather than chaining them with `&&`, `||`, `;`, or pipes, so the permission matcher can authorize them individually.

Exceptions where chaining is fine:

- Pipes that are part of a single logical operation (`grep ... | wc -l`) — these only make sense as one command.
- `cd <dir> && <cmd>` when the directory change must scope to that one command and not persist.

When in doubt, run them separately.
