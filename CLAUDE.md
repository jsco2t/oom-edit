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

### Supported patterns and boundary rules

These are the application's established patterns. Extend them instead of introducing a parallel ownership model, routing path, registry, or public API surface.

#### Public API and crate boundaries

- **Curated crate-root facades.** Implementation modules in `oom-edit-core` stay private; the re-exports in `crates/oom-edit-core/src/lib.rs` are the complete supported API. The `oom-edit` crate exports only `Args`, `ParseOutcome`, and `run`. Add public API deliberately at the crate root and extend the compile-time API guards in the same change.
- **Owned boundary types.** Public errors and DTOs are project-owned. Do not expose third-party parser, terminal, renderer, or `hjkl` types in public signatures.
- **Dependency direction is one-way.** `oom-edit` may depend on `oom-edit-core`; core must not know about App, ratatui, crossterm, terminal capabilities, overlays, or screen geometry beyond explicit renderer-neutral inputs such as width and viewport.

#### Canonical state and mutations

- **One owner for live text.** Private `LiveDocument` is the sole owner of mutable editor text and synchronously-derived highlighting/front-matter caches. `VimCore` supplies the authoritative text. `Document` owns file identity, serialization policy, and I/O metadata; saves receive the authoritative text explicitly. Never add a second mutable text copy.
- **Atomic mutation gateway.** Text changes go through `LiveDocument` and return `MutationOutcome`; the gateway refreshes every derived cache before control returns. Do not mutate the Vim buffer, highlighter, front matter, or rendered invalidation state independently.
- **One session facade.** Hosts interact through `EditorSession`: feed terminal-neutral input, query state/layout, and consume typed effects. New editing behavior belongs behind that facade unless it is strictly App-owned presentation or lifecycle orchestration.
- **Closed state machines over flag bags.** Modes, selections, search prompts, registers, pending chords, confirmations, and lifecycle requests use enums whose variants contain all state needed for that state. Avoid parallel `Option` fields and booleans that can encode stale or impossible combinations.

#### Input, commands, and effects

- **Translate input once.** Crossterm events become the core `KeyInput`/`KeyCode`/`Modifiers` model at the TUI boundary and are forwarded unchanged. Do not introduce subsystem-specific key models.
- **Single routing owner per grammar.** App owns only the Space-prefix command grammar and modal exclusivity; core owns counts, `g` sequences, registers, Vim motions/operators, and ex input. Unknown or incomplete App chords forward to core without consuming or rewriting their first key.
- **Static registries are sources of truth.** TUI commands/bindings are declared once in `command::COMMANDS`; dispatch, which-key, hints, and palette rows are projections with drift-prevention tests. `LangDef` rows own grammar/query/aliases, and `BuiltinThemeSpec` rows own built-in theme ordering and compatibility. Do not maintain duplicate lists.
- **Typed, closed effects.** Cross-boundary requests use enums with semantic payloads (`SetWrap(bool)`, `TabAction`, lifecycle request types), not command strings, magic numbers, or loosely-related flags. Core command/reference rows are metadata unless an actual core dispatcher owns them; never present metadata-only rows as executable App commands.

#### Rendering and source provenance

- **Attach provenance at parser leaves.** Rendered source ownership is constructed as mapped atoms when Markdown leaves are parsed. Text, semantic style, and source span travel together through inline composition, wrapping, tables, and block layout.
- **Synthetic output is source-less.** Borders, padding, list decorations, continuation prefixes, and other renderer-created glyphs must not claim a source byte. Never reconstruct provenance afterward with global substring matching, candidate searches, or column correction.
- **Normalization is context-aware.** Markdown escape and code normalization must follow the construct being rendered (for example, table-cell code versus prose). Preserve exact UTF-8 byte ranges and cover ambiguous/repeated text, escapes, entities, wrapping, and tables with byte-exact regression tests.
- **Keep operation DTOs consumer-owned.** Rendered selections are projected into the private `vim::ProjectedSelection` operation model before reaching the Vim wrapper. `vim.rs` must not import rendered-layout DTOs.

#### Lifecycle, determinism, and verification

- **One lifecycle executor.** `App::execute_lifecycle` is the sole workflow for save, close, replace, open-tab, and quit-all actions. `LifecycleAction` values capture target tab indices; confirmations own the complete action/continuation. Never re-read the active tab to finish a target-relative continuation.
- **Modal interactions are exclusive.** While an overlay, prompt, or confirmation owns input, background command routing does not also act on that input. Destructive transitions preserve the no-data-loss rules; force behavior exists only behind an explicit bang/force request.
- **Inject nondeterminism at boundaries.** Pure builders receive clocks and other environmental inputs. Sample an event timestamp after reading the event and before dispatch. Capture environment into owned values and query through adapters; do not leak data to manufacture `'static` lifetimes.
- **Guard architectural constraints with tests.** Add or update public-API compile tests, dependency-hygiene guards, registry completeness/uniqueness tests, exact mode/command meta-tests, and focused transition tests whenever a boundary changes. A passing end-to-end snapshot is not a substitute for these guards.

## Engineering principles

1. **Test-forward.** Every Must-Have requirement has automated tests; the modal surface is guaranteed by a conformance suite with meta-tests that make silent coverage loss impossible. Property-based tests for incremental-highlight equivalence and rendered↔source position mapping are first-class deliverables, not nice-to-haves.
2. **Simplicity over cleverness.** Prefer obvious code, narrow abstractions, and well-trodden patterns. Don't introduce generality for hypothetical future requirements. Three similar lines beats a premature abstraction.
3. **Limited external dependencies.** Only add external dependencies when it is very clear they solve a major gap in functionality. For minor features it's worth evaluating writing the code directly in the repo (vs adding N more dependencies). Worked examples already decided at planning time: hand-rolled CLI args (no clap), hand-rolled snapshot harness (no insta), hand-rolled OSC 52 + base64 (no clipboard crates), hand-rolled fuzzy matcher.
4. **Registry as single source of truth (TUI).** App commands are declared once; the hint bar, which-key, command palette, and dispatch are projections of that registry, with drift-prevention meta-tests. Nothing user-facing is hand-maintained in two places.
5. **Pure build + thin render.** Layout/content computations are pure functions over injected inputs (width, clock, state) and unit-testable headlessly; ratatui adapters stay thin. Clocks are always passed in as parameters.
6. **No data loss.** Saves are atomic (write temp + fsync + rename); the live file is never truncated. External modifications are detected before overwrite. A crash or panic never corrupts the file being edited and never leaves the terminal in raw mode.

## Supply chain rules (non-negotiable)

- **Minimize external dependencies — actively.** Every new direct dependency drags transitives, expands the license-audit and security-review surface, increases the vendored tree, and adds versions that go stale. **Before adding a dep, evaluate whether the functionality could be written directly in the repo.** Hand-rolling a small well-specified algorithm is almost always preferable to importing a feature-rich library that solves a hundred problems we don't have. If a dep is genuinely needed, prefer the narrowest crate that solves only our problem.
- **Permissive licenses only:** MIT, Apache-2.0, BSD-2/3-Clause, ISC, Zlib, Unlicense, CC0-1.0, Unicode-3.0, Unicode-DFS-2016. `HPND-sell-variant` is additionally approved **only for the pinned SCOWL-generated bundled word-list data**: every normalized asset must retain its applicable copyright header, and every source/binary distribution must carry the complete permission notices and warranty disclaimers in supporting documentation.
- **Forbidden:** GPL-2.0, GPL-3.0, LGPL-2.1/3.0, AGPL-3.0, SSPL, Commons Clause, anything copyleft. **GPL-with-linking-exception is also forbidden** (e.g., libgit2 — the ambiguity isn't worth it; `git2` / `libgit2-sys` are banned in `deny.toml`).
- **Pinned exact versions.** `Cargo.lock` is the source of truth and is committed. The `hjkl` and tree-sitter crate families are pinned exact (`=x.y.z`) — pre-1.0 churn and grammar-ABI compatibility make range pins dangerous here.
- **Vendored dependency tree.** All crates vendored at `vendor/` via `cargo vendor`. Builds run `--offline` / `CARGO_NET_OFFLINE=true`.
- **Off-target vendored sources are unavoidable; budget for them.** `Cargo.lock` is target-agnostic by design, and cargo's source replacement validates the entire lockfile against `vendor/`, so Windows/Android/wasm sources of platform-conditional transitives land on disk even though rustc never compiles them for our triples. `deny.toml`'s `targets = [...]` keeps license/advisory checks scoped to the four shipped triples (macOS aarch64/x86_64, Linux x86_64/aarch64). This is one more reason "minimize dependencies" is non-negotiable.
- **No build-script networking.** Dependencies' `build.rs` must not fetch anything or shell out to undocumented executables. (Tree-sitter grammar crates compiling their _vendored_ C sources via `cc` is compliant — they fetch nothing.)
- **Dependency-add/upgrade checklist** (documented in the PR/commit description): license check, upstream maintenance signal, popularity baseline, diff review of vendored sources, **and an honest assessment of "could we hand-roll this instead?"** Record the outcome in `docs/dependencies.md`.
- **Zero-advisory policy includes informational soundness advisories and transitive dependencies.** A tool message such as `Warning: unsound` or `allowed warning found` is unresolved under project policy even if the process exits zero. Green exit status is not an implicit risk acceptance.
- **Exceptions are explicit.** An advisory may be ignored only after documenting the affected dependency path, reachability/impact analysis, compensating controls, owner, and removal condition in `docs/dependencies.md`, then adding the narrow advisory ID plus reason to the audit configuration. Never rely on a scanner's default warning level as an exception mechanism.
- **Enforcement:** `cargo deny` for license + advisory + banned crates; `cargo audit` for RustSec advisories. Both are CI gates via `make deny` / `make audit`; their configuration must fail on direct and transitive vulnerability and unsoundness findings unless the preceding exception process was followed.

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
| `make build-examples`         | Build all examples with locked offline dependencies              |
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
