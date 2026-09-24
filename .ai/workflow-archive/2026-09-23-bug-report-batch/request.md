# Bug Report Batch — September 23, 2026

Resolve the issues reported in:

`/Users/jason/Developer/sources/personal/notebook/projects/oom-edit/bugs/20260923.md`

## Reported requirements

For any fixes, be test-forward. These are problems that must not return. Some
are regressions that the existing test suite did not catch.

- If a document is started with no front matter, the cursor commonly gets stuck
  on line 1. For example, after entering `# foo` on the first line and pressing
  Enter, empty lines are added but the cursor remains on the first line.
- Word wrap is not working correctly. This is a regression. On a screen around
  110 characters wide, text reaches the edge and wraps by splitting words.
  Wrapping must occur at word boundaries.
- Word wrapping must happen at a predictable line limit: 100 characters by
  default, configurable by the user. Tables have a minimum width of 80
  characters; on narrower screens, users must scroll horizontally to see the
  full table.
- Wrapped sentences are awkward to edit. From the line immediately below the
  bottom of a wrapped sentence, Left Arrow or `h` must be able to move to the
  end of the preceding wrapped line, rather than requiring Up followed by a
  traversal from the line start.
- Provide a key combination that inserts a default front-matter section only
  when the document has no front matter, keeping front matter uniform.
- Increase the spell-check debounce/delay so it does not run while words are
  still being typed. The suggested delay is five to seven seconds after the
  last character entry.
- Spell-check completion must not visibly clear all misspelling underlines and
  then restore them during a screen repaint.
- Pressing Space twice currently moves the cursor down twice, but the second
  destination line is also preceded by a space. That inserted space must not
  occur.
- In rendered Normal mode, preserve blank-line separation between bulleted list
  items that are separated by blank lines in Insert/source mode.
