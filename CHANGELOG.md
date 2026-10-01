# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

The 0.6.5 release candidate is not yet tagged or published.

### Changed

- Retained source analysis, rendered blocks, rows, and indexes across ordinary
  edits so large-note editing and navigation avoid full-document rebuilds.
- Made rendered Select, full-line deletion and change, and large code-fence
  edits responsive while preserving exact source mapping and yank behavior.
- Added a 1 MiB kitchen-sink fixture, public-pane interaction benchmarks,
  host-parity checks, and repeated RSS-stability gates.

## [0.6.0] - 2026-09-27

This release introduced the embeddable editor pane and standalone parity.

### Added

- Public `EditorPane` with explicit services, owned input and immutable cell
  frames, focus/timing/idle control, tab metadata, hints and binding ownership.
  The standalone binary uses the same pane implementation.
- Prepared close/retarget and external-change tokens, stable tab/request IDs,
  typed ordered events, and policy-checked document I/O, including bang actions.
- Public configuration, theme/style catalog, clipboard services, configurable
  terminal guard and complete third-party notices; lightweight read-only core
  Markdown analysis does not construct an editing engine.
- Tested embedding guide and split-pane example, independent immutable-Git
  consumer checks, strict executable requirement coverage and API/privacy guards.

- Added shared two-second disk polling and explicit path notifications, with
  focused Normal safe-point reloads, version-bound dirty-buffer decisions and
  non-color pending/missing/error markers.

- Added configurable `[editor] wrap_width` for source and rendered prose, with
  a 100-column default and the existing 80-column table floor preserved.
- Added the Normal-mode `Space m` command for an exact, undoable default YAML
  front-matter template, with safe refusal when front matter already exists.
- Added uppercase `Y` in rendered Select for one-shot syntax-free clipboard
  output while retaining exact Markdown in internal registers.
- Added copyable rendered link-index rows in Normal and Select and configurable
  mode-specific terminal cursor shapes.
- Added a `[clipboard] copy_format` setting that defaults to exact Markdown and
  supports syntax-free `plain-text` clipboard output.

### Changed

- Terminal-reported Shift+V now enters line Select and switches or cancels it
  like unmodified V; Ctrl+V remains block Select.

- Paste uses the rope's byte index instead of copying preceding lines, and
  atomic front-matter refresh borrows the synchronously updated text cache.

- Theme persistence updates the latest valid TOML semantically and atomically,
  retaining unrelated values and refusing malformed or concurrently changed data.

- Retained open buffers and undo after external disappearance; saving requires
  explicit version-checked recreation rather than silently recreating a file.

- Delayed spell work until five seconds after the latest input and kept
  unaffected misspelling decorations stable across repeated local edits.
- Fixed source Insert cursor projection on trailing empty lines and made source
  prose wrap at exact, Unicode-aware word boundaries without changing bytes.
- Preserved visible blank-row separation between sibling items in loose
  unordered, ordered, nested, and task lists.
- Made plain `y` in rendered Select preserve multiline whitespace and the actual
  Markdown source, including hidden delimiters, in both internal registers and
  canonical, padded OSC 52 output.
- Clarified and compacted command discovery, with consistently themed and
  aligned command-palette rows and grouped Space-prefix hints.
- Made rendered tables reflow to an 80-column floor and follow horizontal
  cursor movement when their content exceeds the viewport.
- Stabilized terminal presentation with changed-state-only redraws, resize
  coalescing, and synchronized-update framing around complete frames.

## [0.5.0] - 2026-08-15

### Added

- Added the dependency-free `oom-spell` 0.1.0 crate with resumable dictionary
  construction, text-generic tokenization/policy, lookup, and bounded
  deterministic suggestions.
- Added idle-budgeted spell diagnostics and non-destructive decorations to all
  four editor modes, with `Space s/a/z/d`, `]s`/`[s`, and
  `:set spell`/`:set nospell` workflows.
- Added en_US, en_CA, and en_AU SCOWL-generated word lists, reproducible
  manifests, complete attribution through `--licenses`, and fail-closed data
  license checks.
- Added configurable additional plain-wordlist dictionaries, an atomically
  persisted personal dictionary, and a provider-neutral Trouble overlay.

### Changed

- Bumped `oom-edit-core` and `oom-edit` to 0.5.0 for the new public diagnostic,
  decoration, position, and session spell-checking APIs.
- Extended `make check`, `make bench-check`, and `make bench` with asserting
  data-license and spell performance gates, including the 25 MiB engine heap
  ceiling and 1 MiB incremental-scan budgets.
