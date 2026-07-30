# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
