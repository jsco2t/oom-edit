# Task 001: Add and verify minimal EditorPane status row

Delegation: main-only

## Goal

Expose and verify a minimal status-row presentation option for embedded `EditorPane` hosts.

## Context

`inline_hints=false` already removes in-pane hint strings, but the status row still paints file and transient content. The host needs a row consisting of the mode badge and current right-side spelling/ruler indicators, except when an active prompt needs to show typed input.

## Scope

### In scope

- Public `PaneOptions` field and its default/documentation.
- Existing pane-to-App-to-status-renderer option propagation.
- Regression tests, public API guards, and embedding documentation.

### Out of scope

- Core editing behavior, status-row geometry, new dependencies, and independent host chrome changes.

## Implementation requirements

1. Write failing external-consumer tests before implementation. Cover the middle-region contents for minimal mode both with `inline_hints=false` and `inline_hints=true`, and compare against unchanged default rendering.
2. Test the four mode badges, right-side spell count and ruler, active ex and search prompts, and that public metadata projections remain available.
3. Implement a documented `PaneOptions::minimal_status_bar` field defaulting to `false`; propagate it through existing owned presentation options. Minimal mode must clear ordinary middle content and prevent in-pane which-key painting while leaving metadata projections intact.
4. Preserve active prompt text/cursor and existing default/standalone behavior. Avoid special terminal logic, a new status owner, and a core API change.
5. Update the exact public API guard and embedding guide with behavior and the hidden-notice implication. Do not touch the existing `.codex/config.toml` change.

## Acceptance criteria

- [ ] Public construction can set `minimal_status_bar=true`, and its default is false.
- [ ] Minimal non-prompt row contains only the mode badge and existing right spell/ruler data, with blank middle, regardless of `inline_hints`.
- [ ] Active ex/search prompts and their cursor remain visible/usable.
- [ ] Four modal badges, public metadata projections, and default/standalone presentation remain correct.
- [ ] Tests and documentation cover the feature and `make check` passes.

## Validation

- `make test-pane-public`
- `make test-public-api`
- `make test-pane-bindings`
- `make test-standalone-host`

## Dependencies

None

## Expected areas of change

- `crates/oom-edit/src/pane.rs`
- `crates/oom-edit/src/app.rs`
- `crates/oom-edit/src/screens/editor.rs`
- `crates/oom-edit/tests/pane_public.rs`
- `crates/oom-edit/tests/public_api.rs`
- `DEVELOPER.md`

## Risks / notes

- The current status renderer uses the middle for both ordinary text and active prompts; preserve the latter explicitly.
- The current right region includes both spelling and source-ruler information.
- A separate host status row can display notices and file metadata hidden by this option.
