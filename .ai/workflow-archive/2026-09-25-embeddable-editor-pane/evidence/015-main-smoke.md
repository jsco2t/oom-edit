# Native Linux README-key smoke — main release target

Target `87d5b48f766eb35f30c2136e23d2a4448329713b`, clean tracked tree on main.
Linux x86_64, actual 80×24 PTY, TERM=xterm-256color, 2026-09-27 16:43–16:44 UTC.
Invocation: `TERM=xterm-256color make run-isolated ARGS=/tmp/oom-edit-task015-main-smoke.md`.
Disposable initial file: `Alpha **bold** text.\n`.

| README key | Actual observation | Result |
| --- | --- | --- |
| i | INSERT/source delimiters/bar cursor; typing X changes the buffer | PASS |
| Esc | NORMAL/rendered bold/block cursor | PASS |
| v | SELECT, one rendered character highlighted, underscore cursor | PASS |
| V | SELECT, full rendered line highlighted; second V cancels to NORMAL | PASS |
| Ctrl-V | Byte 0x16 enters block SELECT; Esc cancels | PASS |
| y | `0wvey` emits OSC 52 `Kipib2xkKio=` = exact `**bold**` | PASS |
| Y | `0wveY` emits OSC 52 `Ym9sZA==` = plain `bold` | PASS |
| : | :CMD prompt and bar cursor; Esc returns NORMAL | PASS |
| Space h | Registry command palette including V row; Esc closes it | PASS |
| Space w | Saved message; actual disk bytes `XAlpha **bold** text.\n` | PASS |
| Space m | Metadata card; after save exact `---\ntitle: ""\n---\n\nXAlpha **bold** text.\n` | PASS |
| Space q | Exit 0; cursor, paste, mouse, synchronized-update and alternate-screen teardown sequences emitted | PASS |

Intermediate and final bytes inspected with `od -An -tx1c`. OSC 52 emission
verified, not desktop clipboard acknowledgement. No personal documents/config
were touched. Native automated PTY and user-accepted macOS evidence are separate.
