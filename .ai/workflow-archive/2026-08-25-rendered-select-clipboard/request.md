# Work Request

Implement the clipboard work package specified by:

`/Users/jason/Developer/sources/personal/notebook/projects/oom-edit/features/03-clipboard/prd.md`

The supplied PRD is titled **03-clipboard: System Clipboard Write Solution Research** and was provided as the complete work description for this workflow. Its required outcome is:

- In rendered character, line, and block Select modes, plain `y` must keep the existing non-destructive internal yank behavior and also publish the exact selected raw Markdown source through the existing system-clipboard path.
- Preserve the unnamed register, yank register `"0`, selection shape, exact source provenance, and existing `p`/`P` behavior.
- Mirror the completed default yank into the existing system register and emit exactly one typed clipboard-write effect.
- Preserve explicit `"+y` and `"*y`; explicit named and black-hole register yanks must not implicitly overwrite the system clipboard.
- Do not broaden clipboard publication to delete, change, source Normal-mode `yy`/`y{motion}`, or full Vim `clipboard=unnamedplus` semantics.
- Do not implement a live system-clipboard read for `"+p`; terminal-native bracketed paste in Insert mode remains the supported clipboard-input path.
- Continue using the existing terminal-neutral `ClipboardSink`/typed-effect boundary and the dependency-free OSC 52 TUI transport. Do not add clipboard crates, runtime helper programs, a second clipboard abstraction, or terminal dependencies in `oom-edit-core`.
- Make the existing Base64 output canonical RFC 4648 padded output while retaining BEL termination and the current explicit 100 KiB raw-payload limit.
- Document that OSC 52 is terminal/multiplexer dependent and best-effort, that successful output cannot confirm terminal acceptance, and that oversize or I/O failures remain visible.
- Add automated coverage for exact character/line/block payloads, register and put semantics, named/black-hole exclusions, source Normal-mode non-emission, provenance edge cases, canonical OSC 52 bytes, error paths, and size boundaries.
- Update relevant user-facing help/reference documentation and conformance metadata so the new plain-`y` behavior is accurately represented and guarded against drift.
- Use one implementation across supported macOS and Linux terminals, add no dependency, and pass the repository-native `make check` gate.

The PRD explicitly leaves broader source Normal-mode clipboard publication outside this package. Manual terminal compatibility observations may document environmental OSC 52 behavior, but automated acceptance is based on exact emitted bytes and injected sink behavior because OSC 52 provides no portable acknowledgement.
