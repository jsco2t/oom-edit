# oom-edit

A console/TUI markdown editor written in Rust with four modes: rendered
**Normal**, source **Insert**, rendered character/line/block **Select**, and **Command**.
Markdown is rendered and navigable by default, with tree-sitter highlighting
for source editing, YAML/TOML front matter, and fenced code blocks.

The editing core ships as the reusable `oom-edit-core` crate — embeddable in
other applications with zero terminal dependencies. The `oom-edit` binary is
a thin ratatui shell over it.

**License:** MIT.

## Quickstart

```bash
make help          # list all targets
make check         # fmt + lint + build + test + deny + audit + data licenses
make run           # run the editor
make run ARGS=file.md   # open a file
make run-isolated ARGS=file.md  # verify without reading/writing your config
```

**`make` is the build system of record.** Every developer-facing workflow has
a make target; CI runs identical commands.

## Configuration

oom-edit loads `$XDG_CONFIG_HOME/oom-edit/config.toml` (falling back to
`~/.config/oom-edit/config.toml`). Line numbers are absolute by default. Set
the top-level TOML key `relative_line_numbers = true` to use hybrid-relative
numbers in rendered Normal, Select, and Command modes; the current line remains
absolute. Source Insert always uses absolute numbers. Wrapped and synthetic
rendered rows leave the gutter blank.

The colored defaults are `default-dark` and `default-light`; `accessible` is
an explicit, color-free choice. `--theme` overrides configuration, while a
valid `[theme]` dark/light slot overrides the built-in fallback. Startup logs
the active theme, effective palette, terminal capability, winning source, and
display mode. Tests and manual verification use a temporary
`XDG_CONFIG_HOME`; normal `make run` intentionally uses and may persist the
real user configuration.

### Spell checking

Spell checking is enabled by default and runs only during proven idle time. It
marks English prose in all four modes without replacing Markdown or source-code
styling. `Space s` opens suggestions, `Space a` adds the current word to the
personal dictionary, `Space z` toggles the active session, `]s`/`[s` navigate
diagnostics, and `Space d` opens Trouble. `:set spell` and `:set nospell` are
the command equivalents of the session toggle.

Configure dictionaries in the same `config.toml`:

```toml
[spell]
enabled = true
language = "en_US" # en_US, en_CA, or en_AU
additional_dictionaries = ["team.words", "/opt/shared/company.words"]
```

Exactly one bundled dialect is selected; additional dictionaries are merged in
declaration order and deduplicated. Relative paths are resolved against the
directory containing `config.toml`, while absolute paths are unchanged. An
invalid dialect emits a warning and falls back to `en_US`. A missing,
unreadable, larger-than-16-MiB, or non-UTF-8 additional dictionary disables the
spell engine for that run with a stable warning; it is not retried in the idle
loop.

Additional dictionaries are UTF-8 plain word lists with one entry per line.
Leading/trailing ASCII whitespace is ignored; blank lines and lines whose first
non-whitespace character is `#` are comments. Entries are normalized to
lowercase ASCII letters with optional internal straight apostrophes and a
64-byte maximum; ineligible entries are skipped. Hunspell `.aff`/`.dic` files
are not accepted directly—convert them to a plain list offline first.

The personal dictionary is `dictionary.txt` beside `config.toml`. Successful
adds are normalized, sorted, deduplicated, written with LF and a final newline,
and persisted atomically before the live engine changes. Runtime toggles are
per-session and are not written to configuration. Use `oom-edit --licenses` to
print the complete bundled SCOWL data notices without starting the terminal UI.

## Rendered editing

Expanded YAML/TOML front matter is a source-faithful metadata panel: comments,
blank lines, nesting, quoting, order, and physical line numbers are retained.
Long metadata lines wrap inside the panel, with blank gutters on continuation
rows; `z` collapses or expands it.

Select follows Vim's three shapes:

| Key      | Shape                                         |
| -------- | --------------------------------------------- |
| `v`      | Character-wise, inclusive source-backed atoms |
| `V`      | Complete physical source lines                |
| `Ctrl-V` | Display-column block rectangle                |

Motions extend the active endpoint, `o` swaps endpoints, and `Esc`/`Ctrl-C`
cancels. `y`, `d`/`x`, `c`, `>` and `<` operate on raw Markdown; generated
bullets, borders, padding, and link indexes never enter registers. Named,
black-hole, unnamed, numbered/small-delete, and `"+` clipboard registers keep
their Vim semantics, including shape-aware `p`/`P` and one-step block undo.
