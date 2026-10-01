# Task 008: Incremental source analysis transaction

Delegation: main-only

## Goal

Make ordinary local mutations update exact source Markdown structure and
highlighting without a whole-document Tree-sitter parse or injection walk.

## Context

The current `LiveDocument` gateway calls `Highlighter::apply_edit` before
returning, and structural edits can spend hundreds of milliseconds in its
incremental parse. Task 007 must identify safe bounded update boundaries and
the cases that genuinely propagate farther.

## Scope

### In scope

- Integrate the proven bounded source update behind the existing
  `LiveDocument` mutation gateway; keep Vim text authoritative.
- Maintain exact line starts, Markdown structure, injection ranges, compiled
  query reuse, bounded source-fence parse cache, front-matter span and
  reference-label/spell exclusion dependencies after sequential edits.
- Expand invalidation when fences, containers, front matter or global
  definitions change; measure and expose test-only affected-range/reuse
  counters.
- Add differential/property tests against a fresh `Highlighter::new` for
  random edit/undo sequences and all supported source constructs.

### Out of scope

- Rendered block/layout caching or changes to host event routing.

## Implementation requirements

- `highlight_lines` must remain fully current and exact at every visible
  viewport; no stale/plain temporary source styling or asynchronous worker.
- Preserve the one-owner text and atomic-mutation contracts. Do not grow a
  second mutable text buffer or share a lock on the key path.
- Keep per-language parser/query registry in `LangDef`; no third-party type
  enters the public facade.
- A true wide invalidation may take wider work, but an ordinary local line
  edit must not trigger a full-document parse by default.

## Acceptance criteria

- [ ] Differential tests show source lines, semantic spans, front matter,
  injection ranges and relevant spell/reference data match a fresh parse
  after sequential UTF-8/CRLF, fence, list, table and front-matter edits.
- [ ] Property tests exercise random local/global edits and undo, with
  reproducible seeds and a meta-test guarding the case inventory.
- [ ] Counters prove bounded source reprocessing for fixed 1 MiB ordinary
  edits and correctly wider reprocessing for delimiter/definition edits.
- [ ] Existing source-mode, public-pane and conformance tests remain green.

## Validation

- `make test-incremental`
- `make test-pane-public`
- `make test-first-frame`

## Dependencies

007

## Expected areas of change

`crates/oom-edit-core/src/syntax/mod.rs`,
`crates/oom-edit-core/src/session/live_document.rs`,
`crates/oom-edit-core/src/frontmatter.rs`, `crates/oom-edit-core/src/spell/`,
`crates/oom-edit-core/tests/`

## Risks / notes

An unchanged structural tree is not proof that injected-language or
reference-definition semantics are unchanged. Validate each derived cache
against the complete reference and widen invalidation when uncertain.
