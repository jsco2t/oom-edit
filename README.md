# oom-edit

A console/TUI markdown editor written in Rust with four modes: rendered
**Normal**, source **Insert**, rendered character/line/block **Select**, and **Command**.
Markdown is rendered and navigable by default, with tree-sitter highlighting
for source editing, YAML/TOML front matter, and fenced code blocks.

The editing core ships as the reusable `oom-edit-core` crate — embeddable in
other applications with zero terminal dependencies.  The `oom-edit` binary is
a thin ratatui shell over it.

**License:** MIT.

## Planning docs

Engineering plans live outside this repo in the project notebook:

- [Project index](https://github.com/earendil-works/notebook/tree/main/projects/oom-edit)
- [Plan & specification](https://github.com/earendil-works/notebook/tree/main/projects/oom-edit/plan.md)
- [Architecture](https://github.com/earendil-works/notebook/tree/main/projects/oom-edit/architecture.md)
- [Task list](https://github.com/earendil-works/notebook/tree/main/projects/oom-edit/tasks/index.md)

## Quickstart

```bash
make help          # list all targets
make check         # fmt + lint + build + test + deny + audit — the CI gate
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

## Rendered editing

Expanded YAML/TOML front matter is a source-faithful metadata panel: comments,
blank lines, nesting, quoting, order, and physical line numbers are retained.
Long metadata lines wrap inside the panel, with blank gutters on continuation
rows; `z` collapses or expands it.

Select follows Vim's three shapes:

| Key | Shape |
| --- | --- |
| `v` | Character-wise, inclusive source-backed atoms |
| `V` | Complete physical source lines |
| `Ctrl-V` | Display-column block rectangle |

Motions extend the active endpoint, `o` swaps endpoints, and `Esc`/`Ctrl-C`
cancels. `y`, `d`/`x`, `c`, `>` and `<` operate on raw Markdown; generated
bullets, borders, padding, and link indexes never enter registers. Named,
black-hole, unnamed, numbered/small-delete, and `"+` clipboard registers keep
their Vim semantics, including shape-aware `p`/`P` and one-step block undo.
