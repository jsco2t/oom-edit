# EditorPane minimal status bar

## Objective

On branch `feature/editor-pane-minimal-status-bar`, give `EditorPane` hosts an explicit construction option for a minimal editor status row. With the option enabled, the row shows the modal badge and the existing right-aligned spelling/ruler indicators, with its ordinary middle content blank. Active ex and search prompts remain visible while the user types, as confirmed by the requester.

## Current behavior

`PaneOptions::inline_hints` controls inline hints and which-key, but disabling it leaves the filename, dirty/new-file and spelling-off markers, and transient messages in the middle of the editor row. `EditorPane::render` returns a frame containing a one-line status row. The public `status()`, `hints()`, and `which_key()` projections let a host draw separate chrome. The standalone editor enables inline hints and must retain its current rendering.

## Proposed implementation

1. Write external-consumer rendering tests first for the minimal and default presentations, including the mode badge, right indicators, blank middle, active prompts, and public metadata availability.
2. Add a documented `PaneOptions::minimal_status_bar: bool`, defaulting to `false`. Thread it through the existing `EditorPane` to `AppRenderOptions` to status-row rendering path. In minimal mode, suppress the middle's filename/dirty/new-file/spell-off text, inline hints, overlay hints, transient notices, and which-key painting. Preserve the existing mode badge and right spelling/ruler rendering. Keep active ex/search prompts and their cursor behavior.
3. Update public API guards and embedding documentation. Run focused tests and the repository quality gate.

## Architectural decisions

- `PaneOptions` remains the single host presentation input; no new mutable status-bar model, routing layer, or core API is added.
- The option affects only pane rendering. `status()`, `hints()`, `which_key()`, input routing, lifecycle events, and the editor's internal state remain available and behave as before.
- Default behavior and standalone presentation stay unchanged. The new option takes precedence over `inline_hints` for content painted into the pane row.
- Keep the right region's current spelling count and source ruler; the request's right-hand status indicator refers to that rendered region.
- An active ex or search prompt temporarily occupies the middle because typing into an invisible prompt would make the editor unusable. The requester explicitly chose this behavior.

## Work included

- One public host option for the minimal status row.
- Test-first external-consumer tests for default and minimal rows, all four modal badges, spelling/ruler retention, prompts, and the `inline_hints` interaction.
- Focused renderer tests where needed, compile-time public API coverage, and embedding documentation.

## Task sequence

1. [Task 001 — Add and verify minimal EditorPane status row](tasks/001-minimal-status-row.md)

## Quality gate

`make check` is the standard and final gate. The repository's target runs formatting, strict Clippy, warning-free build, the entire test suite, dependency/license/advisory checks, and bundled-data license checks. Task-specific `make` targets exercise the changed public API and presentation behavior before the full gate. No dependency change is planned, so vendoring is not needed.

## Risks

- Minimal mode hides transient notices and filename/dirty markers in the pane row. Hosts selecting it should use the exported metadata and events for any information they need in their own chrome; this effect must be documented.
- The status row also hosts input prompts. Suppressing these would conceal active input, so prompt visibility and cursor placement need regression tests.
- Adding a field to `PaneOptions` must be carried through all internal full struct constructions while preserving existing defaults and standalone snapshots.
- The pre-existing `.codex/config.toml` worktree edit is unrelated and must remain untouched.

## Out of scope

- Changing the size or placement of the pane's status row.
- Custom strings, arbitrary host-rendered widgets, or a general status-bar plug-in API.
- Changes to core editing semantics, commands, themes, or dependencies.
- Reworking the separate host-owned bottom row in the split-pane reference.

## Final acceptance criteria

- A host can enable the documented `PaneOptions` option at construction without using private modules or mutating a returned frame.
- With the option enabled, a non-prompt status row paints only the mode badge and existing right spelling/ruler indicators; no hints, filename, dirty/new-file/spell-off marker, transient notice, overlay hint, or which-key text appears in its middle.
- Active ex and search prompts remain visible and usable; their cursor position remains correct.
- All four mode badges and right indicators remain correct; `status()`, `hints()`, and `which_key()` still export host metadata.
- The default option and standalone editor keep their existing status-bar behavior.
- Public API guards, focused regression tests, and `make check` pass.
