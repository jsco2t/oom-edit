# Terminal input and guard

`TerminalGuard` is process-exclusive. `TerminalGuard::new()` uses library
defaults: raw mode, alternate screen, mouse capture and bracketed paste, with
keyboard enhancement and focus reporting off. Standalone and the split example
enable `TerminalGuardOptions::focus_change` with `with_options`, acquiring
focus-loss/focus-gain reports for their terminal session. An embedding host may
choose mouse, paste, keyboard enhancement and focus reporting, and may change
mouse capture through `set_mouse_capture` while its guard remains active. A second live guard returns
`TerminalError::Busy`. Dropping the guard restores only acquired features;
partial setup, panic, SIGHUP and SIGTERM use the same acquisition record.

Focus reporting uses the terminal's focus protocol and requires terminal or
multiplexer support. Hosts route delivered reports through the existing pane
focus API: sample the timestamp after reading an event, call `tick(now)`, then
`set_focused(false)` or `set_focused(true)`, and drain typed pane events. Enabled
focus autosave considers modified named tabs only on a true focus-loss
transition. Periodic autosave uses the same pane timer independently of focus
reports; hosts continue ticking hidden panes. See [autosave configuration and
safety behavior](../README.md#autosave).

After a panic restores the terminal, that guard cannot toggle mouse capture.

Keyboard enhancement requests the kitty protocol's escape disambiguation and
event-kind reporting only when the terminal reports support. Unsupported
terminals remain on the legacy path; no keyboard pop is sent unless a push was
acquired. The standalone defaults deliberately do not request enhancement.

The TUI translates a terminal key to the core's `KeyInput` once, at its input
boundary. Legacy terminals cannot distinguish these pairs, so the enhanced
representations are normalized to the same logical key:

| Terminal keys | Core key |
| --- | --- |
| Esc / Ctrl-[ | Esc |
| Tab / Ctrl-i | Tab |
| Enter / Ctrl-m | Enter |
| Backspace / Ctrl-h | Backspace |
| BackTab / Shift-Tab | BackTab |

Other Ctrl, Alt and Shift modifiers are preserved. Only press events dispatch
editor actions; repeat and release reports are ignored. Hosts should use the
same rules when forwarding terminal-neutral `KeyInput` values to a pane. The
executable vector inventory is
[`terminal_keys.tsv`](../crates/oom-edit/tests/data/terminal_keys.tsv), exercised
by the standalone translator test and available for embedding examples.

Run `make terminal-guard-pty-test` for the native PTY ownership, termios,
panic, signal, mouse and keyboard-enhancement checks. `make test` includes the
same tests.

## Host shortcut ownership

`EditorPane::key_ownership(key)` is a read-only projection of the active routing
state. It distinguishes App commands, the core grammar, pending sequences,
text entry, exclusive modals, frozen lifecycle targets and unclaimed inputs.
Consumed does not mean a named command ran: an ignored key in an active grammar
still belongs to that grammar. Use the existing core `KeyInput` model unchanged.

`EditorPane::host_reservation(key)` identifies optional outer-host families
(Alt characters, Ctrl-g and plain F1). It never changes pane dispatch. A host
that uses a global shortcut must deliberately intercept it before forwarding
input; otherwise Alt characters retain their existing insertion behavior, and
pending/text/modal contexts can consume Ctrl-g or function keys.

`hints()` exports complete ordered cells, including compact text and disabled
state; an overlay supplies its existing bespoke hint as one cell. `which_key()`
exports the focused pending Space prefix and ordered key/label entries after
150 ms of host-supplied time. Tick the pane even when not drawing it. These
exports are independent of the inline-hint option. `status()` is the active
editor-line snapshot (or none for an empty pane), with one-based source ruler,
path, dirty/new-file, spell, active-search counts, prompt and disk-marker data.

`bindings()` projects the single registry with stable IDs, modes, concrete core
key sequences, variable patterns or Ex grammar, and explicit execution roles.
Executable core describes the existing session dispatcher, not a palette
action. Reference-only metadata never creates an executable App command; an Ex
reference can prefill the existing prompt and still requires user submission.
Consolidated reference presentations retain the standalone palette's exact
wording and ordering. Run `make test-pane-bindings` for the public dispatch
matrix, registry guards, every overlay and unchanged terminal snapshots.

The core session owns active-search occurrence counts (`SearchStatus`) and
`clear_search()`. `EditorPane::status()` exposes the same optional summary.
`Space c` is an App-owned chord dispatched through the canonical registry to
that session operation; `:noh` uses the same operation through the core ex grammar.
Prompts and overlays retain exclusive input ownership. No search is active for
an empty draft, and cancelling a draft restores the previously submitted search.

For source Insert navigation, a core-only host can call
`EditorSession::set_source_viewport(text_width, wrap)` before dispatching input.
`render_source` supplies the same geometry. Width excludes gutters and host
chrome; the pane supplies its effective width and configured wrap limit before
key dispatch, including input received before a paint. Unmodified Up/Down use
visual source rows while wrapping is enabled, and logical lines otherwise.
Horizontal positioning, edits and width changes re-anchor the desired column.

Spelling becomes due after two seconds without input, using the pane's
post-read event timestamp. Hosts use `tick(now).idle_due` to decide when to
call `idle_unit`, check for ready input between 4 KiB work units, and stop the
idle drain at the 8 ms slice budget. New input restarts the idle deadline;
text changes cancel the current document scan through the core mutation
boundary. A completed scan replaces its window atomically. Valid spelling
corrections publish updated diagnostic counts before returning to the host.
