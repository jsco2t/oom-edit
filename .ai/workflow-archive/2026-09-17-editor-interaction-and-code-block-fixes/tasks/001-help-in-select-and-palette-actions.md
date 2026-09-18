# Task 001: Help in Select and palette actions

Delegation: main-only

## Goal

Open help with plain `?` in every rendered Select shape and make palette action selection honest for the current mode.

## Context

The registry limits the direct help alias to Normal, and the palette starts on and can navigate to non-executable rows.

## Scope

### In scope

- Registry alias and projections for Normal and Select.
- Palette focus, navigation, and actionable highlighting based on enabled executable rows.
- Tests of direct key routing, mode-specific rows, and Enter execution.

### Out of scope

- Ex commands, file lifecycle, rendering code blocks, and command history.

## Implementation requirements

- Keep `command::COMMANDS` authoritative for binding and context. Do not add a parallel help list.
- Preserve metadata-only Vim reference rows as non-executable and visibly distinct.
- Ensure the current mode is captured when opening the palette; no background key routing while it is open.
- Check character, line, and block Select, including direct `?` with terminal Shift modifier.

## Acceptance criteria

- [ ] Plain `?` opens one palette in Normal and each Select shape without entering backward search or changing selection.
- [ ] Palette initial focus and Up/Down highlight only enabled executable commands; Enter executes the highlighted command in the captured mode.
- [ ] Disabled commands and reference rows do not appear actionable, and help key labels match direct bindings in both modes.
- [ ] Regression tests cover registry, palette, and App transitions.

## Validation

- `make test`

## Dependencies

None

## Expected areas of change

- `crates/oom-edit/src/command/registry.rs`
- `crates/oom-edit/src/command/keymap.rs`
- `crates/oom-edit/src/overlay/palette.rs`
- `crates/oom-edit/src/app.rs`
- `crates/oom-edit/src/snapshot_tests.rs` and snapshots if visible output changes

## Risks / notes

Do not turn core Vim reference metadata into App commands. Keep filtering useful even when no executable command matches.
