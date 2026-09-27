# Final Work-Package Evidence

## Work package

- ID: `2026-09-25-embeddable-editor-pane`
- Title: Embeddable Editor Pane and Standalone Parity
- Approved plan: revision 5, acceptance round 1
- Frozen hash: `c750cc88e70cccb9392b9c74aab93d75da004b86adf29cd74c51cd3a74033a9b`
- Baseline: `eafaa6afa796d0258b72a6e5ab6ca5a3699d600a`
- Final main/release: `87d5b48f766eb35f30c2136e23d2a4448329713b`
- Published tag: `v0.6.0`, annotated object `9dfd4deeb546ce5336ea8a627b1c5011e2a4f71e`
- Verification host: Linux x86_64, Rust 1.97.1, Ryzen 7 5700U, eight logical CPUs
- Final gate attempt: 1; every listed command exited 0 on 2026-09-27

## Original objective

Implement the complete referenced editor DRD/contracts/test plan: a reusable
owned `EditorPane` API around the existing editor, with standalone using that
same pane and no unintended functionality or performance regression. Deliver
host lifecycle/file-policy coordination, safe disk reconciliation, explicit
configuration/services, owned rendering/metadata and tested Rust integration
documentation. Prepare and independently verify version 0.6.0, publishing its
tag only after separate release authorization. The approved narrow Shift+V
repair and the DRD's explicit safety/disk/theme changes are intentional parity
exceptions; unrelated product features, crates.io and automatic archive are not.

## Completed tasks

All tasks are COMPLETE, implemented/reviewed by the main thread. Historical
completed task documents/evidence remain unchanged.

| Task | Deliverable | Evidence |
| --- | --- | --- |
| 001 | Characterization and executable coverage baseline | [001.md](001.md) |
| 002 | Versioned core disk state and safe reload | [002.md](002.md) |
| 003 | Lightweight read-only Markdown analysis | [003.md](003.md) |
| 004 | Public config and safe semantic persistence | [004.md](004.md) |
| 005 | Themes, owned styles and notices | [005.md](005.md) |
| 006 | Configurable exclusive terminal guard | [006.md](006.md) |
| 007 | Bounded pane-local owned rendering | [007.md](007.md) |
| 008 | Construction, input, focus, time and stable tab protocol | [008.md](008.md) |
| 009 | Policy-checked prepared lifecycle transactions | [009.md](009.md) |
| 010 | First-frame regression repair and standalone public-pane migration | [010.md](010.md) |
| 011 | Disk polling and safe-point reconciliation | [011.md](011.md) |
| 012 | Structured binding registry, ownership and hints | [012.md](012.md) |
| 013 | Embedded example and independent consumer | [013.md](013.md) |
| 014 | Documentation, API/privacy guards and integrated performance | [014.md](014.md) |
| 015 | Exact-main release validation and authorized published tag | [015.md](015.md) |

## Whole-package review

The primary thread read the original request, approved plan, all tasks and all
primary task evidence, ran the cumulative baseline diff and inspected integrated
source with surrounding code. All nine code-review perspectives and all 15
workflow review concerns were applied. Report: [final-review.md](final-review.md).
Zero Critical / zero Important findings above confidence 80. No review agent,
source mutation, speculative cleanup or new release ref was needed.

The API is owned and curated; no mutable session/App escape or terminal dependency
was added to core. Both hosts use one pane/App/lifecycle dispatcher and one atomic
text/cache gateway. Captured tab IDs, single-use leases, all-target validation,
policy checks and version-bound save/reload decisions preserve retained buffers
and target identities. Modal input, registry ownership, explicit clocks/services,
bounded owned frames and terminal teardown remain consistent across components.
Compile-time privacy/dependency guards and executable consumer tests complement
snapshots, conformance and property tests.

Generated parser changes were reviewed via controlling grammar/scanner/build
sources, exact named-tree/highlighting/provenance fingerprints, incremental
equivalence and recorded byte-identical pinned regeneration, not claimed as a
manual audit of every generated C line. No golden, performance threshold,
comparator, dependency version or native Linux oracle was weakened.

## High-confidence findings fixed

None during the final integrated review. Earlier repairs and their failing/green
probes are documented in task evidence: first-frame performance, owned renderer
conversion privacy, real paired dirty-diagnostic timing validation, candidate
dirty-edit performance, and the separately approved terminal-realistic Shift+V
repair. The pulled macOS harness fixes were reviewed on Linux before main tagging.

## Final acceptance criteria

| Plan criterion | Result and evidence |
| --- | --- |
| 1 — Executable Must/Should/NFR coverage and test-forward evidence | PASS: final CI compiles/tests the full suite; strict coverage resolves 191 cases across 91 requirements and rejects missing/ignored/compiled-out/stub cases. FR-066 is the sole specification-authorized exclusion. Task evidence records red/green or unchanged extraction characterization; Task 004's historical-log qualification and retrospective probes remain disclosed in `014-review.md`. |
| 2 — Complete owned public host API and core independence | PASS: Tasks 003/005/007/008/012/013/014; exact facade/signature/privacy/dependency guards, empty/multiple-tab hosts, owned input/frame/metadata, prepared operations and ordered correlated events. The independently vendored public consumer passes at the actual published tag. |
| 3 — Policy, suspension, stale identity/version and no-data-loss matrix | PASS: Tasks 002/009/011; equal-metadata replacements, missing files, stale decisions, denied bang/force, symlink revalidation, aborted/dropped/foreign tokens, all-target atomic retarget, undo retention and pre/post-replacement failure cases. Portable noncooperating-writer CAS remains explicitly out of scope. |
| 4 — Standalone/public-host parity and intentional exceptions | PASS: Tasks 001/010/012/014/015; real terminal replay/cell/cursor traces and unchanged snapshots cover four modes and existing routing/policies. README native Linux smoke verifies Shift+V, selection/clipboard, metadata/save/quit. Only DRD safety/disk/theme behavior and approved Shift+V intentionally differ. |
| 5 — Unchanged absolute and same-host performance limits | PASS: final debug/release gates; five exact-main trials per 15 cases (75/75) and 60/60 metrics against the approved preserved same-host original baseline. Owned-frame <=1 ms, bounded analysis/input/idle and two-second per-backed-tab polling are measured/guarded. See `015-main-tag.md`, active TSVs and final measurements below. |
| 6 — Full canonical quality/platform/docs/notice gates | PASS: all five fresh final commands below, seven standard checks, strict coverage, independent/provenance/negative checks and native Linux PTY/manual smoke. macOS is PASS explicitly accepted by the user, not independently observed execution; see platform qualification below. Makefile/CI discoverability and bundled-data/notice guards pass. |
| 7 — Independent offline/locked integration and accurate documentation | PASS: Tasks 013/014/015; candidate and actual-tag consumers own manifest/lock/vendor/config/Cargo home, assert main plus all four patch revisions, compile warning-free and exercise behavior. Real provenance/checksum negatives pass. AGENTS/README/CONTRIBUTING/CHANGELOG/DEVELOPER and runnable byte-identical integration snippet are guarded/exercised. |
| 8 — 0.6.0 exact-main tag publication and independent consumption | PASS: `015.md` and `015-publication.md`. User separately authorized local tagging, then approved pushing only v0.6.0 and final checks. Published annotated/peeled objects and GitHub main match the gated SHA, reverified at 17:58 UTC. Fresh actual-GitHub-tag consumer passes offline/locked without a local source rewrite. No archive occurred. |

## Final quality gate

Ran every `gate.json -> final_commands` entry in order after final review, without
source changes between commands. All observed process exit codes were 0; no
warning, error or unresolved advisory appeared in the final logs.

| Command | Result | Durable diagnostic log |
| --- | --- | --- |
| `make ci` | PASS, exit 0 | `target/workflow-verification/final-ci.log` |
| `make build-examples` | PASS, exit 0 | `target/workflow-verification/final-examples.log` |
| `make doc` | PASS, exit 0; warnings fatal | `target/workflow-verification/final-doc.log` |
| `make bench-check` | PASS, exit 0 | `target/workflow-verification/final-bench-check.log` |
| `make bench` | PASS, exit 0 | `target/workflow-verification/final-bench.log` |

CI itself also completes release build, the seven-check standard gate, strict
191-case coverage, examples, docs and asserting debug/release performance.
The separate post-publication/post-reconnect `make check` and newly prepared
published-tag consumer passed before final review; `015.md` records their logs.
All recorded final commands are actual executions, not inferred from earlier CI.

Selected measurements from the last separate complete release benchmark:

| Measurement | Observed | Unchanged limit |
| --- | --- | --- |
| 1 MiB source first-frame, worst | 39.57 ms | <150 ms |
| 1 MiB rendered layout, worst | 160.62 ms | <250 ms |
| 1 MiB retained rendered layout | 46,869,472 bytes | <=64 MiB |
| Additional dirty-diagnostic time, worst | 0 microseconds; both raw durations nonzero | <100 microseconds |
| 64-document analysis, worst | 649.12 microseconds | <10 ms |
| Retained analysis capacity | 6,572 bytes | <16 KiB |
| 200x60 owned-frame conversion p95 | 545.82 microseconds | <=1 ms |

All 15 standalone TUI performance cases pass. Exact committed performance TSV
checks remain 75/75 rows at the final SHA and 60/60 comparisons; SHA-256 values
reverified after reconnect: candidate
`43f13d479688b0a7a86ebea6f59800613e77f9809c0bdf2f512bfcf78670a49d`, comparison
`bc50ca8190301f2f6590569c36789e3c87dbc7072299345847a1144634264286`.
Earlier candidate artifacts are preserved under revision-specific filenames.

## Cumulative diff

Baseline to final main: 132 tracked files, 57,294 insertions and 34,966 deletions,
dominated by checked-in generated parser tables. Substantive changes cover the
owned facade, reusable disk/analysis core, pane/render/lifecycle/watcher/input
boundaries, config/theme/terminal services, standalone migration, registries,
examples, API/property/integration/fault/coverage/performance guards, independent
consumer and make/CI tooling, documentation/notices and 0.6.0 manifests/lock.
Tracked worktree is clean; cumulative `git diff --check` passes. Finalization
only adds ignored workflow evidence/state/logs, not another source commit.

## Remaining non-blocking concerns

- macOS PASS is the user's explicit disposition for the reviewed source tree.
  Architecture, native command logs and manual smoke details were not supplied;
  no native Mac run or run on the squash SHA is fabricated. The old candidate
  and new main have the identical tree, but are not an ancestor-preserving merge.
- Task 004's original red log was lost; later real reconstructed failure probes
  are accurately labeled retrospective. Earlier /tmp logs/fixtures disappeared
  during the machine disconnect. Historical compact results remain preserved;
  the interrupted gate and actual published-tag consumer were rerun successfully.
- The documented portable filesystem race against a noncooperating writer is
  unchanged. Version revalidation is not represented as cross-process CAS.

No unresolved implementation, performance, publication or authority blocker.
Completed work remains active/unarchived; archive requires the user's explicit
`$feature-workflow archive` instruction.
