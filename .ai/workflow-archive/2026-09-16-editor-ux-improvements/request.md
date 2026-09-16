# Original request — Editor UX improvements

The user requested these improvements as one work package:

1. Instead of using `W` to warn that a line has a misspelling in the gutter, use `●`. Color the symbol to match the reported condition. The warning color should remain the current warning color; an error should use a red/orange color.
2. In the bottom status bar, color the boxed `S` spelling count indicator to match the gutter indicator. The color must follow the active theme. See [spelling status reference](reference/spelling-status.png).
3. Add about one space of padding at the right edge of the bottom status bar. See [right-edge reference](reference/status-right-edge.png).
4. Substantially reduce the right margin after the gutter line number while retaining a small gap before document text. Current examples are `<MARGIN>122<MARGIN>` and `W122<MARGIN>`. See [gutter reference](reference/gutter-spacing.png).
5. Make pointer clicks move the cursor to the clicked document location, including after scrolling in Normal mode. Dragging should select the dragged area and switch to rendered Select/visual mode.
6. When scrolling with a mouse/pointer, move the cursor with the viewed content so a subsequent mode change does not jump the view back to the old cursor.
7. In Normal mode, `?` should open the which-key style UI instead of search. After the Space special key, both `h` and `?` should open that UI.

The supplied screenshots are preserved in `reference/`.

## Acceptance follow-up — Round 2

The gutter is still taking up too much space for what it provides. The new [gutter screenshot](reference/gutter-acceptance-round-2.png) shows the current result.

1. Add a small visual buffer, described as a few pixels, on both the left and right sides of the trouble marker so it is separated from the application window edge and the line number.
2. Reduce the space after the line number before the content area further.

The user chose a smaller circular dot with built-in visual side space and no gap after the number, so the gutter shrinks by one terminal cell. This supersedes the original exact `●` glyph request for the gutter.

After reviewing the terminal-cell limitation, the user clarified that the existing trailing space **must remain** to separate the line number from document content. The smaller dot remains the requested way to create apparent side space around the marker. This clarification supersedes the earlier round-two request to remove the gap or shrink the three-digit gutter by one cell.
