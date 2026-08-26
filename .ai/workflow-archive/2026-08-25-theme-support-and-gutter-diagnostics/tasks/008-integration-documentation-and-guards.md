# Task 008: Integration, Documentation, and Repository Guards

Delegation: main-only

## Goal

Close the user-facing and architectural integration surface and make every catalog, attribution, startup, rendering, and repository invariant drift-resistant.

## Context

The feature paths now exist. This task verifies them as one private boundary chain, updates discoverability, and strengthens compliance/API/dependency guards before candidate performance measurement.

## Scope

### In scope

- End-to-end private tests for file-loaded themes through startup resolution, App ownership, source/rendered cells, cycling, and config persistence.
- End-to-end diagnostic publication through per-tab marker state and both gutters, including clearing/cancellation.
- CLI help wording for built-in or user theme names and README documentation of exact built-ins, directory, schema, selection/fallback/cycling, startup warnings, and exclusions.
- Final review of the performance documentation and README link so source/rendered NFR definitions and make-owned reproduction commands remain discoverable after feature integration.
- One-to-one built-in attribution registry/provenance/raw-license/notice/`--licenses` guards.
- Existing public API, dependency direction, no-network/no-plugin, no-hard-coded-color, exact registry, and make-target meta-tests updated for the final structure.
- Cleanup of obsolete static lookup helpers, duplicate lists, dead comments, and accidental scope expansion.

### Out of scope

- New user features, theme-list command, additional palettes, live reload, or performance rebaselining.
- Candidate evidence execution, owned by Task 009.

## Implementation requirements

- Keep README/schema examples synchronized with the strict parser and exact built-in names/order.
- Keep `docs/performance.md`, benchmark labels/constants, TUI case names, fixture version, Makefile help, and retained evidence terminology synchronized; guards must fail on stale source/rendered target names or values.
- Help must not claim a closed three-name list.
- `--licenses` remains byte-for-byte canonical notice content and pre-terminal.
- Cross-surface attribution tests derive from the single registry identity and fail on missing/duplicate entries without creating a second user-facing order list.
- Public facade remains exactly three exports and core remains free of ratatui/crossterm/theme/network dependencies.
- Color locality guards permit fixed glyph literals outside theme code but no hard-coded marker/gutter colors.
- Review all new source comments for concise behavior-based wording and remove obsolete static-theme commentary.

## Acceptance criteria

- [ ] A file-loaded custom dark/light theme crosses loader → catalog → resolver → App → both renderers and persists only the active config slot.
- [ ] Invalid selected and unrelated invalid files prove warning order and matching fallback before terminal construction.
- [ ] Real bounded diagnostics cross provider → pending build → completed snapshot → both gutters and invalidate correctly on edit/tab/lifecycle transitions.
- [ ] Help and README accurately document the complete supported contract and omit excluded/unsupported behavior.
- [ ] Registry, provenance, licenses, canonical notices, and CLI output have exact one-to-one compliance coverage; all existing SCOWL guarantees remain.
- [ ] Compile/source guards prove the public facade, dependency direction, color locality, static/no-network posture, and Makefile discoverability remain intact.
- [ ] No duplicate routing/list, dead implementation, unapproved dependency, lockfile/vendor change, or stale comment remains.

## Validation

- `make data-license-check`
- `make doc`
- `make test`

## Dependencies

- Task 007

## Expected areas of change

- `README.md`
- `crates/oom-edit/src/args.rs`
- `crates/oom-edit/src/lib.rs`
- `crates/oom-edit/src/theme.rs` and private submodules
- `crates/oom-edit/src/app.rs`
- `crates/oom-edit/tests/public_api.rs`
- `crates/oom-edit/tests/dependency_hygiene.rs`
- Bundled-data/license tests and script

## Risks / notes

This task is intentionally integration-focused and must not redesign approved behavior. Documentation is authoritative user contract; examples must be copied from tested complete fixtures rather than a divergent hand-maintained schema.
