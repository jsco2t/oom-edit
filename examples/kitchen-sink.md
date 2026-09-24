---
id: 0x00cafe
createdate: 2026-08-08T12:00:00-07:00
title: "oom-edit Kitchen Sink: Manual Verification Document"
tags:
  - oom-edit
  - testing
  - markdown
  - verification
  - kitchen-sink
aliases:
  - kitchen sink test
  - manual verification
publish: false
category: testing
status: draft
---

> **Expected rendering:** The leading YAML becomes a structured metadata panel, `Space m` reports that front matter is already present, and six heading levels remain individually navigable.

# Heading Level 1

## Heading Level 2

### Heading Level 3

#### Heading Level 4

##### Heading Level 5

###### Heading Level 6

---

## Inline Formatting

> **Expected rendering:** Inline emphasis, strong text, strikethrough, code, and escapes keep distinct styles; both hard-break forms start a new physical row without a paragraph gap.

This paragraph has **bold text**, _italic text_, _**bold and italic text**_, and ~~strikethrough text~~. It also has `inline code` and an inline code span with backtick escaping: `` `backticks` inside code ``.

Backslash hard break follows this line:\
This line should start on a new row without a paragraph gap.

Two-space hard break follows this line:  
This line should also start on a new row without a paragraph gap.

You can also use _underscores for emphasis_ and **underscores for strong emphasis** and _**both at once**_.

Backslash escapes: \*not italic\*, \`not code\`, \[not a link\], \#not a heading.

---

## Text wrapping

> **Expected rendering:** The long physical line wraps at word boundaries; its first row has a line number, width continuations show `↳`, and the physical blank row below retains its own number and cursor position.

This is a very long sentence to see how the editor handles text that needs to wrap to multiple lines the content of the sentence doesn't matter and infact it's clearly not even valid english that's ok as this is just a test to see how well wrapping works.

---

## Links and Images

> **Expected rendering:** Link labels and image alt text remain styled and source-backed, while generated numeric markers identify their destinations without claiming source bytes.

### Inline Links

Here is [an inline link](https://example.com) and one with a title: [titled link](https://example.com "Example Site"). Links can contain **[bold text](https://example.com)** and _[italic text](https://example.com)_.

### Autolinks

Angle-bracketed: <https://example.com> and <user@example.com>.

Bare URL (GFM extended autolink): https://example.com/path?query=value&other=123#fragment

### Reference Links

Here is a [reference link][ref1] and a [collapsed reference link][collapsed reference link] and a [shortcut reference link].

[ref1]: https://example.com/reference "Reference Title"
[collapsed reference link]: https://example.com/collapsed
[shortcut reference link]: https://example.com/shortcut

### Images

![Alt text for an image](https://example.com/image.png "Image Title")

![Reference image][img-ref]

[img-ref]: https://example.com/photo.jpg "Photo"

---

## Block Quotes

> **Expected rendering:** Every authored quote line remains a separately numbered rendered row with a `┃` prefix; nesting repeats the prefix and preserves embedded lists and code.

> This is a simple block quote. It can contain **formatted text** and `code`.

> Block quotes can span multiple lines.
> Each line is prefixed with a `>` character.
>
> They can contain blank lines too.

> Nested block quotes:
>
>> This is a nested quote.
>>
>>> And a triple-nested quote.

> A block quote with a list inside:
>
> 1. First item
> 2. Second item
>    - Nested unordered
>
> And a code block:
>
> ```
> echo "hello from a block quote"
> ```

---

## Unordered Lists

> **Expected rendering:** Tight lists remain compact, loose siblings retain one blank rendered row, nested indentation stays aligned, and `-`, `+`, and `*` start distinct list runs.

- Item one
- Item two
- Item three with **bold** and `code`

Nested unordered lists:

- Top-level item
  - Second-level item
    - Third-level item
      - Fourth-level item
  - Another second-level item
- Back to top level

Different bullet characters start different lists:

* Asterisk item one
* Asterisk item two

+ Plus item one
+ Plus item two

- Dash item one
- Dash item two

Loose list with blank-line-separated siblings and multiple blocks:

- First loose item.

- Second loose item with its first paragraph.

  This second paragraph remains inside the second item.

### Heading After a List and Physical Blank

The physical blank line before this heading should retain its own gutter number and cursor identity.

---

## Ordered Lists

> **Expected rendering:** Ordered lists retain declared starting values, accept both `.` and `)` source delimiters, and keep nested ordered and unordered children aligned.

1. First item
2. Second item
3. Third item

Starting from a different number:

5. This starts at five
6. Six
7. Seven

Parenthesis delimiters start a separate ordered list:

1) Parenthesis item one
2) Parenthesis item two

Nested ordered lists:

1. Top-level ordered
   1. Nested ordered
   2. Another nested
      1. Third level: misspelledd lazertific
   3. Back to second level
2. Back to top level

Mixed nesting:

1. Ordered top level
   - Unordered nested
   - Another unordered
     1. Ordered inside unordered
     2. Continuation
   - Back to unordered
2. Back to ordered

---

## Task Lists

> **Expected rendering:** Checked and unchecked task markers remain visible non-color signals, including on nested tasks with inline formatting.

- [ ] Unchecked task
- [x] Checked task
- [ ] Another unchecked task with `inline code`
- [x] Checked with **bold text**
  - [ ] Nested unchecked subtask
  - [x] Nested checked subtask

---

## Tables

> **Expected rendering:** Pipe tables align columns and preserve inline styles; wide cells retain their complete content and use the table's horizontal layout instead of prose reflow.

### Simple Table

| Name    | Age | City      |
| ------- | --- | --------- |
| Alice   | 30  | Portland  |
| Bob     | 25  | Seattle   |
| Charlie | 35  | San Diego |

### Aligned Columns

| Left-aligned | Center-aligned | Right-aligned |
| :----------- | :------------: | ------------: |
| left         |     center     |         right |
| data         |      data      |          data |
| more         |      more      |          more |

### Multi-Column Table with Formatted Content

| Feature          | Status  | Priority | Assignee | Notes                                        |
| ---------------- | ------- | -------- | -------- | -------------------------------------------- |
| Vim motions      | Done    | P0       | core     | `hjkl`, `w`, `b`, `e`, `0`, `$`, `gg`, `G`   |
| Syntax highlight | Done    | P0       | core     | Tree-sitter based, **all** built-in grammars |
| Rendered Normal  | Done    | P1       | core     | Rendered markdown with navigation            |
| Search           | Planned | P1       | core     | `/pattern` with `n`/`N` for next/prev        |
| Command palette  | Planned | P2       | tui      | `:` prefix, fuzzy matching                   |
| Clipboard        | Planned | P1       | core     | OSC 52 — _no external dependencies_          |

### Table Requiring Text Wrapping

This table has cells with content long enough to force horizontal scrolling or wrapping behavior in a terminal:

| Component               | Description                                                                                                                      | Implementation Details                                                                                                                                                                                                                                          |
| ----------------------- | -------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Incremental Highlighter | The incremental highlighting engine re-highlights only the ranges affected by an edit, using tree-sitter's edit + reparse cycle. | Uses `InputEdit` to inform the old tree of byte-range changes, then calls `parser.parse()` with the old tree to produce a new tree. Only changed node ranges are re-queried for highlights, keeping large-file edits fast.                                      |
| Rendered Layout         | Converts the canonical markdown buffer into styled headings, wrapped paragraphs, and navigable structure.                        | Walks the pulldown-cmark event stream, applying semantic styles per block type. Headings become bold + colored, code blocks get background fill, and links show their URL in a muted style. Cursor maps bi-directionally between rendered and source positions. |
| Atomic Save             | Ensures that a crash or kill signal during write never corrupts the file being edited.                                           | Writes to a temporary file in the same directory, calls `fsync()`, then atomically renames over the target. If rename fails, the original file is untouched. The temp file name includes the PID to avoid collisions.                                           |

---

## Thematic Breaks

> **Expected rendering:** Each of the three CommonMark rule syntaxes becomes the same full-width visual separator.

Three different syntaxes:

---

***

___

---

## Code Blocks

> **Expected rendering:** Fenced blocks keep literal whitespace and language-specific highlighting, unknown or absent languages remain preformatted, and indented code stays literal.

### Fenced Code Blocks

```rust
use std::collections::HashMap;

fn main() {
    let mut scores: HashMap<&str, i32> = HashMap::new();
    scores.insert("Alice", 100);
    scores.insert("Bob", 85);

    for (name, score) in &scores {
        println!("{name}: {score}");
    }

    // Pattern matching with Option
    let value: Option<i32> = Some(42);
    match value {
        Some(v) if v > 0 => println!("Positive: {v}"),
        Some(v) => println!("Non-positive: {v}"),
        None => println!("No value"),
    }
}
```

```go
package main

import (
	"fmt"
	"sync"
)

func main() {
	var wg sync.WaitGroup
	ch := make(chan int, 10)

	// Producer
	wg.Add(1)
	go func() {
		defer wg.Done()
		for i := 0; i < 10; i++ {
			ch <- i * i
		}
		close(ch)
	}()

	// Consumer
	wg.Add(1)
	go func() {
		defer wg.Done()
		for val := range ch {
			fmt.Printf("received: %d\n", val)
		}
	}()

	wg.Wait()
}
```

```python
from dataclasses import dataclass
from typing import Optional

@dataclass
class TreeNode:
    value: int
    left: Optional["TreeNode"] = None
    right: Optional["TreeNode"] = None

def inorder(node: TreeNode | None) -> list[int]:
    if node is None:
        return []
    return inorder(node.left) + [node.value] + inorder(node.right)

root = TreeNode(4, TreeNode(2, TreeNode(1), TreeNode(3)), TreeNode(6, TreeNode(5), TreeNode(7)))
print(inorder(root))  # [1, 2, 3, 4, 5, 6, 7]
```

```javascript
async function fetchWithRetry(url, retries = 3) {
  for (let attempt = 1; attempt <= retries; attempt++) {
    try {
      const response = await fetch(url);
      if (!response.ok) throw new Error(`HTTP ${response.status}`);
      return await response.json();
    } catch (err) {
      if (attempt === retries) throw err;
      const delay = Math.pow(2, attempt) * 100;
      await new Promise(resolve => setTimeout(resolve, delay));
    }
  }
}
```

```typescript
interface Config<T> {
  readonly key: string;
  value: T;
  validate: (v: T) => boolean;
}

function createConfig<T>(
  key: string,
  value: T,
  validate: (v: T) => boolean,
): Config<T> {
  if (!validate(value)) {
    throw new Error(`Invalid value for ${key}`);
  }
  return { key, value, validate };
}

const port = createConfig("port", 8080, (v) => v > 0 && v < 65536);
```

```bash
#!/usr/bin/env bash
set -euo pipefail

readonly LOG_DIR="/var/log/app"
readonly RETENTION_DAYS=30

find "$LOG_DIR" -name "*.log" -mtime +"$RETENTION_DAYS" -print0 |
  while IFS= read -r -d '' file; do
    echo "Removing old log: $file"
    rm -f "$file"
  done

echo "Cleanup complete. Remaining files:"
ls -lh "$LOG_DIR"
```

```c
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

typedef struct Node {
    int data;
    struct Node *next;
} Node;

Node *prepend(Node *head, int data) {
    Node *node = malloc(sizeof(Node));
    node->data = data;
    node->next = head;
    return node;
}

void print_list(const Node *head) {
    for (const Node *cur = head; cur; cur = cur->next)
        printf("%d -> ", cur->data);
    printf("NULL\n");
}

int main(void) {
    Node *list = NULL;
    for (int i = 5; i >= 1; i--)
        list = prepend(list, i);
    print_list(list);  /* 1 -> 2 -> 3 -> 4 -> 5 -> NULL */
    return 0;
}
```

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: web-server
  labels:
    app: web
spec:
  replicas: 3
  selector:
    matchLabels:
      app: web
  template:
    metadata:
      labels:
        app: web
    spec:
      containers:
        - name: nginx
          image: nginx:1.25-alpine
          ports:
            - containerPort: 80
          resources:
            limits:
              memory: "128Mi"
              cpu: "250m"
```

```toml
[package]
name = "oom-edit"
version = "0.1.0"
edition = "2021"

[dependencies]
ratatui = { version = "0.28", default-features = false, features = ["crossterm"] }
tree-sitter = "=0.24.7"

[profile.release]
lto = true
strip = true
```

```json
{
  "name": "example-config",
  "version": "2.1.0",
  "settings": {
    "theme": "dark",
    "font_size": 14,
    "line_numbers": true,
    "word_wrap": false,
    "rulers": [80, 120],
    "languages": {
      "rust": { "tab_size": 4, "format_on_save": true },
      "go": { "tab_size": 4, "format_on_save": true }
    }
  }
}
```

```sql
SELECT
    u.name,
    u.email,
    COUNT(o.id) AS order_count,
    COALESCE(SUM(o.total), 0) AS total_spent
FROM users u
LEFT JOIN orders o ON o.user_id = u.id
WHERE u.created_at >= '2024-01-01'
GROUP BY u.id, u.name, u.email
HAVING COUNT(o.id) > 0
ORDER BY total_spent DESC
LIMIT 25;
```

```css
:root {
  --bg-primary: #1a1b26;
  --fg-primary: #c0caf5;
  --accent: #7aa2f7;
}

.editor {
  display: grid;
  grid-template-rows: auto 1fr auto;
  height: 100vh;
  font-family: "JetBrains Mono", monospace;
  background: var(--bg-primary);
  color: var(--fg-primary);
}

.editor__line.active {
  background: rgba(255, 255, 255, 0.05);
  border-left: 2px solid var(--accent);
}
```

```
This is a fenced code block with no language specified.
It should be rendered as plain preformatted text.
No syntax highlighting is applied.
```

~~~text
This block uses tilde fences instead of backticks.
Its contents remain plain preformatted text.
~~~

### Indented Code Block

    This is an indented code block.
    It uses four spaces of indentation.
    No info string is possible here — it is always plain text.
    fn but_this_is_not_highlighted() {
        println!("just monospace text");
    }

---

## Very Long Lines

> **Expected rendering:** Long prose wraps without losing words or source positions, while indivisible code spans and URLs remain navigable across every displayed row.

This is a very long line that should test horizontal scrolling and line wrapping behavior in the editor. It contains enough text to exceed the typical terminal width of 80 or even 120 columns, and it just keeps going and going and going with more words and clauses and phrases to push it well past any reasonable column limit that a user might have configured in their terminal emulator or window manager. The purpose is to verify that the editor handles extremely wide content gracefully, whether through soft-wrapping, horizontal scrolling, or some other mechanism. Does the cursor track correctly at column 300? Column 400? Let's find out by adding even more text to this single unbroken paragraph line.

Here is a line with a very long inline code span: `fn this_is_a_really_long_function_name_that_goes_on_and_on(parameter_one: &str, parameter_two: &mut Vec<SomeGenericType<AnotherLongTypeName>>, parameter_three: Option<Result<HashMap<String, Vec<u64>>, SomeErrorType>>) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>>`

A very long URL in a link: [this link has an absurdly long URL](https://example.com/api/v2/resources/documents/12345678-abcd-efgh-ijkl-mnopqrstuvwx/versions/latest/content?format=markdown&include_metadata=true&include_frontmatter=true&recursive=true&depth=unlimited&token=eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9)

---

## Deeply Nested Structures

> **Expected rendering:** Six levels of mixed lists remain visibly nested, and the deepest block quote retains its own prefix, formatting, and source mapping.

1. Level one ordered
   - Level two unordered
     1. Level three ordered
        - Level four unordered
          1. Level five ordered
             - Level six unordered — this is getting deep
               > And a block quote at the bottom of the nesting
               >
               > With **formatted content** inside.

---

## Paragraphs and Soft Breaks

> **Expected rendering:** Blank lines separate paragraphs, while every plain source newline inside a paragraph starts a distinct numbered rendered row; nested styles survive those row boundaries.

This is the first paragraph. It has multiple sentences. Each sentence adds to the overall content of the paragraph and tests how the editor handles flowing text within a single block.

This is the second paragraph, separated from the first by a blank line. Paragraphs are the most basic block element in markdown and are used more than any other construct.

These three authored prose lines belong to one Markdown paragraph.
Each physical source line should keep its own rendered row and line number.
No blank lines separate these statements, so their soft breaks remain visible.

This **strong phrase continues
on the next physical source line** and [this link also crosses
a physical source line](https://example.com/soft-break).

This multiline code span contains a source newline: `alpha
beta`; its internal line ending should normalize to a space on one rendered row.

---

## HTML (Inline and Block)

> **Expected rendering:** Inline HTML stays within prose, while the block HTML remains a literal multi-line block without Markdown interpretation inside its tags.

Inline HTML: This has a <strong>strong tag</strong> and a <em>em tag</em> and a <code>code tag</code>.

Block HTML:

<div>
  <p>This is a raw HTML block.</p>
  <ul>
    <li>Item A</li>
    <li>Item B</li>
  </ul>
</div>

---

## Footnotes

> **Expected rendering:** Footnote references remain link-like in prose, and definitions are collected into one ordered footnote area after the document content.

This sentence has a footnote[^1]. And here is another[^longnote].

[^1]: This is the footnote content.

[^longnote]: This is a longer footnote with multiple paragraphs.

    The second paragraph of the footnote is indented to show it belongs
    to the same footnote.

---

## Escapes and Special Characters

> **Expected rendering:** Escapes suppress Markdown punctuation, entities decode to their characters, literal table pipes remain inside code spans, and Unicode stays intact.

Escaped characters: \* \_ \` \# \~ \[ \] \( \) \{ \} \| \\ \! \. \- \+

Literal pipes in a table need escaping:

| Expression        | Result |
| ----------------- | ------ |
| `a \| b`          | a or b |
| `true \|\| false` | true   |

HTML entities: &amp; &lt; &gt; &quot; &#42; &#x2a;

Unicode: em dash — en dash – ellipsis … bullet • copyright © section § degree °

---

## Spell-check Diagnostics

> **Expected rendering:** Both invented words receive independent diagnostics; editing the correctly spelled middle line does not make the unaffected diagnostics disappear while rescanning waits for idle time.

The invented word qzorpulated is intentionally misspelled.
Edit this correctly spelled middle line repeatedly during manual verification.
The invented word flarnivex is also intentionally misspelled.

---

## Edge Cases

> **Expected rendering:** Empty and consecutive headings remain safe and navigable, inline formatting survives inside headings, and Setext underlines produce real level-one and level-two headings.

### Empty Heading

Normally a heading has text, but can the editor handle one followed immediately by content?

### 

The above line is `###` with nothing after it — some parsers treat it as an empty heading.

### Heading with `code` and **bold** and _italic_

### Heading with a [link](https://example.com)

### Consecutive Headings

#### No Content Between Them

##### Still Going

###### The Deepest Level

Back to a paragraph.

### Setext Headings

This is a setext h1
===================

This is a setext h2
-------------------

---

## Long Table Stress Test

> **Expected rendering:** The ten-row table keeps stable borders, aligned columns, inline code, and full-width horizontal navigation on narrow terminals.

| #  | Method | Endpoint                   | Status | Latency | Description                                             |
| -- | ------ | -------------------------- | ------ | ------- | ------------------------------------------------------- |
| 1  | GET    | `/api/v1/users`            | 200    | 45ms    | List all users with pagination                          |
| 2  | POST   | `/api/v1/users`            | 201    | 120ms   | Create a new user account with email verification       |
| 3  | GET    | `/api/v1/users/:id`        | 200    | 32ms    | Retrieve a specific user by ID                          |
| 4  | PUT    | `/api/v1/users/:id`        | 200    | 88ms    | Update user profile information                         |
| 5  | DELETE | `/api/v1/users/:id`        | 204    | 55ms    | Soft-delete a user account                              |
| 6  | GET    | `/api/v1/users/:id/orders` | 200    | 150ms   | List all orders for a specific user                     |
| 7  | POST   | `/api/v1/users/:id/orders` | 201    | 200ms   | Create a new order for a user with inventory validation |
| 8  | GET    | `/api/v1/health`           | 200    | 2ms     | Health check endpoint                                   |
| 9  | POST   | `/api/v1/auth/login`       | 200    | 180ms   | Authenticate and receive JWT tokens                     |
| 10 | POST   | `/api/v1/auth/refresh`     | 200    | 45ms    | Refresh an expired access token using a refresh token   |

---

## Everything in a Block Quote

> **Expected rendering:** The outer quote prefix applies to every nested heading, paragraph, list, table, fence, and inner quote without stealing their source identity.

> # Quoted Heading
>
> A paragraph with **bold**, _italic_, ~~strikethrough~~, and `code`.
>
> - List item one
> - List item two
>   - Nested
>
> | Col A | Col B |
> | ----- | ----- |
> | one   | two   |
>
> ```rust
> fn quoted_code() -> &'static str {
>     "hello from inside a block quote"
> }
> ```
>
>> Nested quote with a [link](https://example.com).

---

## Final Paragraph

> **Expected rendering:** This final summary is an ordinary paragraph following a numbered physical blank row and remains reachable through document-end navigation.

This document exercises headings (ATX and Setext, levels 1-6), inline formatting (bold, italic, bold-italic, strikethrough, hard and soft breaks, and single- or multi-line code spans), links (inline, reference, collapsed, shortcut, autolinks, and links crossing source lines), images, block quotes (including nested and with embedded content), ordered and unordered lists (tight, loose, alternate delimiters, deep nesting, and mixed types), task lists, tables (simple, aligned, wide, and long), all thematic-break syntaxes, backtick and tilde fenced code blocks (Rust, Go, Python, JavaScript, TypeScript, Bash, C, YAML, TOML, JSON, SQL, CSS, text, and unspecified), indented code blocks, very long lines, spelling diagnostics, HTML (inline and block), footnotes, escape sequences, special characters, and edge cases. If oom-edit renders all of the above correctly, it handles real-world Markdown well.
