# Task 002: Share compiled highlight queries

Delegation: main-only

## Goal

Compile each registered fenced-code highlight query once per canonical language in the owning `Highlighter`, then reuse it in rendered layouts.

## Context

Every rendered fence currently calls `Query::new`; source injections already use a per-language `Highlighter::query_cache`.

## Scope

### In scope

- Route rendered snippet highlighting through that existing cache and static `LangDef` registry.
- Add test-only compile counts and exact output comparisons for repeated fences, aliases, unknown languages, width rebuilds, edits and reloads.

### Out of scope

- Cursor mapping, snippet span grouping, parser trees or public API additions.

## Implementation requirements

- Cache ownership stays in core `Highlighter`; avoid a second global/thread-local registry and avoid mutable text copies.
- Preserve independent parsing and all current semantic styles and source atoms.
- Failed or unknown grammar lookups retain the existing `CodeBlock` fallback.

## Acceptance criteria

- [ ] Repeated fences and repeated layouts do not recompile a query for an already cached canonical language.
- [ ] Alias/unknown-language behavior and full styled-line/provenance output match baseline tests.
- [ ] No terminal dependency or new public facade surface is introduced.

## Validation

- `make test-first-frame`
- `make bench-check`

## Dependencies

001

## Expected areas of change

`crates/oom-edit-core/src/syntax/mod.rs`, `crates/oom-edit-core/src/rendered/mod.rs`, relevant core tests

## Risks / notes

The source injection registry and rendered parser may encounter aliases or malformed info strings differently; tests must catch any drift.
