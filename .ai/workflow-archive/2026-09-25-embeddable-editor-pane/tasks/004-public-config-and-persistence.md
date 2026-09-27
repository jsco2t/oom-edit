# Task 004: Public configuration and safe persistence

Delegation: main-only

## Goal

Make the complete editor configuration host-owned and serializable while preserving standalone defaults and making theme/dictionary writes safe.

## Context

Current `ConfigPresence` is private and separate from `Config`; App reloads/saves the whole config during theme cycling. Hosts need explicit roots and sinks with no implicit process reads.

## Scope

### In scope

FR-060–065 and FR-067: public config and validation, slot presence, base theme/dictionary directories, separate personal dictionary path, theme persistence sink and standalone semantic TOML update.

### Out of scope

The DRD-excluded Nice FR-066 comment/format preservation and oom's own config file/layering.

## Implementation requirements

- Add red tests for every key/default, serde sub-table roundtrip, explicitly set theme slots, wrap_width=0, unknown language, invalid theme mode, unreadable extra dictionary, directory resolution and separate dictionary path.
- Fold slot presence into the public config representation without breaking existing key names/serde/default behavior. Public validation returns typed warnings and exact current fallbacks; no hidden process env/config reads in pane construction.
- `Space t` calls an injected sink once; failure is visible. File-backed standalone sink rereads latest valid TOML, changes only the active theme value semantically, preserves all unknown/nested values, checks expected version, writes atomically and leaves malformed/concurrently changed bytes untouched.
- Personal dictionary additions use the supplied path with atomic write/error semantics. Add a guard that standalone reads no setting absent from public Config.
- Extend crate-root API docs and compile guards; do not expose TOML/serde parser types in signatures other than supported serde traits.

## Acceptance criteria

- [ ] Every config field and default roundtrips; host-built slot presence resolves like a written slot.
- [ ] Each validation fallback produces the exact typed warning and effective value.
- [ ] Theme persistence changes only the active semantic value; unrelated/unknown/nested values survive; malformed/concurrent files remain byte-identical with visible failure.
- [ ] Personal dictionary path and base directory are independent and no implicit editor config path is read by pane construction.
- [ ] Standalone/public config field-parity guard passes.

## Validation

`make test`

`make data-license-check`

## Dependencies

001

## Expected areas of change

`crates/oom-edit/src/config.rs`, `spell_host.rs`, `app.rs`, `lib.rs`, configuration/API tests and Makefile only if a new workflow is required.

## Risks / notes

The sink must not serialize a known-schema `Config` over a host's whole TOML document. Existing atomic-save failure seams are reusable; preserve unknown values semantically even though formatting preservation is excluded.
