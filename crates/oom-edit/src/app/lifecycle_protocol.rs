//! Captured host transactions executed through App's lifecycle gateway.

use super::*;
use crate::overlay::{ConfirmationResolution, DirtyCloseChoice, ExternalSaveChoice};
use crate::pane::{ExternalChangeToken, PreparedClose};

pub(super) enum SaveProgress {
    Saved,
    Pending,
    Failed,
}

enum CloseStep {
    Decision,
    SavePath,
    Overwrite,
}

pub(super) struct ClosePreparation {
    pub(super) request: RequestId,
    targets: Vec<TabId>,
    current: usize,
    auto_commit: bool,
    all: bool,
    step: CloseStep,
}

impl ClosePreparation {
    pub(super) fn current_tab(&self) -> Option<TabId> {
        self.targets.get(self.current).cloned()
    }
}

enum LeaseDelivery {
    Unissued(Arc<()>),
    Issued(Weak<()>),
}

impl LeaseDelivery {
    fn abandoned(&self) -> bool {
        matches!(self, Self::Issued(alive) if alive.upgrade().is_none())
    }
    fn take(&mut self) -> Option<Arc<()>> {
        match self {
            Self::Unissued(alive) => {
                let token = Arc::clone(alive);
                *self = Self::Issued(Arc::downgrade(&token));
                Some(token)
            }
            Self::Issued(_) => None,
        }
    }
}

enum SuspendedKind {
    External,
    Close {
        targets: Vec<TabId>,
        all: bool,
        explicit: bool,
    },
    Retarget(Vec<RetargetTarget>),
}

struct RetargetTarget {
    tab: TabId,
    from: PathBuf,
    to: PathBuf,
    preparation: oom_edit_core::RetargetPreparation,
}

pub(super) struct Suspension {
    pub(super) request: RequestId,
    delivery: LeaseDelivery,
    kind: SuspendedKind,
}

impl App {
    pub(super) fn start_retarget_preparation(
        &mut self,
        request: RequestId,
        targets: Vec<crate::pane::Retarget>,
    ) -> LifecycleOutcome {
        if self.overlay.is_some() {
            return LifecycleOutcome::Failed(Self::busy_error());
        }
        match self.capture_retargets(targets) {
            Err(error) => LifecycleOutcome::Failed(error),
            Ok(targets) => {
                let mappings = targets
                    .iter()
                    .map(|target| crate::pane::Retarget {
                        tab: target.tab.clone(),
                        path: target.to.clone(),
                    })
                    .collect();
                let alive = Arc::new(());
                self.lifecycle_state = LifecycleState::Suspended(Suspension {
                    request: request.clone(),
                    delivery: LeaseDelivery::Issued(Arc::downgrade(&alive)),
                    kind: SuspendedKind::Retarget(targets),
                });
                self.lifecycle_events.push(PaneEvent::PreparedRetarget {
                    request: request.clone(),
                    targets: mappings,
                });
                LifecycleOutcome::Retarget(crate::pane::PreparedRetarget {
                    token: ExternalChangeToken {
                        request,
                        _lease: alive,
                    },
                })
            }
        }
    }

    fn capture_retargets(
        &self,
        mappings: Vec<crate::pane::Retarget>,
    ) -> Result<Vec<RetargetTarget>, PaneError> {
        let mut targets: Vec<RetargetTarget> = Vec::new();
        for mapping in mappings {
            let index = self.tab_index(&mapping.tab)?;
            let session = &self.tabs[index].session;
            let from = session
                .path()
                .ok_or_else(|| PaneError {
                    kind: PaneErrorKind::InvalidPath,
                    path: None,
                    detail: "an unnamed buffer cannot follow a filesystem move".into(),
                })?
                .to_path_buf();
            self.authorize_file(FileOperation::Retarget, &from)?;
            let to = self.authorize_file(FileOperation::Retarget, &mapping.path)?;
            if targets
                .iter()
                .any(|target| target.tab == mapping.tab || (target.to == to && target.from != from))
            {
                return Err(PaneError {
                    kind: PaneErrorKind::InvalidPath,
                    path: Some(to),
                    detail: "duplicate tab or conflicting batch destination".into(),
                });
            }
            let source = oom_edit_core::DiskVersion::observe(&from).map_err(|error| PaneError {
                kind: PaneErrorKind::Io,
                path: Some(from.clone()),
                detail: error.message().into(),
            })?;
            let preparation = session
                .prepare_retarget(&to, &source)
                .map_err(|error| Self::retarget_error(&to, error))?;
            targets.push(RetargetTarget {
                tab: mapping.tab,
                from,
                to,
                preparation,
            });
        }
        for target in &targets {
            if !target.preparation.destination_version().is_missing()
                && !targets.iter().any(|source| source.from == target.to)
            {
                return Err(PaneError {
                    kind: PaneErrorKind::Stale,
                    path: Some(target.to.clone()),
                    detail: "destination already belongs to another file".into(),
                });
            }
            if self.command_policy == CommandPolicy::Embedded
                && self.tabs.iter().any(|entry| {
                    entry.session.path() == Some(target.to.as_path())
                        && !targets
                            .iter()
                            .any(|source| source.tab == self.id_for(entry))
                })
            {
                return Err(PaneError {
                    kind: PaneErrorKind::Stale,
                    path: Some(target.to.clone()),
                    detail: "destination is bound to an uncaptured tab".into(),
                });
            }
        }
        targets.sort_by_key(|target| self.tab_index(&target.tab).expect("validated captured tab"));
        Ok(targets)
    }

    fn retarget_error(path: &std::path::Path, error: oom_edit_core::RetargetError) -> PaneError {
        PaneError {
            kind: match error {
                oom_edit_core::RetargetError::Io(_) => PaneErrorKind::Io,
                _ => PaneErrorKind::Stale,
            },
            path: Some(path.into()),
            detail: error.to_string(),
        }
    }

    pub(super) fn finish_retarget(&mut self, request: &RequestId) -> LifecycleOutcome {
        if !matches!(&self.lifecycle_state, LifecycleState::Suspended(flow) if &flow.request == request && matches!(flow.kind, SuspendedKind::Retarget(_)))
        {
            return LifecycleOutcome::Failed(Self::stale_token_error());
        }
        let LifecycleState::Suspended(flow) =
            std::mem::replace(&mut self.lifecycle_state, LifecycleState::Idle)
        else {
            unreachable!()
        };
        let SuspendedKind::Retarget(targets) = flow.kind else {
            unreachable!()
        };
        self.lifecycle_state = LifecycleState::Executing(LifecycleContext {
            request: request.clone(),
            target: None,
        });
        let result = self.validate_retargets(&targets);
        let bindings = match result {
            Ok(bindings) => bindings,
            Err(error) => {
                self.lifecycle_failure(error.clone());
                self.lifecycle_state = LifecycleState::Idle;
                return LifecycleOutcome::Failed(error);
            }
        };
        // All binding generations are checked after the final IO. No code
        // between this check and the commits can mutate a document binding.
        for (index, binding) in &bindings {
            if !self.tabs[*index].session.can_commit_retarget(binding) {
                let error = Self::stale_token_error();
                self.lifecycle_failure(error.clone());
                self.lifecycle_state = LifecycleState::Idle;
                return LifecycleOutcome::Failed(error);
            }
        }
        let mut observations = Vec::new();
        for ((index, binding), target) in bindings.into_iter().zip(&targets) {
            let change = match binding.disk_state() {
                oom_edit_core::DiskState::Modified { version } => {
                    Some(DiskChange::Modified(version))
                }
                oom_edit_core::DiskState::Missing { version } => Some(DiskChange::Missing(version)),
                _ => None,
            };
            observations.push((index, change));
            self.tabs[index]
                .session
                .commit_retarget(binding)
                .expect("prevalidated IO-free binding generation");
            self.lifecycle_events.push(PaneEvent::Retargeted {
                request: request.clone(),
                tab: target.tab.clone(),
                from: target.from.clone(),
                to: target.to.clone(),
            });
        }
        for (index, change) in observations {
            self.record_disk_observation(index, change);
        }
        self.next_disk_poll = self.now + disk_watch::POLL_INTERVAL;
        self.lifecycle_events.push(PaneEvent::PreparedCommitted {
            request: request.clone(),
        });
        self.lifecycle_state = LifecycleState::Idle;
        LifecycleOutcome::Applied
    }

    fn validate_retargets(
        &self,
        targets: &[RetargetTarget],
    ) -> Result<Vec<(usize, oom_edit_core::RetargetBinding)>, PaneError> {
        targets
            .iter()
            .map(|target| {
                let index = self.tab_index(&target.tab)?;
                let resolved = self.authorize_file(FileOperation::Retarget, &target.to)?;
                if resolved != target.to {
                    return Err(Self::stale_token_error());
                }
                self.authorize_file(FileOperation::Inspect, &target.to)?;
                let binding = self.tabs[index]
                    .session
                    .validate_retarget(&target.preparation)
                    .map_err(|error| Self::retarget_error(&target.to, error))?;
                Ok((index, binding))
            })
            .collect()
    }
    pub(super) fn busy_error() -> PaneError {
        PaneError {
            kind: PaneErrorKind::Busy,
            path: None,
            detail: "another lifecycle operation owns the pane".into(),
        }
    }

    pub(super) fn cancel_pending_request(&mut self, request: &RequestId) -> LifecycleOutcome {
        let matches = match &self.lifecycle_state {
            LifecycleState::Preparing(flow) => &flow.request == request,
            LifecycleState::Confirmation(context) => &context.request == request,
            LifecycleState::Suspended(flow) => {
                &flow.request == request && matches!(flow.delivery, LeaseDelivery::Unissued(_))
            }
            _ => false,
        };
        if !matches {
            return LifecycleOutcome::Failed(Self::stale_token_error());
        }
        self.overlay.close();
        self.cancel_lifecycle();
        LifecycleOutcome::Applied
    }

    pub(super) fn stale_token_error() -> PaneError {
        PaneError {
            kind: PaneErrorKind::Stale,
            path: None,
            detail: "unknown, stale or already consumed lifecycle token".into(),
        }
    }

    fn tab_index(&self, id: &TabId) -> Result<usize, PaneError> {
        if !self.contains_tab_id(id) {
            return Err(PaneError {
                kind: PaneErrorKind::UnknownTab,
                path: None,
                detail: "unknown or stale tab identity".into(),
            });
        }
        self.tabs
            .iter()
            .position(|entry| entry.serial == id.serial)
            .ok_or_else(Self::stale_token_error)
    }

    pub(super) fn sync_abandoned_transaction(&mut self) -> bool {
        let abandoned = matches!(&self.lifecycle_state, LifecycleState::Suspended(flow) if flow.delivery.abandoned());
        if abandoned {
            self.invalidate_presentation();
            self.cancel_lifecycle();
            self.overlay.close();
            self.reconcile_released_transaction();
        }
        abandoned
    }

    pub(super) fn active_tab_frozen(&self) -> bool {
        let Some(active) = self.active_tab_id() else {
            return false;
        };
        match &self.lifecycle_state {
            LifecycleState::Suspended(Suspension {
                kind: SuspendedKind::Close { targets, .. },
                ..
            }) => targets.contains(&active),
            _ => false,
        }
    }

    pub(super) fn start_close_preparation(
        &mut self,
        request: RequestId,
        targets: Vec<TabId>,
        auto_commit: bool,
        all: bool,
    ) -> LifecycleOutcome {
        if self.overlay.is_some() {
            return LifecycleOutcome::Failed(Self::busy_error());
        }
        let mut ordered = Vec::with_capacity(targets.len());
        for tab in targets {
            let index = match self.tab_index(&tab) {
                Ok(index) => index,
                Err(error) => return LifecycleOutcome::Failed(error),
            };
            if ordered.iter().any(|(existing, _)| *existing == index) {
                return LifecycleOutcome::Failed(PaneError {
                    kind: PaneErrorKind::InvalidPath,
                    path: None,
                    detail: "duplicate close target".into(),
                });
            }
            ordered.push((index, tab));
        }
        ordered.sort_by_key(|(index, _)| *index);
        self.pointer_gesture = PointerGesture::Idle;
        self.pending_input = PendingAppInput::Idle;
        self.lifecycle_state = LifecycleState::Preparing(ClosePreparation {
            request: request.clone(),
            targets: ordered.into_iter().map(|(_, tab)| tab).collect(),
            current: 0,
            auto_commit,
            all,
            step: CloseStep::Decision,
        });
        self.advance_close_preparation();
        LifecycleOutcome::Request(request)
    }

    fn advance_close_preparation(&mut self) {
        let LifecycleState::Preparing(mut flow) =
            std::mem::replace(&mut self.lifecycle_state, LifecycleState::Idle)
        else {
            return;
        };
        while let Some(tab) = flow.current_tab() {
            let index = match self.tab_index(&tab) {
                Ok(index) => index,
                Err(error) => {
                    self.lifecycle_state = LifecycleState::Preparing(flow);
                    self.lifecycle_failure(error);
                    self.lifecycle_state = LifecycleState::Idle;
                    return;
                }
            };
            if !self.tabs[index].session.is_dirty() {
                flow.current += 1;
                continue;
            }
            flow.step = CloseStep::Decision;
            self.lifecycle_state = LifecycleState::Preparing(flow);
            self.focus_tab_id(&tab);
            self.overlay = Overlay::open_confirm_quit(CloseTabRequest {
                target: index,
                force: false,
                dirty_policy: DirtyClosePolicy::Confirm,
            });
            let request = self.event_request();
            self.lifecycle_events
                .push(PaneEvent::AttentionRequired { request, tab });
            return;
        }
        self.overlay.close();
        let request = flow.request.clone();
        let auto_commit = flow.auto_commit;
        let targets = flow.targets;
        self.lifecycle_state = LifecycleState::Suspended(Suspension {
            request: request.clone(),
            delivery: LeaseDelivery::Unissued(Arc::new(())),
            kind: SuspendedKind::Close {
                targets: targets.clone(),
                all: flow.all,
                explicit: !auto_commit,
            },
        });
        if auto_commit {
            self.execute_lifecycle(LifecycleAction::CommitClose { request });
        } else {
            self.lifecycle_events.push(PaneEvent::PreparedClose {
                request,
                tabs: targets,
            });
        }
    }

    pub(super) fn resolve_preparation_confirmation(&mut self, resolution: ConfirmationResolution) {
        match resolution {
            ConfirmationResolution::DiskChange { .. } => {
                unreachable!("disk decisions never belong to a close preparation")
            }
            ConfirmationResolution::DirtyClose { choice, .. } => {
                self.execute_lifecycle(LifecycleAction::ResolveClose { choice });
            }
            ConfirmationResolution::ExternalSave {
                request,
                disk_path,
                version,
                choice,
            } => {
                self.execute_lifecycle(LifecycleAction::ResolveSave {
                    request,
                    disk_path,
                    version,
                    choice,
                });
            }
        }
    }

    pub(super) fn resolve_close_choice(&mut self, choice: DirtyCloseChoice) -> LifecycleOutcome {
        if !matches!(self.lifecycle_state, LifecycleState::Preparing(_)) {
            return LifecycleOutcome::Failed(Self::stale_token_error());
        }
        if choice == DirtyCloseChoice::Cancel {
            self.overlay.close();
            self.cancel_lifecycle();
            return LifecycleOutcome::Applied;
        }
        let LifecycleState::Preparing(flow) = &self.lifecycle_state else {
            return LifecycleOutcome::Failed(Self::stale_token_error());
        };
        if !matches!(flow.step, CloseStep::Decision)
            && !(matches!(flow.step, CloseStep::SavePath) && choice == DirtyCloseChoice::Discard)
        {
            return LifecycleOutcome::Failed(Self::busy_error());
        }
        if choice == DirtyCloseChoice::Discard {
            if let LifecycleState::Preparing(flow) = &mut self.lifecycle_state {
                flow.current += 1;
            }
            self.advance_close_preparation();
        } else {
            self.save_prepared_target(None, false, None);
        }
        LifecycleOutcome::Applied
    }

    fn save_prepared_target(
        &mut self,
        path: Option<PathBuf>,
        force: bool,
        expected: Option<&oom_edit_core::DiskVersion>,
    ) {
        let LifecycleState::Preparing(mut flow) =
            std::mem::replace(&mut self.lifecycle_state, LifecycleState::Idle)
        else {
            return;
        };
        let Some(tab) = flow.current_tab() else {
            return;
        };
        let index = match self.tab_index(&tab) {
            Ok(index) => index,
            Err(error) => {
                self.lifecycle_state = LifecycleState::Preparing(flow);
                self.lifecycle_failure(error);
                self.lifecycle_state = LifecycleState::Idle;
                return;
            }
        };
        if path.is_none() && self.tabs[index].session.path().is_none() {
            flow.step = CloseStep::SavePath;
            let request = flow.request.clone();
            self.lifecycle_state = LifecycleState::Preparing(flow);
            self.overlay = Overlay::open_save_path_wait(CloseTabRequest {
                target: index,
                force: false,
                dirty_policy: DirtyClosePolicy::Confirm,
            });
            self.lifecycle_events
                .push(PaneEvent::SavePathRequested { request, tab });
            return;
        }
        self.lifecycle_state = LifecycleState::Executing(LifecycleContext {
            request: flow.request.clone(),
            target: Some(tab),
        });
        let progress = self.execute_save_version(
            SaveRequest {
                target: index,
                path,
                force,
                retarget: true,
                continuation: SaveContinuation::StayOpen,
            },
            expected,
        );
        match progress {
            SaveProgress::Saved => {
                flow.current += 1;
                self.lifecycle_state = LifecycleState::Preparing(flow);
                self.advance_close_preparation();
            }
            SaveProgress::Pending => {
                flow.step = CloseStep::Overwrite;
                self.lifecycle_state = LifecycleState::Preparing(flow);
            }
            SaveProgress::Failed => {
                self.overlay.close();
                self.lifecycle_state = LifecycleState::Idle;
            }
        }
    }

    pub(super) fn supply_prepared_save_path(
        &mut self,
        request: &RequestId,
        path: Option<PathBuf>,
    ) -> LifecycleOutcome {
        if !matches!(&self.lifecycle_state, LifecycleState::Preparing(flow) if &flow.request == request && matches!(flow.step, CloseStep::SavePath))
        {
            return LifecycleOutcome::Failed(Self::stale_token_error());
        }
        self.overlay.close();
        if let Some(path) = path {
            self.save_prepared_target(Some(path), false, None);
            if let Some(PaneEvent::Failed {
                request: failed,
                error,
                ..
            }) = self.lifecycle_events.last()
            {
                if failed == request {
                    return LifecycleOutcome::Failed(error.clone());
                }
            }
        } else {
            self.cancel_lifecycle();
        }
        LifecycleOutcome::Applied
    }

    pub(crate) fn take_prepared_close(
        &mut self,
        request: &RequestId,
    ) -> Result<PreparedClose, PaneError> {
        self.sync_abandoned_transaction();
        let LifecycleState::Suspended(flow) = &mut self.lifecycle_state else {
            return Err(Self::stale_token_error());
        };
        if &flow.request != request || !matches!(flow.kind, SuspendedKind::Close { .. }) {
            return Err(Self::stale_token_error());
        }
        let alive = flow.delivery.take().ok_or_else(Self::stale_token_error)?;
        Ok(PreparedClose {
            token: ExternalChangeToken {
                request: request.clone(),
                _lease: alive,
            },
        })
    }

    pub(super) fn commit_prepared_close(&mut self, request: &RequestId) -> LifecycleOutcome {
        if !matches!(&self.lifecycle_state, LifecycleState::Suspended(flow) if &flow.request == request && matches!(flow.kind, SuspendedKind::Close { .. }))
        {
            return LifecycleOutcome::Failed(Self::stale_token_error());
        }
        let LifecycleState::Suspended(flow) =
            std::mem::replace(&mut self.lifecycle_state, LifecycleState::Idle)
        else {
            unreachable!();
        };
        let SuspendedKind::Close {
            targets,
            all,
            explicit,
        } = flow.kind
        else {
            unreachable!();
        };
        let mut indices = Vec::new();
        for tab in &targets {
            match self.tab_index(tab) {
                Ok(index) => indices.push(index),
                Err(error) => {
                    self.lifecycle_state = LifecycleState::Suspended(Suspension {
                        request: flow.request,
                        delivery: flow.delivery,
                        kind: SuspendedKind::Close {
                            targets,
                            all,
                            explicit,
                        },
                    });
                    return LifecycleOutcome::Failed(error);
                }
            }
        }
        self.lifecycle_state = LifecycleState::Executing(LifecycleContext {
            request: request.clone(),
            target: None,
        });
        // Removing in descending index order preserves captured membership;
        // publish the resulting Closed events in the original tab order.
        let event_start = self.lifecycle_events.len();
        let previous_active = self.active_tab_id();
        for index in indices.into_iter().rev() {
            self.do_close_tab(index);
        }
        let emitted = self.lifecycle_events.split_off(event_start);
        let mut closed = emitted
            .iter()
            .filter(|event| matches!(event, PaneEvent::Closed { .. }))
            .cloned()
            .collect::<Vec<_>>();
        closed.reverse();
        self.lifecycle_events.extend(closed);
        if let Some(active) = self
            .active_tab_id()
            .filter(|active| Some(active) != previous_active.as_ref())
        {
            self.lifecycle_events.push(PaneEvent::ActiveTabChanged {
                request: request.clone(),
                tab: active,
            });
        }
        if all {
            self.lifecycle_events.push(PaneEvent::AllClosed {
                request: request.clone(),
            });
        }
        if explicit {
            self.lifecycle_events.push(PaneEvent::PreparedCommitted {
                request: request.clone(),
            });
        }
        self.lifecycle_state = LifecycleState::Idle;
        LifecycleOutcome::Applied
    }

    pub(super) fn start_external_change(&mut self, request: RequestId) -> LifecycleOutcome {
        if self.overlay.is_some() {
            return LifecycleOutcome::Failed(Self::busy_error());
        }
        let alive = Arc::new(());
        self.lifecycle_state = LifecycleState::Suspended(Suspension {
            request: request.clone(),
            delivery: LeaseDelivery::Issued(Arc::downgrade(&alive)),
            kind: SuspendedKind::External,
        });
        LifecycleOutcome::External(ExternalChangeToken {
            request,
            _lease: alive,
        })
    }

    pub(super) fn resolve_save_choice(
        &mut self,
        request: SaveRequest,
        disk_path: PathBuf,
        version: oom_edit_core::DiskVersion,
        choice: ExternalSaveChoice,
    ) -> LifecycleOutcome {
        if !matches!(
            self.lifecycle_state,
            LifecycleState::Preparing(_) | LifecycleState::Confirmation(_)
        ) {
            return LifecycleOutcome::Failed(Self::stale_token_error());
        }
        match choice {
            ExternalSaveChoice::Cancel => {
                self.overlay.close();
                self.cancel_lifecycle();
            }
            ExternalSaveChoice::Overwrite => {
                if matches!(self.lifecycle_state, LifecycleState::Preparing(_)) {
                    self.save_prepared_target(request.path, false, Some(&version));
                } else {
                    self.lifecycle_state = LifecycleState::Executing(LifecycleContext {
                        request: self.event_request(),
                        target: self
                            .tabs
                            .get(request.target)
                            .map(|entry| self.id_for(entry)),
                    });
                    self.execute_save_version(request, Some(&version));
                }
            }
            ExternalSaveChoice::Reload => {
                if matches!(self.lifecycle_state, LifecycleState::Preparing(_)) {
                    self.cancel_lifecycle();
                    self.execute_lifecycle(LifecycleAction::ReloadVersion {
                        target: request.target,
                        path: disk_path,
                        version,
                    });
                } else {
                    self.reload_version(request.target, &disk_path, &version);
                    self.lifecycle_state = LifecycleState::Idle;
                }
            }
        }
        LifecycleOutcome::Applied
    }

    pub(super) fn reload_version(
        &mut self,
        target: usize,
        path: &std::path::Path,
        version: &oom_edit_core::DiskVersion,
    ) {
        match self.apply_disk_reload(target, path, version) {
            Ok(()) => {
                let tab = self.id_for(&self.tabs[target]);
                let request = self.event_request();
                self.lifecycle_events.push(PaneEvent::Reloaded {
                    request,
                    tab,
                    path: path.into(),
                });
            }
            Err(error) => self.lifecycle_failure(error),
        }
    }

    pub(super) fn apply_disk_reload(
        &mut self,
        target: usize,
        path: &std::path::Path,
        version: &oom_edit_core::DiskVersion,
    ) -> Result<(), PaneError> {
        self.authorize_file(FileOperation::Reload, path)
            .and_then(|resolved| {
                let entry = self
                    .tabs
                    .get_mut(target)
                    .ok_or_else(Self::stale_token_error)?;
                if entry.session.path() != Some(resolved.as_path()) {
                    return Err(Self::stale_token_error());
                }
                entry
                    .session
                    .reload_from_disk(version)
                    .map_err(|error| PaneError {
                        kind: match error {
                            oom_edit_core::ReloadError::StaleVersion => PaneErrorKind::Stale,
                            oom_edit_core::ReloadError::NotUtf8(_) => PaneErrorKind::NotUtf8,
                            _ => PaneErrorKind::Io,
                        },
                        path: Some(resolved),
                        detail: error.to_string(),
                    })
            })?;
        self.tabs[target].disk_change = disk_watch::DiskObservation::Current;
        self.clamp_reloaded_viewport(target);
        Ok(())
    }

    fn clamp_reloaded_viewport(&mut self, target: usize) {
        let width = self.viewport_width.min(usize::from(u16::MAX)).max(1) as u16;
        let layout_width = width.min(self.wrap_width).max(1);
        let entry = &mut self.tabs[target];
        entry.invalidate_gutter_trouble_if_present();
        entry.top_line = entry.top_line.min(
            entry
                .session
                .line_count()
                .saturating_sub(self.viewport_height.max(1)),
        );
        let height = entry
            .session
            .visual_row_info(entry.top_line, 0, layout_width, self.wrap_enabled)
            .1;
        entry.skip_rows = entry.skip_rows.min(height.saturating_sub(1));
        let source = entry.session.document();
        let max_source_width = source
            .lines()
            .map(|line| ratatui::text::Line::raw(line).width())
            .max()
            .unwrap_or(0);
        entry.left_col = if self.wrap_enabled {
            0
        } else {
            entry
                .left_col
                .min(max_source_width.saturating_sub(usize::from(width)))
        };
        let layout = entry.session.render_layout(layout_width);
        entry.rendered_top = entry.rendered_top.min(
            layout
                .lines
                .len()
                .saturating_sub(self.viewport_height.max(1)),
        );
        let max_rendered_width = layout
            .lines
            .iter()
            .flat_map(|line| &line.atoms)
            .map(|atom| atom.columns.end)
            .max()
            .unwrap_or(0);
        entry.rendered_left_col = entry
            .rendered_left_col
            .min(max_rendered_width.saturating_sub(usize::from(width)));
    }

    pub(super) fn abort_owned_transaction(&mut self, request: &RequestId) -> LifecycleOutcome {
        if !matches!(&self.lifecycle_state, LifecycleState::Suspended(flow) if &flow.request == request)
        {
            return LifecycleOutcome::Failed(Self::stale_token_error());
        }
        self.overlay.close();
        self.cancel_lifecycle();
        self.reconcile_released_transaction();
        LifecycleOutcome::Applied
    }

    pub(super) fn finish_external_change(
        &mut self,
        request: &RequestId,
        paths: Vec<PathBuf>,
    ) -> LifecycleOutcome {
        if !matches!(&self.lifecycle_state, LifecycleState::Suspended(flow) if &flow.request == request && matches!(flow.kind, SuspendedKind::External))
        {
            return LifecycleOutcome::Failed(Self::stale_token_error());
        }
        self.lifecycle_state = LifecycleState::Executing(LifecycleContext {
            request: request.clone(),
            target: None,
        });
        let result = self.reconcile_transaction_paths(&paths);
        self.next_disk_poll = self.now + disk_watch::POLL_INTERVAL;
        self.lifecycle_state = LifecycleState::Idle;
        if let Err(error) = result {
            self.lifecycle_events.push(PaneEvent::Failed {
                request: request.clone(),
                tab: None,
                error: error.clone(),
            });
            return LifecycleOutcome::Failed(error);
        }
        self.lifecycle_events
            .push(PaneEvent::ExternalChangeCommitted {
                request: request.clone(),
                paths,
            });
        LifecycleOutcome::Applied
    }
}
