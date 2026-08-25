# Task 002: Wire clipboard format configuration

Delegation: main-only

## Goal

Add a persistent Markdown-by-default copy-format preference and make `App` send
the configured representation through the existing clipboard sink.

## Context

Task 001 gives `Effect::ClipboardWrite` both Markdown and plain-text forms. The TUI
already owns TOML configuration, startup wiring, injected clipboard transport,
status feedback, and OSC 52 limits, so it is the correct owner of the user's
format preference and final choice.

## Scope

### In scope

- TUI configuration types, defaults, serde names, loading, validation, and
  round-trip tests.
- Startup/App wiring and injected-sink tests for both formats.
- OSC 52 boundary/error regression tests using the chosen representation.
- README and changelog documentation.
- Any exact registry or snapshot adjustment directly required by accurate copy
  behavior documentation.

### Out of scope

- Runtime commands, palette entries, or per-document overrides.
- Alternate clipboard backends, reads, queries, or terminal probing.
- Changing internal register or put behavior.
- Changing OSC 52 encoding, size policy, or acknowledgement semantics.

## Implementation requirements

- Add a TUI-owned `ClipboardConfig` and closed copy-format enum following existing
  owned configuration patterns.
- Use `[clipboard] copy_format = "markdown"` as the serialized default and accept
  `"plain-text"` as the sanitizing alternative.
- Missing clipboard sections and fields must default to Markdown. Unknown/invalid
  values must use existing typed configuration error handling.
- Pass the loaded preference into `App` through explicit construction rather than
  global state or terminal queries.
- On `Effect::ClipboardWrite`, select exactly one field and pass it to the existing
  injected `ClipboardSink`; do not format Markdown in the TUI.
- Apply the choice to all Markdown-backed clipboard effects. Format-invariant URL
  effects remain identical.
- Keep internal Vim registers, cached `"+p`, selection state, and undo independent
  from the external preference.
- Evaluate the existing 100 KiB limit and sink error feedback against the selected
  outgoing bytes.
- Document the Markdown default, the exact TOML setting, plain-text behavior,
  external-only register scope, bracketed-paste input contract, and best-effort
  OSC 52 transport without claiming terminal acknowledgement.

## Acceptance criteria

- [ ] Default and absent configuration send the Markdown representation to the
      sink, preserving the exact user example.
- [ ] `copy_format = "plain-text"` sends only the sanitized representation for
      the same action.
- [ ] Config defaults, valid values, partial configs, save/load round trips, and
      invalid values have focused automated coverage.
- [ ] App tests use the injected sink and prove exactly one write of the selected
      representation with unchanged status/error behavior.
- [ ] The 100 KiB acceptance/rejection boundary applies to selected bytes and no
      partial output is written on rejection.
- [ ] URL-only copies behave identically in both modes.
- [ ] Internal register shape and `p`/`P` behavior are unchanged by the setting.
- [ ] README and changelog accurately describe the default, setting, sanitization,
      register scope, clipboard input, and OSC 52 limitations.

## Validation

- `make test`

## Dependencies

- Task 001

## Expected areas of change

- `crates/oom-edit/src/config.rs`
- `crates/oom-edit/src/app.rs`
- `crates/oom-edit/src/lib.rs`
- `crates/oom-edit/src/clipboard.rs`
- `README.md`
- `CHANGELOG.md`
- Relevant TUI integration tests and snapshots if user-facing text changes

## Risks / notes

The preference belongs at the final outbound boundary. Applying it earlier would
silently change cached system-register text or discard linewise/blockwise shape.
Constructor call sites and test helpers must all make the default explicit enough
that future configuration additions cannot accidentally invert the policy.
