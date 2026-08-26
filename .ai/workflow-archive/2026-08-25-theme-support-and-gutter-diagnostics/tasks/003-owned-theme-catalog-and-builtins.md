# Task 003: Owned Theme Catalog and Built-ins

Delegation: main-only

## Goal

Replace static theme ownership/routing with one owned catalog and add the exact five attributed dark palettes without changing legacy theme behavior.

## Context

Runtime user themes require owned names and style tables. The repository forbids parallel registries, public leakage, and default palette drift, so ownership, builder lowering, built-in order, App lookup, resolution, cycling, and tests must move coherently.

## Scope

### In scope

- Owned `Theme`/palette tables and project-owned 18-role palette lowering for all `SemanticStyle`/`UiSlot` values and tiers.
- New document/body, gutter background, normal gutter text, and active gutter text slots/roles.
- One ordered `ThemeCatalog` and built-in declaration with exact names, appearance compatibility, order, fallbacks, palette seeds, origin, and attribution identity.
- Catppuccin Mocha, Dracula, Nord, Solarized Dark, and Tokyo Night built-ins with explicit ANSI mappings and shared monochrome behavior.
- App/startup/theme resolver/cycle/render lookup conversion to catalog ownership for built-ins.
- Exact preservation and regression tests for `default-dark`, `default-light`, and `accessible`.

### Out of scope

- Filesystem user-theme discovery/loading and warnings.
- Gutter renderer wiring or diagnostic marker state.
- Public API/core changes.

## Implementation requirements

- Keep implementation private to `oom-edit`; `App` owns one catalog and never falls back to a global static lookup.
- Preserve the static built-in declaration only as the sole materialization source, not a second runtime catalog.
- Use the pinned assets/mapping notes from Task 002; exact anchor values and attribution identity must be testable.
- Retain old exact default expected rows verbatim. Assert the new body/gutter rows separately; legacy bodies remain terminal-default.
- Enforce complete lowering, non-color carriers, color-free monochrome, explicit ANSI semantics, exact eight-theme uniqueness/order/modes, one fallback per appearance, and absence of excluded themes.
- Dark compatible order is `default-dark`, five new themes, `accessible`; light is `default-light`, `accessible` before future user entries.
- No new dependency, core import of terminal types, or public export.

## Acceptance criteria

- [ ] One owned catalog drives built-in lookup, mode compatibility, fallback, cycling, and App rendering/persistence; old free routing paths are removed.
- [ ] Registry names/order/modes are exact and unique, with five attributed entries and no Gruvbox/Rosé Pine.
- [ ] All built-ins resolve every closed semantic/UI slot at TrueColor, ANSI-16, and Monochrome and retain required modifiers/carriers.
- [ ] Every legacy expected style remains exact; their body remains terminal-default and only separately asserted gutter values are added.
- [ ] Each new bundled color theme has reviewed body/gutter/text/accent/status/diagnostic mappings, a body distinct from its gutter in TrueColor and ANSI-16, and curated contrast/anchor tests.
- [ ] Public API, core dependency direction, color locality, Cargo dependencies, lockfile, and vendor tree remain unchanged.

## Validation

- `make test`

## Dependencies

- Task 002

## Expected areas of change

- `crates/oom-edit/src/theme.rs` or private `theme/` submodules
- `crates/oom-edit/src/app.rs`
- `crates/oom-edit/src/lib.rs`
- Existing theme/App/screen test call sites

## Risks / notes

This is the central architectural refactor and is intentionally main-only. Avoid temporary duplicate lookup paths. Owned table design should stay narrow and obvious; no general style-map abstraction or user schema is needed in this task.
