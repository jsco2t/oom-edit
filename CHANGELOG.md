# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Source mode now wraps long lines by default, with blank continuation gutters and visual-row-aware cursor following.
- `[editor].wrap` configures the startup behavior, and `:set wrap` / `:set nowrap` switch it for the current session.
- No-wrap mode now follows the cursor horizontally and marks hidden content with `«` and `»` edge indicators.

### Changed

- **Breaking (0.2.0):** `oom-edit-core::Viewport` now accepts `wrap`, `left_col`, and `skip_rows`; `SourceFrame` exposes per-row `line_numbers`; and `Effect::SetOption` delegates runtime option changes to hosts. `oom_edit::Config` now includes `editor: EditorConfig` and `relative_line_numbers: bool` fields. Embedders must initialize the new viewport/config fields and handle the new effect variant.

### Fixed

- View navigation now reuses the renderer-selected layout width without rebuilding and cloning the rendered document on every scroll key, eliminating width-dependent CPU thrash and lag.
- Theme cycling now keeps light and dark config slots mode-compatible, the default dark TrueColor palette uses exact Zed OneDark Markdown RGB values, and View headings apply their per-level colors to heading text as well as markers.
- The command palette now clears its full surface, grows responsively from a 40×12 floor, keeps the filter fixed, and scrolls to keep the selected command or Vim reference visible.
- The bottom application row now keeps a fixed, mode-colored badge at the left edge, uses a distinct full-row background, shows unique registry-derived hints, and opens Help through `Space h` without an F1 binding.
- Source-editor line numbers now leave a two-cell gap before content and are absolute by default; the top-level `relative_line_numbers = true` setting restores hybrid-relative numbering in Normal, Visual, and Command modes.
- CRLF files without a final newline no longer gain a stray carriage return when saved.
- View-mode cursor-line highlighting now spans the full viewport width while preserving semantic syntax styles.
- View mode now preserves the visible reading position when terminal resizing changes line wrapping.
- Edit mode now renders a single status row and uses the reclaimed row for editor content.
- Nested links, headings, and footnotes in lists and blockquotes now retain document-level View metadata, use sequential link markers, and finalize footer panels only once.
- Words that end exactly at the View wrap boundary no longer wrap prematurely to the next line.
- Semantic highlight spans now consistently use character indices, fixing non-ASCII styling alignment in source highlighting, fenced code, front matter, tables, and wrapped View content.
- Fenced-code and front-matter injections now use their grammar-specific highlight queries, preserve full-region parse context for partial viewports, and keep language styles visible over the Markdown code-block fallback.
- Injection discovery now reuses compiled queries and avoids cloning the document text and Markdown syntax tree after every edit.

## [0.1.0] — 2026-08-03

### Added

- **`oom-edit-core` crate**: embeddable markdown editing engine with Vim-style modal editing, tree-sitter syntax highlighting, and renderer-agnostic styling.
- `EditorSession`: main editing session type with full key-routing (Normal, Insert, Visual, View modes).
- `render_source(Viewport) -> SourceFrame`: renders highlighted source with cursor, Visual selections, and search-match ranges.
- `render_view(width) -> &ViewLayout`: cached view layout with invalidation on edit or width change.
- `cursor()` / `view_cursor_line()`: exposes cursor position for host-owned scrolling.
- `Effect::HelpRequested`: emitted when `:help` is invoked.
- Public API re-exports in `lib.rs` — the `pub use` list is the complete API contract.
- Rustdoc examples on `EditorSession::from_text`, `handle_key`, `render_source`, `render_view`.
- Headless example (`examples/headless.rs`): demonstrates session usage, key scripting, and rendering.
- Dependency-hygiene test (FR-8.1): asserts no terminal/async dependencies in `oom-edit-core`.
- Session-level integration tests: dirty-flag lifecycle, `:wq`, yank, help effects.

### Fixed

- Incremental highlighting correctness: fixed `apply_edit` to handle backward-range edits from `hjkl` and zero-length insertions without corrupting the tree-sitter parse state.

[0.1.0]: https://github.com/anomalyco/oom-edit/releases/tag/v0.1.0
