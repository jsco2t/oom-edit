# Task 003: Lightweight read-only Markdown analysis

Delegation: main-only

## Goal

Export parser-equivalent metadata, body spans, first H1 and diagnostics from borrowed source without creating an editing session.

## Context

The oom host needs the editor's authoritative Markdown/front-matter interpretation for indexing, while core must remain terminal-free and avoid Vim/highlighter/spell allocation.

## Scope

### In scope

FR-115 and the ANALYSIS test family in the automated plan.

### Out of scope

oom title/date/tag/search policy, index storage, Markdown semantic changes.

## Implementation requirements

- Write parser-equivalence tests first for YAML, TOML, absent/malformed front matter, CRLF, ATX/Setext H1, entities/escapes, fenced pseudo-headings, Unicode byte ranges and repeated text.
- Reuse core frontmatter and Markdown parsing; return project-owned metadata/spans/diagnostics and decoded first top-level H1. Malformed front matter leaves the full source as body and returns a diagnostic.
- Assert no `EditorSession`, Vim buffer, highlighter, spell engine or terminal service is constructed. Add batch allocation/runtime measurements through make-owned benchmarks without weakening existing perf budgets.
- Add crate-root API and compile/dependency guards.

## Acceptance criteria

- [ ] All named fixtures match the existing editor parser's metadata/body interpretation byte-for-byte.
- [ ] Source is borrowed, unchanged and analyzed without an editing session or terminal dependency.
- [ ] First H1 ignores fenced examples and handles both heading forms and decoding.
- [ ] Public API and analysis performance guards pass.

## Validation

`make test`

`make bench-check`

## Dependencies

001

## Expected areas of change

`crates/oom-edit-core/src/frontmatter.rs`, `rendered/`, a focused analysis module, `lib.rs`, core fixture/API/performance tests, `Makefile` if a new developer workflow is added.

## Risks / notes

The authoritative Markdown specification is `docs/markdown-spec.md`. Do not introduce a second parser or alter rendered/source provenance.
