# Task 012: Standalone and embedded fidelity acceptance

Delegation: main-only

## Goal

Verify the complete incremental implementation preserves all existing editor
fidelity, safety and public hosting behavior, and document its measured limits.

## Context

Both the standalone binary and embedded pane use the same private `App` and
core session, but a green microbenchmark alone does not prove that edits,
navigation, lifecycle and source provenance remain correct.

## Scope

### In scope

- Expand direct `EditorSession`, public `EditorPane`, standalone and split-pane
  tests with identical interaction vectors across modes, viewport widths,
  local/global edits, undo, save, reload, resize, tab switching and external
  change handling.
- Run full-build differential/property suites, modal conformance/meta-tests,
  API/privacy/dependency-hygiene guards and the entire repository quality
  gate. Remove any unused test-only prototype path from task 007 while keeping
  the independent full-build oracle and instrumentation.
- Update `docs/performance.md` with the interaction-first measurement protocol,
  exact fixture thresholds, p99/worst results, memory, distinct first-ever
  source-to-rendered and warmed-return distributions, cold-first-frame limits
  and global-edit caveat.

### Out of scope

- Modifying the integrating `oom` project, adding new modes or publishing a
  release/tag.

## Implementation requirements

- Compare actual owned cells, semantic modifiers, source atoms and cursor/
  selection behavior; compilation-only parity is insufficient.
- Preserve four public modes, single input routing, no-data-loss save and
  external-version protections, renderer-neutral core and curated crate-root
  facades. Update exact compile-time guards if any public API changes.
- Review the cumulative diff against the baseline for dead reference paths,
  accidental second mutable state, unbounded caches and parser/renderer drift.
- Do not declare completion with any failed gate or unexplained required
  fidelity/performance deviation.

## Acceptance criteria

- [ ] Direct-core, standalone, embedded pane and split-pane tests exercise the
  same edits and produce equivalent current-text frames and operations.
- [ ] Exact differential, highlighting/mapping property, mode/conformance,
  lifecycle, API/privacy and dependency-hygiene suites pass.
- [ ] The full `make check`, existing `make bench`, revised
  `make bench-realistic` and `make bench-interactions` gates pass without
  warnings, with documented fixture identity and measured distributions.
- [ ] Performance documentation accurately distinguishes ordinary local
  interactions, the one-time source-first rendered projection, warmed
  mode returns, globally invalidating edits, cold open and reload, and
  contains no universal latency claim unsupported by measurements.

## Validation

- `make test-incremental`
- `make test-public-api`
- `make test-embedding-example`
- `make test-standalone-host`
- `make bench-interactions`
- `make bench-realistic`
- `make bench`

## Dependencies

011

## Expected areas of change

`crates/oom-edit/tests/`, `crates/oom-edit-core/tests/`, `examples/`,
`docs/performance.md`, API/dependency-hygiene tests if boundaries change

## Risks / notes

Host parity must be tested through public owned frames and real routed input,
not inferred from a common implementation module. Keep the full-build oracle
independent from production caches.
