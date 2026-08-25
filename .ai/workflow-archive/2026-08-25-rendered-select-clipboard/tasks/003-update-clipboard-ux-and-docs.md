# Task 003: Update clipboard UX and documentation

Delegation: worker-eligible

## Goal

Align the TUI feedback, registry-derived help, integration tests, and user documentation with default rendered Select clipboard publication and OSC 52's best-effort contract.

## Context

The App already routes typed clipboard effects to an injected sink and reports success or failure. Its test helper currently uses an explicit system register, the success message claims an unacknowledged clipboard update, and the README does not explain clipboard behavior. The static command registry is the source of truth for Select hints and palette metadata.

## Scope

### In scope

- App clipboard feedback wording and injected-sink tests.
- The Select-yank command registry description and exact registry guard.
- README clipboard usage/limitations and the Unreleased changelog entry.
- Cleanup of directly touched stale requirement-number comments.

### Out of scope

- Selection/register mechanics and Base64 encoding.
- New UI state, terminal probing, configuration, live clipboard reads, new commands, or new effects.
- External notebook edits or manual cross-platform environment provisioning.

## Implementation requirements

- Change the App clipboard test helper to use plain rendered Select `y`, without a register prefix.
- Preserve visible warning feedback for sink errors.
- Use success wording that confirms text was sent/emitted to the clipboard path without claiming the terminal acknowledged or applied it; wording must remain accurate for non-yank clipboard effects such as copyable link-index rows.
- Update all exact App assertions for the chosen wording.
- Update the single `COMMANDS` registry row for Select yank so help, hints, and palette projections describe yank plus clipboard publication; update its exact drift-prevention test in the same change.
- Document plain rendered Select `y`, explicit `"+y`/`"*y`, terminal-native paste in Insert, in-process `"+p`, OSC 52 terminal/tmux configuration, best-effort acknowledgement, the 100 KiB limit, and visible errors.
- Add a concise Unreleased changelog entry.
- Do not add a duplicate help list or bypass the registry.

## Acceptance criteria

- [ ] App success and failure tests exercise plain rendered Select `y` through the injected sink; success wording does not promise terminal acknowledgement and failure remains a visible warning.
- [ ] Copyable link-index feedback remains correct with the same generic clipboard emission wording.
- [ ] The registry-derived Select-yank description accurately advertises clipboard publication and its exact metadata test passes.
- [ ] README documentation covers output, input, explicit register alternatives, cached `"+p`, OSC 52/tmux support, best-effort semantics, size limits, and errors without promising universal platform behavior.
- [ ] The Unreleased changelog records the behavior change.
- [ ] No parallel registry, command route, clipboard read, terminal capability state, or configuration option is introduced.

## Validation

- `make test`

## Dependencies

- Task 001
- Task 002

## Expected areas of change

- `crates/oom-edit/src/app.rs`
- `crates/oom-edit/src/command/registry.rs`
- `README.md`
- `CHANGELOG.md`

## Risks / notes

The success message is shared by selection yanks and other clipboard effects, so it must refer generically to sending text. Registry descriptions feed multiple UI projections and must remain short enough for compact layouts while still stating the behavior.
