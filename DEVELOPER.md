# Integrating oom-edit into a Rust host

Use `oom_edit::EditorPane` for the full editor experience, or
`oom_edit_core::EditorSession` for the terminal-free engine. The crate-root
re-exports are the supported APIs; App, rendering adapters, the command
dispatcher and implementation modules remain private. The standalone binary
uses the same public pane. There is no mutable session escape from the pane.

The [host requirement reachability checklist](docs/host-api-reachability.md)
maps every editor-dependent oom PRD clause to public entry points, tested
examples and the responsibilities that remain in the consuming host.

## Dependency and source identity

The canonical origin is `https://github.com/jsco2t/oom-edit`. The editor and
**all four downstream patches** must select one immutable revision. A
dependency's Cargo patches do not propagate into its consumer's workspace.

The example below pins the published `v0.6.0` commit. To consume a newer
release, replace all five revisions with its same full commit SHA and use the
[make-owned source verification](docs/downstream-consumer.md). Verify any tag
against the gated SHA after publication.

```toml
[dependencies]
oom-edit = { git = "https://github.com/jsco2t/oom-edit", rev = "87d5b48f766eb35f30c2136e23d2a4448329713b" }

[patch.crates-io]
hjkl-buffer = { git = "https://github.com/jsco2t/oom-edit", rev = "87d5b48f766eb35f30c2136e23d2a4448329713b" }
hjkl-engine = { git = "https://github.com/jsco2t/oom-edit", rev = "87d5b48f766eb35f30c2136e23d2a4448329713b" }
tree-sitter-md = { git = "https://github.com/jsco2t/oom-edit", rev = "87d5b48f766eb35f30c2136e23d2a4448329713b" }
dirs-sys = { git = "https://github.com/jsco2t/oom-edit", rev = "87d5b48f766eb35f30c2136e23d2a4448329713b" }
```

Replace all five revisions together for another candidate. Retain your own
Cargo.lock and vendor configuration; do not inherit this repository's lock or
assume a warm Cargo cache proves consumption. Explicitly allow the canonical
origin in your cargo-deny `[sources].allow-git` policy, without allowing
arbitrary Git dependencies. Keep your own dependency/license audit policy.

The [independent consumer workflow](docs/downstream-consumer.md) creates an
out-of-workspace project with its own lock/vendor/config and then checks it
offline/locked with a fresh empty Cargo home. It asserts both actual resolved
sources and all four patch revisions, runs warning-denied Clippy and drives the
public pane. Preparation alone may fetch. Its negative check rejects incorrect
source identity and real source-byte corruption.

## Construction and configuration

Construct a `PaneInit` explicitly. An empty initial-path list creates zero
tabs; call `new_buffer` if your host wants an unnamed starting buffer.
Construction reports each initial path independently, retaining successful
tabs when another path fails. Display its warnings/errors using host-owned
chrome; the pane never discovers argv, process configuration, terminal setup
or environment during construction.

`Config` is owned and serde-compatible with the standalone schema. Deserialize
it from your host's sub-table instead of copying a second schema. Validate it
against your explicit configuration base directory and display typed warnings.
Zero wrap width falls back to 100, invalid theme mode to dark, unknown spelling
language to en_US, and an unreadable additional dictionary disables spelling
for the run. Use `ThemeConfig::set_dark/set_light` when constructing explicit
theme slots so presence has the same meaning as a written TOML slot.

Load user themes through `ThemeCatalog::load_from_base`, or use built-ins only.
Pass a `ThemeSelection` with an explicit name/appearance/tier; selection does
not read the environment. A host may deliberately capture `ThemeEnvironment`
and derive selection at its process boundary, or supply its own policy.
User themes and relative dictionaries use the supplied configuration base;
personal dictionary persistence uses its separate supplied filename.

Provide your own clipboard sink, theme persistence sink, document file policy
and working directory. `AllowAllFileAccess` is for unrestricted integrations,
not vault containment. A vault host should authorize canonical existing paths
and canonical parents of new paths, including symlinks, and reject outside
targets. File policy remains active for bang/force commands. A noncooperating
writer can still race a final filesystem replacement; these checks are not an
OS-enforced sandbox or portable cross-process compare-and-swap.

The following complete `src/lib.rs` sample is byte-identical to the independently
compiled consumer fixture. It deliberately disables clipboard and theme
persistence, performs no document writes, and exercises a full public lifecycle.
The same sample also runs in `make test-package-guards`.

```rust
//! Independently compiled public consumer, with no private or terminal imports.

#[cfg(test)]
mod tests {
    use oom_edit::{
        AllowAllFileAccess, ClipboardError, ClipboardSink, Config, ConfigPersistError, DisplayMode,
        EditorPane, KeyCode, KeyCodeKind, KeyInput, Mode, Modifiers, OpenOptions, PaneEvent,
        PaneInit, PaneInput, PaneOptions, PaneServices, ThemeCatalog, ThemePersistenceSink,
        ThemeSelection, ThemeSlot, Tier,
    };
    use std::time::Instant;

    struct DisabledPersistence;
    impl ClipboardSink for DisabledPersistence {
        fn copy(&mut self, _: &str) -> Result<(), ClipboardError> {
            Ok(())
        }
    }
    impl ThemePersistenceSink for DisabledPersistence {
        fn persist_theme(&mut self, _: ThemeSlot, _: &str) -> Result<(), ConfigPersistError> {
            Ok(())
        }
    }
    fn key(kind: KeyCodeKind) -> PaneInput {
        PaneInput::Key(KeyInput {
            code: KeyCode { kind },
            mods: Modifiers::default(),
        })
    }

    #[test]
    fn fr_112_independent_consumer() {
        let base = std::env::current_dir().unwrap();
        let now = Instant::now();
        let construction = EditorPane::construct(PaneInit {
            config: Config::default(),
            theme_catalog: ThemeCatalog::builtins(),
            theme_selection: ThemeSelection::new(None, DisplayMode::Dark, Tier::Monochrome),
            services: PaneServices {
                clipboard_sink: Box::new(DisabledPersistence),
                theme_sink: Box::new(DisabledPersistence),
                file_access_policy: Box::new(AllowAllFileAccess),
                config_base_directory: base.clone(),
                personal_dictionary_path: base.join("personal.txt"),
                working_directory: base,
            },
            options: PaneOptions::default(),
            initial_paths: Vec::new(),
            now,
        });
        assert!(construction.report.paths.is_empty());
        let mut pane = construction.pane;
        assert!(pane.tabs().is_empty());
        let id = pane.new_buffer(OpenOptions::default()).unwrap();
        let first = pane.render(80, 24, now);
        assert_eq!(first.cells.len(), 80 * 24);
        assert!(!pane.hints().is_empty());
        assert_eq!(pane.bindings().len(), 55);
        pane.handle_input(key(KeyCodeKind::Char('i')), now);
        pane.handle_input(PaneInput::Paste("# Independent λ\n".into()), now);
        assert_eq!(pane.status().unwrap().mode, Mode::Insert);
        assert_eq!(pane.text(&id).unwrap(), "# Independent λ\n");
        let inserted = pane.render(80, 24, now);
        assert_eq!(inserted.cells.len(), 80 * 24);
        assert!(inserted.cursor.is_some());
        assert!(!inserted.visually_equals(&first));
        pane.handle_input(key(KeyCodeKind::Esc), now);
        pane.handle_input(key(KeyCodeKind::Char('u')), now);
        assert_eq!(pane.text(&id).unwrap(), "");
        assert!(!pane.tabs()[0].dirty);
        pane.drain_events();
        let request = pane.close_all().unwrap();
        let events = pane.drain_events();
        assert!(
            matches!(events.as_slice(), [PaneEvent::Closed { request: closed, .. }, PaneEvent::AllClosed { request: all }, PaneEvent::Completed { request: done }] if [closed, all, done].iter().all(|id| **id == request))
        );
        assert!(pane.tabs().is_empty());
        assert_eq!(pane.render(80, 24, now).cells.len(), 80 * 24);
    }
}
```

## Input, focus and time

Translate terminal reports once at your boundary into `KeyInput`, owned paste
text or pane-local mouse coordinates. Dispatch **presses only**; ignore repeat
and release reports. Preserve modifiers and normalize the legacy control
aliases and Shift-Tab according to [the shared contract](docs/terminal-input.md).
The standalone and split example exercise the same key vectors with keyboard
enhancement enabled and disabled.

Intercept your explicit host-global keys before pane dispatch. Reservations
are optional intentional overrides, not a guarantee that the editor ignores
those keys: Alt characters can insert text, Ctrl-g can be owned by a pending
grammar, and a modal interaction can consume function keys. Consult structured
bindings/ownership to validate configurable shortcuts. Do not turn reference
metadata into a second dispatcher or add host commands to the editor palette.

Call `set_focused` when editor focus changes. It suppresses editing/cursor and
cancels App Space chords/drag state on focus loss; native grammar and modal
state suspend rather than being fabricated with an Escape. Remove the pane's
screen origin once from mouse coordinates before dispatch.

Sample an event timestamp **after** reading the event and before dispatch.
Call `tick(now)` even while the pane is hidden, honoring `next_deadline` and
redraw requests. When `idle_due` allows spelling work, call bounded
`idle_unit(max_bytes)` units and measure actual runtime in the host; do not
make one unbounded idle call or use a fresh wall clock inside pure layout.

## Owned frames and host chrome

Render with pane width/height, not whole-screen geometry. `PaneFrame` is an
immutable row-major owned snapshot; earlier frames remain valid after edits.
Apply the screen origin once when copying cells and once when projecting its
cursor. A continuation cell is the hidden half of a wide grapheme and must
not print independently. Preserve explicit default/indexed/RGB colors,
underline color and both applied/removed modifiers. Missing color means leave
the existing value alone; Default explicitly resets it.

Use `hints()`, `which_key()` and `status()` for host chrome. Hints include
compact text and disabled state in registry order; which-key appears after
150 ms of focused pending Space input. Set `inline_hints=false` to avoid
competing hint rows. The pane still owns its editor line. Query catalog styles
for your host surfaces and preserve non-color state signals. Cursor shape and
terminal escape output are the host's responsibilities.

Set `PaneOptions::minimal_status_bar=true` to leave only the mode badge and
right-side spelling/ruler indicators in the pane's editor line. This option
also suppresses inline hints and which-key, even when `inline_hints=true`.
Active ex and search prompts remain visible while typing. File details,
dirty markers and transient notices are hidden in the pane row. File state
remains available through `status()` and lifecycle events, but ordinary
transient notices are not exported for separate host rendering.

The [living split host](crates/oom-edit/examples/embedded.rs) and its
[consumer-owned adapter](crates/oom-edit/examples/support/embedded_host.rs)
demonstrate complete drawing, styles, mouse translation, global interception,
host hints, ticking, bounded idle work, terminal cursor shape and event drain.

## Prepared lifecycle and disk coordination

TabId and RequestId are opaque and pane-generation-scoped. Capture identities
for continuations; never finish a request by re-reading the active tab.
Drain events after inputs, ticks, idle work and host operations. Exactly one
Completed/Cancelled/Failed terminal result correlates with an accepted request;
progress events retain that request identity. DiskWriteCommitted is a distinct
partial-durability outcome, not a Saved event or permission to auto-close.

For ordinary host exit, use `close_all()`: dirty decisions are resolved in
tab order, then auto-committed. AttentionRequired identifies a tab needing
presentation. An unnamed save emits SavePathRequested; provide an explicit
filename or cancel, and keep host path input exclusive. Do not invent a path.
The split host tests successful save, cancellation and failure recovery.

For coordinated delete/trash/vault changes, use `prepare_close(targets)`.
Resolve confirmations and obtain the single-use token after PreparedClose.
Only then perform your recoverable host filesystem operation. Commit the token
after success; abort after failure. Discard choices retain text/undo until
commit; successful prerequisite saves remain saved after cancellation.
Prepared targets are frozen and conflicting requests return Busy.

For rename/move, prepare a batch of `Retarget` mappings before moving files.
Commit only after the filesystem succeeds; abort on failure. Destination
validation is all-or-none, preserves identities/text/undo/dirty state and does
not silently acknowledge unrelated destination bytes. Tokens suspend pane
document I/O and polling while the host applies the move.

For a broader external sync operation, obtain `begin_external_change()`,
perform the host mutation, then commit with affected paths or abort. Ordinary
editing may continue during this lease; document I/O/polling may not.
An abandoned token is cancelled on the next pane boundary, not a successful
commit. Do not drop a token as your normal error-handling plan.

Routine document metadata polling occurs at most once per backed tab per two
seconds. Notify changed paths after your transaction so equal-size/equal-mtime
replacements are content-validated. Notifications are not accepted during a
held mutation lease; use its completion protocol. Clean active focused Normal
tabs reload only when search, pending grammar, overlays and lifecycle requests
are absent. Dirty tabs retain edits and get version-bound choices; missing
files retain text and require explicit recreation. Hidden observations set a
non-color marker and DiskChangePending, never AttentionRequired.

## Notices and terminal ownership

Ship `third_party_notices()` with your host: bundled SCOWL and theme permission
notices and warranty disclaimers must travel with the application. The notices
are crate-local for independently vendored consumers and byte-identical to
the canonical root document.

The host decides whether to use `TerminalGuard`. One guard may be active per
process; a second acquisition returns a typed error. Choose mouse, paste and
supported keyboard-enhancement options explicitly. Acquired features are
restored on drop, panic, partial failure and handled fatal signals; prior panic
hooks are chained. Mouse capture may be toggled while active. The core has no
terminal dependency. The pane itself neither acquires a terminal nor writes
stdout/stderr; `Osc52Clipboard::new(writer)` deliberately writes only through
the host-supplied writer.

## Runnable checks

```console
make build-examples
make run-embedded ARGS=path/to/note.md
make run-embedded ARGS="--legacy-keys path/to/note.md"
make test-embedding-example
make test-package-guards
make coverage-check
make doc
```

For independent candidate preparation/checks and exact-tag verification, use
[the documented make workflow](docs/downstream-consumer.md). Published-tag
consumption runs only after separate release authorization, never as a gate
that requires the future tag to exist.
