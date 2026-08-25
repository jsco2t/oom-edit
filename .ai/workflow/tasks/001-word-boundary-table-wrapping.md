# Task 001: Word-boundary table wrapping

Delegation: main-only

## Goal

Wrap rendered table-cell prose at word boundaries while preserving exact mapped
source provenance, display widths, alignment, and safe hard wrapping for tokens
that cannot fit.

## Context

Table cells currently use `split_cell_text`, which slices mapped display
fragments at the allocated width. The general rendered wrapper prefers spaces,
but table wrapping must keep cell ownership intact and must not reconstruct
provenance after wrapping.

## Scope

### In scope

- Direct mapped-fragment word wrapping for table cells.
- Intentional handling of consumed boundary whitespace and overlong tokens.
- Table layout and source-provenance regression tests.

### Out of scope

- Selection projection.
- Row striping or theme/config changes.
- Table parsing, borders, column allocation policy, or source Insert wrapping.

## Implementation requirements

- Prefer the last whitespace boundary that fits the allocated cell width.
- Hard-wrap only when the next complete token cannot fit on an empty visual
  line; always make progress for over-wide display groups.
- Keep wide characters and zero-width suffixes in intact display groups.
- Partition existing `MappedFragment` values directly. Do not use substring
  searches, global candidate matching, or post-layout column correction.
- Preserve inline semantic styles and exact source byte ownership for every
  displayed fragment; table-created glyphs and padding remain source-less.
- Keep each complete rendered table row at one deterministic display width.
- Add focused and integration-level tests for ordinary prose, exact-boundary
  words, repeated/ambiguous text, multiple spaces, overlong tokens, CJK/emoji,
  inline styles, and source mapping.

## Acceptance criteria

- [ ] A cell containing `This is some interesting long text content written here.` wraps before `content` at the applicable constrained width rather than splitting `content`.
- [ ] Words that fit exactly are retained on the current line, while a single overlong token is hard-wrapped without stalling or exceeding the cell allocation except for an indivisible over-wide glyph.
- [ ] All visual rows retain equal table width and correct left/center/right first-line alignment plus continuation alignment.
- [ ] Every visible source-backed atom has its original valid UTF-8 byte range, including repeated text, styled inline content, and Unicode display groups; synthetic cells remain source-less.
- [ ] Existing paragraph and source-mode wrapping behavior remains unchanged.

## Validation

- `make test`

## Dependencies

None

## Expected areas of change

- `crates/oom-edit-core/src/rendered/table.rs`
- `crates/oom-edit-core/src/rendered/wrap.rs` if a narrow mapped helper is shared
- `crates/oom-edit-core/src/rendered/tests/blocks.rs`
- `crates/oom-edit-core/src/rendered/tests/goldens.rs`
- `crates/oom-edit-core/tests/goldens/vw9_tables.txt`
- `crates/oom-edit/src/snapshot_tests.rs`
- `crates/oom-edit/tests/snapshots/rendered_table.txt`

## Risks / notes

Whitespace may own a real source byte even when omitted at a continuation
boundary. Its omission must be explicit and must not cause neighboring atoms to
inherit its provenance. The architecture's parser-leaf provenance rule is the
primary constraint.
