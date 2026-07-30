# oom-edit

A console/TUI markdown editor written in Rust: true Vim-style modal editing
(Normal / Insert / Visual + a rendered, navigable **View** mode) over
tree-sitter-highlighted markdown, including rich highlighting of YAML/TOML
front matter and fenced code blocks.

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
make check         # fmt + lint + build + test — the CI gate
make run           # run the editor
make run ARGS=file.md   # open a file
```

**`make` is the build system of record.** Every developer-facing workflow has
a make target; CI runs identical commands.
