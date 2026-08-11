//! Pure App-owned Space-prefix transition over static registry rows.

use std::num::NonZeroUsize;
use std::time::Instant;

use oom_edit_core::{KeyCodeKind, KeyInput, Modifiers};

use super::registry::{app_chord, space_continuations, AppCommand, CommandSpec, Contexts};

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
        PendingAppInput::Idle => AppInputTransition::Forward(key),
        PendingAppInput::Space { .. } => {
            if key.mods == Modifiers::default() {
                if let KeyCodeKind::Char(continuation) = key.code.kind {
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
            AppInputTransition::Forward(ch('z'))
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
    fn resolve_stamps_pending_with_supplied_instant() {
        let supplied = Instant::now() + Duration::from_secs(42);
        assert_eq!(
            resolve(PendingAppInput::Idle, Contexts::SELECT, ch(' '), supplied),
            AppInputTransition::Pending(PendingAppInput::Space { since: supplied })
        );
    }
}
