# Task 005: Make continuation markers visible

Delegation: main-only

## Goal

Make the `↳` width-continuation marker visibly distinct from the gutter
background in the default theme and guard the composed visibility contract
across all theme tiers.

## Context

Task 003 correctly classifies continuation rows and writes the glyph into the
number field, but default-dark TrueColor assigns the marker and gutter surface
the same `#2f343e` color. Existing tests verify glyph placement and modifiers
without testing final foreground/background contrast, allowing the screenshot's
invisible marker regression.

## Scope

### In scope

- The `UiSlot::GutterContinuation` color derivation for built-in, legacy, and
  test-loaded custom themes.
- Fully composed default-dark gutter-cell coverage.
- Cross-theme and cross-tier visibility, accessibility, and diagnostic
  coexistence tests.

### Out of scope

- Core layout, continuation classification, Markdown rendering, navigation,
  line numbering, status behavior, or source provenance.
- Changing the glyph, gutter geometry, custom-theme schema, or unrelated theme
  roles.

## Implementation requirements

- Add a regression that fails because default-dark TrueColor currently renders
  the `↳` foreground equal to the inherited gutter background.
- Keep continuation styling behind `UiSlot::GutterContinuation`; do not assign
  colors directly in the screen renderer.
- Base the colored continuation slot on an existing visible gutter foreground
  and retain `DIM | ITALIC` as the quieter and non-color signal.
- For every colored test theme/tier, verify the continuation foreground is not
  the gutter background. For monochrome and accessible modes, verify the
  existing color-free modifier contract remains intact.
- Inspect the final ratatui buffer cell to cover style patching and verify a
  simultaneous diagnostic glyph remains in the adjacent leading cell.
- Do not modify dependencies, public APIs, configuration, core code, or build
  workflows.

## Acceptance criteria

- [ ] Default-dark TrueColor displays `↳` with effective foreground distinct
      from the gutter background.
- [ ] Colored built-in and test-loaded custom theme tiers cannot assign the
      continuation marker the gutter background color.
- [ ] The marker remains dim/italic and distinct from ordinary numbers, while
      monochrome and accessible tiers remain color-free.
- [ ] A diagnostic glyph and `↳` render together in their established cells
      without changing gutter width or content alignment.
- [ ] Focused gutter/theme suites and the complete quality gate pass without
      changes outside the approved TUI presentation scope.

## Validation

- `cargo test -p oom-edit --offline --locked gutter_continuation`
- `cargo test -p oom-edit --offline --locked rendered_gutter`
- `cargo test -p oom-edit --offline --locked theme::tests`
- `cargo test -p oom-edit --offline --locked snapshot_tests`
- `make check`

## Dependencies

- 003

## Expected areas of change

- `crates/oom-edit/src/theme.rs`
- `crates/oom-edit/src/screens/editor.rs`
- `crates/oom-edit/src/screens/rendered.rs`

## Risks / notes

The marker must be visibly distinct after the gutter background is composed,
not merely different from the standalone ordinary-number style. Preserve the
leading diagnostic cell and trailing content gap exactly.
