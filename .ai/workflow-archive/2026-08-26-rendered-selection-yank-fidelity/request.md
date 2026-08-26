# Request

When selecting multiple lines of text and then yanking/copying with the `y` key in rendered Select/view mode, newline characters are completely stripped and other whitespace may also be lost.

The default `y` action must copy the selected Markdown faithfully, preserving formatting characters, newline characters, and whitespace.

Also investigate and define an alternate yank keyboard shortcut for the less-common operation that copies the rendered selection as plain text with Markdown formatting characters removed. The alternate must be discoverable and must not weaken the fidelity of default `y`.

## Acceptance follow-up — Round 2

The change we just committed is very - very good. I would like to make one small refinement.

When copying codeblocks it's very challenging to actually copy the code block.

Here is what I would like:

1. IF I copy content **within** a code block: No change - it's working as expected.

2. IF I select the entire code block (including the top and bottom "```") then I want all of that copied as is - including any language marker on the first "```".

If I copy this (the whole block):

```text
This is some text
```

And then paste it - I should get:

```text
This is some text
```

IF I instead copy just the content inside of the code block (in this case "This is some text") then I want that copied as is - including the whitepspace formatting we just fixed.

Note - the work we have been doing was just committed as I didn't want to loose it.
