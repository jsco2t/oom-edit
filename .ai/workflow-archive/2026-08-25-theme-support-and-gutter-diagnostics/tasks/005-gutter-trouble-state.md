# Task 005: Gutter Trouble State and Bounded Builder

Delegation: main-only

## Goal

Implement the compact immutable gutter marker summary and resumable generation-safe builder as pure TUI-private state, without integrating it into App scheduling or rendering yet.

## Context

Core diagnostics remain canonical, but rendering needs a source-line summary that can be built cooperatively outside open/edit/layout/paint paths. Establishing the state machine and invariants first makes later App and renderer integration narrow.

## Scope

### In scope

- Private gutter module with `GutterTroubleSnapshot`, `PendingGutterTroubleBuild`, generation/cancellation state, lookup, and memory accounting/test counters.
- Explicit Error > Warning > Info > Hint deduplication per zero-based source line.
- Fixed glyph/semantic-role mapping `E`/error, `W`/warning, `I`/info, `H`/text-muted with application-owned emphasis.
- Bounded advance by explicit unit budget, partial invisibility, atomic completion, cancellation, and compact storage tests.

### Out of scope

- Per-tab ownership, spell publication detection, App/event-loop scheduling, lifecycle invalidation wiring, or screen rendering.
- Core diagnostic ownership/API changes.

## Implementation requirements

- Store only source line and severity in a compact sorted representation; retain no diagnostic message, source text, range collection, or layout DTO.
- Use explicit severity comparison rather than relying on enum order.
- Partial builds never mutate/publish the completed snapshot. Completion swaps one immutable result atomically at the owner boundary.
- Generation cancellation makes stale builders incapable of publication.
- One advance maps no more than its configured item/byte unit and zero budget performs no work.
- Snapshot capacity stays within 24 bytes per unique marked line plus 4 KiB fixed overhead; repeated immutable lookup cannot grow memory.
- Glyph and modifier remain present in Monochrome; color roles are optional reinforcement resolved later by the theme.

## Acceptance criteria

- [ ] Shuffled/repeated diagnostics deduplicate correctly and highest severity is exact for every pair/order.
- [ ] Fixed glyph/role/modifier mapping is exhaustive and independent of theme-authored input.
- [ ] Boundary budgets, partial visibility, atomic completion, cancellation, and stale-generation refusal have deterministic tests.
- [ ] Memory-shape tests prove compact O(unique marked lines) storage and stable capacity under lookup.
- [ ] The module is TUI-private, has no terminal/core ownership reversal, and does not add dependencies or background threads.

## Validation

- `make test`

## Dependencies

- Task 004

## Expected areas of change

- `crates/oom-edit/src/gutter.rs`
- `crates/oom-edit/src/lib.rs`
- Colocated unit tests

## Risks / notes

Although pure, this state defines data-integrity and cancellation semantics used by later async integration, so it remains main-only. Keep the operation model consumer-owned and do not import rendered layout DTOs.
