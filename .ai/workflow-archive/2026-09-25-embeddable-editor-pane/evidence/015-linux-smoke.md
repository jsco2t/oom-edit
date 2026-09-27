# Native Linux README-key smoke — candidate ab30fa4

Candidate: `ab30fa474a8cd816e018cbe9c3a5f5577706b2ee`, clean tracked tree.
Linux x86_64, Rust 1.97.1; actual interactive PTY, 80x24. Command:
`TERM=xterm-256color make run-isolated ARGS=/tmp/oom-edit-task015-smoke.md`.
Disposable initial file: `Alpha **bold** text.\n`. No repository or personal
document was modified. Observed 2026-09-27 13:52–13:56 UTC.

| README key | Actual observation | Result |
| --- | --- | --- |
| `i` | INSERT label, source `**bold**` delimiters and bar cursor; typing X changes the buffer | PASS |
| `Esc` | NORMAL label, rendered bold text and block cursor | PASS |
| `v` | SELECT label, selected rendered character and underscore cursor | PASS |
| `V` | Repeated uppercase V leaves NORMAL unchanged; no line selection | FAIL |
| `Ctrl-V` | Byte 0x16 enters SELECT, selected character and underscore cursor | PASS |
| `y` in Select | `0wve` selects bold; emitted OSC 52 payload `Kipib2xkKio=` decodes to exact `**bold**` | PASS |
| `Y` in Select | Same selection emits `Ym9sZA==`, exact syntax-free `bold` | PASS |
| `:` | `:CMD` label, input prompt and bar cursor | PASS |
| `Space h` | Command Palette displays registry commands, including V; Escape closes it | PASS |
| `Space w` | Saved message; actual disk bytes become `XAlpha **bold** text.\n` | PASS |
| `Space m` | Metadata card appears; saved bytes have exact `---\ntitle: ""\n---\n\n` prefix and unchanged body | PASS |
| `Space q` | Exit 0; cursor/paste/mouse/synchronized-update/alternate-screen teardown emitted | PASS |

OSC 52 emission was inspected, not treated as a terminal/desktop clipboard
acknowledgement. Final file bytes were verified using `od`, including every
newline. `make terminal-guard-pty-test` separately passes native automated
termios, singleton, partial-acquisition, panic, signals and keyboard scenarios
in `/tmp/oom-edit-task015-linux-pty.log`.

## Line-Select failure and parity boundary

Vendored crossterm `char_code_to_event` adds Shift for uppercase characters
(`vendor/crossterm/src/event/sys/unix/parse.rs:129`). Translation preserves that
modifier. Core rendered Normal only handles V inside
`key.mods == Modifiers::default()` (`src/session.rs`); Shift-V therefore falls
through without entering line Select. The original immutable baseline
`eafaa6afa796d0258b72a6e5ab6ca5a3699d600a` has exactly this condition at lines
3019–3025 and preserves Shift in its original App translation at lines
2724–2725. This is a pre-existing defect, confirmed by baseline source, not a
new candidate regression. No baseline PTY run is claimed.

The existing shared vector preserves Shift-I but does not assert Shift-V's
real rendered-mode transition; plain-key headless mode tests do not cover this
terminal report. The mandatory README smoke is FAILED, not waived. Fixing this
pre-existing behavior would amend the plan's strict standalone parity boundary,
so it requires explicit user approval before changing code. No fix is applied
and this candidate must not be published as fully verified.
