# v0.5.0 characterization baseline

Baseline commit: `eafaa6afa796d0258b72a6e5ab6ca5a3699d600a`

Host: Linux x86_64, AMD Ryzen 7 5700U, 6 visible logical CPUs, Rust 1.97.1. Performance TSV: `evidence/perf-baseline.tsv`.

## Functional oracle

The unchanged editor suite passed under `make test`. The following existing exact tests and 26 TUI snapshot goldens are the migration oracle; Task 001 added two command-policy characterization tests before production changes.

| Surface | Baseline assertion |
| --- | --- |
| Four modes and editing | Core `conformance` (43 tests), `session_integration` (82 tests), `app_handle_event_enters_insert`, `app_handle_event_escapes_to_normal`, `app_plain_v_enters_select`, `app_dispatches_every_select_binding_in_select_context`. |
| README key table | `app_space_h_opens_palette`, `app_space_w_saves`, `app_space_q_quits`, `front_matter_command_dispatches_only_in_normal_and_reports_results`, `ctrl_v_enters_block_selection`, `select_default_and_explicit_system_yanks_emit_exact_payload_once`, `select_plain_text_yank_preserves_register_publication_rules`. |
| Tabs and quit | `standalone_tabnew_keeps_duplicate_file_tabs`, `standalone_wqa_is_not_an_ex_command`, `quit_all_dirty_refuses_and_force_discards_all`, `dirty_close_cancel_preserves_target_and_dirty_state`, `dirty_last_tab_discard_sets_should_quit`, `confirmation_modal_blocks_tab_mutation_until_resolution`. |
| Input and time | `app_forwards_g_and_numeric_prefixes_unchanged_to_core`, `app_which_key_delay_gate`, `queued_resize_burst_dispatches_only_the_final_dimensions`, `event_timestamp_is_sampled_after_read_before_dispatch`, `spell_idle_delay_is_five_seconds_and_resets_after_each_input`. |
| Render and status | `golden_which_key_space`, `golden_*` in `snapshot_tests.rs`, the 26 files under `crates/oom-edit/tests/snapshots/`, plus mode cursor-shape and redraw scheduler tests in `event.rs`. |
| Clipboard, spell, front matter | Existing App clipboard/suggestion/trouble tests, core spell integration and front-matter panel/byte-range tests. |
| API and architecture | TUI root exports `Args`, `ParseOutcome`, `run`; `tests/public_api.rs` checks the exact `pub use`; core private-module and dependency hygiene tests pass. Exactly one executable `#[allow(unsafe_code)]` attribute exists, in `terminal_guard.rs`'s signal module. |

All existing golden files are preserved. Any later golden change requires a direct explanation against the DRD's explicit behavior changes.

## Performance oracle

`make bench-check` passed. The five-trial release recorder retained every measurement. All cases except the two existing first-frame absolute limits passed on this host. The five-trial median source first frame was about 246 ms (limit <150 ms); rendered first frame was about 654 ms (limit <350 ms). Those ten failing trial rows remain marked `fail-absolute-limit` in the TSV. The baseline recorder returns success so its evidence can be compared; candidate recording, `make bench`, and final acceptance still require passing absolute limits. No limit was raised.

This baseline is tied to this machine's metadata. Candidate measurements must be made on the same host/toolchain and must meet both the unchanged absolute limits and the existing comparison thresholds. If another host is used for release gating, capture a fresh v0.5.0 baseline on that same host.
