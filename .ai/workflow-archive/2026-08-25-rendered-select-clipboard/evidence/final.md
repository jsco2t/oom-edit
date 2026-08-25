# Final Evidence — Rendered Select Clipboard

## Final acceptance criteria

- [x] Plain `y` from rendered character, line, and block Select emits exactly one byte-exact clipboard effect, leaves text unchanged, and returns to Normal.
  - Evidence: Task 001 private wrapper, session integration, and conformance shape-matrix tests assert exact one-element effect lists, unchanged documents, and Normal-mode return for all three shapes.
- [x] Plain rendered Select yanks preserve unnamed and `"0`, mirror the complete character/line/block slot into `+`/`*`, and retain ordinary and explicit-system put shape.
  - Evidence: Task 001 wrapper tests compare text, linewise, blockwise, and block-width fields across the slots; conformance performs and undoes both ordinary `P` and `"+P` for every selection shape.
- [x] Explicit `"+y` and `"*y` still emit once; named/black-hole, empty/synthetic-only, delete/change, and source Normal-mode exclusions remain intact.
  - Evidence: Task 001 tests assert exact cardinality for both aliases, unchanged mirrored state for named/black-hole yanks, empty-selection no-ops, no default delete/change effects, and retained source Normal non-emission guards.
- [x] Existing source-provenance cases exercise the new plain-`y` path without synthetic display content entering payloads.
  - Evidence: Updated session and conformance cases cover UTF-8 combining/CJK text, entities, Markdown constructs, source-backed links, wrapping, tables, repeated content, resized blocks, and synthetic gaps with exact payload assertions.
- [x] OSC 52 uses canonical padded RFC 4648 output, accepts exactly 100 KiB, rejects maximum plus one before output, and returns typed write/flush errors.
  - Evidence: Task 002 RFC vectors, byte-exact full-sequence test, boundary tests, and injected writer failures all pass; the final review aligned the core error message to the same 100 KiB contract.
- [x] App feedback visibly reports sink errors and describes successful emission without claiming acknowledgement; injected-sink tests use plain rendered Select `y`.
  - Evidence: Task 003 App tests drive `v` then `y`, assert Warning failure feedback, and assert the generic `sent text to system clipboard` Info message for both selection and link-index effects.
- [x] Registry, README, changelog, and derived palette snapshots accurately describe the clipboard behavior and best-effort output/input contract.
  - Evidence: The single Select-yank registry row and exact guard advertise publication; regenerated goldens prove both palette projections. README documents plain/explicit yanks, terminal/tmux OSC 52 support, non-acknowledgement, limits/errors, Insert paste, and cached `"+p`; the Unreleased changelog records the behavior.
- [x] No dependency, public API, second text owner, terminal dependency in core, or additional input/effect route was introduced.
  - Evidence: The cumulative diff changes existing private wrapper policy, the existing typed clipboard transport, tests, registry metadata, and documentation. Cargo manifests, `Cargo.lock`, public facades, and routing/state ownership are unchanged; dependency-hygiene and public-API guards pass.
- [x] `make check` passes at every task gate and at final acceptance.
  - Evidence: `evidence/001.md`, `evidence/002.md`, and `evidence/003.md` record passing task gates. The final integrated run passed all seven stages with zero failures.

## Final review

Reviewed the cumulative diff from baseline `6bf7c7aee91601e2d9cad201d0db3777e47561b5` against the frozen request, plan, all task documents, architecture constraints, and task evidence. The review found and corrected stale plan-number comments plus inconsistent 100 KB wording in the directly related clipboard surface. After that correction, no high-confidence correctness, compatibility, architecture, test, documentation, or scope defect remained.

## Final quality gate

| Command | Result |
| --- | --- |
| `make check` | PASS — fmt-check, lint, build, test, deny, audit, and data-license-check; 7 passed, 0 failed |

## Cumulative diff

- Mirrored completed unnamed rendered-selection yanks into the shared system slot and emitted one typed clipboard effect while preserving canonical Vim register recording.
- Expanded exact register, shape, put, exclusion, provenance, and conformance coverage for default and explicit clipboard yanks.
- Canonicalized the dependency-free OSC 52 encoder to padded Base64 and covered exact bytes, 100 KiB boundaries, and I/O failures.
- Updated App feedback/tests, registry-derived help and goldens, README clipboard guidance, changelog, and stale clipboard comments.
- Changed 11 tracked files with no dependency or manifest changes.

## Remaining non-blocking concerns

None
