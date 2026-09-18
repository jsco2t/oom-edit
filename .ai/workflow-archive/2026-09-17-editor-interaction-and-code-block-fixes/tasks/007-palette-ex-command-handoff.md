# Task 007: Palette ex-command handoff

Delegation: main-only

## Goal

Make Enter on a concrete ex-command help row open an editable, prefilled core Command prompt without executing the command yet.

## Context

Palette rows currently expose only App commands as actions. All core ex entries are rendered as references; Enter closes the palette with “reference entry.” Core has no prefilled Command-mode facade operation.

## Scope

### In scope

- Explicit ex prefill metadata for concrete core ex rows in the existing registry/reference sources.
- A typed palette selection result and App handoff to core Command mode.
- A terminal-neutral core method that enters Command mode with a safe prefill.
- Tests for `:wq`, argument templates, editing, Escape, history, Normal/Select contexts, and read-only/disabled rows.

### Out of scope

- Direct execution on the first palette Enter.
- Mouse selection, command completion, or new ex command grammar.

## Implementation requirements

- Build App commands from `command::COMMANDS`, and attach ex prefill metadata to the owning `BindingRole::CoreEx` or `VIM_REFERENCE` entry. Do not parse display strings as command input at runtime or add a parallel ex dispatcher in App.
- Prefills are unprefixed, single-line command text. Concrete rows use the exact command; argument templates use only literal prefixes and omit braces/placeholders. A row combining alternatives stays read-only unless split into separate concrete rows.
- The core method validates its input, enters Command mode from Normal or Select without editing document text, and returns typed mode effects. App closes the palette and forwards the result through its normal effect path.
- A first Enter on an ex row opens the prompt without executing or recording history. The next Enter submits via the existing core ex path; Escape cancels without history. Read-only and disabled rows remain open and inert on Enter.
- Update registry/reference completeness tests, public API guards, and snapshots affected by action markers or hints.

## Acceptance criteria

- [ ] Enter on `:wq` closes the palette and opens Command mode with `wq` prefilled; the document is unchanged and no lifecycle action has run until the next Enter.
- [ ] The next Enter executes the existing core command and records history; Escape cancels without execution or history.
- [ ] `:e {path}` and `:tabnew {path}` prefill editable prefixes without placeholder text, and a completed path follows the existing safe lifecycle path.
- [ ] Enabled App commands retain immediate Enter behavior; read-only, combined-command, and disabled rows are visibly distinct and inert on Enter.
- [ ] Behavior works when the palette opens from Normal and every Select shape; modal input remains exclusive.

## Validation

- `make test`

## Dependencies

- 006

## Expected areas of change

- `crates/oom-edit/src/command/registry.rs`
- `crates/oom-edit/src/overlay/palette.rs`
- `crates/oom-edit/src/overlay/mod.rs`
- `crates/oom-edit/src/app.rs`
- `crates/oom-edit-core/src/session.rs`
- `crates/oom-edit-core/tests/public_api.rs`
- Core/App tests and palette snapshots

## Risks / notes

Opening a prefilled prompt must use the core facade even for ex commands described by the TUI registry. Never treat an ex row as an executable App command, and never turn a composite display string into command input.
