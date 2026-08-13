# Local dependency patches

This directory contains complete crate sources used through the workspace's
`[patch.crates-io]` entries. These are maintained source inputs, not generated
copies under `vendor/`. Cargo therefore excludes the replaced registry packages
when `make vendor` regenerates `vendor/`; the absence of `vendor/hjkl-buffer`
and `vendor/tree-sitter-md` is intentional.

## Provenance and removal criteria

| Crate | Base release | crates.io package checksum | Upstream commit | Reviewed local delta | Remove the patch when |
| --- | --- | --- | --- | --- | --- |
| `hjkl-buffer` | `0.39.0` | `8c9f9314b826a7e6c2bb4531442b0e73d279e785f9e74685f806ac4762c7f237` | `fa25c86eb122573784ebc29ff64e911933072f68` | One public, read-only `Buffer::current_undo_seq` accessor returning the current undo node's stable sequence number under the existing mutex. | A compatible `hjkl` family release exposes an equivalent public O(1) history-state identity and the dirty-tracking conformance suite passes without the fork. Version `0.41.2`, checked 2026-08-07, does not yet do so. |
| `tree-sitter-md` | `0.5.3` | `2efd398be546456c814598ee56c0f51769a77241511b4a58077815d120afa882` | `f969cd3ae3f9fbd4e43205431d0ae286014c05b5` | Replace two C `isdigit` calls with explicit ASCII digit-range checks so a Unicode `TSLexer::lookahead` cannot index a libc classification table out of bounds. | A compatible release contains the equivalent scanner fix and the deterministic Unicode regression test passes without the fork. No such release existed when checked 2026-08-07. |

The package checksums above authenticate the original immutable crates.io
archives. `.cargo-checksum.json` files are deliberately not retained inside the
patch directories: their per-file hashes become false as soon as a local source
file changes, and Cargo does not use them for path-patched packages. The
`.cargo_vcs_info.json` files are retained because they accurately identify the
upstream source commits and paths.

## Supply-chain assessment

Both crates were already accepted, exact-pinned runtime dependencies before the
patches were introduced. The patches add no package or transitive dependency,
so the original adoption/popularity baseline is unchanged. Both remain MIT
licensed. Maintenance is checked against upstream releases and source before
each update; the complete local delta is small enough to review directly.

The `hjkl-buffer` behavior cannot be safely hand-rolled in oom-edit without
mirroring the engine's branching, pruning, and history-node identity. The
`tree-sitter-md` correction is hand-written locally because it is two explicit
ASCII comparisons and adding another dependency would not help.

## Updating or removing a patch

1. Check the newest compatible upstream release for an equivalent fix or API.
2. Review its license, maintenance signal, dependency changes, and complete
   source diff before changing the exact pin.
3. If the patch remains necessary, replace the directory with the exact release
   sources, reapply only the documented delta, and update the provenance table.
   If upstream contains the fix, remove the `[patch.crates-io]` entry and the
   corresponding patch directory instead.
4. Run `make vendor` so `vendor/`, `Cargo.lock`, and Cargo's source replacement
   agree.
5. Run `make check`, including the dirty-tracking conformance cases and the
   deterministic Unicode Markdown regression test.
6. Update this record and `docs/dependencies.md` in the same change.
