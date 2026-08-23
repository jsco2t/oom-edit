# Usability refinements from the 2026-08-22 review

Create a single feature workflow plan that resolves every item documented in:

`/home/jason/Developer/sources/personal/notebook/projects/oom-edit/reviews/reports.md`

The reported requirements are:

1. Eliminate the multiple full-screen repaints/flicker observed when launching either a debug or release build with a document path.
2. Correct the command palette's partially unreadable/inconsistent theming and inconsistent indentation/column alignment, using the accompanying `command-pallet.png` as visual context.
3. Make bottom-bar help text unambiguous and more compact:
   - use only one description for each key or chord;
   - clearly distinguish Space-prefix chords from standalone keys;
   - retain `/` as search and `:` as command mode;
   - reduce overflow at narrow/dynamic terminal widths where practical.
4. Make rendered link-reference items at the bottom of a document useful from rendered Normal and rendered Select modes: when such an item is selected, `y` or Enter should copy its text to the system clipboard.
5. Make rendered-mode tables support an 80-column minimum layout rather than behaving as though their minimum is approximately 120 columns.
6. When rendered content such as a table is wider than the viewport, make horizontal navigation follow cursor movement so clipped content can be brought into view in rendered Normal mode.
7. Add a setting for mode-dependent terminal cursor shapes:
   - rendered Normal uses a block cursor;
   - source Insert uses a bar cursor;
   - rendered Select uses a configurable mode-appropriate cursor;
   - users can disable mode-dependent shapes so a block cursor is used in all modes.

The plan must honor the repository's architecture, testing, no-data-loss, dependency, and complete quality-gate requirements.
