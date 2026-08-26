# Request

When selecting multiple lines of text and then yanking/copying with the `y` key in rendered Select/view mode, newline characters are completely stripped and other whitespace may also be lost.

The default `y` action must copy the selected Markdown faithfully, preserving formatting characters, newline characters, and whitespace.

Also investigate and define an alternate yank keyboard shortcut for the less-common operation that copies the rendered selection as plain text with Markdown formatting characters removed. The alternate must be discoverable and must not weaken the fidelity of default `y`.
