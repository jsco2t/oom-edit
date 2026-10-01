# Independent consumer verification

The canonical credential-free origin is `https://github.com/jsco2t/oom-edit`.
Cargo patches belong to the consuming workspace: a dependency's root patches
do not propagate. Pin the editor and all four patches to one full immutable
commit, not a branch or an unapproved future tag.

The example below uses the published `v0.6.0` commit. For an untagged 0.6.5
candidate, use its full committed SHA for all five sources. A local candidate
can be prepared through an isolated Git URL rewrite without changing the
working repository or its Git history. Verify the exact tag only after that
tag has been separately created and published.

```toml
[dependencies]
oom-edit = { git = "https://github.com/jsco2t/oom-edit", rev = "87d5b48f766eb35f30c2136e23d2a4448329713b" }

[patch.crates-io]
hjkl-buffer = { git = "https://github.com/jsco2t/oom-edit", rev = "87d5b48f766eb35f30c2136e23d2a4448329713b" }
hjkl-engine = { git = "https://github.com/jsco2t/oom-edit", rev = "87d5b48f766eb35f30c2136e23d2a4448329713b" }
tree-sitter-md = { git = "https://github.com/jsco2t/oom-edit", rev = "87d5b48f766eb35f30c2136e23d2a4448329713b" }
dirs-sys = { git = "https://github.com/jsco2t/oom-edit", rev = "87d5b48f766eb35f30c2136e23d2a4448329713b" }
```

Replace **all five** `rev` values together when testing another candidate. A
consumer using cargo-deny must explicitly allow this origin in its own policy:

```toml
[sources]
unknown-git = "deny"
allow-git = ["https://github.com/jsco2t/oom-edit"]
```

## Make-owned workflow

Choose an absent or empty output directory outside this workspace. Nothing
already present is overwritten. Set `REV` to the candidate's full SHA and
`SOURCE` to a local Git checkout containing that commit, if available.

```console
make downstream-prepare DOWNSTREAM_DIR=/tmp/oom-edit-consumer DOWNSTREAM_REV="$REV" DOWNSTREAM_SOURCE="$SOURCE"
make downstream-candidate-check DOWNSTREAM_DIR=/tmp/oom-edit-consumer DOWNSTREAM_REV="$REV"
make downstream-negative-check DOWNSTREAM_DIR=/tmp/oom-edit-consumer DOWNSTREAM_REV="$REV"
```

Preparation alone may fetch Git sources. An empty `DOWNSTREAM_SOURCE` fetches
the canonical origin; a supplied local checkout provides its immutable Git
objects through a private configuration without changing the canonical source
identity. No authenticated remote URL is read, printed or copied. Audited
registry dependencies are seeded from this repository's vendor directory.

The resulting consumer owns `Cargo.toml`, `Cargo.lock`, `.cargo/config.toml`,
`vendor/`, a small public-API integration test and a candidate identity file.
The check uses a newly created empty Cargo home and `--offline --locked` for
metadata, warning-denied Clippy and tests. It asserts the workspace boundary,
fixture-local source paths, exact manifest patches, and the actual lockfile
and resolved Git provenance of the editor and all four patches. It drives
construction, input, owned frames, hints, bindings, undo and correlated
close-all events. A target cache is only compiled output, never a source seed.

The negative check mutates the actual resolved graph for each of the five
sources and verifies rejection. A separate temporary copy corrupts a real
vendored source file and must fail Cargo's checksum validation; the original
consumer is not changed. Directory-to-directory Cargo vendoring can create an
invalid checksum self-reference. Preparation removes only that generated
self-reference, preserving every real source-file and registry-package hash.

For uncommitted development only, this target creates a temporary immutable
source snapshot and prints its exact SHA and local source path:

```console
make downstream-snapshot DOWNSTREAM_SNAPSHOT=/tmp/oom-edit-candidate-source
```

Use the printed values as `REV` and `SOURCE` above. A snapshot is preliminary
evidence, never a substitute for a committed release candidate. CI checks its
actual checkout SHA after `make ci`, using the same preparation and offline
validation targets; it does not depend on a tag that does not yet exist.

After separate publication authorization and a successful release push, prepare
a **new** consumer with `DOWNSTREAM_TAG=v0.6.5` and the gated SHA still supplied
as `DOWNSTREAM_REV`. Then run `make downstream-tag-check` with those same
variables. This selects the actual tag for all five Git sources and asserts
that their resolved commit equals the gated SHA. The tag target is never part
of a pre-tag gate and does not create or push any ref.
