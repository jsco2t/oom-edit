# Working with the oom-edit codebase

`oom-edit` is currently a single-developer project and is not accepting
external code contributions or pull requests. Issues and reproducible bug
reports are welcome. This document exists to make the codebase understandable
and to support maintenance, auditing, local development, and forks.

## Toolchain and repository setup

The workspace uses stable Rust **1.97.1**, pinned in `rust-toolchain.toml`, with
the `rustfmt` and `clippy` components. The remaining developer tools are:

- GNU Make as the build-system entry point;
- `cargo-deny` for license, advisory, and banned-crate policy;
- `cargo-audit` for RustSec advisory checks; and
- Bash for repository scripts and isolated test configuration.

Install the pinned Rust toolchain and Cargo tools with:

```console
make toolchain
```

Dependencies are locked in `Cargo.lock` and vendored under `vendor/`. Normal
builds, tests, and documentation generation run with Cargo's `--offline` and
`--locked` options. A fresh checkout therefore should not need the network once
the toolchain and Cargo tools are installed.

Run `make help` to see every supported workflow. The most important targets
are:

| Target | Purpose |
| --- | --- |
| `make fmt` | Format the Rust workspace |
| `make fmt-check` | Check formatting without changing files |
| `make lint` | Run Clippy for all workspace targets with warnings denied |
| `make build` | Build the complete workspace |
| `make test` | Run the complete test suite with isolated configuration |
| `make check` | Run the full local CI gate |
| `make test-all` | Run tests and build examples |
| `make bench-check` | Run debug performance smoke gates |
| `make bench` | Run release performance gates |
| `make doc` | Build API documentation without dependencies |
| `make run ARGS=file.md` | Run the editor against a file |
| `make run-isolated ARGS=file.md` | Run without reading or writing user configuration |

`make check` is the definition-of-done gate. It runs formatting, linting,
building, all tests, dependency policy checks, RustSec auditing, and bundled
data-license verification. A change is not complete while this target is red
or while any step emits an unresolved warning.

If a new developer or CI workflow is introduced, add a discoverable Make
target in the same change. CI and local development should invoke the same
target rather than maintaining separate command sequences.

## Workspace architecture

The Cargo workspace has exactly three project-owned product crates. The
repository also carries a patched support crate outside that product
workspace:

```text
oom-edit (terminal application)
    |
    +--> oom-edit-core (editing and rendering engine)
    |        |
    |        +--> oom-spell (generic spell-checking primitives)
    |
    +--> oom-spell (dictionary host integration)

dirs-sys-patched (local permissive-license replacement for a transitive crate)
```

The dependency direction is intentional. `oom-edit` may depend on the core,
but `oom-edit-core` must never depend on ratatui, crossterm, terminal geometry,
terminal capabilities, filesystem configuration, or application overlays.
Anything another application could reuse belongs in the core and communicates
through renderer-neutral input, output, and effect types.

### `oom-edit-core`

`crates/oom-edit-core` is the embeddable editor engine. It has no terminal
dependencies and exposes a deliberately small facade from `src/lib.rs`.
Implementation modules remain private; adding a public item requires an
explicit crate-root re-export and a matching update to the compile-time public
API guards.

Its major components are:

- `session.rs`: `EditorSession`, the host-facing state machine. It owns the
  four public modes, routes terminal-neutral `KeyInput`, coordinates rendered
  navigation and source editing, and emits typed `Effect` values for work the
  host must perform.
- `session/live_document.rs`: the sole owner of mutable editor text. Every
  edit passes through this mutation gateway, which updates syntax,
  front-matter, and spelling state before returning. Do not introduce a second
  mutable text cache.
- `vim.rs`: the only wrapper around the pinned `hjkl` crates. No `hjkl` type
  may escape this module or appear in a public signature. Rendered selections
  are projected into operation-specific types before reaching this boundary.
- `document.rs`: file identity, line-ending and final-newline preservation,
  external-modification detection, and atomic saving. The live text is passed
  into document saves explicitly.
- `syntax/`: tree-sitter Markdown parsing, injected fenced-language
  highlighting, incremental reparsing, source spans, and the static language
  registry. Grammars are statically linked; there is no runtime loading.
- `rendered/`: construction, wrapping, tables, navigation, selection
  projection, and rendered-to-source mapping for Normal and Select modes.
  Source provenance is attached while parser leaves are converted to mapped
  atoms. Generated borders, padding, list markers, and other synthetic glyphs
  remain source-less.
- `frontmatter.rs`: YAML and TOML front-matter parsing into project-owned
  values while preserving source-facing behavior for rendering.
- `spell/`: Markdown-aware exclusions, resumable session scanning,
  diagnostics, and edit invalidation on top of `oom-spell`.
- `input.rs`, `style.rs`, and `clipboard.rs`: boundary types used by hosts.
  Input is terminal-neutral, styles are semantic rather than color-based, and
  clipboard writes cross the boundary through a project-owned trait.
- `error.rs`: project-owned errors for open, save, and front-matter failures.

The crate-root API is the supported contract. In particular,
`EditorSession` is the single session facade; `Mode` is a closed four-variant
enum; `Effect` carries semantic host requests; and layout DTOs carry semantic
styles and byte-accurate source provenance without terminal types.

### `oom-edit`

`crates/oom-edit` is the thin ratatui/crossterm application shell. Its crate
root intentionally exports only `Args`, `ParseOutcome`, and `run`.

Its major components are:

- `args.rs` and `lib.rs`: hand-written CLI parsing and startup ordering. CLI
  messages and file-open failures occur before raw terminal mode is entered.
- `app.rs`: the single owner of live TUI state, including tabs, per-tab scroll
  positions, overlays, pending application chords, transient messages, theme,
  and injected host services. It forwards core input and consumes core
  effects.
- `event.rs`: the draw/poll/dispatch loop. Event timestamps are sampled after
  input is read, and bounded spell work runs only during proven idle time.
- `command/`: the static `COMMANDS` registry and the application-owned
  Space-prefix grammar. Dispatch, which-key text, hint bars, and palette rows
  are projections of this one registry.
- `lifecycle.rs`: closed request types for save, close, replace, open-tab, and
  quit-all workflows. `App::execute_lifecycle` is the only executor for these
  actions, and every target-relative request captures its tab index.
- `screens/` and `widgets/`: thin ratatui adapters for core layouts, status and
  tab bars, hints, and semantic spans. Layout decisions should stay in pure,
  headlessly testable functions when possible.
- `overlay/`: exclusive modal UI for confirmations, help/palette, spelling
  suggestions, and diagnostics. When an overlay owns input, background command
  routing must not also process it.
- `config.rs`: TOML loading, defaults, and atomic persistence under the XDG
  configuration directory. Configuration failures warn and fall back to
  defaults instead of preventing startup.
- `theme.rs`: built-in theme registry, light/dark resolution, capability
  detection, and mapping from core `SemanticStyle` slots to terminal colors
  and modifiers. Accessibility signals must never rely on color alone.
- `spell_host.rs`: bundled/additional/personal dictionary loading, resumable
  engine construction, and safe personal-dictionary persistence.
- `clipboard.rs`: OSC 52 output and the small in-tree base64 encoder.
- `terminal_guard.rs`: raw mode, alternate-screen lifecycle, and restoration
  on normal exit, panic, or supported fatal signals.

The TUI translates crossterm input exactly once into core `KeyInput`. It owns
only presentation, terminal lifecycle, tabs, the Space-prefix command grammar,
and services such as configuration and clipboard output. Counts, registers,
motions, operators, search, and ex input remain core responsibilities.

### `oom-spell`

`crates/oom-spell` is a dependency-free, reusable English spell-checking
library. It has no filesystem, Markdown, terminal, editor, or clock knowledge.

- `engine.rs` normalizes entries, incrementally builds a deduplicated engine,
  performs lookup, and produces bounded suggestions.
- `tokenize.rs` incrementally tokenizes UTF-8 text around supplied exclusion
  ranges.
- `policy.rs` decides which generic text candidates are eligible for checking.

Filesystem policy and bundled dictionary data belong to the application;
Markdown exclusions and diagnostic positions belong to the core; word-list
algorithms belong here.

### Patched and vendored sources

`crates/dirs-sys-patched` is a narrowly modified local replacement used to
avoid a forbidden-license transitive dependency. `patches/` contains other
auditable overrides, including the `hjkl-buffer` and `tree-sitter-md` patches.
These are part of the dependency boundary, not product architecture; keep
changes minimal and document their reason.

`vendor/` is generated dependency source. Do not edit it as the first step of
an ordinary code change. Dependency updates must update manifests,
`Cargo.lock`, `vendor/`, and `docs/dependencies.md` together through the
repository's dependency workflow.

## Runtime data flow

Understanding the following paths prevents most architectural drift.

### Startup

1. `main` parses CLI arguments before terminal setup.
2. `run` loads configuration, resolves the theme and dictionaries, and opens
   an `EditorSession`.
3. `App` is constructed with explicit clipboard, configuration, and spelling
   services.
4. `TerminalGuard` enters raw mode and the alternate screen.
5. The event loop draws, polls, timestamps, and dispatches events until the
   lifecycle state requests exit.

### Input and effects

1. Crossterm events are translated once at the TUI boundary.
2. An active overlay receives input exclusively.
3. In rendered modes, the application checks its Space-prefix state machine.
4. Unconsumed input is passed unchanged to `EditorSession`.
5. The session returns typed effects such as save, quit, clipboard, message,
   wrap, help, and tab requests.
6. `App` consumes those effects, invokes host services, follows scrolling, and
   renders the resulting state.

Unknown or incomplete application chords must fall through to the core
without consuming or rewriting their first key.

### Text mutation and rendering

1. Vim operations mutate the authoritative buffer through `LiveDocument`.
2. The same mutation refreshes syntax, front matter, and spell invalidation.
3. Rendered layout caches are invalidated as part of session handling.
4. The renderer builds styled, wrapped, source-mapped rows at the requested
   width.
5. The TUI maps semantic styles to the active theme and paints the layout.

Never update the Vim buffer, highlighter, front-matter cache, spell state, or
render invalidation independently.

### Saving and destructive lifecycle actions

The core tracks document metadata and serializes authoritative text; the TUI
executes lifecycle policy and confirmations. Saves use a temporary file,
`fsync`, and rename. External changes are detected before overwrite, and force
behavior is available only through explicit bang/force actions. Confirmations
retain the complete target and continuation so switching tabs cannot redirect
an in-progress operation.

## Making changes safely

Follow the established extension points instead of adding parallel ownership
or registries:

- Add reusable editing behavior behind `EditorSession`; keep TUI-only
  presentation in `oom-edit`.
- Add an application command or binding to `command::COMMANDS`, then update
  dispatch and registry drift tests. Do not hand-maintain separate palette,
  hint, or which-key lists.
- Add a fenced-code grammar through the `LangDef` registry and its uniqueness
  and completeness tests. Grammar crates are statically linked.
- Add a built-in theme through the `BuiltinThemeSpec` registry and its
  compatibility tests. Core code emits semantic slots, never colors.
- Add configuration fields with serde defaults, round-trip/default/malformed
  tests, and startup or persistence tests. Older partial configurations must
  remain valid.
- Change public APIs only through the crate-root facades and update public API
  compile tests in the same change. Never expose third-party types.
- Model mutually exclusive states as enums containing all required state; do
  not add flag bags or parallel `Option` fields that allow impossible states.
- Inject clocks, environment observations, and host services at boundaries so
  builders and transition logic stay deterministic under test.

`docs/markdown-spec.md` is the authoritative description of supported
Markdown parsing. Parser or renderer changes must agree with it and add
byte-exact regressions for affected syntax, including ambiguous text, escaped
characters, entities, UTF-8, wrapping, tables, and repeated content where
relevant.

## Tests and verification

Tests live beside implementation code and in each crate's `tests/` directory.
The suite includes:

- unit tests for state transitions and pure layout helpers;
- core session integration tests;
- a Vim conformance suite with meta-tests that guard coverage;
- property tests for incremental highlighting and source/rendered mapping;
- compile-time public API and dependency-boundary tests;
- hand-rolled golden and terminal snapshot tests;
- document I/O and no-data-loss tests;
- bundled dictionary provenance, hash, and license checks; and
- debug and release performance gates.

Every behavior change needs focused automated coverage in the same change.
Snapshot success is not a replacement for invariant, transition, registry, or
API-boundary tests.

Before considering a change complete, run:

```console
make fmt-check
make check
```

`make check` already includes the full build and test suite. Run additional
targets such as `make bench` when the change affects a separately gated
performance path.

## Dependency and supply-chain policy

New dependencies are a last resort. Prefer a small, well-specified in-tree
implementation when it avoids a broad dependency and remains maintainable.
All direct dependencies are exact-version pinned, and the project permits
only the licenses listed in `deny.toml` and `docs/dependencies.md`. Copyleft
licenses, `git2`, and `libgit2-sys` are forbidden.

For every dependency addition or upgrade:

1. Decide whether the functionality can reasonably be implemented in-tree.
2. Check the upstream license, maintenance signal, and popularity baseline.
3. Review the changed vendored source and build scripts for networking or
   undocumented executables.
4. Record the decision and dependency role in `docs/dependencies.md`.
5. Update the exact manifest pin, `Cargo.lock`, and `vendor/` together.
6. Run `make deny`, `make audit`, and the full `make check` gate.

Warnings about vulnerabilities, unsoundness, licenses, or bundled data are
failures even when an underlying scanner exits successfully. Exceptions
require a documented dependency path, reachability and impact analysis,
compensating controls, an owner, a removal condition, and a narrowly scoped
advisory entry.

## Documentation and comments

Keep comments concise and explain current invariants or non-obvious behavior.
Do not encode task identifiers or historical requirement numbers in new code
comments; those labels become stale. Remove or correct dead comments when
nearby behavior changes.

Update user-facing behavior in `README.md`, dependency decisions in
`docs/dependencies.md`, and supported parsing behavior in
`docs/markdown-spec.md`. Keep developer instructions here, close to the code
and commands they describe.
