# Optional autosave

## Objective

Add two independently configurable autosave triggers to oom-edit: every five minutes and on loss of editor focus. Both default to disabled and can be enabled together. Save modified named tabs through the existing lifecycle and atomic document-save protocol, preserving unsaved work when a write cannot safely complete.

This document is a proposed plan awaiting explicit approval, not authorization to implement.

## Current behavior

- Saving is explicit through Space w, ex commands, or lifecycle requests. All workflows converge on `App::execute_lifecycle` and its save handler (`crates/oom-edit/src/app.rs:2399`, `:2761`).
- The timer (`app.rs:1059`) handles presentation deadlines and disk polling. It performs no periodic save. `EditorPane::tick` exposes the next deadline to hosts (`pane.rs:1227`); hosts must advance it even while the pane is hidden.
- `EditorPane::set_focused` is the existing focus authority (`pane.rs:1204`). Focus loss suspends native modal state and cancels App Space chords/dragging (`app.rs:1296`).
- The standalone event dispatcher and adapter ignore terminal focus events (`event.rs:285`, `standalone.rs:71`). The split example already routes them (`examples/support/embedded_host.rs:201`).
- The vendored crossterm supports focus reporting with EnableFocusChange/DisableFocusChange and FocusLost/FocusGained (`vendor/crossterm/src/event.rs:377`). TerminalGuard does not currently acquire or restore this feature.
- Current manual save handling opens an overwrite/recreation confirmation on externally modified or missing files. That presentation must not be reused automatically when focus is lost.
- The core reads authoritative live text and captures the save baseline only after a confirmed save (`crates/oom-edit-core/src/session.rs:2883`). A committed but uncertain write remains dirty. No second text store is needed.
- Config is host-owned, serializable, and live-updatable through `EditorPane::apply_config`; implementation modules remain private behind curated crate-root exports.
- The initial working tree contains a pre-existing change to `.codex/config.toml`; it is unrelated to this package. Baseline HEAD is recorded in state.json. No local project-context file was present.

## Proposed implementation

### Configuration

Add a project-owned public `AutosaveConfig` in the existing config module and export it at the crate root. Add a defaulted `Config.autosave` subtable:

```toml
[autosave]
every_five_minutes = false
on_focus_loss = false
```

The switches are independent preferences, so off, periodic only, focus only, and both are valid. Use serde defaults for omitted tables/fields; wrong field types follow the existing malformed-config fallback. Do not add a configurable interval, command binding, or dependency. Extend ConfigField and ConfigApplyReport with the exact settings, and make apply_config apply both now without persistence or resource reconstruction.

### Periodic timing

App owns a private autosave scheduler because this is pane lifecycle orchestration; the core's text and save APIs already provide the editing behavior needed. Represent the timer as a closed enum, such as Disabled or Scheduled { next_due }, rather than a parallel enabled flag and optional deadline.

- Use the injected monotonic host clock, never a new internal wall-clock sample.
- The first interval expires 300 seconds after construction or enabling periodic autosave.
- At expiry, consider all modified named tabs, including inactive tabs and tabs in an unfocused pane. Hosts must continue ticking.
- After an expiry, schedule the next interval for the observed host time plus 300 seconds. A delayed tick performs one cycle, with no catch-up burst.
- Manual saves and focus-triggered saves do not reset this cadence. Unchanged tabs are filtered out, preventing redundant writes.
- Unrelated config updates and reapplying the same config retain the current deadline. Disabling removes it; re-enabling creates a fresh deadline from App's most recently supplied time.
- Include the deadline in the existing PaneTick calculation. Disabled autosave contributes no new deadline.

### Focus loss

Trigger exactly once on an actual focused-to-unfocused transition in the existing App focus setter. Calling set_focused(false) repeatedly, gaining focus, or switching the active tab inside a focused pane does not trigger a save. Consider all modified named tabs. Preserve mode, source cursor, selection, native pending input, text, and undo history.

Standalone routes FocusLost/FocusGained through EditorPane, supplying the event's timestamp via tick before changing focus. Maintain the same ordering in the split example. Do not introduce a second input model or focus owner. In an embedded host, set_focused(false) also covers moving focus to surrounding host UI.

Add `focus_change` to owned TerminalGuardOptions, default false for library hosts. Standalone and the interactive split example opt into reporting. Reporting remains acquired for the terminal session, independent of whether autosave is enabled, so live configuration changes can use already available focus reports. Add typed setup failure handling and an acquired-feature bit. Enable and disable through the guard, restoring in reverse acquisition order on normal exit, setup failure, panic, and Unix signals. Update exact option/escape assertions and all affected struct literals.

Terminal focus detection is best effort: terminals/multiplexers must emit and forward the focus protocol. A host cannot infer an event its terminal never reports. Document that limitation and the periodic option as an alternative.

### Safe save execution and user feedback

Use a private typed save origin (manual versus automatic with its trigger), or an equivalent closed request variant, to give automatic saves noninteractive handling inside the existing lifecycle executor. Existing manual requests retain their confirmation behavior. Do not introduce another writer or call session/document saves from the scheduler, public pane, or host adapters.

- Filter clean and unnamed buffers before dispatch. A named new-file buffer may be created on its first autosave under existing new-file semantics. An existing file deleted after opening must not be silently recreated.
- Each candidate uses its captured pane-generation TabId and is resolved to the current target index before dispatch; never save a subsequently selected tab by accident.
- Automatic requests always use the current document path, force=false, and StayOpen. No retarget, save-copy, close, or save-path prompt is permitted.
- Require the lifecycle to be Idle and no overlay, ex/search prompt, or pending input grammar before initiating a cycle. A blocked trigger is skipped, with a warning when there is modified named work; no queued action acts on a later identity. A periodic skip advances to the next five-minute trigger. A focus skip retries only on a later real focus-loss transition or a periodic trigger. Document this policy explicitly.
- Ordinary Insert editing and rendered Select are allowed when there is no exclusive prompt or pending grammar. Saving must not inject Escape, split an edit unexpectedly, or discard selection/undo state.
- Preserve FileAccessPolicy checks, atomic write/fsync/rename, serialization, external-version checks, and committed-uncertain behavior.
- Conflicts, missing formerly existing files, policy denials, and I/O failures leave work dirty, emit existing correlated typed failure/write-committed events as appropriate, and show a warning/error. Automatic conflicts never open an overwrite or recreation overlay or acknowledge a changed disk version.
- Process remaining independent candidates after one candidate fails. Successful saves emit existing Saved events; no new public autosave command/event executor is necessary.
- Batch feedback identifies autosave and affected failures. A later success must not hide a failure in the same cycle. Include textual status so the signal is not color-only. Repeated ticks before the next trigger do not repeat attempts or notices.
- When periodic and focus triggers coincide, the first successful save clears dirty state; the second cannot rewrite that clean tab. Tick/set_focused-created events must be drainable exactly once through the existing pane event path.

## Architectural decisions

1. Core remains independent of terminal events, focus, App, and clocks. Preserve its existing atomic save behavior; change core only if focused tests demonstrate a genuine defect needed for autosave correctness.
2. App owns scheduling and lifecycle orchestration; hosts own terminal acquisition, focus/event translation, and clock sampling. All writes still enter App::execute_lifecycle.
3. Use current Config, ConfigApplyReport, PaneTick, set_focused, and PaneEvent surfaces. Add only deliberate owned config/guard API. Update exact public API, privacy, and dependency guards in the same package.
4. No new mutable text copy, generic background service, asynchronous write task, registry, command binding, plugin, or dependency.
5. Do not weaken existing manual confirmation or external-change protections to make automatic writes succeed.
6. Configuration booleans describe independent user preferences; runtime timer/request state uses closed enums. Target identity is captured before action.

## Work included

- Both optional triggers, live config, exact scheduling, all-tab scope, named-new-file handling, and unchanged-tab filtering.
- Noninteractive automatic save handling with safe conflicts, failures, host transactions, and accessible notices.
- Standalone and split-host focus wiring; terminal acquisition, rollback, panic, and signal cleanup.
- Config/default/roundtrip/type-error tests and compile-time public API/privacy guards.
- Deterministic time/focus tests using injected Instants, never real five-minute sleeps.
- File-backed public pane tests for exact on-disk bytes and dirty state; mixed successful/failed batches, correct TabIds/events, and prompt/transaction exclusivity.
- Mode, cursor, selection, undo/redo, CRLF/final-newline, manual save, disk reconciliation, and focus suspension regressions.
- Documentation of settings, cadence, named/unnamed behavior, blocked triggers, focus limitations, host obligations, and failure recovery through explicit save commands.

## Task sequence

1. `tasks/001-implement-optional-autosave.md` — complete implementation, focused behavior tests, and required boundary/terminal guard updates.
2. `tasks/002-verify-and-document-autosave.md` — complete external-consumer acceptance coverage, host/documentation contracts, and integrated verification.

Tasks execute in order in the primary thread. No product code changes occur before approval.

## Quality gate

Standard completion gate for each task: `make check`. This is the actual repository CI gate and includes fmt-check, warning-fatal lint, build, the entire test suite, deny, audit, and bundled-data license checks. It satisfies all listed definition-of-done checks. Supply-chain warnings remain failures under project policy even if a tool exits zero.

Final package gate, in order:

1. `make check`
2. `make test-public-api`
3. `make build-examples`
4. `make doc`
5. `make coverage-check`
6. `make terminal-guard-pty-test`

These add explicit facade/privacy checks, all example consumers, warning-free public docs, the existing compiled requirement inventory, and terminal restoration/signal evidence. All commands already exist in Makefile. No new developer workflow command is needed. Any additional focused target introduced during implementation must be registered in Makefile and help in the same change. No validation category has been omitted. Plan creation does not claim implementation gates have passed.

## Risks

- Terminal/multiplexer focus forwarding varies; support is conditional on actual delivered reports. Test dispatch deterministically and document the protocol requirement.
- Adding Config/TerminalGuardOptions fields and ConfigField/error variants affects exhaustive downstream construction/matches. Update in-tree fixtures and exact public facade tests; document additive API changes.
- Saves are synchronous: a cycle over many large dirty tabs can delay an event loop. Reuse the current protocol and avoid extra text copies, per-frame writes, or catch-up loops. Do not introduce concurrency solely to avoid the existing save cost.
- Automatic conflicts must not create overlays, resolve pending manual confirmations, or acknowledge unseen external versions. Pre-write races must still be rejected by core checks.
- Clearing dirty state while Insert has an active undo transaction must keep subsequent changes dirty and undo/redo baselines correct. Test before accepting the implementation.
- External-change leases and prepared close/retarget tokens suspend document I/O; timer/focus triggers must not interfere with them.
- A committed-uncertain save may have replaced disk contents without confirmed durability. Retain the core's dirty/error semantics and typed committed event; never report it as a confirmed successful save.
- Skipped busy triggers give a longer unsaved window; the explicit policy is visible in docs and approval criteria.

## Out of scope

Recovery/swap files, configurable intervals, per-tab settings, idle debounce, saving on tab switch or application exit, cloud/network sync, CLI switches, palette commands, filesystem transactions performed by the host, publishing, and unrelated .codex settings. Archives from earlier packages are not inputs to this plan.

## Final acceptance criteria

- [ ] Old/missing config disables autosave; every setting combination parses and roundtrips. Live changes report exact ConfigFields and take effect without rebuilding a pane or writing config.
- [ ] A modified named tab is unchanged on disk at 299.999 seconds and saved at the first eligible tick at/after 300 seconds. All modified named tabs, including background/unfocused ones, participate; clean/unnamed tabs cause no save writes.
- [ ] Late ticks produce one cycle and a new deadline 300 seconds later. Disable/re-enable, unchanged config, manual saves, and focus-triggered saves follow the specified cadence without duplicate writes.
- [ ] A real focus-loss transition saves eligible modified named tabs when enabled; repeated loss, focus gain, and in-pane tab switches do not. Both settings work independently and together.
- [ ] Standalone focus events and embedded focus transitions reach the same public pane path with the host-sampled timestamp. Focus reporting is acquired/restored exactly once where requested, including rollback, panic, and signals.
- [ ] Autosave changes disk/dirty state without changing mode, cursor, selections, native pending-input suspension, or undo/redo behavior. Subsequent Insert edits become dirty again.
- [ ] Automatic external-change/missing-file failures never overwrite/recreate or open an automatic confirmation. Policy denials, I/O failures, and committed-uncertain outcomes retain correct dirty state and visible feedback; a mixed batch still saves other eligible files.
- [ ] Prompts, overlays, pending grammars, prepared lifecycle actions, and external-change leases prevent autosave interference. Skipped triggers follow the documented retry policy and never loop every tick.
- [ ] Saves preserve exact existing serialization policy, target TabIds, active-tab identity, and correlated event delivery. Named never-created files can receive their first safe autosave; unnamed text remains intact.
- [ ] Manual save/close/reload behavior and existing four-mode, registry, public API, dependency, and no-data-loss guards continue to pass.
- [ ] README and DEVELOPER document both settings, all safety/skip semantics, focus-reporting limitations, and host time/focus responsibilities.
- [ ] Every final quality-gate command passes; no dependency or vendor changes are needed.

## Approved scope extension — 2026-10-12

The user explicitly approved repairing the existing core syntax defect documented in evidence/final-attempt-001.md. This supersedes the original exclusion of unrelated fixes only for that defect. Completed tasks 001/002 and their evidence remain immutable; the package, baseline, acceptance round and final gate remain unchanged.

### Finding and approach

The persisted proptest case compares incremental source highlighting after sequential edits against a fresh Highlighter. It reports a start-column mismatch on line 0. Core source was unchanged by autosave. Highlighter::apply_edit owns block parsing and injection invalidation/rebasing; inspect those paths, reproduce and minimize the exact saved sequence, then repair the responsible path conservatively without disabling incremental processing globally. Retain the persisted seed and add a readable deterministic regression that compares complete StyledLine values at every edit.

### Additional task

3. tasks/003-fix-incremental-highlighting.md — diagnose and fix the saved incremental/fresh highlighting mismatch; run focused highlighting and autosave regressions, the standard gate, then all original final commands. No other core feature, dependency or terminal behavior is added.

### Additional acceptance criteria

- [ ] The persisted failure and a readable minimized regression produce identical complete incremental/fresh highlighting through every edit, including boundary and reverse transitions where relevant.
- [ ] The responsible invalidation/parser decision is corrected without a global fallback, new dependency, public API change or weakened equivalence/property test.
- [ ] Highlighting/performance-path regressions and autosave tests pass; all original package acceptance criteria and final gates pass.

### Risks

Conservative invalidation must preserve ordinary bounded/incremental performance and injection ownership. A randomized passing rerun alone does not resolve a persisted failure. The existing final retry count/history remains recorded.

## Acceptance follow-up — Round 2

### Objective and supersession

Restore established editing performance while retaining complete incremental/fresh highlighting equality and the implemented autosave behavior. Round 1's final evidence records its actual checks, but its claim that optimized paths were sufficiently preserved is superseded by the release measurements in evidence/round-002-review.md. Passing correctness tests or broad absolute latency limits alone is insufficient for this round.

The package identity and baseline remain unchanged. Tasks 001–003, their evidence and evidence/final-round-001.md are immutable completed history. Only tasks 004–005 belong to this acceptance round. This section requires explicit re-approval before product changes.

### Observed acceptance failure

The approved syntax repair rejects every zero-width line-start edit from the block-neutral proof. The 1 MiB core edit-at-start benchmark grows from 0.579 ms average / 0.879 ms worst to 11.28 ms average / 25.04 ms worst, failing its unchanged 10 ms worst limit. TUI edit-frame median wall time grows from 5.958 to 25.523 ms; CPU and peak RSS also fail the existing native comparison. Public-pane character insert/delete medians at the top and middle grow 75–81%, even though the broader 50 ms p99 gate passes. Both autosave switches are disabled in these measurements.

### Repository findings and fix strategy

1. Highlighter::apply_edit first attempts retained fence and bounded source paths. bounded_source_region rejects offsets below four, front-matter overlap and edits with possible global semantics. The blanket line-start veto therefore causes document-wide parser/reference/injection work at byte zero and more expensive processing at other line starts.
2. The original defect is real: a pure insertion at an owning node's start can make Tree::edit move that node beyond the inserted bytes. Tree shape equality does not prove correct coordinates. Reverting to the original shortcut without correcting ownership is unacceptable.
3. The pinned vendored Tree-sitter implementation, vendor/tree-sitter/src/subtree.c:633–784, distinguishes pure insertion from replacement. An edit ending within subtree padding shifts the subtree; an edit overlapping its content resizes it. At a shared boundary, a pure insertion is assigned to the first touching child. This supports a narrow coordinate repair using an equivalent replacement anchor.
4. For the already-supported ASCII word-only, block-neutral prefix insertion, represent the tree-coordinate edit as replacing the following existing ASCII character with the inserted prefix plus that unchanged character. The logical text operation remains the original insertion through the existing mutation gateway. The equivalent anchored tree edit overlaps the owning content instead of moving it past the prefix. Calculate byte and Point endpoints consistently; keep this helper private and apply it only after the existing lexical/context/global-semantics checks succeed.
5. Remove the blanket veto only together with that ownership repair and complete differential tests. Other edits retain their current canonical/bounded paths; do not change general parser policy or rendered provenance. If the anchored operation cannot preserve the tested ownership/semantics, stop for a revised design rather than silently broadening document-wide fallback or weakening correctness.
6. Add deterministic tests that distinguish actual fast work from merely matching output: safe prose and Setext prefix edits must avoid extra document-wide block parses, reference scans and injection rediscovery, while structural/global edits keep their conservative behavior. Exercise byte zero and line starts in large documents through insert/delete/undo/redo; retain source caches and all original correctness regressions.

### Architecture and test boundaries

- Keep the sole LiveDocument mutation path and authoritative text; the anchor is a private tree-coordinate representation of the identical edit, not a second document or dispatcher.
- Keep parser types private, terminal dependencies out of core, and all autosave configuration, scheduling, lifecycle, I/O safety, focus and host contracts intact.
- Prove complete StyledLine equality after each transition, including both Setext levels, multiline headings, Unicode surroundings, inline constructs, adjacent blocks and reversed ranges. Compare affected spelling exclusion/reference/injection data where the optimized path owns it. Preserve the saved seed and property case count.
- Use existing counters and region bounds for deterministic work assertions, including document sizes around 64 KiB and 1 MiB. Keep elapsed-time assertions in the existing release harnesses.
- Extend existing performance tooling with a small stdlib comparator for public-pane raw JSONL. Reuse the authoritative fixture, case, cycle and absolute gate definitions from interaction_performance.py; do not maintain a duplicate inventory. Add discoverable make targets and contract tests in make test.
- The comparator checks the complete 12-case, five-trial, 21-cycle inventory. Cold and warmed mode returns remain separate. For each group, fail median input/render/total growth only when it exceeds both 10% and 100 microseconds; fail median process peak RSS growth only when it exceeds both 5% and 1 MiB. These use the established native TUI comparison allowances and add a stricter acceptance check without relaxing any existing absolute limit.
- Validate fixture identity, geometry, modes, positive coherent timing fields, phase labels and exact trial/cycle/step completeness before comparing. Missing, duplicated, reclassified or mismatched evidence is a failure. Preserve raw rows and comparison output on a measured regression and exit nonzero.

### New task sequence

4. tasks/004-restore-line-start-performance.md — repair tree-coordinate ownership while restoring the established cheap edit path; prove correctness, work bounds and unchanged release budgets.
5. tasks/005-enforce-performance-acceptance.md — add the public-pane comparison gate and contract tests, record fresh baseline/candidate evidence and complete integrated acceptance.

Both tasks are main-only. No worker or reviewer agent is required.

### Quality gate additions

Keep standard_commands as make check. Retain every original final command. Add make bench, the existing five-trial TUI recording/comparison targets, both existing public-pane interaction gates, and the new interaction-perf-compare target introduced by task 005. The exact commands are in gate.json. Their baseline revision is the original baseline_sha, not the rejected working tree.

Use the existing clean isolated checkout /tmp/oom-autosave-perf-baseline-20261012 after verifying its exact revision and clean worktree. If absent, recreate it with the existing tui-perf-baseline-prepare target. Never reset or clean the user's active checkout. Baseline binaries may be rebuilt; baseline sources must remain unchanged. Run timing-sensitive builds and benchmarks sequentially on the same host/toolchain. If machine metadata changes, recollect both sides rather than mixing evidence.

New measurement files use round-002-* names; original round-002-rejected-* acceptance records are immutable. Record source provenance separately because an uncommitted candidate and baseline can report the same HEAD. Keep failed raw measurements and explain any reruns; do not select favorable samples, filter tails, change fixtures or raise thresholds.

### Round-specific risks and boundaries

An equivalent tree edit must retain exact UTF-8/Point endpoints, sequential edit coordinates, adjacent-node attribution and cache ownership. A correct-looking first viewport can hide errors elsewhere, so complete small-document differential cases and multiple large-document viewports are required. Initial-character anchors must be real unchanged source characters admitted by the existing lexical proof; empty/unsupported boundaries use the established safe path.

The baseline's separate unchanged oom-spell 4 KiB builder step missed its existing 2 ms limit at 2.113484 ms. The final candidate must still pass the full make bench command. Diagnose any repeated miss under serial measurements and preserve evidence. A reproducible unrelated spelling defect requires a scope decision before changing that subsystem; never skip it, relax the gate or mark this round DONE with a failure.

Out of scope: new autosave triggers or intervals, async save architecture, dependency/grammar upgrades, general parser redesign, unrelated source refactors, destructive filesystem operations, commits, publication and archival. Actual autosave durability I/O remains synchronous and is documented separately from normal edit-path latency. No claim of cost-free filesystem writes is part of acceptance.

### Round 2 acceptance criteria

- [ ] Both existing deterministic highlighting regressions, the saved property seed and complete incremental/fresh comparisons pass without removed cases or reduced coverage.
- [ ] Correct ownership and endpoints are preserved after anchored insertion and its reverse/redo, including source styles, affected exclusion/reference/injection data and sequential edits.
- [ ] Safe line-start word edits at byte zero, top and middle use the restored inexpensive path without extra document-wide parser/reference/injection work. Structural/global edits retain their existing safe handling.
- [ ] All unchanged make bench release budgets pass, including the 10 ms core edit worst limit, TUI absolute limits and 1 ms owned-frame conversion gate.
- [ ] All existing native five-trial TUI baseline comparisons pass, including edit-frame wall, CPU and RSS; candidate-only absolute/scaling guards remain unchanged.
- [ ] Both 2,310-sample public-pane runs pass existing absolute budgets, and every group passes the new relative comparison. Top/middle character edits must no longer retain the observed 75–81% slowdown.
- [ ] Public-pane comparison contracts reject measured regressions and incomplete/mismatched data, preserve failed output, use the existing inventory and run through make test/check.
- [ ] Autosave timing, focus transitions, all-tab scope, mode/undo preservation, nonforce atomic/conflict behavior and correlated events remain covered and pass.
- [ ] Every original final gate and the strengthened gate pass without warnings, weakened budgets, omitted commands, hidden benchmark failures or deferred work.
- [ ] Final evidence links fresh raw comparisons and closes the performance review finding; original completed-round history and rejected measurements remain intact.
