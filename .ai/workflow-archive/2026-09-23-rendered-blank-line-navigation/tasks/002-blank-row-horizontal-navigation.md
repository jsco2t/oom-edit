# Task 002: Enter adjacent content from a blank row directionally

Delegation: main-only

## Goal

Make `h` and Left from an atom-free blank rendered row land on the final
source-backed character before that row, including when the preceding source
line wraps across multiple rendered rows.

## Context

The prior cross-row horizontal fix begins by resolving any atom-free row to a
nearby source-backed point. On the blank row after a wrapped paragraph, that
resolution selects the first atom of the paragraph's last rendered row; the
subsequent backward step therefore lands at the end of the previous rendered
row. Direction must select the adjacent edge itself when the starting row has
no source-backed atom.

## Scope

### In scope

- Test-first reproduction for both `h` and Left from task 001's physical blank
  row after a paragraph wrapped across at least three rows.
- Direction-aware traversal from atom-free rows.
- Counted traversal, document boundaries, Unicode atoms, generated synthetic
  rows, and forward symmetry.
- Canonical source cursor and rendered Select endpoint synchronization.
- Regression coverage for ordinary source-backed cross-row movement.

### Out of scope

- Blank-line/gutter ownership, which task 001 completes first.
- Word motions, vertical motion desired columns, pointer hit-testing, or search.
- Adding cursor stops to generated decorations or glyphs.

## Implementation requirements

- Add the failing `h` and Left regressions before changing navigation and
  confirm that the old code lands one wrapped row too early.
- If the cursor's current row has an exact source-backed atom, preserve existing
  adjacent-atom semantics.
- If it has no source-backed atom, consume the first backward step at the last
  atom strictly before the row, or the first forward step at the first atom
  strictly after it.
- Counts must consume exactly one source-backed atom per step and clamp at the
  first/last available atom without wrapping.
- Do not change the shared nearest-source helper used by pointer/decorations;
  keep the directional rule local to horizontal navigation unless repository
  evidence proves a shared contract is appropriate.
- Preserve canonical cursor commits and Select projection through the existing
  session facade.

## Acceptance criteria

- [ ] One `h` and one Left from the blank row each land on the final atom of the
      final rendered row of the preceding wrapped paragraph.
- [ ] The canonical cursor points to that exact final source character, including
      with multibyte Unicode near the boundary.
- [ ] Counted backward movement proceeds from that character without an extra
      row jump, skip, or duplicate step.
- [ ] Forward movement from atom-free rows is directionally symmetric and both
      document boundaries clamp safely.
- [ ] Character Select keeps its active rendered endpoint and source ranges in
      sync with the corrected motion.
- [ ] Existing horizontal movement from source-backed wrapped rows is unchanged.

## Validation

- `cargo test -p oom-edit-core --offline --locked horizontal_navigation_from_blank_line`
- `cargo test -p oom-edit-core --offline --locked rendered_horizontal_navigation`
- `cargo test -p oom-edit-core --offline --locked rendered_select`
- `cargo test -p oom-edit-core --offline --locked random_mode_width_and_motion_mapping_stays_in_bounds`
- `make check`

## Dependencies

- 001

## Expected areas of change

- `crates/oom-edit-core/src/rendered/nav.rs`
- `crates/oom-edit-core/tests/session_integration.rs`
- relevant rendered navigation/conformance tests

## Risks / notes

Synthetic rows may appear between tables, metadata, loose lists, footnotes, and
link indexes. The traversal must use display order and exact atom ownership, not
assume every atom-free row is a physical blank line.
