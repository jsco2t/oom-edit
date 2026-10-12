//! Automatic-save scheduling and candidate selection. Writes use the lifecycle gateway.

use super::*;
use crate::config::AutosaveConfig;
use crate::lifecycle::{AutosaveTrigger, SaveOrigin};
use std::time::Duration;

const INTERVAL: Duration = Duration::from_secs(300);

enum Timer {
    Disabled,
    Scheduled { next_due: Instant },
}

pub(super) struct AutosaveState {
    timer: Timer,
    on_focus_loss: bool,
}

impl AutosaveState {
    pub(super) fn new(config: AutosaveConfig, now: Instant) -> Self {
        let mut state = Self {
            timer: Timer::Disabled,
            on_focus_loss: false,
        };
        state.configure(config, now);
        state
    }

    pub(super) fn configure(&mut self, config: AutosaveConfig, now: Instant) {
        match (&self.timer, config.every_five_minutes) {
            (Timer::Disabled, true) => {
                self.timer = Timer::Scheduled {
                    next_due: now + INTERVAL,
                }
            }
            (Timer::Scheduled { .. }, false) => self.timer = Timer::Disabled,
            _ => {}
        }
        self.on_focus_loss = config.on_focus_loss;
    }

    pub(super) fn deadline(&self) -> Option<Instant> {
        match self.timer {
            Timer::Disabled => None,
            Timer::Scheduled { next_due } => Some(next_due),
        }
    }

    pub(super) fn take_due(&mut self, now: Instant) -> bool {
        if self.deadline().is_some_and(|deadline| now >= deadline) {
            self.timer = Timer::Scheduled {
                next_due: now + INTERVAL,
            };
            true
        } else {
            false
        }
    }

    pub(super) fn on_focus_loss(&self) -> bool {
        self.on_focus_loss
    }
}

impl App {
    pub(super) fn run_autosave(&mut self, trigger: AutosaveTrigger) {
        let candidates: Vec<_> = self
            .tabs
            .iter()
            .filter(|tab| tab.session.path().is_some() && tab.session.is_dirty())
            .map(|tab| self.id_for(tab))
            .collect();
        if candidates.is_empty() {
            return;
        }
        if !matches!(self.lifecycle_state, LifecycleState::Idle)
            || self.overlay.is_some()
            || self.input_grammar_pending()
            || self.session().is_some_and(|session| {
                session.mode() == oom_edit_core::Mode::Command
                    || session.rendered_search_prompt().is_some()
            })
        {
            self.set_transient(
                "Autosave skipped: an editor interaction or file transaction is pending".into(),
                oom_edit_core::Severity::Warning,
            );
            return;
        }

        let mut saved = 0;
        let mut failures = Vec::new();
        for id in candidates {
            let Some(target) = self.tabs.iter().position(|tab| self.id_for(tab) == id) else {
                continue;
            };
            let event_start = self.lifecycle_events.len();
            self.execute_lifecycle(LifecycleAction::Save(SaveRequest {
                origin: SaveOrigin::Automatic(trigger),
                target,
                path: None,
                force: false,
                retarget: true,
                continuation: SaveContinuation::StayOpen,
            }));
            for event in &self.lifecycle_events[event_start..] {
                match event {
                    PaneEvent::Saved { .. } => saved += 1,
                    PaneEvent::Failed { error, .. } => failures.push(error.to_string()),
                    PaneEvent::DiskWriteCommitted { path, detail, .. } => {
                        failures.push(format!("{}: {detail}", path.display()));
                    }
                    _ => {}
                }
            }
        }
        if failures.is_empty() {
            self.set_transient(
                format!("Autosaved {saved} file(s)"),
                oom_edit_core::Severity::Success,
            );
        } else {
            self.set_transient(
                format!(
                    "Autosave: {saved} saved; {} failed: {}",
                    failures.len(),
                    failures.join("; ")
                ),
                oom_edit_core::Severity::Error,
            );
        }
    }
}

#[cfg(test)]
mod tests;
