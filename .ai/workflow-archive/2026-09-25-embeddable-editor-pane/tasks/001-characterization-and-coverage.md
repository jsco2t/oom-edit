# Task 001: Characterization and coverage baseline

Delegation: main-only

## Goal

Freeze observable v0.5.0 behavior and performance and establish a clause-level test plan before changing production code.

## Context

The binary currently owns the App loop, while the DRD requires a public pane with no unapproved standalone functional or performance regression. Existing snapshots/conformance and `docs/performance.md` provide the starting oracle.

## Scope

### In scope

Capture baseline frames, command/lifecycle/input traces, README-key-table behavior, and five-trial same-host TUI performance evidence. Create a machine-readable EDIT-FR/NFR clause manifest with stable case names, fixtures, exact expected results and make targets; include a meta-test for missing/unknown/ignored/unjustified-exclusion entries. Register the DRD's sole explicit Nice exclusion, FR-066, precisely. Record current public API/dependency/unsafe surface.

### Out of scope

Pane, core disk, configuration, renderer or terminal guard implementation; changes to existing expected behavior.

## Implementation requirements

- Run baseline performance recording before production source edits; keep the TSV in workflow evidence. Record toolchain, host and baseline SHA.
- Characterize standalone duplicate `:tabnew`, dirty `:qa` refusal, lack of `:wqa`, last-tab exit, four modes, command/search prompts, Space/pending grammar, paste/mouse, all overlays, status/hints/which-key, cursor/redraw/idle timing and exact notices/config behavior.
- Draw clause IDs from the DRD and linked automated plan; name cases and expected state/bytes/events/frame outcomes for every included Must/Should FR and NFR. A planned case may remain non-executable only until its owning implementation task; the final coverage target must reject any such case. Test the manifest validator against removed, invented and ignored entries.
- Use disposable fixtures. Do not modify baseline snapshots or performance thresholds to make characterization pass.

## Acceptance criteria

- [ ] Baseline characterization tests pass against v0.5.0 and expose exact observable values for the high-risk behaviors above.
- [ ] Five-trial baseline TSV for the existing TUI fixture is recorded with baseline SHA and same-host metadata.
- [ ] The clause manifest covers every included EDIT-FR/NFR, names exact outcomes and owning tasks, and the meta-test rejects missing/unknown/ignored/unjustified entries.
- [ ] Existing public API, dependency direction and one unsafe-block baseline are captured for later exact comparison.

## Validation

`make test`

`make bench-check`

`make tui-perf-record BRANCH_ROLE=baseline OUTPUT=.ai/workflow/evidence/perf-baseline.tsv TRIALS=5`

## Dependencies

None

## Expected areas of change

`crates/oom-edit/tests/`, `crates/oom-edit/src/snapshot_tests.rs`, `crates/oom-edit/src/event.rs` tests, coverage manifest/meta-test under `tests/` or `scripts/`, `Makefile`, `.github/workflows/ci.yml`, workflow evidence.

## Risks / notes

Do not record the baseline after changing the App rendering/input path. The manifest's planned status is a planning device, not final acceptance; every planned case becomes an executable behavioral assertion by Task 014.
