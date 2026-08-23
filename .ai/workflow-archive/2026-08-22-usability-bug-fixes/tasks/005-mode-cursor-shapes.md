# Task 005: Add configurable mode cursor shapes

Delegation: main-only

## Goal

Use mode-appropriate terminal cursor shapes by default and provide one documented setting that disables shape changes in favor of a block cursor everywhere.

## Context

The current shell never emits cursor-shape commands. Insert and Command expose a real terminal cursor but inherit an arbitrary prior shape; rendered Normal and Select rely on semantic painted carriers. Cursor shapes are terminal presentation and must stay out of the reusable core.

## Scope

### In scope

- The optional `[editor].cursor_shapes` Boolean setting, defaulting to `true`.
- TUI-private mapping from the four public modes to Crossterm steady cursor styles.
- Real cursor positioning on visible rendered Normal/Select points while retaining semantic accessibility carriers.
- Shape application on mode changes and restoration to the terminal's user default on clean, error, panic, and fatal-signal exits.
- Configuration, terminal-command, rendered-position, mode-transition, README, and snapshot tests.

### Out of scope

- Adding cursor types to core or changing the four public modes.
- Arbitrary user-authored shape names or blink-rate configuration.
- Theme-driven cursor shapes.
- Cursor behavior inside non-editor overlay text fields beyond preserving current modal behavior.

## Implementation requirements

- Add `cursor_shapes: bool` to `EditorConfig` with a serde default of `true`; missing/partial configuration must remain backward compatible and serialization must round-trip.
- When enabled, use steady block for Normal, steady bar for Insert and Command, and steady underscore for Select. When disabled, use steady block for all modes.
- Keep Crossterm values in the TUI terminal/event boundary. App may expose only the current project-owned `Mode` needed by that boundary.
- Set a rendered cursor position only when its row and display column are visible after vertical and horizontal offsets. Keep the existing painted Normal cursor and Select carrier so shape support is not the sole signal.
- Avoid re-emitting an unchanged shape on every idle loop.
- Restore `DefaultUserShape` through the ordinary restore function and the audited Unix signal byte sequence.
- Document the setting and defaults in README using only current mode names.

## Acceptance criteria

- [ ] Missing configuration and `cursor_shapes = true` map Normal/Insert/Select/Command to block/bar/underscore/bar respectively.
- [ ] `cursor_shapes = false` maps every mode to block.
- [ ] Insert, Command, rendered Normal, and rendered Select place the real cursor at their active visible position without removing semantic carriers.
- [ ] Mode changes update the terminal shape once, while unchanged idle iterations do not emit redundant shape changes.
- [ ] Clean/error/panic/signal restoration emits the user-default cursor-shape reset in addition to existing terminal cleanup.
- [ ] Config default, missing section, explicit false, round-trip, cursor mapping, emitted command, and visible rendered-position tests pass.
- [ ] README documents the exact setting and default behavior.

## Validation

- `make test`

## Dependencies

- Task 004

## Expected areas of change

- `crates/oom-edit/src/config.rs`
- `crates/oom-edit/src/lib.rs`
- `crates/oom-edit/src/app.rs`
- `crates/oom-edit/src/event.rs`
- `crates/oom-edit/src/terminal_guard.rs`
- `crates/oom-edit/src/screens/rendered.rs`
- `crates/oom-edit/src/screens/editor.rs`
- `README.md`
- `crates/oom-edit/src/snapshot_tests.rs`

## Risks / notes

Rendered cursor positioning must account for gutter, `rendered_top`, horizontal offset, zero-size areas, and wide glyphs. The fatal-signal handler can only use async-signal-safe syscalls, so its reset remains a literal audited escape sequence rather than calling Crossterm.
