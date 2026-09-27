# Task 015 macOS portability change — Linux review

Scope: `6682ea4d6b8c4541aa0626d7dfc39daaef1bfd40` to
`45b687c3e80399ddd33efd371629e09d92af6c96`, the single pulled commit
"Make verification harnesses portable on macOS". Eleven files, 167 insertions,
29 deletions. Tracked worktree initially clean. Review-only user request; no
source fixes, commits, publication or plan amendments are authorized here.
Approved revision 5 hash re-verifies as
`c750cc88e70cccb9392b9c74aab93d75da004b86adf29cd74c51cd3a74033a9b`.
Tasks 001–014 and their evidence remain immutable.

## Main-thread local code review

Code-reviewer routes to comp-reviewomatic local mode, threshold 80, executed
in the main thread under feature-workflow's no-reviewer-agent rule. Applied
all nine perspectives: API/schema, architecture/abstraction, conventions/docs,
infrastructure, integration/deployment, language, operability, security/data
protection and systems correctness. Critical findings: 0; Important findings: 0.
No issues found above the confidence threshold.

The Rust changes in App and public-pane suites only canonicalize temporary
roots before creating test directories. Surrounding assertions still check
exact bytes, stable identities, canonical symlink deduplication, outside-root
policy rejection, retarget rollback, launch-directory resolution and disk state.
This aligns fixtures with existing production canonicalization without relaxing
the oracle or changing production path semantics.

Generator production code already resolves output paths. The existing test's
expected command now does likewise; a real symlinked output fixture also checks
guarded generated parser bytes. ABI/version, header restoration and extension
isolation assertions remain intact. No parser or vendored bytes changed.

PTY snapshots now use the master before and after the child exits, avoiding a
revoked slave descriptor on Darwin. The loop exits only when the child has
exited and the current read has no payload, preserving output drainage and the
existing timeout. Darwin-only comparison masks just PENDIN; Linux compares every
bit. The Rust Darwin test rejects ICANON, ECHO and VMIN changes. Four new Python
cases use real subprocesses for restored/unrestored raw mode and exercise all
termios fields under the narrow Darwin mask, with other platforms unchanged.
Intentional signal/panic child exits remain checked by existing scenario and
escape-count assertions, not misclassified as harness failure.

The Darwin normalization is consistent with Apple's primary XNU source:
[tty.c](https://github.com/apple-oss-distributions/xnu/blob/main/bsd/kern/tty.c#L1313)
sets PENDIN on a transition into ICANON and carries it into the resulting flags
at line 1345. This supports the narrow comparison adjustment; it does not prove
the particular Mac's execution or substitute for user acceptance below.

The new Python suite has a discoverable make target and runs through make test,
make check, the dedicated PTY target and make ci's check stage. Existing CI
continues to invoke make. No new dependency, public API, unsafe block, production
editing branch, performance threshold or golden change is introduced.

## macOS acceptance supplied by the user

Initial report: "The MacOS gate is clean." Asked whether make ci, native PTY
and README manual smoke ran on this SHA and for logs/platform details, the user
answered: "No - I don't have that info. Please consider the gate as passing."

Record macOS verification as PASS — explicitly accepted by the user. No Mac
runner, architecture, command exit statuses, raw logs or independently observed
manual smoke evidence were supplied. Do not fabricate those details or claim
the main thread ran native macOS tests. This removes the macOS acceptance
blocker at the user's direction, not via a weaker automated Linux oracle.
Separate publication authority is still not given by either message.

## Linux verification

Full exact-SHA CI, native PTY, isolated five-trial performance comparison,
symlinked-temporary-root public-pane checks and fresh independent consumer
verification are recorded separately in `015-linux-portability-verification.md`
once each finishes. Prior candidate measurements were preserved byte-for-byte
as `perf-release-6682ea4-candidate.tsv` and
`perf-release-6682ea4-comparison.tsv` before regenerating active output paths.
This review report alone is not a passing gate or release completion.
