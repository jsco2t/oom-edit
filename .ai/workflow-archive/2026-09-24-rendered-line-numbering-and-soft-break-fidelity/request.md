# Request — Rendered line numbering and soft-break fidelity

Resolve two related Normal-mode rendering problems shown in the supplied
screenshots.

## 1. Complete rendered line-number identity

Normal mode still omits some physical source line numbers even when the
missing rows are not visual continuations caused by width wrapping. In the
reported example, physical source line 43 contains its own Markdown list item,
but rendered Normal mode displays the item without gutter line number 43.

Correct rendered gutter numbering so every displayed row that begins a
distinct physical source line shows that physical line number. Rows created
only because a single physical line soft-wraps to additional display rows must
remain distinguishable from numbered source-line starts.

Add a subtle gutter marker to those width-wrapped continuation rows. Use a
familiar downward-then-right Unicode continuation symbol or similarly clear
non-color glyph. It must be visually dimmer than line numbers and must not
compete with them. Preserve accessibility: the distinction cannot rely on
color alone.

## 2. Preserve physical soft line breaks in rendered prose

Investigate the repository's authoritative Markdown specification and the
relevant upstream Markdown rules before deciding the fix.

Currently, consecutive prose source lines separated by a single line ending
are parsed as one paragraph and rendered as a single reflowed text block. This
causes author-chosen physical line boundaries to disappear in Normal mode. The
reported example has separate source statements on lines 3–6, but Normal mode
joins and rewraps them into an artificial paragraph.

Determine the standards-compliant behavior and improve the Normal-mode
experience without changing source bytes or Insert-mode semantics. Physical
line boundaries in prose should remain visibly meaningful, while true
width-created continuations should remain separately identifiable through the
new gutter marker.

## Delivery requirements

- Follow the core-owned rendered-layout and source-provenance architecture.
- Make the changes test-forward.
- Cover absolute and relative gutters, cursor/status mapping, Unicode, narrow
  wrapping, Markdown constructs affected by soft line breaks, and synthetic
  rows.
- Run all focused tests for rendered parsing/layout, wrapping, provenance,
  navigation, gutters, status, themes/accessibility, and snapshots, followed
  by the repository's complete required quality gate.

## Acceptance follow-up — Round 2

Between yesterday, and today we have fixed a decent set of bugs. Can you update
`examples/kitchen-sink.md` includes examples which should be fixed based on the
changes we have made. Also examine the document to see if there are other key
user scenarios which should be included as examples in the document.

### Additional clarification

Actually - let's take this as a chance to make the document more explicit.

For each `---` section can you add a call-out at the top of the section
(consistently formatted) which details the expected rendering.

## Acceptance follow-up — Round 3

With the default theme the text-wrap indicators in the gutter are not visible.
I'm not sure if this is a theme issue or a character layering issue.

The supplied screenshot shows a wrapped physical source line whose continuation
rows retain gutter diagnostic markers, but no visible `↳` continuation glyph.
