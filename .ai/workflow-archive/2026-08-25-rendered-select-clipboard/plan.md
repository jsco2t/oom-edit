# Rendered Select Clipboard

## Objective

Make plain `y` in rendered character, line, and block Select modes preserve the complete existing Vim yank while also sending the exact source-backed selection through the existing system-clipboard effect. Keep the core terminal-independent, retain the dependency-free OSC 52 host transport, make that transport emit canonical padded RFC 4648 Base64, and accurately document the best-effort terminal contract.

## Current behavior

Rendered selections are already projected from parser-provided source provenance into private `vim::ProjectedSelection` values. `VimCore::apply_selection` builds exact character, line, and block payloads, records them in the engine register bank, and returns typed effects to `EditorSession`; `App` consumes `Effect::ClipboardWrite` through an injected `ClipboardSink`.

Plain rendered Select `y` currently records an unnamed yank and returns to Normal without a clipboard effect. Explicit `"+y` and `"*y` target the shared system register and emit one clipboard effect. The TUI already emits OSC 52 without a dependency, but its in-tree Base64 encoder omits canonical `=` padding and its success message implies stronger confirmation than OSC 52 can provide.

The existing tests cover source-backed selection projection, explicit system-register payloads, register/put shapes, clipboard sink injection, and basic OSC 52 emission. They do not require default rendered Select yanks to mirror their complete register slot or cover canonical padding and writer failure boundaries.

## Proposed implementation

Extend only `VimCore::apply_selection`'s rendered-selection yank policy. Record the requested yank through the existing engine API first. For an unnamed yank, clone the resulting unnamed slot into the existing system clipboard slot so characterwise, linewise, and blockwise metadata are preserved, then emit exactly one `VimEffect::ClipboardYank` carrying the already constructed payload. Explicit system-register yanks keep their existing single emission. Named and black-hole yanks do not mirror or emit implicitly.

Update core unit, session integration, provenance, and conformance tests so plain `v…y`, `V…y`, and `Ctrl-V…y` prove exact output, unchanged text, Normal-mode return, internal register behavior, and system-slot shape. Retain explicit system-register coverage and direct source Normal-mode non-emission coverage.

Change the in-tree encoder to canonical padded RFC 4648 output and harden its tests around exact OSC 52 bytes, UTF-8, multiline text, the 100 KiB raw-input boundary, and injected write/flush failures. Keep BEL termination and reject oversize content before writing.

Update App-level tests to drive plain rendered Select `y`, make successful host output wording reflect best-effort emission rather than terminal acknowledgement, update the static command registry description and its exact drift guard, and document clipboard output/input behavior in the README and changelog.

## Architectural decisions

- The change stays behind `EditorSession` and the private `vim.rs` engine wrapper. No public API, effect, command dispatcher, state owner, or clipboard abstraction is added.
- Default rendered yanks remain `Register::Unnamed`. Changing the target to `Register::System` would skip the engine's unnamed-yank update of `"0` and would break established Vim semantics.
- The system slot is mirrored only after the canonical unnamed yank is recorded, by cloning the complete private engine slot. Text-only synchronization is insufficient because linewise and blockwise metadata controls `p`/`P`.
- Implicit publication is limited to plain rendered Select `y`. Plain delete/change and source Normal-mode yanks remain unchanged. Existing explicitly targeted system-register operations retain their behavior.
- The public clipboard boundary remains typed and renderer-neutral. `oom-edit-core` never performs terminal I/O; `oom-edit` remains the only OSC 52 owner.
- OSC 52 remains one dependency-free transport for supported macOS and Linux terminals. A successful write means the escape sequence was emitted and flushed; it cannot prove that the terminal or multiplexer accepted it.
- System clipboard reads are not added. `"+p` continues to use the in-process system register, and desktop clipboard input continues through terminal-native bracketed paste in Insert mode.
- Human approval of this frozen workflow request and plan is the explicit requirements change for this work package. The external notebook research file is input provenance, not a repository deliverable.

## Work included

1. Rendered Select yank mirroring in the private Vim wrapper, with exact default/explicit/excluded register policy.
2. Automated core coverage for character, line, and block output; source provenance; register shape and put behavior; empty selections; named and black-hole yanks; delete/change non-expansion; and source Normal-mode non-emission.
3. Canonical RFC 4648 Base64 padding in the existing OSC 52 writer, with exact byte, size-boundary, and I/O failure tests.
4. TUI clipboard feedback and injected-sink tests driven by plain rendered Select `y`.
5. Registry, README, and changelog updates describing default rendered Select copy, explicit register alternatives, terminal-native Insert paste, cached `"+p`, terminal/multiplexer requirements, best-effort acknowledgement, and payload errors.

## Task sequence

1. `tasks/001-mirror-rendered-select-yanks.md`
2. `tasks/002-canonicalize-osc52-output.md`
3. `tasks/003-update-clipboard-ux-and-docs.md`

## Quality gate

The standard and final gate is `make check`, the repository's build system of record. It runs formatting verification, Clippy with warnings denied, a workspace build, the complete test suite, dependency/license/advisory checks, and bundled-data license verification. Running this full command after every task satisfies the repository's non-negotiable definition of done and catches integration drift between the core and TUI.

Task-specific validation also runs `make test` to exercise the complete workspace tests against each coherent behavior change before the standard gate.

There is no separate generic type-check command because Rust compilation and type checking are already performed by the build and Clippy stages inside `make check`. No dependency/vendor workflow is included because the implementation adds or upgrades no dependency. Cross-terminal/macOS/Linux acceptance cannot be automated in this workspace because OSC 52 has no portable write acknowledgement and terminal configuration is environmental; byte-exact output and injected-sink behavior are the deterministic acceptance boundary.

## Risks

- Mirroring only text would lose linewise or blockwise metadata and make `"+p` behave differently from the original yank. Tests must inspect and exercise all three shapes.
- Treating the default yank as explicitly system-targeted would stop updating `"0`. The implementation must preserve the original unnamed recording path.
- Adding a second emission for explicit `"+y`/`"*y` would cause duplicate terminal writes. Effect counts must be asserted, not merely effect presence.
- Broad conformance rewrites could accidentally change explicit system-register delete/change behavior while narrowing plain operations. Tests must distinguish implicit default publication from explicitly targeted operations.
- Terminals and multiplexers may ignore valid OSC 52 output. Documentation and status wording must describe successful emission without claiming acknowledgement.
- Padding and payload-size changes can create off-by-one or partial-write behavior. Tests must cover all RFC remainder lengths, the raw-input maximum, maximum plus one, and injected write/flush errors.
- User-facing registry metadata is an exact source of truth and has drift-prevention tests; description changes must update those guards together.

## Out of scope

- Full Vim `clipboard=unnamedplus` behavior.
- Implicit system clipboard writes for source Normal-mode `yy`, `y{motion}`, deletes, changes, or other operators.
- Live desktop clipboard reads, OSC 52 queries, or a new `"+p` provider.
- Native AppKit, X11, or Wayland backends; `pbcopy`, `wl-copy`, `xclip`, or similar helpers; clipboard crates; terminal capability probing; or configuration for alternate clipboard policies.
- Changing the 100 KiB raw-input policy or BEL sequence terminator.
- Editing external notebook planning files. The supplied PRD remains the external research source, while this explicitly approved workflow is the frozen implementation authority.
- Guaranteeing clipboard acceptance in every terminal, multiplexer, remote session, or operating-system environment.

## Final acceptance criteria

- Plain `y` from rendered character, line, and block Select emits exactly one `Effect::ClipboardWrite` containing the byte-exact raw Markdown represented by the source-backed selection, leaves the document unchanged, and returns to rendered Normal.
- A plain rendered Select yank preserves the unnamed and `"0` yank state and mirrors the complete characterwise, linewise, or blockwise slot into the shared `+`/`*` register so unnamed and explicit system-register puts retain the selected shape.
- Explicit `"+y` and `"*y` still emit exactly once; named and black-hole yanks do not implicitly emit or replace the system slot; empty/synthetic-only selections remain non-mutating; and plain rendered deletes/changes plus source Normal-mode yanks do not gain implicit clipboard output.
- Existing exact-provenance scenarios—including UTF-8, entities, Markdown syntax, wrapping, tables, repeated content, and synthetic gaps—exercise the new plain-`y` path without reconstructed display glyphs entering the payload.
- OSC 52 output uses canonical padded RFC 4648 Base64 inside `ESC ] 52 ; c ; <payload> BEL`, accepts a raw payload of exactly 100 KiB, rejects 100 KiB plus one before emitting bytes, and returns typed errors for write and flush failures.
- App feedback reports sink errors visibly and describes successful emission without claiming terminal acknowledgement; the injected-sink tests invoke plain rendered Select `y`.
- The registry, README, and changelog accurately describe rendered Select copy and the best-effort OSC 52/input contract, with registry drift tests updated.
- No dependency, public API, second text owner, terminal dependency in core, or additional input/effect routing path is introduced.
- `make check` passes in full at each task gate and at final acceptance.
