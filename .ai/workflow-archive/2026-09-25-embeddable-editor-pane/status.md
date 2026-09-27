# Work-Package Status

Work ID: `2026-09-25-embeddable-editor-pane`

Title: `Embeddable Editor Pane and Standalone Parity`

Phase: `DONE`

Plan revision: `5`

Acceptance round: `1`

Prior completed rounds: `0`

Approved: yes

Current task: none — all 15 tasks complete; final gate attempt 1 passed

Task retry limit: `3` attempts; final-gate retry limit: `2` attempts

## Tasks

- [x] 001 — Characterization and coverage baseline — COMPLETE — attempt 2
- [x] 002 — Versioned core disk state and safe reload — COMPLETE — attempt 1
- [x] 003 — Lightweight read-only Markdown analysis — COMPLETE — attempt 1
- [x] 004 — Public configuration and safe persistence — COMPLETE — attempt 1
- [x] 005 — Public theme catalog, styles and notices — COMPLETE — attempt 2
- [x] 006 — Configurable public terminal guard — COMPLETE — attempt 1
- [x] 007 — Pane-local owned rendering — COMPLETE — attempt 1
- [x] 008 — EditorPane construction, input and tab protocol — main — 1 attempt
- [x] 009 — Policy-checked prepared lifecycle — COMPLETE — attempt 1
- [x] 010 — Standalone binary on the public pane — COMPLETE — main — attempt 2
- [x] 011 — Disk watching and safe-point reconciliation — COMPLETE — main — attempt 1
- [x] 012 — Structured binding registry and host hints — COMPLETE — main — attempt 1
- [x] 013 — Embedding example and independent consumer — COMPLETE — main — attempt 1
- [x] 014 — Integrated documentation, guards and performance proof — COMPLETE — main — attempt 1
- [x] 015 — Version 0.6.0 release candidate and authorized tag — COMPLETE — main — attempt 1

## Package quality gate

Fresh exact-main-SHA release checks pass: full CI/check, 191-case/91-requirement
coverage, examples/docs/debug/release benchmarks, four Python/seven native PTY
cases, actual README smoke, 75/75 performance trials, 60/60 comparisons and
independent candidate/provenance/negative checks. Local annotated v0.6.0 is
created at main and now published under explicit user approval. A new consumer
has fetched the actual GitHub tag without a local source rewrite; its exact-tag
check and the post-publication standard gate both passed after reconnect.
Task 015 is complete with `evidence/015.md`. Final integrated package review
has no findings above confidence 80; `evidence/final-review.md` records all nine
review perspectives. All unchanged final commands passed in order, attempt 1:
make ci, make build-examples, make doc, make bench-check and make bench.
`evidence/final.md` maps all eight package acceptance criteria, qualifications
and final results. Logs live under `target/workflow-verification/`.

## Current blocker

None. The user explicitly answered "Approved" to pushing only v0.6.0, followed
by published-tag verification and final workflow gates. The approved tag-only
push succeeded with push.followTags=false. Remote tag object and peeled main
commit match locally. No branch or other tag was pushed; no force update.

All 15 tasks are COMPLETE. Final immutable 0.6.0 release:
`87d5b48f766eb35f30c2136e23d2a4448329713b`; clean tracked worktree on main.
GitHub main and the published v0.6.0 tag were reverified at 17:58 UTC. This is a
squash merge, not an ancestor-preserving merge of 45b687c. The tree is exactly
identical (`e8f2d6b86ff5aae444f18844273b6e587a54e983`), so prior source review
and the user's explicit macOS PASS acceptance remain applicable to the same
contents; no native Mac results on the new SHA are fabricated. Fresh Linux
execution and independent provenance checks target the new main SHA.

Current release evidence: `evidence/015.md`, `evidence/015-main-tag.md`,
`evidence/015-main-smoke.md` and `evidence/015-publication.md`. Historical candidate,
source review, failing/red-green Shift+V and previous passing gates remain
preserved. The 45b687c performance outputs were archived byte-for-byte before
active paths were regenerated. Frozen approved revision-5 hash is unchanged.
Task 015 is COMPLETE, attempt 1; final_attempts is 1. Final package acceptance
and release publication are complete, with macOS PASS accepted by the user.
Local tag object is `9dfd4deeb546ce5336ea8a627b1c5011e2a4f71e`, peeled exactly
to main and matches the published remote object.

Completed at: `2026-09-27T18:01:30Z`

Final SHA: `87d5b48f766eb35f30c2136e23d2a4448329713b`

Archived at: `2026-09-27T18:08:29Z`

Archive status: archived under the user's explicit `$feature-workflow archive`
instruction. The complete package is preserved in
`.ai/workflow-archive/2026-09-25-embeddable-editor-pane/`; the active workflow slot is empty
after the verified directory move.

After a machine disconnect the old process and /tmp consumer/logs were absent.
The tag/main were reverified remotely; the interrupted standard gate and a new
GitHub-prepared tag consumer both reran successfully. Current logs are under
`target/workflow-verification/`; no prior gate completion is fabricated and no
tag was recreated or repushed. The interruption consumes no failure retry.
