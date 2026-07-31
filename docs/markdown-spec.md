# Markdown Schema Reference

A single-page description of Markdown's structural rules — the closest thing Markdown
has to a "schema." Based on **CommonMark 0.31.2** (2024-01-28) plus the **GFM 0.29-gfm**
extensions. Markdown has no formal grammar in the BNF/XSD sense; CommonMark instead
defines a _document model_ — a tree of **blocks** and **inlines** — and a parsing
strategy, and that model is what's summarized here.

---

## 1. The document model

A document is a tree. Every node is either a **block** or an **inline**.

- **Block** = structural unit occupying a range of lines (paragraph, heading, list, …).
- **Inline** = content _within_ a block's text (emphasis, code span, link, …).

Blocks split into two kinds by containment:

| Kind                | Can contain                                      | Examples                                       |
| ------------------- | ------------------------------------------------ | ---------------------------------------------- |
| **Container block** | other blocks                                     | document, block quote, list, list item         |
| **Leaf block**      | inline content _or_ nothing — never other blocks | paragraph, heading, code block, thematic break |

The single most important rule: **only container blocks nest blocks.** A leaf block's
children are inlines (or raw literal text), never blocks. This is why "a paragraph
inside a table cell" is invalid in pipe tables — the cell is inline context, not a
block container.

---

## 2. Leaf blocks

Contain inline content or literal text; cannot contain child blocks.

- **Thematic break** — `---`, `***`, `___` (3+ of the same char). No children.
- **ATX heading** — `# … ######` (levels 1–6). Children: inlines.
- **Setext heading** — text underlined by `===` (h1) or `---` (h2). Children: inlines.
  Only wraps a preceding _paragraph_ line; cannot follow other blocks.
- **Indented code block** — lines indented ≥4 spaces. Children: **literal text only**
  (no inline parsing).
- **Fenced code block** — `` ``` `` or `~~~`, optional info string. Children: literal
  text only. Info string's first word is the language.
- **HTML block** — raw HTML at block level. Children: literal text (passed through).
- **Link reference definition** — `[label]: url "title"`. Produces _no_ output node;
  populates a reference map consumed during inline parsing.
- **Paragraph** — default leaf. Children: inlines.
- **Blank line** — separator; not a node in the tree.

**Table of what parses inlines vs. literal text:** headings and paragraphs → inlines;
code blocks (both kinds) and HTML blocks → literal text, untouched.

---

## 3. Container blocks

Contain other blocks. Only four exist in CommonMark:

- **Document** — the root. Contains any blocks.
- **Block quote** — lines prefixed `>`. Contains any blocks (including nested quotes,
  lists, code). Fully recursive.
- **List** — a sequence of list items of the same type. Contains **list items only**.
  - Bullet (`-`, `+`, `*`) or ordered (`1.`, `1)`).
  - Has a `tight`/`loose` property: loose if any items are separated by blank lines or
    contain multiple block children; affects whether item text is wrapped in `<p>`.
  - A change of bullet char or ordered delimiter starts a _new_ list.
- **List item** — contains any blocks. Content column is set by the marker width +
  following space; continuation lines must reach that column.

Containers can nest arbitrarily: a block quote can hold a list whose items hold code
blocks and nested quotes, etc.

---

## 4. Inlines

The leaves of the tree, parsed only inside inline-context blocks (headings,
paragraphs, table cells). Ordered roughly by precedence:

- **Code span** — `` `code` ``. Highest precedence after backslash escapes; contents
  are literal (no further inline parsing inside).
- **Raw HTML** — inline tags like `<span>`.
- **Autolink** — `<https://…>` or `<user@host>` (angle-bracketed).
- **Hard line break** — line ending in 2+ spaces, or a backslash `\` before newline.
- **Soft line break** — a plain newline within a paragraph (rendered as a space/`\n`).
- **Emphasis / strong** — `*`/`_` (em), `**`/`__` (strong). Governed by the
  left/right _flanking delimiter run_ rules; can nest.
- **Link** — `[text](url "title")` or `[text][ref]`. Link text may contain other
  inlines but **not** another link (no nested links).
- **Image** — `![alt](url "title")`. Same shape as a link; alt is inline content
  flattened to text on render.
- **Textual content** — everything else, literal.
- **Backslash escape** — `\` before an ASCII punctuation char escapes it. Highest
  precedence of all.
- **Entity / numeric reference** — `&amp;`, `&#123;`. Cannot be used to fabricate
  structural characters.

**Nesting constraint worth remembering:** links can't nest in links; code spans and
autolinks are opaque (no inline children).

---

## 5. GFM extensions

Layered on top of CommonMark; each must be explicitly enabled in most parsers.

- **Tables** — pipe tables only. A header row, a delimiter row (`---`, with `:` for
  alignment), then body rows. **Cells contain inline content only** — no paragraphs,
  lists, or block code. Use `<br>` for a visual line break; escape literal pipes as
  `\|`. Column count is fixed by the delimiter row.
- **Task list items** — list items beginning `[ ]` / `[x]`. A list-item variant, so
  they live only inside lists.
- **Strikethrough** — `~~text~~`. An inline, like emphasis.
- **Autolinks (extended)** — bare URLs/emails without angle brackets, recognized in
  text.
- **Disallowed raw HTML** — a small tag blocklist (`<script>`, `<iframe>`, …) filtered
  on output.

For real block content inside table cells you must leave pipe tables entirely: Pandoc
**grid tables** / **multiline tables** allow block-level cell content, or embed a raw
HTML `<table>`.

---

## 6. How the tree gets built (two-phase parse)

CommonMark parsing is explicitly two passes — useful to mirror in an implementation:

1. **Block structure phase.** Consume the input line by line, opening/closing blocks
   and building the block tree. Inline content is accumulated as raw text on the leaf
   blocks but _not yet parsed_. Link reference definitions are collected here.
2. **Inline phase.** Walk each inline-context leaf and parse its accumulated text into
   inline nodes, resolving links against the reference map from phase 1.

Consequences:

- Block structure always wins over inline structure. A line's role (list item vs.
  paragraph vs. code) is decided before any `*` or `[` is interpreted.
- Reference links can point _forward_ to definitions that appear later, because all
  definitions are known before inline parsing starts.

---

## 7. Precedence summary (highest → lowest)

**Block level:** blank lines and container markers (`>`, list markers, indentation)
are evaluated first; among leaves, an indented/fenced code block or HTML block beats a
paragraph interpretation of the same lines.

**Inline level:** backslash escapes → code spans / autolinks / raw HTML (opaque) →
emphasis & links (delimiter matching) → plain text.

---

## 8. Implementer gotchas

- **Leaf ≠ container.** If your AST lets a paragraph or table cell hold block children,
  it diverges from the spec. Keep the inline/block boundary strict.
- **Tabs** expand to a 4-column tab stop for the purpose of indentation, but are _not_
  blindly replaced with spaces in content.
- **Tight vs. loose lists** is a render-time property derived from blank-line placement
  and item contents; store enough info in phase 1 to compute it.
- **Setext headings** are a re-interpretation of a paragraph line — easy to miss if you
  finalize paragraphs too eagerly.
- **Link reference definitions** produce no visible node; don't emit an empty block for
  them.
- **HTML blocks** have seven start conditions with distinct end conditions — the
  fiddliest leaf block to get right.

---

## 9. Parser notes (Rust)

- **comrak** — CommonMark + GFM, closest to the reference. GFM features (`table`,
  `tasklist`, `strikethrough`, `autolink`, `tagfilter`) are opt-in via
  `ComrakExtensionOptions`. Cells remain inline-only; raw `<br>` passes through when
  `render.unsafe_` (or the escaping option) permits raw HTML.
- **pulldown-cmark** — event/pull-based (emits start/end events rather than a
  materialized tree). CommonMark-compliant; GFM tables, task lists, strikethrough,
  and footnotes behind feature flags. Same inline-only cell model.
- Neither supports Pandoc grid/multiline tables out of the box — implement that
  yourself if block-level cell content matters for your editor.

---

_Spec sources: CommonMark 0.31.2 (spec.commonmark.org) and GFM 0.29-gfm
(github.github.com/gfm). Verify against those for edge cases — this page is a working
summary, not a substitute for the conformance test suite (500+ examples)._
