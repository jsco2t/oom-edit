# Task 001: Clarify command discovery surfaces

Delegation: worker-eligible

## Goal

Make the command palette readable and consistently aligned, and make the bottom key help compact and unambiguous at dynamic widths.

## Context

The command registry already drives dispatch, the palette, which-key, and quick hints, but the quick bar reuses long descriptions and repeats every Space prefix. The palette separately formats command and reference rows with incompatible hard-coded widths and insufficiently explicit styles. This task fixes both projections without creating another registry.

## Scope

### In scope

- Registry-owned compact quick-hint labels and standalone `/` search and `:` Command-mode reference rows.
- Grouping eligible Space-prefix hints into one `Space [...]` chord while preserving registry priority and whole-group width fitting.
- One pure, display-width-aware palette row layout for command and reference rows.
- Dedicated palette UI theme slots for surface/text, secondary or disabled text, and selected rows across every theme/capability tier.
- Palette/hint unit tests, registry drift tests, theme completeness/accessibility tests, and 40/80-column snapshots.

### Out of scope

- Changing command dispatch or introducing executable TUI commands.
- Link-index behavior, rendered layout, horizontal scrolling, terminal cursor commands, or event-loop presentation.
- Redesigning other overlays.

## Implementation requirements

- Keep `command::COMMANDS` as the only binding/ordering source. Compact labels must be metadata on those rows, not a parallel hint list.
- Render Help with one concise meaning; do not emit `help / command palette` or any text that suggests `/` opens the palette.
- Render `/` as standalone search and `:` as standalone Command mode. Space continuations must visibly share a single prefix.
- Fit only complete quick-hint units; never slice a key chord or its description. Use Ratatui display widths rather than byte length.
- Use one column calculation for every palette row variant. Truncate or omit lower-priority columns predictably at narrow widths without changing row identity/filtering/selection.
- Selected, disabled, and reference states must retain a non-color carrier and readable foreground/background combinations in default-dark, default-light, 16-color, and monochrome tiers.
- Preserve modal exclusivity, fuzzy filtering, command execution, registry completeness, and palette viewport-follow behavior.

## Acceptance criteria

- [ ] Normal-mode quick help contains a single grouped Space chord and standalone `/=search` and `:=command` meanings, with no ambiguous slash description.
- [ ] Width fitting retains or drops complete hint units only and is covered at widths below, at, and above each boundary.
- [ ] Executable and reference palette rows begin each visible column at the same display-cell offsets.
- [ ] Enabled, selected, disabled, and reference palette rows are distinguishable and readable across every built-in theme tier without color-only signaling.
- [ ] Palette filtering, navigation, executable-row selection, reference non-executability, and registry drift guards still pass.
- [ ] Updated wide and 40x12 snapshots demonstrate stable alignment and sensible narrow degradation.

## Validation

- `make test`

## Dependencies

None

## Expected areas of change

- `crates/oom-edit/src/command/registry.rs`
- `crates/oom-edit/src/command/keymap.rs`
- `crates/oom-edit/src/widgets/hint_bar.rs`
- `crates/oom-edit/src/widgets/which_key.rs`
- `crates/oom-edit/src/overlay/palette.rs`
- `crates/oom-edit/src/theme.rs`
- `crates/oom-edit/src/snapshot_tests.rs`
- `crates/oom-edit/tests/snapshots/`

## Risks / notes

The same registry descriptions currently serve several projections. Prefer explicit presentation metadata on one row over parsing rendered key strings or hard-coding a second list. Unicode display width and monochrome modifiers need direct assertions because plain-text snapshots do not capture style.
