# oom-edit

```text
       ___
      (o o)
     (  V  )
    /--m-m--\
```

`oom-edit` is a keyboard-driven Markdown editor for the terminal. It is built
for reading and navigating rendered Markdown without giving up precise access
to the source: documents open in a rendered view, while Insert mode exposes the
highlighted Markdown text for editing.

The editor supports YAML and TOML front matter, syntax-highlighted fenced code
blocks, Vim-style navigation and selections, multiple tabs, and idle-time spell
checking. Its four public modes are rendered **Normal**, source **Insert**,
rendered character/line/block **Select**, and **Command**.

Release performance targets and their make-owned reproduction commands are
documented in [docs/performance.md](docs/performance.md).

## Using oom-edit

Open a document, or start with an empty buffer:

```console
oom-edit notes.md
oom-edit
```

When a path does not exist, the file is created on the first save. Use
`oom-edit --help` for the complete command-line interface, or select a built-in
or user theme for one run:

```console
oom-edit --theme accessible notes.md
```

The interface includes a context-sensitive hint bar. A few useful starting
points are:

| Key | Action |
| --- | --- |
| `i` | Enter source Insert mode |
| `Esc` | Return to rendered Normal mode |
| `v`, `V`, `Ctrl-V` | Start character, line, or block Select mode |
| `y` (in Select) | Yank the selection and send its exact Markdown source to the system clipboard |
| `:` | Enter Command mode |
| `Space h` | Open help and the command palette |
| `Space w` | Save |
| `Space q` | Quit |

## Clipboard

In rendered Select, plain `y` preserves the normal in-process yank and also
sends the selection's exact Markdown source—including inline delimiters such as
backticks—to the system clipboard. This Markdown-preserving behavior is the
default. Set `clipboard.copy_format` to `"plain-text"` to strip Markdown syntax,
decode escapes and entities, and copy the rendered text instead. Explicit `"+y`
and `"*y` remain available as alternatives. Automatic clipboard publication
applies only to rendered Select yanks; deletes, changes, and yanks from source
Normal mode do not publish implicitly.

Clipboard output uses OSC 52 and is best effort. It requires OSC 52 support to
be enabled in the terminal and, when applicable, in tmux. The success message
only confirms that oom-edit emitted the sequence; terminals do not acknowledge
whether they applied it. Outgoing text over 100 KiB and terminal output errors
produce a visible warning. The size limit is applied after the configured copy
format is selected.

To paste the desktop clipboard, enter Insert mode and use the terminal's native
paste action. Bracketed paste is inserted as one operation. `"+p` uses
oom-edit's cached in-process system-register value, including a preceding
rendered Select yank, and does not read the desktop clipboard. The copy-format
setting changes only outgoing system-clipboard text; it does not sanitize or
reshape internal registers or `p`/`P` operations.

## Configuration

Configuration is optional. `oom-edit` reads
`$XDG_CONFIG_HOME/oom-edit/config.toml`, or
`~/.config/oom-edit/config.toml` when `XDG_CONFIG_HOME` is not set. Missing
settings use the defaults shown below; a missing file is normal, and malformed
configuration produces a warning before the editor falls back to defaults.

```toml
# Use hybrid-relative numbers in rendered modes. The current line stays
# absolute, and source Insert mode always uses absolute numbers.
relative_line_numbers = false

[editor]
wrap = true
# Use a steady block in Normal, a steady bar in Insert and Command, and a
# steady underscore in Select. Set false for a steady block in every mode.
cursor_shapes = true

[clipboard]
# Preserve Markdown source by default. Use "plain-text" to copy rendered text
# without Markdown syntax.
copy_format = "markdown"

[theme]
# Omit mode to infer light or dark from COLORFGBG, with dark as the fallback.
# mode = "dark" # "dark" or "light"
dark = "default-dark"
light = "default-light"

[spell]
enabled = true
language = "en_US" # "en_US", "en_CA", or "en_AU"
additional_dictionaries = []
# Example: ["project.words", "/opt/shared/company.words"]
```

### Themes

The built-in themes, in catalog order, are `default-dark`,
`catppuccin-mocha`, `dracula`, `nord`, `solarized-dark`, `tokyo-night`,
`default-light`, and the color-free `accessible` theme. The five additional
dark palettes are distributed with their upstream notices; Gruvbox and Rosé
Pine are not bundled.

User themes are direct `.toml` children of the `themes/` directory beside
`config.toml`, for example
`$XDG_CONFIG_HOME/oom-edit/themes/my-dark.toml`. The filename without `.toml`
is the theme name and must be lowercase kebab-case; built-in names are
reserved. Files are loaded once at startup in lexical filename order, must be
UTF-8, and may be at most 65,536 bytes. Nested files and non-TOML files are
ignored.

A complete user theme uses this strict schema. Every `[palette]` entry is
required and must be an exact `#RRGGBB` value. `[ansi]` is optional; omitted
ANSI roles use the application defaults for the declared appearance.
`appearance` must be `dark` or `light`; change the example's value to create a
light theme.

```toml
appearance = "dark"

[palette]
background = "#101010"
background-alt = "#202020"
gutter-background = "#303030"
gutter-text = "#909090"
gutter-text-active = "#ffffff"
surface = "#252525"
surface-active = "#454545"
border = "#707070"
text = "#eeeeee"
text-muted = "#888888"
text-emphasis = "#ffffff"
primary = "#cc66ff"
secondary = "#66ccff"
info = "#3399ff"
success = "#33cc66"
warning = "#ffcc33"
error = "#ff3366"
attention = "#ff9933"

[ansi]
gutter-background = "black"
gutter-text = "dark-gray"
gutter-text-active = "white"
primary = "magenta"
info = "blue"
success = "green"
warning = "yellow"
error = "red"
attention = "light-yellow"
```

Valid ANSI names are `black`, `red`, `green`, `yellow`, `blue`, `magenta`,
`cyan`, `gray`, `dark-gray`, `light-red`, `light-green`, `light-yellow`,
`light-blue`, `light-magenta`, `light-cyan`, and `white`. Unknown fields,
missing palette roles, other color forms, inheritance, includes, custom style
scopes, and user-controlled modifiers or gutter glyphs are rejected. Each
rejected file produces one path-specific warning before the terminal starts;
valid sibling themes remain available. Themes are not reloaded while the
editor is running.

Theme selection follows this order: `--theme`, the `OOM_EDIT_THEME`
environment variable, the configured theme for the active light/dark mode,
then the matching `default-dark` or `default-light`. An unknown, rejected, or
appearance-incompatible selected name uses that matching default; it does not
implicitly select `accessible`. `NO_COLOR` and `TERM=dumb` select monochrome
terminal output. `Space t` cycles compatible built-ins followed by compatible
user themes in lexical order and saves only the active light or dark config
slot.

Themes style the document body and the complete line-number gutter. Published
Trouble diagnostics add one fixed marker before the aligned line number on each
affected source line: `E`, `W`, `I`, or `H` for error, warning, info, or hint.
The highest severity wins when a line has more than one diagnostic; glyphs and
modifiers preserve the signal without color.

Wrapping can also be changed for the running session with `:set wrap` and
`:set nowrap`. Spell checking can be toggled with `Space z`, `:set spell`, or
`:set nospell`; runtime toggles are not written back to configuration.

Additional dictionaries are UTF-8 plain-text word lists with one entry per line.
Relative paths are resolved from the directory containing `config.toml`.
Blank lines and lines whose first non-whitespace character is `#` are ignored;
entries have a 64-byte maximum. A missing, unreadable, non-UTF-8, or
larger-than-16-MiB additional dictionary disables spell checking for that run
and produces a warning. The personal dictionary is stored as `dictionary.txt`
beside `config.toml`; `Space a` adds the word at the cursor. Use `Space s` for
suggestions, `[s` and `]s` to move between diagnostics, and `Space d` to view
all document diagnostics.

## Project status

`oom-edit` was written and is maintained by a single developer. Issues and bug
reports are welcome, but the project is not currently accepting code
contributions or pull requests.

Developers who want to understand, build, audit, or fork the codebase should
read [CONTRIBUTING.md](CONTRIBUTING.md).

## License

`oom-edit` is released under the [MIT License](LICENSE). Bundled dictionary and
theme notices are available with `oom-edit --licenses`.
