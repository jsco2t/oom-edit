# Native Linux README-key smoke — macOS portability candidate

Candidate `45b687c3e80399ddd33efd371629e09d92af6c96`, clean tracked tree.
Linux x86_64; actual interactive 80×24 PTY, TERM=xterm-256color, 2026-09-27,
completed at 16:28 UTC. Invocation:
`TERM=xterm-256color make run-isolated ARGS=/tmp/oom-edit-task015-portability-smoke.md`.
Disposable initial file: `Alpha **bold** text.\n`. Observations came from actual
terminal output and disk-byte inspection, not a headless replay.

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
| Space h | Registry command palette, including V row; Esc closes it | PASS |
| Space w | Saved message; actual disk bytes `XAlpha **bold** text.\n` | PASS |
| Space m | Metadata card; after save exact `---\ntitle: ""\n---\n\nXAlpha **bold** text.\n` | PASS |
| Space q | Exit 0; cursor, paste, mouse, synchronized-update and alternate-screen teardown sequences emitted | PASS |

Intermediate and final disk bytes inspected using `od -An -tx1c`. OSC 52
emission is verified, not desktop clipboard acknowledgement. No personal
document/config was touched. Native automated PTY results are separate.
Historical failing and repaired Shift+V smoke reports remain unchanged.
