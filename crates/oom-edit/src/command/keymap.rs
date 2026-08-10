//! Trigger → [`Command`] resolution: the keymap and chord state machine.
//!
//! [`Keymap`] maps `(context, trigger) → command`. It is built once from a
//! static trigger table. Three consumers read it:
//!
//! - **Per-key dispatch** (`Keymap::matches`) — "does this key fire command X?"
//! - **Chord resolution** (`Keymap::resolve`) — Space-leader state machine.
//! - **Key rendering** (`Keymap::rendered_keys`) — palette and hint bar.
//!
//! Command *descriptions* live once in [`super::registry::COMMANDS`]; this
//! module carries triggers only.

use std::time::Instant;

use oom_edit_core::session::{KeyCode, KeyCodeKind, KeyInput};

use super::registry::{spec_for, Command, CommandSpec, Contexts};

// ── Trigger ─────────────────────────────────────────────────────────────────

/// How a command is triggered.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)]
pub enum KeyTrigger {
    /// A single key event.
    Key(KeyInput),
    /// A fixed two-key chord (e.g. `Space h`).
    Chord([KeyInput; 2]),
}

/// The outcome of feeding one key event to [`Keymap::resolve`].
#[derive(Debug)]
pub enum Resolution {
    /// A command fired.
    Command(Command),
    /// A chord prefix is pending; the pairs are the possible next keys and the
    /// commands they would reach (which-key data).
    #[allow(dead_code)]
    Pending(Vec<(KeyInput, &'static CommandSpec)>),
    /// The key is not a chord trigger — fall through to the engine.
    None,
}

// ── PendingChord state ─────────────────────────────────────────────────────

/// Mutable chord state the caller (`App`) owns and threads through
/// [`Keymap::resolve`]. `resolve` is a pure transition function over
/// `(self, ctx, ev, this)` — no hidden state.
///
/// `since` records when the current chord prefix began; the caller stamps it on
/// the transition into a pending prefix so the which-key render delay (150ms)
/// is a pure function of `(pending, now)`. `resolve` never reads it.
#[derive(Debug, Default)]
pub struct PendingChord {
    /// When the current prefix began (set by the caller on the empty→pending
    /// transition; the which-key delay measures against it).
    /// `None` when no prefix is pending.
    pub since: Option<Instant>,
}

impl PendingChord {
    /// Clear all pending chord state.
    pub fn reset(&mut self) {
        self.since = None;
    }
}

// ── Keymap ──────────────────────────────────────────────────────────────────

/// The keymap: trigger → command pairs, built from a static table.
///
/// Triggers:
/// - `v` / `V` / `Ctrl-V` → character / line / block Select
/// - Select operators/cancel are single-key registry projections
/// - `Space h` → Help
/// - `Space w` → Save
/// - `Space q` → Quit
/// - `Space t` → CycleTheme
pub struct Keymap {
    /// Single-key triggers: (key, command, contexts).
    single: Vec<(KeyInput, Command, Contexts)>,
    /// Two-key chord triggers: ([first, second], command, contexts).
    chords: Vec<([KeyInput; 2], Command, Contexts)>,
}

/// Convenience: a plain character key with no modifiers.
fn ch(c: char) -> KeyInput {
    KeyInput {
        code: KeyCode {
            kind: KeyCodeKind::Char(c),
        },
        mods: oom_edit_core::session::Modifiers::default(),
    }
}

fn ctrl(c: char) -> KeyInput {
    let mut key = ch(c);
    key.mods.ctrl = true;
    key
}

impl Keymap {
    /// Build the default keymap.
    pub fn default() -> Self {
        let space = ch(' ');

        let single: Vec<(KeyInput, Command, Contexts)> = vec![
            (ch('v'), Command::EnterCharacterSelect, Contexts::NORMAL),
            (ch('V'), Command::EnterLineSelect, Contexts::NORMAL),
            (ctrl('v'), Command::EnterBlockSelect, Contexts::NORMAL),
            (
                KeyInput {
                    code: KeyCode {
                        kind: KeyCodeKind::Esc,
                    },
                    mods: oom_edit_core::session::Modifiers::default(),
                },
                Command::CancelSelect,
                Contexts::SELECT,
            ),
            (ch('y'), Command::SelectYank, Contexts::SELECT),
            (ch('d'), Command::SelectDelete, Contexts::SELECT),
            (ch('x'), Command::SelectDelete, Contexts::SELECT),
            (ch('c'), Command::SelectChange, Contexts::SELECT),
            (ch('>'), Command::SelectIndent, Contexts::SELECT),
            (ch('<'), Command::SelectOutdent, Contexts::SELECT),
            (ch('o'), Command::SelectSwapAnchor, Contexts::SELECT),
        ];

        let chords: Vec<([KeyInput; 2], Command, Contexts)> = vec![
            (
                [space, ch('h')],
                Command::Help,
                Contexts::NORMAL.or(Contexts::SELECT),
            ),
            (
                [space, ch('w')],
                Command::Save,
                Contexts::NORMAL.or(Contexts::SELECT),
            ),
            (
                [space, ch('q')],
                Command::Quit,
                Contexts::NORMAL.or(Contexts::SELECT),
            ),
            (
                [space, ch('t')],
                Command::CycleTheme,
                Contexts::NORMAL.or(Contexts::SELECT),
            ),
        ];

        Self { single, chords }
    }

    /// Does `ev` trigger `command` (context-free)? Only `Key` rows match.
    #[allow(dead_code)]
    pub fn matches(&self, command: Command, ev: &KeyInput) -> bool {
        self.single
            .iter()
            .any(|(key, cmd, _)| *cmd == command && key_event_eq(key, ev))
    }

    /// Every trigger bound to `command`, in table order.
    #[allow(dead_code)]
    pub fn triggers_for(&self, ctx: Contexts, command: Command) -> Vec<KeyTrigger> {
        let mut out = Vec::new();
        for (key, cmd, cctx) in &self.single {
            if *cmd == command && cctx.contains(ctx) {
                out.push(KeyTrigger::Key(*key));
            }
        }
        for ([k1, k2], cmd, cctx) in &self.chords {
            if *cmd == command && cctx.contains(ctx) {
                out.push(KeyTrigger::Chord([*k1, *k2]));
            }
        }
        out
    }

    /// The human-readable key string for `command` (context-free): every
    /// trigger bound to it, in table order, joined with `" / "`.
    pub fn rendered_keys(&self, command: Command) -> Option<String> {
        let parts = self.rendered_key_parts(command);
        if parts.is_empty() {
            return None;
        }
        Some(parts.join(" / "))
    }

    /// The individual rendered key strings for `command`, in table order.
    pub fn rendered_key_parts(&self, command: Command) -> Vec<String> {
        self.single
            .iter()
            .filter(|(_, cmd, _)| *cmd == command)
            .map(|(key, _, _)| render_key(key))
            .chain(
                self.chords
                    .iter()
                    .filter(|(_, cmd, _)| *cmd == command)
                    .map(|([k1, k2], _, _)| format!("{} {}", render_key(k1), render_key(k2))),
            )
            .collect()
    }

    /// Resolve one key event against the app keymap.
    ///
    /// Space in NORMAL/SELECT starts a pending chord; any non-continuation key
    /// resets and yields `None` (falls through to the engine).
    ///
    /// # Space consumption
    ///
    /// Space itself is consumed by the chord machine — if the second key doesn't
    /// complete a chord, the Space is not re-delivered to the engine. This is
    /// acceptable: Vim's bare-Space motion is redundant with `l`.
    pub fn resolve(&self, ctx: Contexts, ev: &KeyInput, pending: &mut PendingChord) -> Resolution {
        // If we're already in a Space-pending state.
        if pending.since.is_some() {
            // Does `ev` complete a Space-chord?
            for ([space, second], cmd, cctx) in &self.chords {
                // First element must be Space; check context overlap.
                if cctx.contains(ctx) && key_event_eq(second, ev) && is_space_key(space) {
                    pending.reset();
                    return Resolution::Command(*cmd);
                }
            }
            // Non-continuation key: reset pending, fall through.
            pending.reset();
            return Resolution::None;
        }

        // Check single-key triggers.
        for (key, cmd, cctx) in &self.single {
            if cctx.contains(ctx) && key_event_eq(key, ev) {
                return Resolution::Command(*cmd);
            }
        }

        // Check if this key starts a chord (Space in NORMAL/SELECT).
        let is_space = is_space_key(ev);
        let in_chord_context = ctx.contains(Contexts::NORMAL) || ctx.contains(Contexts::SELECT);

        if is_space && in_chord_context {
            // Start pending chord.
            pending.since = Some(Instant::now());

            // Build continuations for which-key.
            let conts: Vec<(KeyInput, &CommandSpec)> = self
                .chords
                .iter()
                .filter(|([k1, _], _, cctx)| is_space_key(k1) && cctx.contains(ctx))
                .map(|([_, k2], cmd, _)| {
                    (
                        *k2,
                        spec_for(*cmd).expect("every chord command is registered"),
                    )
                })
                .collect();

            return Resolution::Pending(conts);
        }

        // Not a trigger.
        Resolution::None
    }

    /// Get the which-key continuations for a pending Space prefix.
    #[allow(dead_code)]
    pub fn continuations_for(&self, ctx: Contexts) -> Vec<(KeyInput, &'static CommandSpec)> {
        self.chords
            .iter()
            .filter(|([k1, _], _, cctx)| is_space_key(k1) && cctx.contains(ctx))
            .map(|([_, k2], cmd, _)| {
                (
                    *k2,
                    spec_for(*cmd).expect("every chord command is registered"),
                )
            })
            .collect()
    }
}

/// Compare two key events for dispatch.
fn key_event_eq(a: &KeyInput, b: &KeyInput) -> bool {
    a.code.kind == b.code.kind && a.mods == b.mods
}

/// Check whether a key input is an unmodified Space press.
fn is_space_key(key: &KeyInput) -> bool {
    matches!(key.code.kind, KeyCodeKind::Char(' '))
        && !key.mods.ctrl
        && !key.mods.alt
        && !key.mods.shift
}

/// Render a key for display.
fn render_key(key: &KeyInput) -> String {
    let base = match key.code.kind {
        KeyCodeKind::Noop => "Noop".to_string(),
        KeyCodeKind::Char(' ') => "Space".to_string(),
        KeyCodeKind::Char(c) if key.mods.ctrl => c.to_ascii_uppercase().to_string(),
        KeyCodeKind::Char(c) => c.to_string(),
        KeyCodeKind::F(n) => format!("F{n}"),
        KeyCodeKind::Backspace => "Backspace".to_string(),
        KeyCodeKind::Enter => "Enter".to_string(),
        KeyCodeKind::Tab => "Tab".to_string(),
        KeyCodeKind::Esc => "Esc".to_string(),
        KeyCodeKind::Left => "←".to_string(),
        KeyCodeKind::Right => "→".to_string(),
        KeyCodeKind::Up => "↑".to_string(),
        KeyCodeKind::Down => "↓".to_string(),
        KeyCodeKind::Home => "Home".to_string(),
        KeyCodeKind::End => "End".to_string(),
        KeyCodeKind::PageUp => "PageUp".to_string(),
        KeyCodeKind::PageDown => "PageDown".to_string(),
        KeyCodeKind::Delete => "Delete".to_string(),
        KeyCodeKind::BackTab => "Shift+Tab".to_string(),
    };
    let mut modifiers = Vec::new();
    if key.mods.ctrl {
        modifiers.push("Ctrl");
    }
    if key.mods.alt {
        modifiers.push("Alt");
    }
    if key.mods.shift {
        modifiers.push("Shift");
    }
    if modifiers.is_empty() {
        base
    } else {
        format!("{}-{base}", modifiers.join("-"))
    }
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use oom_edit_core::session::Modifiers;

    #[test]
    fn test_is_space_key_positive() {
        assert!(is_space_key(&ch(' ')));
    }

    #[test]
    fn test_is_space_key_negative() {
        for kind in [KeyCodeKind::Char('a'), KeyCodeKind::Esc, KeyCodeKind::Enter] {
            let key = KeyInput {
                code: KeyCode { kind },
                mods: Modifiers::default(),
            };
            assert!(!is_space_key(&key));
        }

        for mods in [
            Modifiers {
                ctrl: true,
                ..Modifiers::default()
            },
            Modifiers {
                alt: true,
                ..Modifiers::default()
            },
            Modifiers {
                shift: true,
                ..Modifiers::default()
            },
        ] {
            let key = KeyInput {
                code: KeyCode {
                    kind: KeyCodeKind::Char(' '),
                },
                mods,
            };
            assert!(!is_space_key(&key));
        }
    }

    #[test]
    fn default_keymap_has_no_function_key_bindings() {
        let km = Keymap::default();

        for (key, command, _) in &km.single {
            assert!(
                !matches!(key.code.kind, KeyCodeKind::F(_)),
                "single-key trigger for {command:?} must not use a function key"
            );
        }
        for ([first, second], command, _) in &km.chords {
            assert!(
                !matches!(first.code.kind, KeyCodeKind::F(_))
                    && !matches!(second.code.kind, KeyCodeKind::F(_)),
                "chord trigger for {command:?} must not use a function key"
            );
        }
    }

    #[test]
    fn plain_v_resolves_enter_select() {
        let km = Keymap::default();
        let mut pending = PendingChord::default();
        let v = ch('v');
        match km.resolve(Contexts::NORMAL, &v, &mut pending) {
            Resolution::Command(Command::EnterCharacterSelect) => {}
            other => panic!("v should resolve EnterCharacterSelect, got {other:?}"),
        }
    }

    #[test]
    fn select_shape_bindings_resolve_from_registry_keymap() {
        let km = Keymap::default();
        for (input, expected) in [
            (ch('v'), Command::EnterCharacterSelect),
            (ch('V'), Command::EnterLineSelect),
            (ctrl('v'), Command::EnterBlockSelect),
        ] {
            let mut pending = PendingChord::default();
            assert!(matches!(
                km.resolve(Contexts::NORMAL, &input, &mut pending),
                Resolution::Command(command) if command == expected
            ));
        }
    }

    #[test]
    fn space_w_resolves_save() {
        let km = Keymap::default();
        let mut pending = PendingChord::default();
        let space = ch(' ');
        let w = ch('w');

        km.resolve(Contexts::NORMAL, &space, &mut pending);
        match km.resolve(Contexts::NORMAL, &w, &mut pending) {
            Resolution::Command(Command::Save) => {}
            other => panic!("Space+w should resolve Save, got {other:?}"),
        }
    }

    #[test]
    fn space_q_resolves_quit() {
        let km = Keymap::default();
        let mut pending = PendingChord::default();
        let space = ch(' ');
        let q = ch('q');

        km.resolve(Contexts::NORMAL, &space, &mut pending);
        match km.resolve(Contexts::NORMAL, &q, &mut pending) {
            Resolution::Command(Command::Quit) => {}
            other => panic!("Space+q should resolve Quit, got {other:?}"),
        }
    }

    #[test]
    fn test_space_chord_resolves_with_new_helper() {
        let km = Keymap::default();
        let mut pending = PendingChord::default();
        let space = ch(' ');
        let t = ch('t');

        km.resolve(Contexts::NORMAL, &space, &mut pending);
        match km.resolve(Contexts::NORMAL, &t, &mut pending) {
            Resolution::Command(Command::CycleTheme) => {}
            other => panic!("Space+t should resolve CycleTheme, got {other:?}"),
        }
    }

    #[test]
    fn modified_space_does_not_start_or_satisfy_space_chord() {
        for mods in [
            Modifiers {
                ctrl: true,
                ..Modifiers::default()
            },
            Modifiers {
                alt: true,
                ..Modifiers::default()
            },
            Modifiers {
                shift: true,
                ..Modifiers::default()
            },
        ] {
            let modified_space = KeyInput {
                code: KeyCode {
                    kind: KeyCodeKind::Char(' '),
                },
                mods,
            };

            let km = Keymap::default();
            let mut pending = PendingChord::default();
            assert!(matches!(
                km.resolve(Contexts::NORMAL, &modified_space, &mut pending),
                Resolution::None
            ));
            assert!(pending.since.is_none());

            let km = Keymap {
                single: Vec::new(),
                chords: vec![(
                    [modified_space, ch('t')],
                    Command::CycleTheme,
                    Contexts::NORMAL,
                )],
            };
            let mut pending = PendingChord {
                since: Some(Instant::now()),
            };
            assert!(matches!(
                km.resolve(Contexts::NORMAL, &ch('t'), &mut pending),
                Resolution::None
            ));
            assert!(pending.since.is_none());
        }
    }

    #[test]
    fn space_h_resolves_help() {
        let km = Keymap::default();
        let mut pending = PendingChord::default();
        let space = ch(' ');
        let h = ch('h');

        km.resolve(Contexts::NORMAL, &space, &mut pending);
        match km.resolve(Contexts::NORMAL, &h, &mut pending) {
            Resolution::Command(Command::Help) => {}
            other => panic!("Space+h should resolve Help, got {other:?}"),
        }
    }

    #[test]
    fn stray_key_resets_pending() {
        let km = Keymap::default();
        let mut pending = PendingChord::default();
        let space = ch(' ');
        let x = ch('x');

        // Space starts pending.
        km.resolve(Contexts::NORMAL, &space, &mut pending);
        assert!(pending.since.is_some());

        // x is not a continuation → reset, fall through.
        match km.resolve(Contexts::NORMAL, &x, &mut pending) {
            Resolution::None => {}
            other => panic!("Space+x should be None, got {other:?}"),
        }
        assert!(
            pending.since.is_none(),
            "pending should be reset after stray key"
        );
    }

    #[test]
    fn space_in_insert_does_not_start_chord() {
        let km = Keymap::default();
        let mut pending = PendingChord::default();
        let space = ch(' ');

        // In INSERT context, Space should not start a chord.
        let result = km.resolve(Contexts::INSERT, &space, &mut pending);
        assert!(
            matches!(result, Resolution::None),
            "Space in INSERT should not start a chord"
        );
    }

    #[test]
    fn rendered_keys_help_uses_space_h_only() {
        let km = Keymap::default();
        assert_eq!(km.rendered_keys(Command::Help), Some("Space h".to_string()));
    }

    #[test]
    fn rendered_keys_enter_select() {
        let km = Keymap::default();
        assert_eq!(
            km.rendered_keys(Command::EnterCharacterSelect),
            Some("v".to_string())
        );
        assert_eq!(
            km.rendered_keys(Command::EnterLineSelect),
            Some("V".to_string())
        );
        assert_eq!(
            km.rendered_keys(Command::EnterBlockSelect),
            Some("Ctrl-V".to_string())
        );
    }

    #[test]
    fn continuations_space() {
        let km = Keymap::default();
        let conts = km.continuations_for(Contexts::NORMAL);
        let keys: Vec<char> = conts
            .iter()
            .filter_map(|(k, _)| match k.code.kind {
                KeyCodeKind::Char(c) => Some(c),
                _ => None,
            })
            .collect();
        assert!(keys.contains(&'h'));
        assert!(keys.contains(&'w'));
        assert!(keys.contains(&'q'));
        assert!(keys.contains(&'t'));
    }

    #[test]
    fn resolution_is_pure() {
        let km = Keymap::default();
        let run = || {
            let mut pending = PendingChord::default();
            let r = km.resolve(Contexts::NORMAL, &ch(' '), &mut pending);
            (matches!(r, Resolution::Pending(_)), pending.since.is_some())
        };
        assert_eq!(run(), run());
    }

    #[test]
    fn every_binding_uses_its_registry_contexts() {
        let km = Keymap::default();
        for (_, command, contexts) in &km.single {
            assert_eq!(
                *contexts,
                spec_for(*command).unwrap().contexts,
                "single-key context drift for {command:?}"
            );
        }
        for (_, command, contexts) in &km.chords {
            assert_eq!(
                *contexts,
                spec_for(*command).unwrap().contexts,
                "chord context drift for {command:?}"
            );
        }
    }
}
