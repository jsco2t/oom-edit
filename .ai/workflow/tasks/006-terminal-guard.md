# Task 006: Configurable public terminal guard

Delegation: main-only

## Goal

Export a single-owner terminal guard with optional mouse, paste and keyboard disambiguation and exact once-only restoration.

## Context

The current private guard installs global hooks, always attempts mouse/paste, and restores unconditional escapes. A host must choose options, toggle mouse and recreate the guard safely.

## Scope

### In scope

FR-090–094, guard options/errors, shared translation vectors, process/PTY tests and the no-new-unsafe guard.

### Out of scope

The pane's input dispatcher or host application key reservations.

## Implementation requirements

- Write red pure-writer and subprocess/PTY tests for all option combinations, second-active failure, sequential reuse, setup failures after each acquired feature, drop/panic/SIGHUP/SIGTERM restore, runtime mouse toggling and prior-hook chaining.
- Track acquired features, restore each once, prevent inactive/dropped hooks from later writing escapes, and keep signal byte writes async-signal-safe within the existing audited module. Add no new unsafe block.
- Default standalone behavior keeps keyboard enhancement off. Document terminal event-to-`KeyInput` mapping including Esc/Ctrl-[, Tab/Ctrl-i, Enter/Ctrl-m, Backspace/Ctrl-h, Shift and press/repeat/release; share executable vectors with standalone and example hosts.
- Export only project-owned options, errors and guard from crate root.

## Acceptance criteria

- [ ] Guard is exclusive per process and sequentially reusable; partial setup leaves terminal state restored.
- [ ] Exact enable/disable byte and termios observations pass for option, drop, panic and signal paths, with no stale writes.
- [ ] Mouse capture toggles during an active guard; enhancement pop occurs exactly once only when acquired.
- [ ] Shared translation vectors pass with enhancement enabled and disabled; unsafe-block count/scope does not grow.

## Validation

`make test`

`make build`

## Dependencies

001

## Expected areas of change

`crates/oom-edit/src/terminal_guard.rs`, `event.rs`, `lib.rs`, guard/PTY/public API tests, `Makefile` and CI if a new platform PTY command is introduced.

## Risks / notes

Process-global hooks and signal handlers are concurrency-sensitive. The single audited unsafe signal module may need internal updates to its restore bytes, but no additional unsafe scope is authorized.
