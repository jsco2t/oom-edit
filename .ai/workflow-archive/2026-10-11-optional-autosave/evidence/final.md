# Final Work-Package Evidence

## Work package

- ID: `2026-10-11-optional-autosave`
- Title: Optional autosave
- Plan revision: 3; acceptance round: 2; final attempt: 1
- Baseline: `1960ade52a394a7c23fe400fdbb395283834ab72`
- Explicit approval: `$feature-workflow approve` for Round 2.

## Original objective

Deliver independently optional saves every five minutes and on actual focus loss, disabled by default, using the existing safe atomic lifecycle. Round 2 additionally restores baseline editing performance while preserving the saved incremental-highlighting correctness repair. No performance allowance, fixture or property coverage is weakened.

## Completed tasks

| Task | Implementer / attempts | Evidence |
| --- | --- | --- |
| 001 — Implement optional autosave | main / 2 | [001](001.md) |
| 002 — Verify and document autosave | main / 1 | [002](002.md) |
| 003 — Fix saved highlighting mismatch | main / 1 | [003](003.md) |
| 004 — Restore line-start performance | main / 2 | [004](004.md) |
| 005 — Enforce baseline performance acceptance | main / 1 | [005](005.md) |

Tasks 001–003 and [Round 1 final evidence](final-round-001.md) are immutable historical records. Round 2 supersedes the earlier assertion that the conservative parser fallback sufficiently preserved performance. It does not erase the original checks or failures.

## Whole-package review

The primary thread reviewed the cumulative diff from the recorded baseline, new private modules absent from git diff, the request, full plan/task sequence and all task evidence. Review covered config and exact facade guards, injected scheduling, captured targets, noninteractive lifecycle failures, both hosts, terminal rollback/panic/signal restoration, public mode/undo/serialization behavior, syntax ownership/work/cache handling, comparator admission/arithmetic/output, documentation and Make ownership.

Automatic writes still enter only App::execute_lifecycle with the current path, force=false and StayOpen. There is no second text owner, writer, command dispatcher or core terminal dependency. Disabled scheduling adds constant timer/focus checks; enabled saves retain synchronous atomic durability work. The ownership repair changes only the private representation passed to Tree::edit: a proven ASCII prefix insertion overlaps its following unchanged real letter. The original logical edit still updates live text and line indexes. Unsupported structural/global/context cases retain conservative processing.

No high-confidence implementation finding remains. The P1 performance finding in [the local review](round-002-review.md) is closed by the complete fresh measurements below. Final review did not introduce speculative refactors or scope expansion.

## High-confidence findings fixed

- Task 003's blanket line-start veto caused document-wide parsing/reference/injection work and severe editing regression. Task 004 removes that veto together with anchored tree-coordinate ownership, preserving the exact Setext/fresh-equivalence fix. Deterministic tests prove source styling and derived data, endpoint/sequence correctness, unchanged scan counts and remote cache ownership at 65,535/65,536/65,537 bytes and 1 MiB.
- The broad public-pane absolute budget admitted the rejected 75–81% character-edit slowdown. Task 005 adds a compatible baseline-relative gate, using the existing authoritative inventory and absolute budgets. The retained rejected records intentionally fail eight input/total comparisons; all 96 rows are retained in [the negative control](round-002-interactions-rejection-check.tsv).
- The first Task 004 standard attempt reported five Clippy errors in range fixtures and sandbox-denied advisory locks. Fixture syntax was corrected without changing assertions or suppressing lints, and every task validation and the full gate were repeated with already-authorized advisory-cache access. This was not a performance-gate failure.

## Performance acceptance

All runs used unchanged fixtures with both autosave triggers disabled, serially on the same host/toolchain. The original baseline checkout remained clean at the exact recorded revision. Because candidate HEAD is still the baseline commit, [provenance](round-002-provenance.json) records every changed candidate file hash and aggregate source digest `9b7159425f07ea60cc19c95c490d5e35c45e667cadf94a27563a5b6f1cc5349a`. Source files and immutable history were revalidated unchanged after the final gate. Artifact hashes are recorded there too.

The full unchanged release benchmark passed three serial executions in this round. Final observations: 1 MiB edit-at-start average **0.29509 ms**, worst **0.44093 ms** against the unchanged <10 ms worst limit; spelling 4 KiB builder **1.55425 ms** against <2 ms; owned 200x60 frame conversion **0.514229 ms p95** against <=1 ms. Every core, spelling and TUI absolute/scaling budget passed. [Release observations](round-002-release-observations.txt) retain the useful raw metrics. The previously observed separate baseline spelling miss did not repeat in the three complete candidate runs; no spelling source or limit was changed.

### Fresh native TUI comparison

Both records contain 75 passing observations (15 cases × five trials). All **60 comparison rows pass**, including candidate-only guards. [Baseline](round-002-tui-baseline.tsv), [candidate](round-002-tui-candidate.tsv), [complete comparison](round-002-tui-comparison.tsv).

| Edit-frame metric | Baseline median | Candidate median |
| --- | ---: | ---: |
| Wall time | 4.604628 ms | 4.600801 ms |
| CPU time | 5.842327 ms | 5.820236 ms |
| Peak RSS | 25,071,616 bytes | 25,083,904 bytes |

The 12,288-byte RSS difference passes both existing comparison allowances. The prior 328% wall/474% CPU/22% RSS regressions are absent.

### Fresh public-pane comparison

Both complete **2,310-sample** records pass all existing absolute budgets. All **96 relative metric rows pass**, with no missing, reclassified, duplicate or discarded samples. Cold returns remain five samples per position and warmed returns 100. [Baseline package copy](round-002-interactions-baseline.jsonl), [candidate](round-002-interactions-candidate.jsonl), [complete comparison](round-002-interactions-comparison.tsv). The baseline package copy is byte-identical to the exact final-gate input `/tmp/oom-autosave-round-002-interactions-baseline.jsonl`.

| Character operation | Baseline median total | Candidate median total | Growth |
| --- | ---: | ---: | ---: |
| Top insert | 4.751354 ms | 4.704442 ms | -0.99% |
| Top delete | 4.732667 ms | 4.679553 ms | -1.12% |
| Middle insert | 5.337654 ms | 5.283699 ms | -1.01% |
| Middle delete | 5.324810 ms | 5.262638 ms | -1.17% |

Maximum median total-time growth across all case/step/phase groups is **3.52%**. Every input/render/total group satisfies the >10% AND >100,000 ns rule; every RSS group satisfies the >5% AND >1,048,576-byte rule. These results establish no regression under the approved acceptance allowances on the measured workloads. They do not claim zero cost for enabled filesystem durability writes.

## Final acceptance criteria

### Original feature

- [x] Missing/old/partial configuration disables autosave; all four combinations and invalid types are tested; live updates report exact owned fields without persistence/reconstruction.
  - Evidence: config_public, pane_autosave live configuration/focus cases, public facade compile guards and full final check.
- [x] No periodic write before 300 seconds; first eligible expiry saves all modified named/background/unfocused tabs and filters clean/unnamed buffers.
  - Evidence: `periodic_saves_all_named_dirty_tabs_at_boundary_including_unfocused_insert`, exact disk bytes and once-only TabId/event assertions.
- [x] Late ticks, disabling/re-enabling, unchanged config and manual/focus saves preserve cadence without bursts or duplicate writes.
  - Evidence: private scheduler test; live config, combined-trigger and manual/focus cadence public cases.
- [x] Focus saving occurs only on a real focused-to-unfocused transition; settings operate independently/together; repeated loss/gain/tab switches do not save.
  - Evidence: independent-switch, live-focus and Select/tab-switch tests.
- [x] Both hosts route post-read timestamps through the pane; requested focus reporting restores once through rollback, drop, panic and signals.
  - Evidence: standalone/embedded host tests, exact acquired-bit guard tests and final native terminal PTY gate.
- [x] Modes, source cursor, selection, suspended native grammar and undo/redo are preserved; subsequent Insert edits become dirty.
  - Evidence: periodic Insert, Select/undo and exclusive-prompt tests; four-mode conformance suite.
- [x] Automatic conflicts/deletion do not overwrite/recreate or open confirmations; denied/failed/uncertain writes remain dirty with typed feedback while independent tabs still save.
  - Evidence: mixed failure fixture; BeforeReplace/ConfirmDurability injection, pre-rename external-change and competing new-file creation cases.
- [x] Prompts, overlays, grammars, prepared actions and external-change leases block interference; skipped triggers follow the documented no-queue retry policy.
  - Evidence: executable prompt ownership assertions and lifecycle-token/manual-confirmation cases.
- [x] Serialization, captured TabIds, active/MRU identity and correlated once-only events are preserved; new named files can save, unnamed buffers remain intact.
  - Evidence: CRLF/final-newline/new-file and multi-tab public cases plus disk/document/lifecycle suites.
- [x] Manual workflows, four modes, registries, API privacy, dependency direction and no-data-loss guards pass.
  - Evidence: full workspace tests, public API, compiled coverage, examples and dependency/package guards.
- [x] README/DEVELOPER document independent options, cadence, safety/skips, terminal limitations and host duties.
  - Evidence: verbatim-parsed README combinations, compiled developer consumer and reviewed terminal/host docs.
- [x] Every final quality command passes; dependencies, lockfile, vendor and advisory policy remain unchanged.
  - Evidence: table below, provenance/diff and warning-fatal deny/audit/data-license checks.

### Approved highlighting extension and Round 2

- [x] Both original regressions, saved seed and unchanged 150-case property suite preserve complete fresh equivalence, including reverse/redo and Unicode.
  - Evidence: seven deterministic regressions, full syntax/property and final workspace tests.
- [x] Source ownership/byte/Point endpoints, source styles and exclusion/reference/injection data remain correct through sequential edits.
  - Evidence: direct endpoint and full differential tests in task 004; original regressions unchanged.
- [x] Safe byte-zero/top/middle prefixes avoid additional document-wide scans and retain caches; unsafe cases retain conservative handling.
  - Evidence: path/counter/cache identity assertions around the 64 KiB boundary and at 1 MiB; structural/context exclusions and broader incremental/fence tests.
- [x] No global fresh-parser fallback, dependency/public API expansion, removed property case or weakened benchmark is introduced by the repair.
  - Evidence: private helper review, unchanged budgets/case inventory and full API/hygiene gates.
- [x] Every unchanged core/spelling/TUI/frame release budget passes.
  - Evidence: full make bench and retained release observations.
- [x] All native five-trial comparisons pass, including edit wall/CPU/RSS and unchanged candidate-only guards.
  - Evidence: 150 native observations and 60 passing comparison rows above.
- [x] Both public runs pass absolute limits and all relative groups; top/middle character slowdown is eliminated.
  - Evidence: 4,620 observations and 96 passing comparison rows above.
- [x] Comparison contracts reject real regression and invalid evidence, preserve failed output and run via make test/check.
  - Evidence: ten contract tests, both Make entry points, eight-failure retained real-data negative control.
- [x] Complete autosave safety/state/timing/event behavior remains intact.
  - Evidence: eleven public/three private focused cases plus all original host/config/terminal/lifecycle tests in the final gate.
- [x] Every original and strengthened command passes without warnings, omissions, hidden failure or deferred implementation.
  - Evidence: exact gate result record and table; first final attempt passed all 13 commands.
- [x] Fresh raw comparisons close the performance review finding and preserve completed-round history.
  - Evidence: linked artifacts and verified immutable-history hashes in provenance.

## Final quality gate

| Command | Result |
| --- | --- |
| `make check` | PASS |
| `make test-public-api` | PASS |
| `make build-examples` | PASS |
| `make doc` | PASS |
| `make coverage-check` | PASS |
| `make terminal-guard-pty-test` | PASS |
| `make bench` | PASS |
| `make tui-perf-record BRANCH_ROLE=candidate TRIALS=5 OUTPUT=.ai/workflow/evidence/round-002-tui-candidate.tsv` | PASS |
| `make tui-perf-record PERF_ROOT=/tmp/oom-autosave-perf-baseline-20261012 BRANCH_ROLE=baseline TRIALS=5 OUTPUT=.ai/workflow/evidence/round-002-tui-baseline.tsv` | PASS |
| `make tui-perf-compare BASELINE=.ai/workflow/evidence/round-002-tui-baseline.tsv CANDIDATE=.ai/workflow/evidence/round-002-tui-candidate.tsv OUTPUT=.ai/workflow/evidence/round-002-tui-comparison.tsv` | PASS |
| `make bench-interactions OUTPUT=.ai/workflow/evidence/round-002-interactions-candidate.jsonl` | PASS |
| `make -C /tmp/oom-autosave-perf-baseline-20261012 bench-interactions OUTPUT=/tmp/oom-autosave-round-002-interactions-baseline.jsonl` | PASS |
| `make interaction-perf-compare BASELINE=/tmp/oom-autosave-round-002-interactions-baseline.jsonl CANDIDATE=.ai/workflow/evidence/round-002-interactions-candidate.jsonl OUTPUT=.ai/workflow/evidence/round-002-interactions-comparison.tsv` | PASS |

[Exact command results](round-002-final-gate-results-attempt-001.json) record sequential execution, exit codes and diagnostic log paths. make check reports seven passed, zero failed. Advisory access was previously authorized; no exception or warning was accepted. The intentional negative control and first task Clippy/sandbox failure remain explicitly recorded, not reclassified as passing observations.

## Cumulative diff

The uncommitted package adds owned autosave config; private clock/focus orchestration through the existing atomic lifecycle; host focus routing and acquired-terminal cleanup; behavior/API/documentation tests; the anchored private syntax ownership repair and persisted regression; strict public-pane comparison tooling, contracts, Make targets and performance documentation. No dependency, lockfile/vendor, mode, command registry, grammar upgrade, commit, publication or archive occurred. The unrelated pre-existing `.codex/config.toml` change remains untouched and excluded from candidate source identity.

## Remaining non-blocking concerns

Focus saves depend on delivered terminal/multiplexer/host focus reports. Enabled saves retain the documented synchronous atomic durability cost. These approved limitations remain; no blocking concern or unfinished implementation remains. The package remains active and awaits an explicit human archive or another acceptance revision.
