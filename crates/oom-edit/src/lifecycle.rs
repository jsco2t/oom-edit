//! Closed App-owned lifecycle requests.
//!
//! Target-relative requests capture their tab indices when created. A save
//! continuation is relative to that same target, so the type cannot encode
//! "save tab A, close tab B".

use std::path::PathBuf;

use crate::overlay::DirtyCloseChoice;
use crate::pane::{ExternalChangeToken, OpenOptions, OpenOutcome, PaneError, RequestId, TabId};

/// Synchronous output from the one App lifecycle executor.
pub(crate) enum LifecycleOutcome {
    Applied,
    Open(OpenOutcome),
    Failed(PaneError),
    Request(RequestId),
    External(ExternalChangeToken),
    Retarget(crate::pane::PreparedRetarget),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SaveContinuation {
    StayOpen,
    CloseSavedTab,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SaveRequest {
    pub target: usize,
    pub path: Option<PathBuf>,
    pub force: bool,
    pub retarget: bool,
    pub continuation: SaveContinuation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DirtyClosePolicy {
    Confirm,
    Refuse,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CloseTabRequest {
    pub target: usize,
    pub force: bool,
    pub dirty_policy: DirtyClosePolicy,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum LifecycleAction {
    PrepareClose {
        targets: Vec<TabId>,
        auto_commit: bool,
        all: bool,
    },
    PrepareRetarget {
        targets: Vec<crate::pane::Retarget>,
    },
    CommitRetarget {
        request: RequestId,
    },
    ResolveClose {
        choice: DirtyCloseChoice,
    },
    CancelConfirmation,
    CancelRequest {
        request: RequestId,
    },
    ResolveSave {
        request: SaveRequest,
        disk_path: PathBuf,
        version: oom_edit_core::DiskVersion,
        choice: crate::overlay::ExternalSaveChoice,
    },
    ReloadVersion {
        target: usize,
        path: PathBuf,
        version: oom_edit_core::DiskVersion,
    },
    ReconcileDiskChange {
        target: usize,
        version: oom_edit_core::DiskVersion,
    },
    ResolveDiskChange {
        target: usize,
        path: PathBuf,
        version: oom_edit_core::DiskVersion,
        choice: crate::overlay::DiskChangeChoice,
    },
    ProvideSavePath {
        request: RequestId,
        path: Option<PathBuf>,
    },
    CommitClose {
        request: RequestId,
    },
    BeginExternalChange,
    CommitExternalChange {
        request: RequestId,
        paths: Vec<PathBuf>,
    },
    AbortToken {
        request: RequestId,
    },
    HostOpen {
        path: PathBuf,
        options: OpenOptions,
        existing_only: bool,
    },
    NewBuffer {
        options: OpenOptions,
    },
    Save(SaveRequest),
    CloseTab(CloseTabRequest),
    ReplaceTab {
        target: usize,
        path: PathBuf,
        force: bool,
    },
    ReloadTabs {
        targets: Vec<usize>,
        force: bool,
    },
    OpenTab {
        path: PathBuf,
    },
    QuitAll {
        force: bool,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_continuation_is_relative_to_its_only_target() {
        let request = SaveRequest {
            target: 2,
            path: None,
            force: false,
            retarget: true,
            continuation: SaveContinuation::CloseSavedTab,
        };
        assert_eq!(request.target, 2);
        assert_eq!(request.continuation, SaveContinuation::CloseSavedTab);
    }
}
