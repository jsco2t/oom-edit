# Final Work-Package Evidence

## Work package

- ID: `2026-09-23-rendered-blank-line-navigation`
- Title: `Rendered Blank-Line and Wrapped Navigation Fixes`

## Original objective

Make rendered Normal mode preserve a physical blank line's source identity so
the gutter and status ruler report the correct line, and make `h`/Left from
that atom-free row enter the final source-backed character of the preceding
wrapped line.

## Completed tasks

- Task 001 — Preserve rendered blank-line source identity — `evidence/001.md`
- Task 002 — Enter adjacent content from a blank row directionally — `evidence/002.md`

## Whole-package review

The integrated review covered the approved request, plan, both tasks, their
evidence, and the cumulative diff from baseline
`235f63ba74d9ba6eba0e4f1ea2a23779e90547d2`. Physical blank rows now receive
line-level source ownership in the core while remaining atom-free; the TUI
continues to project the core's canonical line number and cursor. Horizontal
movement applies direction only when entering source-backed content from an
atom-free row, preserving the established behavior for ordinary rendered
atoms. Focused regressions cover layout, provenance, gutters, status,
navigation, counts, Unicode, Select, synthetic rows, and document boundaries.
No public API, dependency, mutable state owner, or TUI-side editing model was
added.

## High-confidence findings fixed

- Paragraph-boundary motion initially recognized only synthetic separators;
  it now also recognizes atom-free physical blank content rows without
  changing generated separator behavior.
- Cursor, selection, loose-list, conformance, and snapshot expectations that
  encoded the old previous-block identity were updated to assert the physical
  blank line's canonical identity.

## Final acceptance criteria

- [x] A rendered physical blank row is numbered from its exact source line, so
      the representative source line 47 displays gutter number 47 in absolute
      mode and the correct relative value in relative mode. Evidence:
      `rendered_blank_line_preserves_physical_identity_and_canonical_cursor`
      and `rendered_blank_line_gutter_and_status_report_its_physical_position`.
- [x] Moving onto the blank row commits its canonical source line at column
      zero, producing display ruler column one (`47:1` in the representative
      file). Evidence: the same core and TUI regressions assert `(1, 0)` and
      `2:1` in the minimal line-46/47 equivalent.
- [x] The blank row owns its exact LF or CRLF line-level byte range while its
      text and atom list remain empty. Evidence:
      `rendered_blank_line_ranges_are_exact_for_lf_crlf_and_unicode`.
- [x] One `h` or Left from the blank row lands on the final source-backed atom
      of the preceding paragraph after it wraps across at least three display
      rows. Evidence:
      `horizontal_navigation_from_blank_line_uses_directional_edges`.
- [x] Counted backward motion consumes the directional edge exactly once;
      Unicode source offsets, document bounds, and Select endpoints remain
      synchronized. Evidence: the same navigation regression plus the
      rendered Select and randomized mapping suites.
- [x] Presentation-only separators remain synthetic, unnumbered, and without
      source atoms. Evidence: adjacent-block and generated-separator assertions
      in the core session regressions.
- [x] The new regressions were introduced before their product fixes and
      reproduced both escaped behaviors: missing blank-line identity and the
      backward jump to the penultimate wrapped row. Evidence: task evidence
      `001.md` and `002.md`.
- [x] Every task-specific validation and every required task and final quality
      gate passed without warnings, dependency changes, or deferred work.
      Evidence: both task evidence files and the final gate below.

## Final quality gate

| Command | Result |
| ------- | ------ |
| `make check` | PASS |

## Cumulative diff

The work-package portion of the cumulative diff adds exact physical blank-line
discovery during rendered block composition, preserves blank separators inside
loose lists, documents the resulting content-row contract, and teaches
horizontal navigation to enter adjacent source content from the directional
edge of atom-free rows. It adds core/session/TUI regressions and updates golden
and terminal snapshots to show real blank-line numbers. Existing uncommitted
changes from the previously completed package and the user-owned
`examples/kitchen-sink.md` edit were preserved. No dependency files changed for
this package.

## Remaining non-blocking concerns

`None`
