# Task 003: Configurable alternating table rows

Delegation: main-only

## Goal

Add an enabled-by-default user setting that gives alternating rendered table
body rows a subtle, accessible surface in Normal mode, including every wrapped
continuation line.

## Context

Core currently discards table logical-row identity after source mapping and the
TUI sees every table line as ordinary document content. The feature crosses the
public renderer-neutral role, App configuration/startup, theme registry, and
render composition boundaries.

## Scope

### In scope

- Renderer-neutral alternate table-row identity in core.
- `[editor].alternating_table_rows` config with default `true`.
- App/startup/render plumbing.
- Built-in theme slot styling for every capability tier.
- Normal-mode composition behavior, docs, snapshots, and API/meta-tests.

### Out of scope

- User-authored arbitrary row colors or theme-file schema changes.
- Runtime commands or persistence toggles for this setting.
- Header styling, border redesign, or striping in Select/Insert/Command.

## Implementation requirements

- Preserve a dedicated renderer-neutral role on all visual lines belonging to
  the second, fourth, and subsequent even-numbered body rows.
- Do not mark header, border/separator, first/odd body, or non-table lines as
  alternate rows.
- Add `alternating_table_rows: bool` to `EditorConfig`, defaulting to `true`
  for missing files and partial `[editor]` sections, with serialization and
  explicit `false` round-trip coverage.
- Thread the setting through production startup and test constructors into the
  rendered screen without introducing configuration into core.
- Apply the surface only in `Mode::Normal` when enabled.
- Add a `UiSlot` whose color-capable variants use a subtle theme-appropriate
  background and whose every tier carries a visible modifier. Preserve semantic
  foregrounds and higher-priority diagnostics, search, selection, cursor-line,
  and cursor-cell carriers.
- Update every exhaustive theme registry/meta-test and the curated core public
  API guard for the new rendered role.
- Document the setting and default in the README sample config.

## Acceptance criteria

- [ ] Default and explicit `true` config stripe only even-numbered body rows in rendered Normal mode, including all wrapped continuation lines.
- [ ] Explicit `false`, Select mode, headers, borders, odd body rows, and non-table content receive no alternate-row surface.
- [ ] Missing/partial config defaults to enabled and both boolean values serialize and deserialize without disturbing other config fields.
- [ ] Every built-in theme and TrueColor/Color16/Monochrome tier defines a non-color modifier carrier; color tiers use a subtle alternate background while retaining semantic foregrounds.
- [ ] Diagnostics, search, selection, cursor-line, and Normal-cursor composition remain visible and deterministic over an alternate row.
- [ ] README, golden/snapshot fixtures, constructor call sites, role tests, theme completeness tests, and public API guards are updated.

## Validation

- `make test`

## Dependencies

- 001
- 002

## Expected areas of change

- `crates/oom-edit-core/src/style.rs`
- `crates/oom-edit-core/src/rendered/mod.rs`
- `crates/oom-edit-core/src/rendered/table.rs`
- `crates/oom-edit-core/src/rendered/tests/blocks.rs`
- `crates/oom-edit-core/tests/public_api.rs`
- `crates/oom-edit/src/config.rs`
- `crates/oom-edit/src/lib.rs`
- `crates/oom-edit/src/app.rs`
- `crates/oom-edit/src/screens/rendered.rs`
- `crates/oom-edit/src/theme.rs`
- `crates/oom-edit/src/snapshot_tests.rs`
- `crates/oom-edit/tests/snapshots/rendered_table.txt`
- `README.md`

## Risks / notes

The alternate surface is a background layer, not an inline semantic style.
Composition order must preserve more specific carriers. The first body row is
ordinary so a one-row table is unchanged.
