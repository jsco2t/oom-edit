# Work Request

Review the immediately preceding archived clipboard workflow,
`2026-08-25-rendered-select-clipboard`, and correct its clipboard-copy behavior.

## Reported issue

Copying selected text from a Markdown document currently places rendered plain
text on the system clipboard instead of the actual Markdown source.

For example, this Markdown source:

```markdown
`App` consumes `Effect::ClipboardWrite` through an injected `ClipboardSink`.
```

currently round-trips through the system clipboard as:

```text
App consumes Effect::ClipboardWrite through an injected ClipboardSink.
```

## Required outcome

- The default clipboard-copy behavior must place the actual Markdown source on
  the system clipboard, preserving syntax such as inline-code backticks.
- Add a user setting that forces clipboard copies to be converted to rendered
  plain text.
- When the setting is not enabled, Markdown source remains the default.
- When the setting is enabled, users who never want Markdown syntax on the
  clipboard receive sanitized plain text instead.

The implementation must remain consistent with the repository's reusable-core,
thin-TUI architecture and existing clipboard workflow constraints.

## Acceptance follow-up — Round 2

User acceptance testing found another Markdown-source fidelity failure in
`examples/kitchen-sink.md`, near the top of the document.

The reported source is:

```markdown
backtick escaping: `` `backtic` inside of code ``
```

Copying and pasting that rendered line into an external location produces:

```text
backtick escaping: `backticks` inside code ``
```

The repository fixture's exact text is:

```markdown
backtick escaping: `` `backticks` inside code ``
```

The leading double-backtick delimiter must not be dropped. Default Markdown
copy must preserve this complete inline-code span exactly, including delimiter
length, literal backticks inside the code payload, and the delimiter-adjacent
spaces. The plain-text preference must continue to emit the rendered code
content without the outer delimiter run.
