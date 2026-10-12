# Final Work-Package Evidence

## Work package

- ID: `2026-10-11-optional-autosave`
- Title: Optional autosave
- Plan revision: 2; acceptance round: 1
- Approved extension: the user explicitly approved repairing the existing incremental-highlighting defect exposed by the first final gate.

## Original objective

Add optional saves every five minutes and on focus loss. Both settings are independently selectable and disabled by default. Preserve the existing atomic-save, external-change and no-data-loss protocols. The user confirmed `.ai/workflow` as the documentation location.

## Completed tasks

1. Implement optional autosave — [task 001 evidence](001.md), main, 2 attempts.
2. Verify and document autosave — [task 002 evidence](002.md), main, 1 attempt.
3. Fix incremental highlighting mismatch — [task 003 evidence](003.md), main, 1 attempt.

## Whole-package review

Reviewed configuration, scheduler, captured identities, save origins, lifecycle/event flow, both hosts, terminal acquisition/restoration, public boundaries, documentation and regression coverage as one change. Cumulative tracked changes were reviewed against baseline HEAD, and new private source/test modules were inspected explicitly because untracked files are absent from git diff.

Automatic writes use only App::execute_lifecycle, the current document path, force=false and StayOpen. The scheduler owns no document text or independent writer. Timers use injected host Instants; focus uses the existing pane authority. Errors preserve dirty text, emit existing typed results and remain visible after independent successes. Exclusive prompts/transactions skip a cycle without retaining queued targets. Ordinary Insert/Select state, undo and serialization are preserved. Public configuration and terminal options are owned types behind the curated facade; the core retains zero terminal dependencies.

The first final gate uncovered a pre-existing source-highlighting mismatch. Its evidence and persisted seed were retained in [final attempt 001](final-attempt-001.md). After explicit user approval, task 003 repaired the responsible boundary decision; all final commands were rerun. The second final attempt passed. No remaining high-confidence finding or incomplete accepted requirement was identified.

## High-confidence findings fixed

- Core block-neutral handling wrongly skipped parsing for a word insertion at the beginning of a Setext title. Tree::edit moved the existing source node past the inserted word, producing a heading span starting at column 3 instead of 0. Line-start insertions now use the existing parser path. A minimized regression and complete original sequence fail on the previous source and pass after the fix; the property test and saved seed remain active.
- During task implementation, corrected test fixture assumptions for rendered search and selection chrome and corrected a warning-fatal initialization pattern. No production behavior or test expectation was weakened to obtain passing checks.

## Final acceptance criteria

- [x] Missing/old config disables autosave; every switch combination parses/roundtrips, and live updates report exact fields without persistence or reconstruction.
  - Evidence: `autosave_defaults_partial_tables_and_all_switch_combinations_roundtrip`; `live_configuration_rearms_without_persistence_and_late_ticks_do_not_catch_up`; `live_focus_setting_can_be_enabled_disabled_and_reenabled_without_saving_immediately`; facade/config checks.
- [x] No save at 299.999 seconds; save at the first eligible tick at/after 300 seconds, covering all modified named background/unfocused tabs and filtering clean/unnamed buffers.
  - Evidence: `periodic_saves_all_named_dirty_tabs_at_boundary_including_unfocused_insert` asserts exact file bytes, identities, modes, event delivery and unchanged unnamed text.
- [x] Late ticks, disable/re-enable, unchanged config and manual/focus saves obey cadence without duplicate writes.
  - Evidence: `schedule_has_exact_boundaries_and_no_catch_up_or_config_reset`; live configuration test; `manual_and_focus_saves_do_not_reset_the_periodic_cadence`.
- [x] Focus saves only on real loss, with independent/combined settings; repeated loss, gain and tab switching do not save.
  - Evidence: `switches_are_independent_focus_is_a_transition_and_combined_triggers_do_not_duplicate`; live focus configuration and Select/tab-switch tests.
- [x] Standalone/embedded routes use the same pane path and sampled timestamp; requested focus reporting restores exactly once through rollback, drop, panic and signals.
  - Evidence: standalone terminal-event test; split-host terminal/mouse transition test; exhaustive setup/restoration unit tests; native normal/panic/SIGHUP/SIGTERM/post-drop guard probes and final PTY gate.
- [x] Mode, cursor, selection, native pending-input suspension and undo/redo remain valid; subsequent Insert edits become dirty.
  - Evidence: periodic Insert test; `focus_preserves_select_cursor_and_undo_redo_and_does_not_save_on_tab_switch`; exclusive-input cases; existing focus and four-mode conformance suites.
- [x] External changes/deletions never automatically overwrite/recreate or open a confirmation; denials, I/O failures and uncertain commits retain dirty work and feedback while independent tabs still save.
  - Evidence: mixed conflict/missing/denied/directory test; private injected BeforeReplace/ConfirmDurability test; pre-rename external-change test; competing named-new-file creation test. Failed and DiskWriteCommitted events are distinguished from confirmed Saved/Completed.
- [x] Prompts, overlays, pending grammars, prepared lifecycle actions and external-change leases prevent interference; skips follow the no-queue policy.
  - Evidence: `exclusive_prompts_and_pending_grammars_skip_without_consuming_state_or_queuing_saves`; `lifecycle_tokens_and_manual_confirmations_block_automatic_io`; actual prompt ownership is asserted before triggers.
- [x] Exact serialization, TabIds, active/MRU identity and correlated once-only events are preserved; new named files can save and unnamed text remains intact.
  - Evidence: CRLF/absent-final-newline/new-file fixture; multi-tab periodic event/cursor/MRU assertions; conflicting new-path test; lifecycle/disk regression suites.
- [x] Manual save/close/reload behavior and four-mode, registry, API, dependency and no-data-loss guards remain valid.
  - Evidence: targeted public pane/lifecycle/disk/API/package checks and complete make-check suite, plus final public API and compiled coverage gates.
- [x] README and DEVELOPER document settings, cadence, scope, safety/skips, terminal limitations and host time/focus/event obligations.
  - Evidence: README autosave section and four verbatim-parsed configuration examples; DEVELOPER config/input/terminal sections; terminal-input guide; unchanged compiled canonical consumer snippet.
- [x] Every final command passes without dependency or vendor changes.
  - Evidence: final gate table; unchanged manifests, Cargo.lock and vendor; deny/audit/data-license checks pass without new exceptions or scanner warnings.

### Approved extension acceptance

- [x] Persisted sequence and readable minimized regression match complete fresh highlighting at every edit, including reversed deletion, reinsertion, both heading levels, multiline titles and Unicode.
  - Evidence: `saved_edit_sequence_matches_fresh_highlighting`; `line_start_insertion_preserves_setext_heading_spans`; original 150-case property test with retained seeds, passing focused/task/final gates.
- [x] Responsible parser decision is corrected without global fallback, new dependency, public API changes or weakened equivalence checks.
  - Evidence: three-line conservative boundary guard and its explanation; existing interior fast-path tests remain unchanged and pass; no parser, dependency or facade additions in the extension.
- [x] Syntax/performance-path/autosave regressions and all original final requirements/gates pass.
  - Evidence: make test-syntax-incremental-regression, make test-first-frame, make test-pane-autosave, task 003 make check and all final commands below.

## Final quality gate

| Command | Result |
| --- | --- |
| `make check` | PASS — fmt-check, lint, build, full tests, deny, audit, data-license-check; 7 passed, 0 failed |
| `make test-public-api` | PASS |
| `make build-examples` | PASS |
| `make doc` | PASS |
| `make coverage-check` | PASS |
| `make terminal-guard-pty-test` | PASS |

Every command ran in the original approved order against the completed implementation. Advisory checks used approved access for their database locks/updates; no supply-chain exception or policy relaxation was needed.

## Cumulative diff

Baseline: `1960ade52a394a7c23fe400fdbb395283834ab72`.

The current working tree adds owned autosave config, private injected scheduling, safe automatic lifecycle origin handling, focus reporting and host routing, once-only event delivery, tests and user/host docs. The approved extension adds the narrow core source-boundary repair, deterministic regressions and its persisted seed. Makefile owns both new focused workflows. Untracked private modules and tests are included in this review. No commits, dependency/vendor changes, command/mode additions or publication occurred. The pre-existing `.codex/config.toml` edit was left untouched.

## Remaining non-blocking concerns

Focus-loss support requires actual reports delivered by the terminal/multiplexer or embedding host. Saves retain the existing synchronous atomic-write cost, which can delay a cycle over many large dirty files. Both are documented approved limitations. No blocking concerns remain.
