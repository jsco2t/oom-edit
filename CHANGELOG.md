# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Source-faithful YAML/TOML metadata panels with physical-line gutters, semantic styling, width-aware wrapping, and collapse/expand behavior.
- Character-wise (`v`), physical-line (`V`), and rectangular block (`Ctrl-V`) rendered selection with source-mapped atoms and raw-Markdown operators.
- `make run-isolated` and isolated test configuration, preventing verification from reading or persisting the user's real config.
- Source mode now wraps long lines by default, with blank continuation gutters and visual-row-aware cursor following.
- `[editor].wrap` configures the startup behavior, and `:set wrap` / `:set nowrap` switch it for the current session.
- No-wrap mode now follows the cursor horizontally and marks hidden content with `«` and `»` edge indicators.

### Changed

- **Breaking (unreleased 0.4.0):** `oom-edit-core` now exposes only its crate-root facade. Implementation modules, highlighter/parser protocols, document metadata, and renderer-to-Vim adapters are private; embedders should import `EditorSession`, terminal-neutral input types, effects, errors, front-matter values, and renderer-neutral output types directly from `oom_edit_core`. Wrap changes now use `Effect::SetWrap(bool)`.
- Fixed bindings now come from one typed registry, while editor-native `g`/count/tab grammar stays in core. Hosts receive typed `TabNext`, `TabPrev`, and one-based `TabJump` effects, and event time is injected after terminal reads.
- **Breaking (0.4.0):** `EditorSession::rendered_cursor()` now returns the public two-dimensional `RenderedPoint`; desired-column state and renderer navigation internals are private. `RenderedLine` exposes renderer-neutral metadata roles and source atoms; `RenderedSelection` exposes `SelectionShape`, anchor/active points, per-row display intervals, normalized source ranges, and block width. Both workspace crates and the exact internal dependency are now 0.4.0.
- Theme resolution now reports the active theme, effective palette, terminal capability, provenance, and display mode as one value. Missing or malformed config falls back to colored `default-dark`/`default-light`; `accessible` remains opt-in and monochrome.
- **Breaking (0.3.0):** the public mode model is now exactly `Normal`, `Insert`,
  `Select`, and `Command`. Normal and Select use rendered Markdown; Insert is
  the only raw-source editing surface. View-specific render and cursor APIs
  were replaced by `render_layout`, `rendered_cursor`,
  `rendered_cursor_line`, `rendered_layout`, and `rendered_selection`. Public
  rendered value types now use the `Rendered*` prefix and are available from
  `oom_edit_core` and `oom_edit_core::style`; renderer construction and
  navigation helpers are private implementation details. `Mode::View`, `Mode::Visual`,
  `Mode::VisualLine`, and `Mode::VisualBlock` were removed; rendered Normal
  and linewise Select are their user-visible replacements. `Effect::SaveRequested`
  now includes `retarget`, and `EditorSession::save_copy` supports `:w {path}`
  without changing the session's active path or clean state.
- **Breaking (0.2.0):** `oom-edit-core::Viewport` now accepts `wrap`, `left_col`, and `skip_rows`; `SourceFrame` exposes per-row `line_numbers`; and `Effect::SetOption` delegates runtime option changes to hosts. `oom_edit::Config` now includes `editor: EditorConfig` and `relative_line_numbers: bool` fields. Embedders must initialize the new viewport/config fields and handle the new effect variant.

### Fixed

- Rendered fenced code now uses a subtle full-width theme surface while preserving syntax colors, selection, cursor emphasis, and color-free accessible rendering.
- Mode badges now use crisp black text and leave a one-cell status-row-colored gap before adjacent status content.
- Space-q and `:q` now share captured-tab dirty confirmation; save/discard/cancel, external overwrite/reload, and `:wq` continuations act on the original tab. Dirty non-force `:tabclose`, `:e`, and `:qa` consistently refuse with their force remedy, while forced variants affect only their requested target.
- `gt`, `gT`, counted `gt`, and Space-digit tab switching now use one typed routing path without stealing editor-native `gg`, `gu`, or `gU` prefixes.
- Rendered and source scrolling now avoid document-sized navigation, edit-validation, and line-index work on each key or frame, restoring responsive Insert edits and keyboard/mouse scrolling near the 1 MB boundary.
- Rendered Select no longer expands every operation to a whole rendered row; partial, wrapped, Unicode, metadata, and block selections preserve exact UTF-8 source ownership and omit synthetic glyphs.
- Expanded front matter no longer sorts and compacts parsed values into a pseudo-table, so source order, comments, blank lines, nesting, and scalar spelling remain visible.
- Rendered Markdown now preserves nested-list source order and renders depth-specific bullets, declared ordered-list numbers, nested task markers, and aligned continuations.
- Rendered navigation now reuses the renderer-selected layout width without rebuilding and cloning the document on every scroll key, eliminating width-dependent CPU thrash and lag.
- Theme cycling now keeps light and dark config slots mode-compatible, the default-dark TrueColor palette uses the Zed OneDark Markdown colors with a custom H2/H3 order, and rendered headings apply their per-level colors to heading text as well as markers.
- Source and rendered Markdown now assign the same semantic styles to shared heading, table-header, blockquote, fenced-code fallback, and link-destination payload.
- The command palette now clears its full surface, grows responsively from a 40×12 floor, keeps the filter fixed, and scrolls to keep the selected command or Vim reference visible.
- The bottom application row now keeps a fixed, mode-colored badge at the left edge, uses a distinct full-row background, shows unique registry-derived hints, and opens Help through `Space h` without an F1 binding.
- Line numbers now leave a two-cell gap before content and are absolute by default; `relative_line_numbers = true` enables hybrid-relative numbering in rendered Normal, Select, and Command while Insert remains absolute.
- CRLF files without a final newline no longer gain a stray carriage return when saved.
- Rendered-mode cursor-line highlighting now spans the full viewport width while preserving semantic syntax styles.
- rendered mode now preserves the visible reading position when terminal resizing changes line wrapping.
- Edit mode now renders a single status row and uses the reclaimed row for editor content.
- Nested links, headings, and footnotes in lists and blockquotes now retain document-level rendered metadata, use sequential link markers, and finalize footer panels only once.
- Words that end exactly at the rendered wrap boundary no longer wrap prematurely to the next line.
- Semantic highlight spans now consistently use character indices, fixing non-ASCII styling alignment in source highlighting, fenced code, front matter, tables, and wrapped rendered content.
- Fenced-code and front-matter injections now use their grammar-specific highlight queries, preserve full-region parse context for partial viewports, and keep language styles visible over the Markdown code-block fallback.
- Injection discovery now reuses compiled queries and avoids cloning the document text and Markdown syntax tree after every edit.

## [0.1.0] — 2026-08-03

### Added

- **`oom-edit-core` crate**: embeddable markdown editing engine with Vim-style modal editing, tree-sitter syntax highlighting, and renderer-agnostic styling.
- `EditorSession`: main editing session type with the original Normal, Insert, Visual, and View routing.
- `render_source(Viewport) -> SourceFrame`: rendered highlighted source with cursor, Visual selections, and search-match ranges.
- `render_layout(width) -> &RenderedLayout`: cached rendered layout with invalidation on edit or width change.
- Legacy `cursor()` / `view_cursor_line()` exposed cursor positions for host-owned scrolling.
- `Effect::HelpRequested`: emitted when `:help` is invoked.
- Public API re-exports in `lib.rs` — the `pub use` list is the complete API contract.
- Rustdoc examples on `EditorSession::from_text`, `handle_key`, `render_source`, `render_layout`.
- Headless example (`examples/headless.rs`): demonstrates session usage, key scripting, and rendering.
- Dependency-hygiene test (FR-8.1): asserts no terminal/async dependencies in `oom-edit-core`.
- Session-level integration tests: dirty-flag lifecycle, `:wq`, yank, help effects.

### Fixed

- Incremental highlighting correctness: fixed `apply_edit` to handle backward-range edits from `hjkl` and zero-length insertions without corrupting the tree-sitter parse state.

[0.1.0]: https://github.com/anomalyco/oom-edit/releases/tag/v0.1.0
