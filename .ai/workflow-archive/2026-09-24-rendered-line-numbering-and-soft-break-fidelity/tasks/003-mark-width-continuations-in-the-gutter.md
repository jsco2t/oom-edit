# Task 003: Mark width continuations in the gutter

Delegation: main-only

## Goal

Display a subtle `↳` gutter marker on rows created only by width wrapping while
leaving physical-line starts numbered and synthetic/padding rows blank.

## Context

The gutter currently converts every `None` entry in
`RenderedLayout.line_numbers` to spaces. After task 002, exact repeated source
ranges distinguish true width continuations from new physical-line starts, but
the TUI must project that distinction accessibly without introducing another
source-position model.

## Scope

### In scope

- Absolute and relative rendered gutters.
- `↳` placement in the existing number field without changing gutter width.
- A dedicated quieter continuation theme slot across all built-in/custom
  palette paths and capability tiers.
- Diagnostic-marker coexistence, current-row behavior, narrow/multidigit
  gutters, accessibility, and snapshots.
- Width continuations in prose, lists, quotes, metadata, and tables where
  applicable.

### Out of scope

- Insert-mode gutter behavior.
- A user-configurable marker glyph or style.
- Marking synthetic separators, physical blank rows, or viewport padding.

## Implementation requirements

- Add failing TUI tests before product changes and confirm width continuations
  are blank on the baseline.
- Classify a continuation from core-owned layout identity: it must be an
  unnumbered content row repeating the immediately preceding physical source
  range. Do not classify by displayed text or color.
- Render the one-cell `↳` glyph in the number field, preserving the leading
  diagnostic marker cell and trailing content gap.
- Add a `UiSlot` for continuation markers. Colored tiers must be visually
  quieter than ordinary gutter numbers; monochrome must retain the glyph and a
  non-color modifier.
- Extend theme completeness, uniqueness, custom-theme lowering, hardcoded-color
  guards, and snapshot tests as required by the registry conventions.
- Verify that horizontal scrolling never moves the gutter or marker.

## Acceptance criteria

- [ ] Every width-created continuation row displays exactly one `↳` marker in
      both absolute and relative rendered gutters.
- [ ] First physical rows show their number; physical blank rows show their
      number; synthetic and padding rows remain blank.
- [ ] The marker does not replace or shift diagnostic markers and does not
      change gutter/content geometry at narrow or multidigit widths.
- [ ] The marker is quieter than ordinary numbers in color themes and remains
      a glyph-plus-modifier signal in monochrome/accessibility mode.
- [ ] Prose, list/quote, metadata/table wrapping, cursor rows, and horizontal
      scrolling classify continuation rows consistently.
- [ ] Updated snapshots make numbered physical rows, continuation markers, and
      blank synthetic rows visually distinct.

## Validation

- `cargo test -p oom-edit --offline --locked gutter_continuation`
- `cargo test -p oom-edit --offline --locked rendered_gutter`
- `cargo test -p oom-edit --offline --locked theme::tests`
- `cargo test -p oom-edit --offline --locked snapshot_tests`
- `make check`

## Dependencies

- 002

## Expected areas of change

- `crates/oom-edit/src/screens/editor.rs`
- `crates/oom-edit/src/screens/rendered.rs`
- `crates/oom-edit/src/widgets/status_bar.rs`
- `crates/oom-edit/src/theme.rs`
- `crates/oom-edit/src/snapshot_tests.rs`
- `crates/oom-edit/tests/snapshots/`

## Risks / notes

The leading gutter cell is reserved for diagnostic severity markers. The
continuation glyph belongs only in the number field and must be verified as one
terminal display cell across supported layouts.
