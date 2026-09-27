# Task 005: Public theme catalog, styles and notices

Delegation: main-only

## Goal

Expose the existing theme catalog and complete owned styling/notices contracts without changing visual design.

## Context

`ThemeCatalog` and ratatui styles are private, and `--licenses` notices are currently binary-only. The pane and host must resolve the same themes and share semantic/UI styles through owned types.

## Scope

### In scope

FR-070–075 and FR-102: catalog enumeration/load/resolve/cycle, explicit selection inputs and environment helper, owned styles, `set_theme`, runtime-safe `apply_config`, third-party notices export.

### Out of scope

New visual themes, theme schema changes or direct terminal color types in public APIs.

## Implementation requirements

- Add tests before extraction for built-in order, user-theme warnings, exact environment precedence, all semantic/UI slots across every built-in/tier, monochrome color absence and non-color carriers.
- Public theme selection accepts explicit name/appearance/tier and reads no environment. Separate helper reproduces the current `OOM_EDIT_THEME`, `NO_COLOR`, `TERM`, `COLORTERM`, `COLORFGBG` ladder.
- Expose owned style/color/modifier types shared with Task 007's frame. `set_theme` restyles next frame, emits ThemeChanged and never calls the theme sink. `apply_config` reports precisely which settings apply now and which require a new pane.
- Export notices bytes identical to current `--licenses`; keep full data/theme copyright notices. Extend API/docs/coverage guards.

## Acceptance criteria

- [ ] All themes/tier/slots resolve and existing palette snapshots remain unchanged.
- [ ] Environment helper matches baseline while explicit pane inputs are environment-independent.
- [ ] Monochrome styles have no color-only state and `set_theme`/`apply_config` report exact effects.
- [ ] Notices export equals CLI output byte-for-byte and data-license checks pass.

## Validation

`make test`

`make data-license-check`

`make doc`

## Dependencies

004

## Expected areas of change

`crates/oom-edit/src/theme.rs`, `lib.rs`, `args.rs`, `main.rs`, theme/API/notices tests, `THIRD-PARTY-NOTICES.md` only if needed for completeness.

## Risks / notes

Style conversion must retain default versus indexed versus RGB and all modifiers; Task 007 will verify cell-level frame fidelity.
