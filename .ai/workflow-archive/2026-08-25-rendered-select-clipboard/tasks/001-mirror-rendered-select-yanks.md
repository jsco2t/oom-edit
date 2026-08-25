# Task 001: Mirror rendered Select yanks

Delegation: main-only

## Goal

Make plain rendered character, line, and block Select yanks publish their exact source payload while preserving the complete existing Vim register and put semantics.

## Context

`EditorSession` already projects rendered selections into core-owned operation DTOs, and `VimCore::apply_selection` already constructs byte-exact payloads and records yanks. The missing policy is confined to the private wrapper: unnamed projected yanks do not currently mirror into the system register or emit a clipboard effect.

## Scope

### In scope

- The projected selection yank branch in `crates/oom-edit-core/src/vim.rs`.
- Private wrapper tests for register content/shape and effect cardinality.
- Session integration and conformance coverage for the new plain-`y` behavior and exclusions.
- Conformance manifest/metadata adjustments needed to make default clipboard publication mandatory.

### Out of scope

- OSC 52 encoding or terminal I/O.
- TUI feedback wording and user documentation.
- Source Normal-mode clipboard publication, live clipboard reads, or new public APIs.

## Implementation requirements

- Keep default rendered yanks targeted at `Register::Unnamed` and call the existing canonical yank-recording path before mirroring.
- When the target is unnamed, clone the resulting complete unnamed register slot into the existing shared system clipboard slot, preserving text, linewise, blockwise, and block-width metadata.
- Emit exactly one `VimEffect::ClipboardYank` for unnamed and explicit system yanks.
- Do not implicitly mirror or emit for named or black-hole yanks.
- Preserve explicit system-register behavior and all existing delete/change behavior, including explicitly targeted operations.
- Keep every `hjkl_*` type and register-bank detail inside `vim.rs`; do not expose a new boundary type.
- Preserve empty-selection behavior, source provenance, non-destructive Normal-mode return, dirty/undo state, and canonical text ownership.
- Update tests to assert exact effect counts and payloads rather than only effect presence.

## Acceptance criteria

- [ ] Plain projected character, line, and block yanks each emit exactly one clipboard effect with the exact payload.
- [ ] Plain yanks still populate the unnamed and `"0` slots and mirror the complete slot into `+`/`*`; both ordinary and explicit-system puts preserve character, line, and block shape.
- [ ] Explicit `"+y` and `"*y` emit exactly once, while named and black-hole yanks do not implicitly emit or replace the mirrored system slot.
- [ ] Plain rendered delete/change operations and source Normal-mode yanks do not gain implicit clipboard output; existing explicitly system-targeted semantics remain intact.
- [ ] Empty and synthetic-only selections remain non-mutating and do not emit a projected clipboard effect.
- [ ] Session and conformance tests drive plain `v…y`, `V…y`, and `Ctrl-V…y`, prove unchanged text and Normal-mode return, and cover exact source provenance across existing UTF-8, entity, Markdown, wrapping, table, repeated-text, and synthetic-gap cases.
- [ ] The conformance manifest explicitly guards the default rendered Select clipboard behavior.

## Validation

- `make test`

## Dependencies

None

## Expected areas of change

- `crates/oom-edit-core/src/vim.rs`
- `crates/oom-edit-core/tests/session_integration.rs`
- `crates/oom-edit-core/tests/conformance/mod.rs`

## Risks / notes

The key semantic risk is bypassing the unnamed-yank path, which would stop updating `"0`. Mirroring must copy the completed slot, not reconstruct it from payload text. The source Normal-mode wrapper tests that assert ordinary `yy` does not emit are intentional guards and must remain.
