# Editor-dependent host requirement reachability

This cross-check covers the editor-dependent clauses of the oom v1 PRD v1.2;
it does not implement oom. Host-owned behavior is not implemented by the editor:
views, global prefixes, pickers, indexing, vault locking, sync, trash storage and
host configuration transactions belong to the consuming application.

All listed operations use crate-root exports, without private imports, mutable
sessions or a second dispatcher. The [split-pane example](../crates/oom-edit/examples/embedded.rs)
demonstrates interception, focus, owned rendering, hints, event draining and safe
close-all. [DEVELOPER.md](../DEVELOPER.md) explains the full protocol.

Executable witness names below refer to these suites:

- **Bindings:** [bindings_public.rs](../crates/oom-edit/tests/bindings_public.rs):
  real dispatch/ownership, pending grammar, hint/status and registry-mutation tests.
- **Host:** [embedding_example.rs](../crates/oom-edit/tests/embedding_example.rs):
  split status/global interception, two-public-host replay, unnamed saves,
  disk notifications, nonzero-origin mouse and compact frames.
- **Pane:** [pane_public.rs](../crates/oom-edit/tests/pane_public.rs): canonical
  open/focus, exact MRU/events, construction isolation, live config and public
  tab-bar/empty-state/cursor matrix.
- **Lifecycle:** [pane_lifecycle.rs](../crates/oom-edit/tests/pane_lifecycle.rs):
  captured close, cancelled third tab, batch retarget, dropped/aborted/foreign
  tokens, policy races and external leases.
- **Disk:** [pane_disk.rs](../crates/oom-edit/tests/pane_disk.rs) and the
  [injected safe-point matrix](../crates/oom-edit/src/app/disk_watch/tests.rs).
- **Config/themes:** [config_public.rs](../crates/oom-edit/tests/config_public.rs)
  and [theme_public.rs](../crates/oom-edit/tests/theme_public.rs).
- **Guard:** [terminal_guard.rs tests](../crates/oom-edit/tests/terminal_guard.rs):
  native PTY restore, runtime mouse and enhancement acquisition/restore.
- **Facade:** [public_api.rs](../crates/oom-edit/tests/public_api.rs): exact
  exports/signatures, supplemented by rustdoc compile-fail privacy cases and
  the independently compiled consumer.

| PRD clause | Public surface and host responsibility | Executable witnesses |
| --- | --- | --- |
| FR-004 | `hints()`, `which_key()`, `status()` feed host chrome; `inline_hints = false` prevents competing hint rows. | Bindings parity; Host split status/which-key |
| FR-005 | Host intercepts Alt globals before `handle_input` in every mode; reservations describe intentional overrides. | Bindings modified/context matrix; Host global key |
| FR-006 | Host owns Ctrl-g grammar, timeout and cancellation; do not replay unknown outer continuations. `host_reservation` identifies exact Ctrl-g. | Bindings pending/text/modal ownership |
| FR-007 | Intercept only host reservations; forward other `KeyInput` unchanged. `key_ownership`/`input_state` expose editor ownership. | Bindings precedence/noop matrix; shared key vectors |
| FR-008 | Host registry owns outer help; `bindings()` adds structured editor roles/sequences. Reference-only rows never become executable commands. | Bindings registry mutations/ex roles; Host hints |
| FR-009 | `tabs()` supplies title, optional absolute path, dirty/active state and MRU rank. Host filters/sorts picker and calls `focus_tab`. | Pane ordered/idempotent focus/MRU and unnamed metadata |
| FR-010 | Host dismisses its layers. `set_focused(false)` preserves editor mode/prompts/overlays without injecting Escape or switching views. | Pane focus suspension and retained decisions |
| FR-011 | `close_all()` works from any host view. Host presents `AttentionRequired` targets and save-path requests before deciding to exit. | Lifecycle captured order/cancellation; Host safe close-all |
| FR-012 | Share `ThemeCatalog`, explicit `ThemeSelection` and owned styles. Optional `ThemeEnvironment` derives NO_COLOR/TERM policy at host boundary. | Config/themes exact order, precedence and monochrome roles |
| FR-013 | `bindings()`, `host_reservation`, `key_ownership` support collision validation; host owns binding config/rejection/reporting. | Bindings exact registry and behavioral ownership negatives |
| FR-014 | Host routes list/view mouse; editor receives local `PaneMouse`. `TerminalGuard::set_mouse_capture` changes acquisition once per transition. | Guard exact mouse bytes; Host origin/focus |
| FR-047 | `open_existing` canonically opens or focuses, retaining existing-tab state. | Pane canonical symlink/dedup and cursor options |
| FR-063 | Browser uses the same public open-or-focus path and host containment policy as search. | Pane canonical open and typed denial |
| FR-064 | One `EditorPane` with Embedded command policy and explicit services; standalone uses that same implementation. | Host two-public-host replay; standalone parity; Facade |
| FR-065 | Host draws focus title/border with text/modifiers and calls `set_focused` for Alt/Tab/click. Pane cursor disappears when unfocused. | Host split focus; Pane cursor/layout matrix |
| FR-066 | Browser exclusions/read-only explanations are host-only. Never call `open` for unsupported content; file policy can refuse it. | Lifecycle denials preserve sentinel bytes |
| FR-067 | `ActiveTabChanged` and `tabs()` identify active path; host expands/reveals browser ancestors under its own setting. | Pane exact open/focus events and MRU |
| FR-068 | Browser find/filter state is host-owned; suspend editor input with `set_focused(false)` while browser query owns focus. | Pane focus gating and native pending suspension |
| FR-069 | Zero initial paths and `PaneOptions::empty_state_lines` display host guidance without an invented file. | Pane public empty-state matrix; Host zero/tiny frames |
| FR-082 | `EditorPane::prepare_retarget`, commit/abort capture all affected IDs. Host moves only after preparation; commit validates moved versions, preserving text/undo/conflicts. | Lifecycle batch success, final-validation failure, modified-source follow/policy races |
| FR-083 | `EditorPane::prepare_close` settles dirty decisions without closing. Host performs unique recoverable trash move, then commits; filesystem failure aborts. | Lifecycle prepared barrier, abort/drop and cancelled third tab |
| FR-084 | Host validates subtree/counts/attachments/symlinks and owns recoverable filesystem work. Batch close/retarget captures descendants; abort on failure. | Lifecycle all-or-none capture and frozen-target barriers |
| FR-125 | Host validates/persists its own effective vault settings before `set_theme`/`apply_config`. Reports distinguish live/new-pane fields; host theme changes never call editor persistence. | Pane live config without persistence; Config change reports |
| FR-147 | `EditorPane::begin_external_change` suspends document I/O/polling during sync. Commit supplies affected paths; safe-point/version-bound handling follows final reconciliation. | Lifecycle external lease; Disk full safe-point/fault/stale-choice matrix |
| FR-162 | Host validates/locks destination, settles workers, prepares old close and new services before activation. Abort retains buffers; recreated pane rejects old-generation IDs. | Lifecycle retained preparations; Pane drop/recreate and silent construction |
| FR-164 | Deserialize `Config` from host file. `PaneInit` never discovers standalone config, argv or process environment. | Pane isolated subprocess; Config host subtable roundtrip |
| FR-165 | Use public serde Config/validation with explicit host theme/dictionary roots and selection; no copied schema or OOM_EDIT_THEME read in pane construction. | Config exact fallback/path/presence; Pane isolation |
| FR-166 | Supply active-vault `ThemePersistenceSink` and personal dictionary filename. Space-t/Space-a use these; runtime ex settings remain session-only. | Pane theme events; sink failure and dictionary-root tests |

`make test-package-guards` guards this inventory. Editor witnesses run through
`make test`; strict coverage checks compiled locations. This is an API
reachability proof, not a claim that host-only features or native macOS release
verification have already been implemented or executed.
