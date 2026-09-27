# Task 015 — authorized tag publication

The user explicitly answered "Approved" to pushing only `v0.6.0`, then running
published-tag verification and final workflow gates. This is separate release
authority, not inferred from implementation approval. The tag target is main
commit `87d5b48f766eb35f30c2136e23d2a4448329713b`.

Executed successfully, exit 0:

```sh
git -c push.followTags=false push origin refs/tags/v0.6.0:refs/tags/v0.6.0
```

Result: `[new tag] v0.6.0 -> v0.6.0`. No branch or other tag was pushed, no force
update occurred, and no existing ref was overwritten. The sole push destination
was verified as the canonical repository without printing authenticated URLs.

GitHub remote identity was observed after publication and reverified after the
machine disconnected, at 2026-09-27 17:41 UTC:

| Remote ref | Object |
| --- | --- |
| refs/heads/main | 87d5b48f766eb35f30c2136e23d2a4448329713b |
| refs/tags/v0.6.0 | 9dfd4deeb546ce5336ea8a627b1c5011e2a4f71e |
| refs/tags/v0.6.0 peeled commit | 87d5b48f766eb35f30c2136e23d2a4448329713b |

Local HEAD/main/tag target still match; the tracked tree is clean and the
revision-5 plan hash is unchanged. No repush or tag recreation was performed.

The first fresh GitHub-prepared tag consumer passed warning-fatal Clippy,
offline/locked tests and all five required source-provenance checks before the
disconnect. Its `/tmp` fixture/log and the running standard gate process are
now absent; the latter's completion is not assumed. Both checks are being
rerun. Current logs live under `target/workflow-verification/`, outside the
workflow evidence documents, and compact results will be recorded on completion.
This interruption is not a code/gate failure and does not consume a retry.
Final package acceptance is still pending; no DONE or archival is claimed here.

## Reconnect checks completed

Observed 2026-09-27 17:47 UTC: the full fresh `make check` exited 0 (7 passed,
0 failed); the newly GitHub-prepared published-tag consumer also exited 0.
Warning-fatal Clippy, public behavior and all five required source revisions
pass offline/locked. New fixture:
`/tmp/oom-edit-task015-consumer-published-resume-v060`; its lock selects the
actual `tag=v0.6.0` and exact peeled main SHA, without a local source rewrite.
Durable logs are `target/workflow-verification/post-reconnect-standard.log`,
`published-tag-prepare.log` and `published-tag-check.log`. No warning, error or
unresolved advisory was present. Task acceptance is recorded in `015.md`;
separate whole-package final review/gates still precede DONE.
