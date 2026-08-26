# Task 004: Custom Theme Loading and Startup Integration

Delegation: main-only

## Goal

Load strict user-authored palettes deterministically before terminal setup and integrate them into the same catalog, selection, cycling, rendering, and persistence paths as built-ins.

## Context

The owned catalog from Task 003 provides the runtime seam. This task defines the complete file contract and fail-soft startup behavior without adding a second registry, hot reload, or process-global test state.

## Scope

### In scope

- Strict serde/TOML DTOs for appearance, all 18 required RGB roles, and optional closed ANSI overrides.
- Exact `#RRGGBB` and ANSI parsing/lowering into application-owned styles.
- Direct-child lexical discovery beneath the existing config root's `themes/` child.
- Lowercase kebab-case filename identity, reserved built-ins, duplicates, bounded reads, UTF-8/TOML/schema validation, typed warnings, and per-file isolation.
- Startup load report/warning emission before terminal setup.
- Catalog-aware CLI/environment/config selection, matching fallback, cycling, App ownership, and active-slot persistence for custom dark/light entries.
- Real temporary-directory and private startup/App integration tests.

### Out of scope

- Inheritance, includes, arbitrary scopes, multiple roots, file watching, hot reload, runtime network, theme listing, or automatic custom contrast rejection.
- Gutter diagnostic markers.

## Implementation requirements

- Read at most 65,537 bytes before decoding/parsing; accept exactly 65,536 bytes and reject anything larger with size error precedence.
- Consider only direct `.toml` children, sort by filename, and return exactly one stable path-specific warning per rejected file in that order. Missing directory is silent.
- Reject missing roles, unknown fields/sections, invalid/alpha/short colors, invalid appearance/ANSI names, invalid identifiers, reserved names, and duplicates independently.
- Built-ins stay first; compatible valid user themes append in lexical name order. Existing catalog instances do not hot reload.
- A selected unavailable/invalid/opposite-appearance name falls back to the matching default for every source and never implicitly to `accessible`; an unrelated invalid file does not displace a valid selection.
- Capture config paths into owned values; do not mutate `HOME`/`XDG_CONFIG_HOME` in parallel tests.
- User input cannot alter modifiers, glyphs, monochrome colors, or required carriers.

## Acceptance criteria

- [ ] Complete dark/light fixtures parse and lower every slot/tier; all strict invalid-contract cases and exact size boundaries fail with stable categories.
- [ ] Real filesystem tests prove direct-child filtering, lexical order, invalid sibling isolation, missing-directory silence, reserved names, invalid UTF-8, and startup-only immutability.
- [ ] Loader warnings are exactly one per rejected file, ordered, path-specific, and emitted before App/terminal construction.
- [ ] CLI, environment, dark/light config, matching fallback, cycle order, and persistence all use the loaded catalog and preserve unrelated/opposite config values.
- [ ] A file-loaded theme reaches representative source/rendered/body/gutter/semantic/status cells through owned App/catalog lifetime.
- [ ] Help/docs may be prepared for Task 008, but no unsupported feature or new dependency is introduced.

## Validation

- `make test`

## Dependencies

- Task 003

## Expected areas of change

- `crates/oom-edit/src/theme.rs` or private loader/catalog modules
- `crates/oom-edit/src/config.rs`
- `crates/oom-edit/src/lib.rs`
- `crates/oom-edit/src/app.rs`
- Colocated theme/startup/App tests

## Risks / notes

Startup parsing must remain bounded and fail-soft. Warning assertions should use owned warning kind/path fields rather than unstable parser prose. A narrow injected read seam is acceptable for deterministic read-error tests; do not build a general virtual filesystem.
