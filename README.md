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

## Using oom-edit

Open a document, or start with an empty buffer:

```console
oom-edit notes.md
oom-edit
```

When a path does not exist, the file is created on the first save. Use
`oom-edit --help` for the complete command-line interface, or select a built-in
theme for one run:

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
| `:` | Enter Command mode |
| `Space h` | Open help and the command palette |
| `Space w` | Save |
| `Space q` | Quit |

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

The built-in themes are `default-dark`, `default-light`, and the color-free
`accessible` theme. Theme selection follows this order: `--theme`, the
`OOM_EDIT_THEME` environment variable, the configured theme for the active
light/dark mode, then the matching built-in default. `NO_COLOR` and
`TERM=dumb` select monochrome terminal output. `Space t` cycles compatible
themes and saves the selected light or dark theme slot to the configuration
file.

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

`oom-edit` is released under the [MIT License](LICENSE). Bundled dictionary
notices are available with `oom-edit --licenses`.
