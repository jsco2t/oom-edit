# Task 001: Severity indicators

Delegation: main-only

## Goal

Show a compact, theme-aware diagnostic dot in the gutter and a matching severity color on the boxed spelling status symbol.

## Context

The gutter already has severity roles, but currently paints letters. The status bar paints the spelling symbol and ruler as one dimmed span.

## Scope

### In scope

- Gutter marker glyph/style and status spelling symbol style.
- Highest-severity selection for the status spelling summary, with a neutral zero-issue state.
- Focused rendering tests across warning, error, and supported theme tiers; affected snapshots.

### Out of scope

- Gutter width changes, mouse behavior, and key routing.

## Implementation requirements

- Use `●` as the one-cell glyph for diagnostic markers; retain explicit mappings for all existing diagnostic severities.
- Reuse active-theme severity slots for gutter and status; do not hard-code RGB colors.
- Give error a visible modifier distinction in addition to color. Preserve background and monochrome behavior.
- Build status symbol and count as separately styled spans. Derive status severity from the session's current diagnostics without retaining a second diagnostics cache.
- Keep glyph/count width calculations correct for all supported terminal widths.

## Acceptance criteria

- [ ] Warning and error gutter markers show `●` with the active-theme warning/error role and distinct non-color styling.
- [ ] The boxed spelling glyph uses the same severity role as the gutter's highest displayed spelling severity; zero issues are neutral.
- [ ] Count, alignment, theme-tier rendering, and relevant snapshots are covered by tests.

## Validation

- `make test`

## Dependencies

None

## Expected areas of change

- `crates/oom-edit/src/gutter.rs`
- `crates/oom-edit/src/screens/editor.rs`
- `crates/oom-edit/src/widgets/status_bar.rs`
- `crates/oom-edit/src/snapshot_tests.rs`
- `crates/oom-edit/tests/snapshots/`

## Risks / notes

The marker and boxed status symbol must remain discernible in monochrome, where color cannot communicate severity.
