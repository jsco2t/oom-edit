//! Pure App-owned Space-prefix transition over static registry rows.

use std::num::NonZeroUsize;
use std::time::Instant;

use oom_edit_core::{KeyCodeKind, KeyInput, Modifiers};

use super::registry::{
    app_chord, direct_command, space_continuations, AppCommand, CommandSpec, Contexts,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PendingAppInput {
    Idle,
    Space { since: Instant },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TabAction {
    Next,
    Prev,
    Jump(NonZeroUsize),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppInputTransition {
    Pending(PendingAppInput),
    AppCommand(AppCommand),
    TabAction(TabAction),
    Forward(KeyInput),
}

pub fn resolve(
    pending: PendingAppInput,
    ctx: Contexts,
    key: KeyInput,
    now: Instant,
) -> AppInputTransition {
    match pending {
        PendingAppInput::Idle if is_space_key(key) && rendered_context(ctx) => {
            AppInputTransition::Pending(PendingAppInput::Space { since: now })
        }
        PendingAppInput::Idle => {
            if let KeyCodeKind::Char(ch) = key.code.kind {
                if plain_character_key(key, ch) {
                    if let Some(command) = direct_command(ctx, ch) {
                        return AppInputTransition::AppCommand(command);
                    }
                }
            }
            AppInputTransition::Forward(key)
        }
        PendingAppInput::Space { .. } => {
            if let KeyCodeKind::Char(continuation) = key.code.kind {
                if plain_character_key(key, continuation) {
                    if let Some(command) = app_chord(ctx, continuation) {
                        return AppInputTransition::AppCommand(command);
                    }
                    if let Some(tab) = continuation
                        .to_digit(10)
                        .and_then(|value| NonZeroUsize::new(value as usize))
                    {
                        return AppInputTransition::TabAction(TabAction::Jump(tab));
                    }
                }
            }
            AppInputTransition::Forward(key)
        }
    }
}

pub fn continuations_for(ctx: Contexts) -> Vec<(char, &'static CommandSpec)> {
    space_continuations(ctx)
}

fn rendered_context(ctx: Contexts) -> bool {
    ctx.contains(Contexts::NORMAL) || ctx.contains(Contexts::SELECT)
}

fn is_space_key(key: KeyInput) -> bool {
    key.mods == Modifiers::default() && key.code.kind == KeyCodeKind::Char(' ')
}

fn plain_character_key(key: KeyInput, ch: char) -> bool {
    !key.mods.ctrl && !key.mods.alt && (!key.mods.shift || ch == '?')
}

#[cfg(test)]
mod tests {
    use super::*;
    use oom_edit_core::{KeyCode, Modifiers};
    use std::time::Duration;

    fn ch(c: char) -> KeyInput {
        KeyInput {
            code: KeyCode {
                kind: KeyCodeKind::Char(c),
            },
            mods: Modifiers::default(),
        }
    }

    #[test]
    fn pending_space_transitions_are_total() {
        let t0 = Instant::now();
        assert_eq!(
            resolve(PendingAppInput::Idle, Contexts::NORMAL, ch(' '), t0),
            AppInputTransition::Pending(PendingAppInput::Space { since: t0 })
        );
        assert_eq!(
            resolve(
                PendingAppInput::Space { since: t0 },
                Contexts::NORMAL,
                ch('h'),
                t0
            ),
            AppInputTransition::AppCommand(AppCommand::Help)
        );
        assert_eq!(
            resolve(
                PendingAppInput::Space { since: t0 },
                Contexts::NORMAL,
                ch('3'),
                t0
            ),
            AppInputTransition::TabAction(TabAction::Jump(NonZeroUsize::new(3).unwrap()))
        );
        assert_eq!(
            resolve(
                PendingAppInput::Space { since: t0 },
                Contexts::NORMAL,
                ch('z'),
                t0
            ),
            AppInputTransition::AppCommand(AppCommand::SpellToggle)
        );
        assert_eq!(
            resolve(
                PendingAppInput::Idle,
                Contexts::NORMAL,
                ch('g'),
                t0 + Duration::from_secs(1)
            ),
            AppInputTransition::Forward(ch('g'))
        );
    }

    #[test]
    fn every_app_chord_resolves_to_its_registry_command() {
        let t0 = Instant::now();
        for (continuation, spec) in continuations_for(Contexts::NORMAL) {
            let super::super::registry::BindingRole::AppChord { command, .. } = spec.binding else {
                unreachable!()
            };
            assert_eq!(
                resolve(
                    PendingAppInput::Space { since: t0 },
                    Contexts::NORMAL,
                    ch(continuation),
                    t0
                ),
                AppInputTransition::AppCommand(command)
            );
        }
    }

    #[test]
    fn question_aliases_resolve_only_in_declared_contexts() {
        let now = Instant::now();
        let shifted = KeyInput {
            mods: Modifiers {
                shift: true,
                ..Modifiers::default()
            },
            ..ch('?')
        };
        let controlled = KeyInput {
            mods: Modifiers {
                ctrl: true,
                ..Modifiers::default()
            },
            ..ch('?')
        };
        for key in [ch('?'), shifted] {
            assert_eq!(
                resolve(PendingAppInput::Idle, Contexts::NORMAL, key, now),
                AppInputTransition::AppCommand(AppCommand::Help)
            );
            assert_eq!(
                resolve(PendingAppInput::Idle, Contexts::SELECT, key, now),
                AppInputTransition::AppCommand(AppCommand::Help)
            );
            assert_eq!(
                resolve(PendingAppInput::Idle, Contexts::INSERT, key, now),
                AppInputTransition::Forward(key)
            );
            assert_eq!(
                resolve(
                    PendingAppInput::Space { since: now },
                    Contexts::NORMAL,
                    key,
                    now
                ),
                AppInputTransition::AppCommand(AppCommand::Help)
            );
            assert_eq!(
                resolve(
                    PendingAppInput::Space { since: now },
                    Contexts::SELECT,
                    key,
                    now
                ),
                AppInputTransition::AppCommand(AppCommand::Help)
            );
        }
        assert_eq!(
            resolve(PendingAppInput::Idle, Contexts::NORMAL, controlled, now),
            AppInputTransition::Forward(controlled)
        );
        assert_eq!(
            resolve(
                PendingAppInput::Space { since: now },
                Contexts::NORMAL,
                controlled,
                now
            ),
            AppInputTransition::Forward(controlled)
        );
        assert_eq!(
            resolve(PendingAppInput::Idle, Contexts::NORMAL, ch('/'), now),
            AppInputTransition::Forward(ch('/'))
        );
        assert_eq!(
            resolve(
                PendingAppInput::Space { since: now },
                Contexts::NORMAL,
                ch('x'),
                now
            ),
            AppInputTransition::Forward(ch('x'))
        );
    }

    #[test]
    fn resolve_stamps_pending_with_supplied_instant() {
        let supplied = Instant::now() + Duration::from_secs(42);
        assert_eq!(
            resolve(PendingAppInput::Idle, Contexts::SELECT, ch(' '), supplied),
            AppInputTransition::Pending(PendingAppInput::Space { since: supplied })
        );
    }
}
