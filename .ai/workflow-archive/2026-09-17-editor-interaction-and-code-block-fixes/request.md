# Request — Editor interaction and Go code block fixes

The branch already contains a set of bug fixes. This work package adds the following fixes:

1. The `?` key does not show the help/which-key modal dialog in visual mode. Make this consistent with normal mode. The highlighted entries, meaning the actionable items when Enter is pressed, must be correct for the current mode.
2. The Go code block in `examples/kitchen-sink.md` is formatted incorrectly in `oom-edit` compared with the reference screenshot. Go uses tabs for formatting and may not be the only language that does. Correct Go code block formatting, including tab handling. The supplied images show the reference and current `oom-edit` rendering of that block.
3. In addition to `:e!` reloading a file, support `:reload` for the current tab and `:reload-all` for every open tab.
4. Allow ASCII clipboard text to be pasted into `:tabnew` as a file path, with path sanitization.
5. Make `:tabnew ./examples/kitchen-sink.md` open that valid path instead of reporting `invalid substitution range`. Support both fully qualified paths and paths relative to the directory where `oom-edit` was launched.
6. In command mode, let Up and Down select from the last 10 commands used in the current session. History is not persistent. Escape continues to close command mode.

Original examples and artifacts:

- Reference screenshot: user attachment Image #1, preserved as `evidence/go-reference.png`.
- Current rendering screenshot: user attachment Image #2, preserved as `evidence/go-current.png`.

## Acceptance follow-up — Round 2

There is a regression in this change: When I open the which-key/help panel I can no longer use my cursor to view items that are off the bottom of the list of options.

Further - I think the `command` entries in the which-key UI can support using `enter` to activate them. Here is what I would expect: The user highlights one of those command items - say `:wq`. Hitting enter should close `which-key` open the command mode and have the selected command auto-populated. The user can then either add to the command (in the case they need to provide info) or just hit enter to activate the command.

## Acceptance follow-up — Round 3

One more issue I noticed: The golang code block is now correctly formatted in normal mode. However if I switch to insider the same issue reproduces. The code block isn't handling tabs correctly.

Interpretation for this round: “insider” refers to source Insert mode, the mode entered from rendered Normal to edit the Markdown source. This follow-up requires the same visible tab indentation there while preserving the literal tab bytes in the document.
