# Request — Rendered blank-line and wrapped navigation fixes

Resolve two related Normal-mode bugs, using `examples/kitchen-sink.md` around
physical source lines 46–47 as the representative case.

1. When the cursor is on a blank line in rendered Normal mode, the gutter
   omits that line's number and the status bar continues to report the prior
   nonblank line. Blank lines must retain their physical source line identity
   so the gutter and status bar both report the correct line number. In the
   example, moving from wrapped source line 46 to blank source line 47 must show
   gutter line 47 and status position 47:1.

2. From the blank line immediately after a multi-row wrapped source line,
   moving left with `h` or Left currently jumps into a middle rendered row of
   the preceding source line. It must land at the end of that preceding wrapped
   source line.

All fixes must be test-forward. Run every focused test associated with the
impacted rendered layout, provenance, navigation, gutter, and status paths, as
well as the repository's complete required quality gate.
