# Task 002: Canonicalize OSC 52 output

Delegation: worker-eligible

## Goal

Make the existing dependency-free OSC 52 writer emit canonical padded RFC 4648 Base64 and cover its deterministic byte, size, and I/O failure boundaries.

## Context

The TUI already owns the sole terminal clipboard implementation and injects it behind the core `ClipboardSink` trait. Its small Base64 encoder currently omits padding even though OSC 52 refers to standard Base64. No new transport or dependency is needed.

## Scope

### In scope

- The in-tree Base64 encoder and OSC 52 writer tests in `crates/oom-edit/src/clipboard.rs`.
- Concise correction of stale comments in that module where they misstate protocol or project-plan identifiers.

### Out of scope

- Core selection/register policy.
- App feedback, command metadata, or README/changelog content.
- Payload-limit changes, terminal probing, OSC 52 reads, alternate terminators, native clipboard backends, or dependencies.

## Implementation requirements

- Produce canonical RFC 4648 output with `=` padding for one- and two-byte final groups while retaining the standard alphabet.
- Keep the implementation small and in-tree; do not add a crate.
- Preserve the exact OSC 52 structure `ESC ] 52 ; c ; <base64> BEL` and flush after writing.
- Treat `MAX_PAYLOAD` as a raw UTF-8 byte limit: exactly 100 KiB succeeds and 100 KiB plus one fails before output.
- Add deterministic injected writers that separately exercise write and flush errors without external I/O.
- Keep returned errors project-owned through `ClipboardError`.
- Keep comments concise and describe behavior rather than external requirement identifiers.

## Acceptance criteria

- [ ] All RFC 4648 remainder-length vectors, empty input, multiline input, and UTF-8 input produce canonical padded output.
- [ ] A byte-exact test proves the full OSC 52 sequence including escape prefix, `c` selector, padded payload, and BEL terminator.
- [ ] A raw payload exactly at 100 KiB succeeds; maximum plus one returns `ClipboardError::TooLarge` and emits no bytes.
- [ ] Injected write and flush failures return `ClipboardError` and do not panic.
- [ ] No dependency, alternate backend, terminal probe, or clipboard-read path is added.

## Validation

- `make test`

## Dependencies

- Task 001

## Expected areas of change

- `crates/oom-edit/src/clipboard.rs`

## Risks / notes

The encoded payload is larger than the raw limit; enforce the existing limit before encoding. A flush failure occurs after bytes may have been written and should be reported accurately rather than treated as a no-output guarantee.
