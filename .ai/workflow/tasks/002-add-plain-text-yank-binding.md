# Task 002: Add Plain-Text Yank Binding

Delegation: main-only

## Goal

Add discoverable uppercase `Y` in rendered Select as a one-shot syntax-free system copy while preserving Markdown-safe internal registers and the existing default/configured `y` policy.

## Context

Task 001 provides one correct dual-format selection payload. The current global `clipboard.copy_format` defaults to Markdown and optionally makes plain `y` publish plain text. A direct alternate is useful for occasional plain-text copy without requiring a persistent configuration change.

## Scope

### In scope

- Core rendered Select routing for uppercase `Y`.
- Explicit plain-text clipboard publication from the corrected `ClipboardContent`.
- Internal register behavior, register prefixes, synthetic links, mode exit, messages, errors, and size limits.
- Static command registry/help/palette discoverability and drift guards.
- README configuration/clipboard guidance and changelog entry.
- App/core integration and terminal key-translation coverage.

### Out of scope

- Removing or migrating `clipboard.copy_format`.
- New Space chords, a second command registry, source-mode mappings, or configurable keymaps.
- HTML/rich clipboard output or clipboard reads.

## Implementation requirements

- Bind `Y` only in rendered Select and keep it core-owned; App must continue forwarding non-Space modal input unchanged.
- Accept the uppercase terminal representations supported by the existing key model without accepting Ctrl/Alt variants.
- Run the same yank/register operation as `y`, but publish the plain-text representation exactly once for this action regardless of the configured output preference.
- Keep the internal register Markdown-safe so `p`/`P` remain source-preserving.
- Preserve explicit named, system, unnamed, and black-hole register publication rules. Do not make a named or black-hole yank implicitly publish.
- Keep source-backed and synthetic-link behavior deterministic; link destinations remain format-invariant.
- Declare `Y` once in `command::COMMANDS` and update exact count/order/binding and projection tests rather than maintaining duplicate user-facing lists.
- Document that default `y` is Markdown, configured `plain-text` remains a supported global preference, and `Y` is the recommended one-shot plain-text action.

## Acceptance criteria

- [ ] `Y` copies expected syntax-free multiline text with logical newlines exactly once and exits Select mode without editing the document.
- [ ] `Y` bypasses both Markdown and plain-text global preferences consistently while plain `y` retains the existing configured policy and Markdown default.
- [ ] After `Y`, internal put inserts faithful Markdown, not stripped rendered text.
- [ ] Register-prefix matrices preserve current system publication and named/black-hole isolation behavior.
- [ ] Clipboard success, failure, and representation-specific 100 KiB limits continue to apply to the actual `Y` payload.
- [ ] Synthetic-only link selection remains safe and format-invariant for both yank keys.
- [ ] The registry, palette/help, exact binding contract, and context-sensitive projections expose `y` as Markdown/default yank and `Y` as plain-text yank without routing drift.
- [ ] README and changelog describe the behavior and retain accurate `copy_format` compatibility guidance.
- [ ] No public API or dependency changes are introduced.

## Validation

- `make test`

## Dependencies

- Task 001

## Expected areas of change

- `crates/oom-edit-core/src/session.rs`
- `crates/oom-edit-core/tests/session_integration.rs`
- `crates/oom-edit/src/app.rs`
- `crates/oom-edit/src/command/registry.rs`
- `crates/oom-edit/src/overlay/palette.rs`
- `crates/oom-edit/src/snapshot_tests.rs`
- `crates/oom-edit/tests/snapshots/`
- `README.md`
- `CHANGELOG.md`

## Risks / notes

Uppercase input and register prefixes are stateful. Tests must cover the full route through crossterm translation, App forwarding, EditorSession selection handling, Vim register recording, and the injected clipboard sink.
