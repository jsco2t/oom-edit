# Dependencies

All direct dependencies of `oom-edit-core`, with license, rationale, and hand-roll assessment.

## Runtime dependencies

| Crate | Exact version | License | Why it's needed | Could we hand-roll this? |
| --- | --- | --- | --- | --- |
| `hjkl-engine` | `=0.39.0` | MIT | Core editing engine: document model, undo/redo, search, commands. Provides the `Engine` type that `oom-edit-core` wraps. | No. Deeply complex state machine with undo/redo, search, and command dispatch. Hand-rolling would be thousands of lines. |
| `hjkl-buffer` | `=0.39.0` | MIT | Rope-based text buffer with cursor/selection model. Required by `hjkl-engine`. | No. Rope data structure with offset tracking is non-trivial. |
| `hjkl-vim` | `=0.39.0` | MIT | Vim modal editing (Normal/Insert/Visual modes). Provides the Vim state machine. | No. Complete Vim keymap, command parsing, and mode transitions. |
| `tree-sitter` | `=0.26.11` | MIT | Incremental parser runtime. Required by all grammar crates. | No. C FFI bindings to the tree-sitter library. |
| `tree-sitter-md` | `=0.5.3` | MIT | Markdown grammar for syntax highlighting of fenced blocks and front matter. | No. Hundreds of rules for markdown syntax. |
| `tree-sitter-rust` | `=0.24.2` | MIT | Rust grammar (for highlighting Rust code blocks). | No. |
| `tree-sitter-python` | `=0.25.0` | MIT | Python grammar (for highlighting Python code blocks). | No. |
| `tree-sitter-javascript` | `=0.25.0` | MIT | JavaScript grammar (for highlighting JS code blocks). | No. |
| `tree-sitter-typescript` | `=0.23.2` | MIT | TypeScript grammar (for TS/TSX code blocks). | No. |
| `tree-sitter-go` | `=0.25.0` | MIT | Go grammar (for Go code blocks). | No. |
| `tree-sitter-bash` | `=0.25.1` | MIT | Bash/shell grammar (for shell code blocks). | No. |
| `tree-sitter-json` | `=0.24.8` | MIT | JSON grammar (for JSON code blocks). | No. |
| `tree-sitter-c` | `=0.24.2` | MIT | C/C++ grammar (for C code blocks). | No. |
| `tree-sitter-yaml` | `=0.7.2` | MIT | YAML grammar (for YAML front matter). | No. |
| `tree-sitter-toml-ng` | `=0.7.0` | MIT | TOML grammar (for TOML front matter). Replaced `tree-sitter-toml` due to tree-sitter ABI incompatibility (see below). | No. |
| `pulldown-cmark` | `=0.13.4` | MIT | Markdown parser for rendering and offset-based event iteration. Used to parse markdown and map front-matter boundaries. | No. Full CommonMark parser with extensions. |
| `gray_matter` | `=0.3.2` | MIT | Front-matter extraction (YAML + TOML). Parses `---` delimited metadata blocks at document start. | Small: front-matter extraction is ~50 lines of regex. Could be hand-rolled, but gray_matter is well-tested and handles edge cases (quoted values, escape sequences). |

## Patched dependencies

| Crate | Version | License | Why patched | Patch |
| --- | --- | --- | --- | --- |
| `dirs-sys` | `0.5.0` (local fork) | MIT OR Apache-2.0 | Removed transitive dependency on `option-ext` (MPL-2.0, copyleft). Hand-rolled the 3-line `OptionExt::contains` utility locally. | `crates/dirs-sys-patched/` + `[patch.crates-io]` in root `Cargo.toml` |

## Tree-sitter ABI note

`tree-sitter-toml` (v0.20.0) depended on `tree-sitter = "0.20"`, which is ABI-incompatible with the `tree-sitter = "0.26"` used by all other grammar crates. This caused a dual-version conflict where `tree_sitter::Language` types from different versions were incompatible. The fix was to replace `tree-sitter-toml` with `tree-sitter-toml-ng` (v0.7.0), which uses `tree-sitter-language` (version-independent) like the other grammar crates.

## Transitive dependency risks

- **`hjkl` ecosystem**: `hjkl-engine` → `hjkl-bonsai` → `hjkl-xdg` → `dirs` → `dirs-sys` (patched). The `hjkl` crates are pinned pre-1.0 and fast-moving.
- **Duplicate transitive versions**: `crossterm`, `hashbrown`, `rustix`, `syn`, `toml`, `toml_datetime`, `winnow` each have 2-3 versions in the dependency graph due to feature flags in transitive dependencies. This is expected and harmless but increases binary size.
