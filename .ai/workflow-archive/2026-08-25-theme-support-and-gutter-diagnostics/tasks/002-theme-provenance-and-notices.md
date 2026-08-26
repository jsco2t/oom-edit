# Task 002: Theme Provenance and Canonical Notices

Delegation: main-only

## Goal

Materialize and enforce the exact PRD-pinned upstream palette/license evidence and make the root notice file the complete source/binary license surface.

## Context

The five new built-ins are attributed static data rather than Cargo dependencies. Exact immutable source hashes, complete licenses, declared attribution facts, and Tokyo Night's Enkia origin notice must exist before palette implementation can rely on them.

## Scope

### In scope

- Committed provenance manifest with exact repositories, variants, revisions, paths, complete-file hashes, retrieval date, SPDX identifiers, declared facts/absence, and separate oom-edit semantic mapping notes.
- Exact upstream raw license evidence for Catppuccin, Dracula, Nord, Solarized, Tokyo Night, and Enkia.
- Complete per-theme sections in `THIRD-PARTY-NOTICES.md`, retaining the existing SCOWL section.
- `--licenses` emitting the exact canonical root notice content before terminal setup.
- Expanded data-license script/tests that hash-pin all reviewed assets and notice surfaces while preserving temporary cleanup and root override behavior.

### Out of scope

- Theme palette implementation, registry rows, custom loading, gutter behavior, or runtime network access.
- Substituting newer branch tips or adding Gruvbox/Rosé Pine.

## Implementation requirements

- Retrieve/copy only the immutable PRD revisions and verify every complete-file SHA-256 before using it.
- Preserve complete license texts and exact applicable copyright/author facts; do not invent a Tokyo Night project copyright line.
- Repeat full MIT text in each applicable canonical theme section; include full Apache-2.0 and Enkia MIT text for Tokyo Night as specified.
- Keep raw audit assets distinct from the canonical distribution notice and the project-owned mapping notes.
- Change `args.rs` to embed `THIRD-PARTY-NOTICES.md`, and test exact message equality/pre-terminal behavior.
- Keep `data-license-check` in `make check`; do not weaken existing SCOWL checks.
- No Cargo dependency, lockfile, or vendor change.

## Acceptance criteria

- [ ] Every palette/license source and hash matches the immutable PRD table and has a clear provenance/mapping record.
- [ ] `THIRD-PARTY-NOTICES.md` retains SCOWL and contains exactly one complete section for each of the five bundled themes, with Tokyo Night's Apache and Enkia MIT obligations.
- [ ] `oom-edit --licenses` emits the exact canonical notice file and exits before terminal setup.
- [ ] The compliance script fails on missing/tampered assets, provenance, notices, or CLI embedding and passes the unmodified repository.
- [ ] Gruvbox and Rosé Pine are absent, and no dependency or runtime network mechanism is added.

## Validation

- `make data-license-check`
- `make test`

## Dependencies

- Task 001

## Expected areas of change

- `crates/oom-edit/assets/themes/`
- `THIRD-PARTY-NOTICES.md`
- `crates/oom-edit/src/args.rs`
- `scripts/check-data-licenses.sh`
- `crates/oom-edit/tests/dictionary_assets.rs` or a renamed/generalized bundled-data guard

## Risks / notes

Immutable asset retrieval may require network authorization. License compliance is supply-chain sensitive, so this task remains main-only and must preserve exact bytes and all existing SCOWL guarantees.
