//! Closed App-owned lifecycle requests.
//!
//! Every target-relative request captures exactly one tab index. A save
//! continuation is relative to that same target, so the type cannot encode
//! "save tab A, close tab B".

use std::path::PathBuf;

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
    Save(SaveRequest),
    CloseTab(CloseTabRequest),
    ReplaceTab {
        target: usize,
        path: PathBuf,
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
