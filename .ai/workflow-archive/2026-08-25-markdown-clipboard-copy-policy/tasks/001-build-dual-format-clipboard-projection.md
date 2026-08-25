# Task 001: Build dual-format clipboard projection

Delegation: main-only

## Goal

Give every core clipboard write independent Markdown and rendered plain-text
representations while preserving the existing edit/register operation model.

## Context

`RenderedSelection.source_ranges` are exact operation-safe ranges for visible
source-backed atoms. They intentionally omit Markdown delimiters that rendered
editing must preserve. The previous workflow incorrectly reused their concatenated
payload as clipboard presentation. The corrective implementation must not alter
those ranges or the private `vim::ProjectedSelection` semantics.

## Scope

### In scope

- Core rendered inline/source provenance needed for copy-only construct spans.
- Core Markdown and plain-text clipboard projections.
- A project-owned typed clipboard content payload on the public effect boundary.
- Session, Vim-wrapper, rendered/provenance, integration, conformance, and public
  API tests.
- Updates to core effect consumers required for the type change, using Markdown
  as the temporary/default choice until Task 002 wires configuration.

### Out of scope

- Persistent configuration and final App preference selection.
- Alternate terminal/clipboard transports or desktop clipboard reads.
- Changes to edit/register/put/undo semantics.
- User documentation beyond code/API documentation required for the new DTO.

## Implementation requirements

- Preserve `RenderedSelection.source_ranges` and `ProjectedSelection` as the
  operation model used by yank/delete/change and register-shape logic.
- Build a separate clipboard projection before a rendered selection is consumed.
- Preserve exact full source spans for wholly selected inline code, emphasis,
  strong, strikethrough, links, images, nested constructs, escapes, and entities.
- Do not expand partial selections into unselected delimiters, destinations,
  content, or neighboring constructs.
- Produce renderer-accurate plain text with syntax stripped, entities and escapes
  decoded, inline-code normalization honored, and synthetic output excluded.
- Preserve character, line, and block logical shapes through wrapping, tables,
  Unicode, repeated content, multiline selections, and synthetic gaps.
- For source-backed clipboard effects outside rendered Select, populate the same
  typed payload using the existing Markdown parser and a deterministic behavior
  for partial/malformed fragments.
- Represent format-invariant URL-only copies with equal Markdown and plain-text
  values.
- Change `Effect::ClipboardWrite` to carry an owned project DTO and deliberately
  re-export that DTO from the crate-root facade. Extend compile-time public API
  guards in the same change.
- Update all downstream matches atomically. Until Task 002 adds the preference,
  App must send the Markdown representation so the requested default is already
  correct.
- Keep `hjkl_*` types confined to `vim.rs`, core terminal-independent, and live
  text solely owned by `LiveDocument`.
- Use hardcoded expected strings in regression tests. Do not compute expected
  clipboard text from `source_ranges` or another production projection.

## Acceptance criteria

- [ ] The exact user example emits every source backtick in the Markdown field
      and the exact syntax-free sentence in the plain-text field.
- [ ] Complete code, emphasis, strong, strikethrough, link, image, nested,
      escaped, and entity cases have byte-exact Markdown and plain-text tests.
- [ ] Partial selections prove no unselected syntax or content is added.
- [ ] Character, line, and block coverage includes wrapping, tables, Unicode,
      repeated content, multiline output, and synthetic decorations.
- [ ] Link-index URL copies have equal fields and remain source-independent.
- [ ] Plain/default and explicit system yanks retain their existing register
      contents and shapes; puts, delete, change, undo, empty selection, and source
      Normal-mode non-emission tests remain green.
- [ ] The public effect/DTO API is guarded by compile tests and exposes no
      third-party type.
- [ ] App integration compiles and emits the Markdown field as the default.

## Validation

- `make test`

## Dependencies

None

## Expected areas of change

- `crates/oom-edit-core/src/rendered/blocks.rs`
- `crates/oom-edit-core/src/rendered/nav.rs`
- `crates/oom-edit-core/src/session.rs`
- `crates/oom-edit-core/src/vim.rs`
- `crates/oom-edit-core/src/lib.rs`
- `crates/oom-edit-core/tests/public_api.rs`
- `crates/oom-edit-core/tests/session_integration.rs`
- `crates/oom-edit-core/tests/conformance/mod.rs`
- `crates/oom-edit/src/app.rs`

## Risks / notes

Full construct spans are copy-only metadata. They must never replace the atom
ranges used for destructive editing. Nested constructs and partial selections are
the highest-risk boundary: outer syntax is included only when its complete visible
content is selected, while exact inner source remains composable without duplicate
bytes.
