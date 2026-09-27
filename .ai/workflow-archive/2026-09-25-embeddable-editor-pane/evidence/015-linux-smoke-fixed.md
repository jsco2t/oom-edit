# Native Linux README-key smoke — Shift+V candidate

Candidate `6682ea4d6b8c4541aa0626d7dfc39daaef1bfd40`, clean tracked tree.
Linux x86_64; actual interactive 80×24 PTY, TERM=xterm-256color, 2026-09-27
14:41–14:42 UTC. Invocation:
`TERM=xterm-256color make run-isolated ARGS=/tmp/oom-edit-task015-shift-v-smoke.md`.
Disposable initial file: `Alpha **bold** text.\n`. Observations below were made
from actual terminal output and disk-byte inspection, not headless replay.

| README key | Actual observation | Result |
| --- | --- | --- |
| i | INSERT/source delimiters/bar cursor; typing X changes the buffer | PASS |
| Esc | NORMAL/rendered bold/block cursor | PASS |
| v | SELECT, one rendered character highlighted, underscore cursor | PASS |
| V | SELECT, whole rendered source line highlighted; v switches to character, V switches back to line, second V cancels to NORMAL | PASS |
| Ctrl-V | Byte 0x16 enters block SELECT with underscore cursor; Esc cancels | PASS |
| y | `0wvey` emits OSC 52 `Kipib2xkKio=` = exact `**bold**` | PASS |
| Y | `0wveY` emits OSC 52 `Ym9sZA==` = plain `bold` | PASS |
| : | :CMD prompt and bar cursor; Esc returns NORMAL | PASS |
| Space h | Registry command palette, including V row; Esc closes it | PASS |
| Space w | Saved message; actual disk bytes `XAlpha **bold** text.\n` | PASS |
| Space m | Metadata card; after save exact `---\ntitle: ""\n---\n\nXAlpha **bold** text.\n` | PASS |
| Space q | Exit 0 with cursor, paste, mouse, synchronized-update and alternate-screen teardown | PASS |

Final and intermediate disk bytes inspected using `od -An -tx1c`. OSC 52
emission is verified, not a desktop clipboard acknowledgement. No personal
document/config was touched. This actual shifted-uppercase byte now exercises
the defect path; historical failing smoke remains in `015-linux-smoke.md`.
Native automated PTY verification and macOS verification are separate gates.
