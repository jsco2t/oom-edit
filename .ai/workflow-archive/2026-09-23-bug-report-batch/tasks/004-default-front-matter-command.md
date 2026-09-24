# Task 004: Add an idempotent default front-matter command

Delegation: main-only

## Goal

Provide a discoverable `Space m` command that atomically inserts the uniform
default YAML front-matter template only when the document has none.

## Context

App owns Space-prefix command grammar through the static `COMMANDS` registry,
while reusable editing mutations belong behind `EditorSession` and
`LiveDocument`. There is currently no insertion operation. The change crosses
the curated core API and registry projections and therefore requires their
compile/meta guards in the same task.

## Scope

### In scope

- A public `EditorSession` operation for conditional default-front-matter
  insertion.
- Exact deterministic template: `---\ntitle: \"\"\n---\n\n`.
- One atomic/undoable prepend through `LiveDocument`, with highlighting,
  parsed front matter, spell invalidation, dirty state, rendered invalidation,
  original body bytes, and logical cursor target kept coherent.
- Refusal/no-op behavior for any existing YAML or TOML front matter, including
  malformed/unterminated leading blocks recognized as present.
- A Normal-mode `Space m` registry row, App command dispatch, hints,
  which-key/palette projection, status feedback, and documentation.
- Core public-API and TUI registry-drift guard updates.

### Out of scope

- Configurable templates, TOML generation, dates, or additional metadata keys.
- Running the command from Insert, Select, Command, or modal overlays.
- Editing/replacing existing front matter.

## Implementation requirements

- Add failing core and TUI tests before implementation.
- All text mutation must go through `LiveDocument` and return/translate the
  normal typed mutation outcome. App must not rewrite document text.
- Existing body content must remain byte-for-byte identical after the prefix,
  including empty input, no-final-newline input, Unicode, and CRLF-normalized
  live text.
- Preserve the canonical logical target by shifting an existing cursor with
  the inserted prefix; select a deterministic valid body/front-matter target
  for an empty buffer.
- One invocation is one undo step. A refused second invocation changes neither
  text, dirty generation, cursor, nor undo history and emits useful feedback.
- Declare the binding exactly once in `command::COMMANDS`; dispatch and UI rows
  must remain projections.
- Update README, changelog, snapshots, and curated API guards.

## Acceptance criteria

- [ ] `Space m` is visible in registry-derived command discovery and dispatches only in rendered Normal mode.
- [ ] On a front-matter-free document it prepends exactly `---\ntitle: \"\"\n---\n\n` as one undoable mutation, preserves body bytes and logical cursor target, and refreshes every derived cache before returning.
- [ ] Empty, Unicode, and existing-body documents are covered, and undo/redo restore coherent text/front-matter state.
- [ ] A repeat invocation or any existing YAML/TOML front matter performs no mutation and reports that front matter already exists.
- [ ] The curated crate-root API and registry completeness/uniqueness/projection guards pass.
- [ ] README and changelog document the command and exact default template.

## Validation

- `cargo test -p oom-edit-core --offline --locked default_front_matter`
- `cargo test -p oom-edit-core --offline --locked public_facade_types_are_available_at_crate_root`
- `cargo test -p oom-edit --offline --locked front_matter_command`
- `cargo test -p oom-edit --offline --locked command::registry`
- `make check`

## Dependencies

- 003

## Expected areas of change

- `crates/oom-edit-core/src/session.rs`
- `crates/oom-edit-core/src/session/live_document.rs`
- `crates/oom-edit-core/src/vim.rs`
- `crates/oom-edit-core/tests/public_api.rs`
- `crates/oom-edit-core/tests/session_integration.rs`
- `crates/oom-edit/src/command/registry.rs`
- `crates/oom-edit/src/command/keymap.rs`
- `crates/oom-edit/src/app.rs`
- command UI snapshots
- `README.md`
- `CHANGELOG.md`

## Risks / notes

Prepending through the generic replacement primitive currently does not itself
move the cursor, so cursor shifting must be explicit inside the core mutation
path. Do not add a second text owner or expose a third-party/parser type in the
new public signature.
