# Task 002: Make rendered link-index rows copyable

Delegation: main-only

## Goal

Allow users to focus a rendered link-index row and copy its exact destination to the system clipboard with `y` or Enter in rendered Normal and Select modes.

## Context

The generated link list is intentionally source-less, while all existing rendered selections and Vim operators project to raw Markdown ranges. Copyability therefore needs a typed interactive target that does not falsify source provenance or route synthetic text through destructive source operators.

## Scope

### In scope

- The existing project-owned typed link target attached to generated link-index rows when those rows are built.
- Normal and Select key routing for `y` and Enter on a focused link-index row.
- Exact destination-only `Effect::ClipboardWrite` behavior and TUI success/failure feedback through the existing clipboard sink.
- Navigation, selection-carrier, provenance, public API stability, session, App, and snapshot tests.

### Out of scope

- Opening URLs, validating URLs, reading from the clipboard, or adding network behavior.
- Giving synthetic marker/destination glyphs source ranges.
- Making table borders or other generated decorations copyable.
- Changing ordinary source-backed yank/delete/change semantics.

## Implementation requirements

- Represent interaction at layout construction time by adding `TargetKind::Link(index)` to each generated index row. Distinguish this synthetic target from an original inline-link target with the row's existing `LineKind::Synthetic`; do not rediscover a row by searching its rendered string and do not add a public API variant.
- Keep every atom on link separator/index rows source-less. The destination is operation metadata, not claimed Markdown ownership.
- Copy only the destination stored in `RenderedLayout.link_index`; exclude `[n] ` and padding.
- In Normal, plain `y` and Enter on the focused index row emit the clipboard write. In Select, the same focused row is copyable; `y` returns to Normal like an ordinary yank, while Enter leaves the current mode unchanged.
- When focus is not on a link-index row, existing Normal navigation and Select source operators must be unchanged.
- Use the existing terminal-neutral clipboard effect and injected TUI sink. Do not import terminal/OSC types into core.
- Preserve the curated public facade unchanged and keep its compile/API tests passing.

## Acceptance criteria

- [ ] Each generated link-index row has an existing typed `Link` target pointing to the correct destination and retains zero source-backed atoms.
- [ ] `y` and Enter in Normal copy the exact focused destination through `ClipboardWrite`.
- [ ] `y` and Enter in Select copy the exact focused destination with the documented mode transitions.
- [ ] Multiple/repeated links and Unicode destinations resolve by stored index rather than rendered-text matching.
- [ ] Clipboard success and injected clipboard failure produce appropriate non-destructive status feedback.
- [ ] Ordinary source selections, registers, mutation operators, inline-link jump targets, and public facade guards retain their existing behavior.

## Validation

- `make test`

## Dependencies

- Task 001

## Expected areas of change

- `crates/oom-edit-core/src/rendered/mod.rs`
- `crates/oom-edit-core/src/rendered/nav.rs`
- `crates/oom-edit-core/src/session.rs`
- `crates/oom-edit-core/tests/session_integration.rs`
- `crates/oom-edit/src/app.rs`
- `crates/oom-edit/src/snapshot_tests.rs`
- `crates/oom-edit/tests/snapshots/rendered_links_index.txt`

## Risks / notes

The canonical source cursor must not jump merely because focus visits a synthetic row. Selection rendering can use the existing active-row carrier, but source range projection must remain empty for the generated row. Clipboard error feedback must not be overwritten by an unconditional success message.
