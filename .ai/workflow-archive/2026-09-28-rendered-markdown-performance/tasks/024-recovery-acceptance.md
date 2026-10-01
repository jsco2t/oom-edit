# Task 024: Integrated recovery acceptance

Delegation: main-only

## Goal

Verify the completed recovery as one product change across exact UX, fidelity, realistic memory, and both host models.

## Context

Tasks 020–023 must jointly eliminate the prose pause and keep the Rust-fence working set within the narrowly revised cap without a leak. Passing isolated task checks is insufficient after earlier final-gate failures.

## Scope

### In scope

- Execute exact 1 MiB prose/fence edit, Select, navigation, cold, Rust-only RSS, leak-stability and host-parity gates.
- Review cumulative changes from the work-package baseline and preserve raw records.
- Update performance documentation and manual kitty acceptance steps with measured results and honest limits.

### Out of scope

- Declaring physical terminal presentation smooth based solely on PTY or owned-frame timing.
- Relaxing any threshold beyond Revision 7's explicit Rust-only RSS cap or hiding a wide fallback as an ordinary edit.

## Implementation requirements

- Verify at least 100 prose `d` cycles and 100 prose `c` cycles at the exact fixture's representative location, plus another mixed-Markdown location; retain input/frame timing and complete-state assertions.
- Rerun the exact Rust/Go 400-cycle gate, realistic five-process peak-RSS gate, three-process 400-cycle RSS-stability gate, all established final commands, property/differential tests, public API/privacy and standalone/embedded parity.
- Record any broad-semantic fallback separately from ordinary local edits and explain the user's manual visual check.

## Acceptance criteria

- [ ] Every Revision 6 and previous round-2 acceptance criterion passes, except that Revision 7's Rust-only +15% peak-RSS cap supersedes Revision 6's Rust-only +10% cap; all other limits are unchanged. Raw evidence includes the no-growth stability probe.
- [ ] `make check` and every command in `gate.json` pass without warnings or errors.
- [ ] `docs/performance.md` reports before/after prose p99/worst, Rust-fence RSS by size, first-edit cost, leak-stability checkpoints and their finite scope, and terminal-measurement limits without implying universal <50 ms for global semantic changes.
- [ ] No unresolved correctness, UX, memory or standalone/embedded regression remains in the cumulative package review.

## Validation

- `make bench-acceptance-1mb-prose-cycles`
- `make bench-acceptance-1mb-edit-cycles`
- `make bench-realistic`
- `make bench-rss-stability`
- `make bench-interactions`
- `make test-public-api`
- `make test-embedding-example`
- `make test-standalone-host`
- `make check`

## Dependencies

023

## Expected areas of change

`docs/performance.md`, `scripts/`, `Makefile`, `crates/oom-edit-core/tests/`, `crates/oom-edit/tests/`, `.ai/workflow/evidence/`

## Risks / notes

The full package remains incomplete until the final whole-package review and complete final gate pass. Manual kitty acceptance supplements the automated evidence.
