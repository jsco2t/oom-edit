//! Versioned observations and safe-point decisions owned by the one App.

use super::*;
use std::path::Path;
use std::time::Duration;

use crate::overlay::DiskChangeChoice;
use oom_edit_core::{DiskHint, DiskState, DiskVersion, Mode};

pub(super) const POLL_INTERVAL: Duration = Duration::from_secs(2);

/// Presentation/decision state only; Document still owns disk truth and save
/// acknowledgements. Deferred observations remain visible without reprompting.
#[derive(Debug)]
pub(super) enum DiskObservation {
    Current,
    Pending(DiskChange),
    Deferred(DiskChange),
    Acknowledged(DiskVersion),
}

#[cfg(test)]
mod tests;

impl DiskObservation {
    pub(super) fn marker(&self) -> Option<&'static str> {
        match self {
            Self::Pending(DiskChange::Modified(_)) | Self::Deferred(DiskChange::Modified(_)) => {
                Some("[disk changed]")
            }
            Self::Pending(DiskChange::Missing(_)) | Self::Deferred(DiskChange::Missing(_)) => {
                Some("[missing]")
            }
            Self::Pending(DiskChange::IoError(_)) | Self::Deferred(DiskChange::IoError(_)) => {
                Some("[disk error]")
            }
            Self::Current | Self::Acknowledged(_) => None,
        }
    }

    fn matches(&self, change: &Option<DiskChange>) -> bool {
        match (self, change) {
            (Self::Current, None) => true,
            (Self::Pending(old) | Self::Deferred(old), Some(new)) => old == new,
            (Self::Acknowledged(old), Some(DiskChange::Modified(new))) => old == new,
            _ => false,
        }
    }
}

/// Narrow read-only boundary for deterministic polling/error counters. Reloads
/// and writes still use the canonical core/lifecycle operations, never this seam.
pub(super) trait DiskProbe {
    fn hint(&mut self, session: &EditorSession) -> DiskHint;
    fn state(&mut self, session: &EditorSession) -> DiskState;
}

pub(super) struct FilesystemProbe;

impl DiskProbe for FilesystemProbe {
    fn hint(&mut self, session: &EditorSession) -> DiskHint {
        session.disk_hint()
    }
    fn state(&mut self, session: &EditorSession) -> DiskState {
        session.disk_state()
    }
}

fn change_from_state(path: &Path, state: DiskState) -> Option<DiskChange> {
    match state {
        DiskState::Modified { version } => Some(DiskChange::Modified(version)),
        DiskState::Missing { version } => Some(DiskChange::Missing(version)),
        DiskState::IoError(error) => Some(DiskChange::IoError(PaneError {
            kind: PaneErrorKind::Io,
            path: Some(path.into()),
            detail: error.message().into(),
        })),
        _ => None,
    }
}

impl App {
    pub(super) fn disk_poll_deadline(&self) -> Option<Instant> {
        (!matches!(self.lifecycle_state, LifecycleState::Suspended(_))
            && self.tabs.iter().any(|entry| entry.session.path().is_some()))
        .then_some(self.next_disk_poll)
    }

    pub(super) fn poll_disk(&mut self, now: Instant) {
        if matches!(self.lifecycle_state, LifecycleState::Suspended(_)) || now < self.next_disk_poll
        {
            return;
        }
        // Do not catch up missed intervals: one observation per tab is enough.
        self.next_disk_poll = now + POLL_INTERVAL;
        for index in 0..self.tabs.len() {
            let Some(path) = self.tabs[index].session.path().map(PathBuf::from) else {
                continue;
            };
            if let Err(error) = self.authorize_file(FileOperation::Inspect, &path) {
                self.record_disk_observation(index, Some(DiskChange::IoError(error)));
                continue;
            }
            match self.disk_probe.hint(&self.tabs[index].session) {
                DiskHint::Changed | DiskHint::Missing | DiskHint::IoError => {
                    let state = self.disk_probe.state(&self.tabs[index].session);
                    self.record_disk_observation(index, change_from_state(&path, state));
                }
                DiskHint::Unchanged | DiskHint::NeverCreated | DiskHint::Unbacked => {
                    // A cached conflict is content-validated before it can be
                    // cleared; matching metadata alone cannot acknowledge it.
                    if !matches!(self.tabs[index].disk_change, DiskObservation::Current) {
                        let state = self.disk_probe.state(&self.tabs[index].session);
                        self.record_disk_observation(index, change_from_state(&path, state));
                    }
                }
            }
        }
    }

    pub(super) fn record_disk_observation(&mut self, index: usize, change: Option<DiskChange>) {
        if self.tabs[index].disk_change.matches(&change) {
            return;
        }
        self.tabs[index].disk_change = match change.clone() {
            Some(change) => DiskObservation::Pending(change),
            None => DiskObservation::Current,
        };
        self.invalidate_presentation();
        if let Some(change) = change {
            let tab = self.id_for(&self.tabs[index]);
            self.lifecycle_events
                .push(PaneEvent::DiskChangePending { tab, change });
        }
    }

    pub(crate) fn notify_disk_paths(&mut self, paths: &[PathBuf]) -> Result<(), PaneError> {
        self.sync_abandoned_transaction();
        if matches!(self.lifecycle_state, LifecycleState::Suspended(_)) {
            return Err(Self::busy_error());
        }
        self.reconcile_transaction_paths(paths)?;
        self.reconcile_safe_disk_change();
        Ok(())
    }

    pub(super) fn reconcile_transaction_paths(
        &mut self,
        paths: &[PathBuf],
    ) -> Result<(), PaneError> {
        let resolved = paths
            .iter()
            .map(|path| resolve_host_path(&self.launch_dir, path))
            .collect::<Result<Vec<_>, _>>()?;
        let mut observations = Vec::new();
        for index in 0..self.tabs.len() {
            let Some(path) = self.tabs[index].session.path().map(PathBuf::from) else {
                continue;
            };
            if !resolved.is_empty()
                && !resolved
                    .iter()
                    .any(|affected| path == *affected || path.starts_with(affected))
            {
                continue;
            }
            self.authorize_file(FileOperation::Inspect, &path)?;
            let state = self.disk_probe.state(&self.tabs[index].session);
            observations.push((index, change_from_state(&path, state)));
        }
        for (index, change) in observations {
            self.record_disk_observation(index, change);
        }
        Ok(())
    }

    pub(super) fn reconcile_released_transaction(&mut self) {
        self.next_disk_poll = self.now + POLL_INTERVAL;
        if let Err(error) = self.reconcile_transaction_paths(&[]) {
            self.set_transient(
                format!("Disk reconciliation failed: {error}"),
                oom_edit_core::Severity::Warning,
            );
        }
    }

    pub(super) fn reconcile_safe_disk_change(&mut self) {
        if !self.is_focused()
            || self.overlay.is_some()
            || !matches!(self.lifecycle_state, LifecycleState::Idle)
            || self.input_grammar_pending()
        {
            return;
        }
        let Some(entry) = self.active() else {
            return;
        };
        if entry.session.mode() != Mode::Normal || entry.session.rendered_search_prompt().is_some()
        {
            return;
        }
        match &entry.disk_change {
            DiskObservation::Pending(DiskChange::Modified(version)) => {
                let version = version.clone();
                self.execute_lifecycle(LifecycleAction::ReconcileDiskChange {
                    target: self.active_tab,
                    version,
                });
            }
            DiskObservation::Pending(DiskChange::IoError(error)) => {
                let error = error.clone();
                self.tabs[self.active_tab].disk_change =
                    DiskObservation::Deferred(DiskChange::IoError(error.clone()));
                self.set_transient(
                    format!("Disk observation failed: {error}"),
                    oom_edit_core::Severity::Warning,
                );
            }
            _ => {}
        }
    }

    pub(super) fn execute_disk_change(&mut self, target: usize, version: DiskVersion) {
        if self.tabs[target].session.is_dirty() {
            let path = version.path().to_path_buf();
            self.overlay = Overlay::open_disk_change(target, path, version);
        } else {
            self.reload_observed_disk(target, &version);
        }
    }

    fn reload_observed_disk(&mut self, target: usize, version: &DiskVersion) {
        let path = version.path().to_path_buf();
        match self.apply_disk_reload(target, &path, version) {
            Ok(()) => {
                let tab = self.id_for(&self.tabs[target]);
                let request = self.event_request();
                self.lifecycle_events
                    .push(PaneEvent::ReloadedFromDisk { request, tab, path });
                self.set_transient("Reloaded from disk".into(), oom_edit_core::Severity::Info);
            }
            Err(error) => {
                if self.tabs[target]
                    .disk_change
                    .matches(&Some(DiskChange::Modified(version.clone())))
                {
                    self.tabs[target].disk_change =
                        DiskObservation::Deferred(DiskChange::Modified(version.clone()));
                }
                self.lifecycle_failure(error.clone());
                self.set_transient(
                    format!("Reload failed: {error}"),
                    oom_edit_core::Severity::Warning,
                );
                if error.kind == PaneErrorKind::Stale {
                    self.refresh_disk_target(target);
                }
            }
        }
    }

    fn refresh_disk_target(&mut self, target: usize) {
        let Some(path) = self.tabs[target].session.path().map(PathBuf::from) else {
            return;
        };
        let change = match self.authorize_file(FileOperation::Inspect, &path) {
            Ok(_) => change_from_state(&path, self.disk_probe.state(&self.tabs[target].session)),
            Err(error) => Some(DiskChange::IoError(error)),
        };
        self.record_disk_observation(target, change);
    }

    pub(super) fn resolve_disk_change(
        &mut self,
        target: usize,
        path: &Path,
        version: &DiskVersion,
        choice: DiskChangeChoice,
    ) {
        if let LifecycleState::Confirmation(context) = &self.lifecycle_state {
            self.lifecycle_state = LifecycleState::Executing(context.clone());
        }
        self.overlay.close();
        if self.tabs[target].session.path() != Some(path) {
            self.lifecycle_failure(Self::stale_token_error());
            return;
        }
        match choice {
            DiskChangeChoice::Cancel => {
                if self.tabs[target]
                    .disk_change
                    .matches(&Some(DiskChange::Modified(version.clone())))
                {
                    self.tabs[target].disk_change =
                        DiskObservation::Deferred(DiskChange::Modified(version.clone()));
                }
                self.cancel_lifecycle();
            }
            DiskChangeChoice::KeepMine => {
                let result = self
                    .authorize_file(FileOperation::Inspect, path)
                    .and_then(|_| {
                        self.tabs[target]
                            .session
                            .acknowledge_keep_mine(version)
                            .map_err(|error| PaneError {
                                kind: match error {
                                    oom_edit_core::DiskDecisionError::Io(_) => PaneErrorKind::Io,
                                    _ => PaneErrorKind::Stale,
                                },
                                path: Some(path.into()),
                                detail: error.to_string(),
                            })
                    });
                match result {
                    Ok(()) => {
                        self.tabs[target].disk_change =
                            DiskObservation::Acknowledged(version.clone());
                    }
                    Err(error) => {
                        self.lifecycle_failure(error);
                        self.refresh_disk_target(target);
                    }
                }
            }
            DiskChangeChoice::Reload => self.reload_observed_disk(target, version),
        }
        self.invalidate_presentation();
    }
}
