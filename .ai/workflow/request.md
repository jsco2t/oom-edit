# User-reported issues as of 2026-08-24

Source supplied by the user:
`/Users/jason/Developer/sources/personal/notebook/projects/oom-edit/reviews/20260824.md`

## Text wrapping should always happen at word boundaries

This is a general rule for `oom-edit`, but it is clearly broken for text
wrapping in tables.

Currently, when a table cell is forced to wrap its text, the text wraps at a
letter boundary instead of a word boundary.

Current example:

```text
This is some interesting long text co
ntent written here.
```

Expected:

```text
This is some interesting long text
content written here.
```

Provide excellent test coverage for this behavior.

## Dashed table-row boundaries

Revision supplied by the user on 2026-08-25. This supersedes the earlier
alternating table-row background request, which must be removed entirely.

Rendered tables with multiple body rows must show a faint row of ASCII dash
characters between adjacent body rows. Within each table column, the dashes
span the complete interior column width while remaining contained by the
table's vertical borders. A wrapped logical row receives one boundary only
after its final continuation line. The boundary geometry must be recomputed
from the current table layout when terminal width changes.

The dashed boundary is renderer-created output, not document content: it must
not claim source bytes, become selectable text, or extend outside the table.
No configuration setting or alternating-row background styling remains.

## Select/Visual mode selection should wrap within a table cell

When selecting text in a table cell, selection currently does not wrap and
follow the text as laid out in the cell. When selection reaches the next
visual line, the selection block can instead span candidate text in other
rows.

Selecting text within a container such as a table cell must follow the text
as wrapped within that cell.

## Trouble panel column alignment

The diagnostic/trouble panel does not align its text into visual columns. Its
leading columns have computable widths and should be standardized and aligned.

## Cursor visibility in the Trouble/Diagnostics panel

Unless multi-cursor support exists, only one cursor should be visible on the
screen. When the Trouble/Diagnostics modal is open, its first-row highlight
already indicates row selection, but a character cursor can remain visible at
an unrelated location in the panel depending on the editor cursor position.
The editor cursor must not remain visible through the modal.
