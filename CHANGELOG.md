# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0] — 2026-08-03

### Added

- **`oom-edit-core` crate**: embeddable markdown editing engine with Vim-style modal editing, tree-sitter syntax highlighting, and renderer-agnostic styling.
- `EditorSession`: main editing session type with full key-routing (Normal, Insert, Visual, View modes).
- `render_source(Viewport) -> SourceFrame`: renders highlighted source with cursor, Visual selections, and search-match ranges.
- `render_view(width) -> &ViewLayout`: cached view layout with invalidation on edit, width change, or front-matter toggle.
- `cursor_line()` / `view_cursor_line()`: exposes cursor position for host-owned scrolling.
- `Effect::HelpRequested`: emitted when `:help` is invoked.
- Public API re-exports in `lib.rs` — the `pub use` list is the complete API contract.
- Rustdoc examples on `EditorSession::from_text`, `handle_key`, `render_source`, `render_view`.
- Headless example (`examples/headless.rs`): demonstrates session usage, key scripting, and rendering.
- Dependency-hygiene test (FR-8.1): asserts no terminal/async dependencies in `oom-edit-core`.
- Session-level integration tests: dirty-flag lifecycle, `:wq`, yank, help effects.

### Fixed

- Incremental highlighting correctness: fixed `apply_edit` to handle backward-range edits from `hjkl` and zero-length insertions without corrupting the tree-sitter parse state.

[0.1.0]: https://github.com/anomalyco/oom-edit/releases/tag/v0.1.0
