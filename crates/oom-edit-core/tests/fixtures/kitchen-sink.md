# Kitchen Sink — All Rendered Elements

This fixture exercises every rendered element (VW-1 through VW-14) for span-integrity testing.

## VW-1: Headings H1–H6

# Heading 1

## Heading 2

### Heading 3

#### Heading 4

##### Heading 5

###### Heading 6

Headings can contain **bold**, *italic*, `code`, and ~~strikethrough~~ inline content.

---

## VW-2: Paragraphs

This is a paragraph with regular text. It wraps at the layout width and uses the `text` style.

This is a second paragraph. There should be a blank line between blocks in the rendered output.

## VW-3: Emphasis / Strong / Strikethrough

This has *italic text* and **bold text** and ~~crossed-out text~~.

Nested combinations: ***bold and italic*** and ***both*** with `inline code` inside.

## VW-4: Inline Code

Use the `cargo build` command to compile the project.

Multiple `code spans` on one line with `nested **bold**` emphasis.

## VW-5: Fenced Code Blocks

```rust
fn main() {
    println!("Hello, world!");
}
```

```toml
[package]
name = "oom-edit"
version = "0.1.0"
```

An indented code block:

    This is indented
    with multiple lines

## VW-6: Bulleted Lists

- First item
- Second item
- Third item

1. Ordered item one
2. Ordered item two
3. Ordered item three

Nested lists:

- Level 1
  - Level 2
    - Level 3
      - Level 4

Mixed nesting:

1. First ordered
   - Nested unordered
   - Another nested
2. Second ordered

## VW-7: Task Lists

- [ ] Unchecked task
- [x] Checked task
- [ ] Another unchecked

Ordered task list:

1. [ ] First task
2. [x] Completed task

## VW-8: Blockquotes

This is outside the quote.

> This is a blockquote.
> It spans multiple lines.
>
> > Nested blockquote level 2.
> > Still nested.
>
> Back to level 1.

Blockquote with list:

> - Item in quote
> - Another item

## VW-9: Tables

| Left | Center | Right |
|:-----|:------:|------:|
| a    |   b    |     c |
| long text | x | z |

| Simple | Table |
|--------|-------|
| cell   | cell  |

## VW-10: Thematic Break

This appears before.

***

This appears after.

---

___

## VW-11: Links

[Link text](https://example.com)

Another [link with title](https://example.org "Example Title").

Reference-style link: [ref link][ref-id]

[ref-id]: https://reference.example.com

Autolink: <https://autolink.example.com>

## VW-12: Images

![Alt text](https://example.com/image.png)

![Another image](https://example.org/photo.jpg "Photo Title")

Reference image: ![alt ref][img-ref]

[img-ref]: https://img.example.com/pic.png

## VW-13: HTML Blocks

<div class="example">
<p>HTML block content</p>
</div>

<!-- HTML comment -->

<table>
<tr><td>HTML table</td></tr>
</table>

Inline HTML: <span>inline span</span> and <strong>inline strong</strong>.

## VW-14: Footnotes

Text with a footnote reference[^1] and another[^note].

[^1]: Footnote definition one.
[^note]: A longer footnote definition with multiple lines.

Final paragraph after footnotes.
