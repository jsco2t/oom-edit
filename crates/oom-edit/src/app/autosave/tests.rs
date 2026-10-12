use super::*;

#[test]
fn schedule_has_exact_boundaries_and_no_catch_up_or_config_reset() {
    let now = Instant::now();
    let config = AutosaveConfig {
        every_five_minutes: true,
        on_focus_loss: true,
    };
    let mut state = AutosaveState::new(AutosaveConfig::default(), now);
    assert_eq!(state.deadline(), None);
    assert!(!state.take_due(now + INTERVAL));
    state.configure(config, now);
    assert_eq!(state.deadline(), Some(now + INTERVAL));
    assert!(!state.take_due(now + INTERVAL - Duration::from_nanos(1)));
    state.configure(config, now + Duration::from_secs(100));
    assert_eq!(state.deadline(), Some(now + INTERVAL));
    assert!(state.take_due(now + INTERVAL));
    assert!(!state.take_due(now + INTERVAL));
    assert!(state.take_due(now + Duration::from_secs(1500)));
    assert_eq!(state.deadline(), Some(now + Duration::from_secs(1800)));
    state.configure(AutosaveConfig::default(), now);
    assert_eq!(state.deadline(), None);
    state.configure(config, now + Duration::from_secs(1600));
    assert_eq!(state.deadline(), Some(now + Duration::from_secs(1900)));
}

#[test]
fn failed_and_uncertain_autosaves_keep_dirty_work_and_correlated_events() {
    use oom_edit_core::{DiskIoError, DiskIoErrorKind, SaveBoundary, SaveObserver};
    struct Reject(SaveBoundary);
    impl SaveObserver for Reject {
        fn observe(
            &mut self,
            boundary: SaveBoundary,
            _: &std::path::Path,
        ) -> Result<(), DiskIoError> {
            if boundary == self.0 {
                Err(DiskIoError::new(
                    DiskIoErrorKind::Other,
                    "injected autosave failure",
                ))
            } else {
                Ok(())
            }
        }
    }
    for boundary in [SaveBoundary::BeforeReplace, SaveBoundary::ConfirmDurability] {
        let dir = tempfile::tempdir().unwrap();
        let mut app = super::super::tests::file_backed_app(dir.path(), &[("one.md", "original\n")]);
        super::super::tests::dirty_tab(&mut app, 0, "changed ");
        app.save_observer = Some(Box::new(Reject(boundary)));
        let tab = app.id_for(&app.tabs[0]);
        app.run_autosave(AutosaveTrigger::Interval);
        assert!(app.tabs[0].session.is_dirty());
        assert!(matches!(app.lifecycle_state, LifecycleState::Idle));
        assert!(!app.overlay.is_some());
        assert!(app.transient.as_ref().unwrap().text.contains("failed"));
        assert!(!app
            .lifecycle_events
            .iter()
            .any(|event| matches!(event, PaneEvent::Saved { .. } | PaneEvent::Completed { .. })));
        match boundary {
            SaveBoundary::BeforeReplace => {
                assert_eq!(
                    std::fs::read_to_string(dir.path().join("one.md")).unwrap(),
                    "original\n"
                );
                assert!(app.lifecycle_events.iter().any(
                    |event| matches!(event, PaneEvent::Failed { tab: Some(id), .. } if id == &tab)
                ));
            }
            SaveBoundary::ConfirmDurability => {
                assert_eq!(
                    std::fs::read_to_string(dir.path().join("one.md")).unwrap(),
                    "changed original\n"
                );
                assert!(app.lifecycle_events.iter().any(|event| matches!(event, PaneEvent::DiskWriteCommitted { tab: id, .. } if id == &tab)));
            }
        }
    }
}

#[test]
fn external_change_between_temp_write_and_rename_is_rejected_without_blocking_other_tabs() {
    struct ChangeBeforeReplace(std::path::PathBuf);
    impl oom_edit_core::SaveObserver for ChangeBeforeReplace {
        fn observe(
            &mut self,
            boundary: oom_edit_core::SaveBoundary,
            path: &std::path::Path,
        ) -> Result<(), oom_edit_core::DiskIoError> {
            if boundary == oom_edit_core::SaveBoundary::BeforeReplace && path == self.0 {
                std::fs::write(path, "external during save\n").unwrap();
            }
            Ok(())
        }
    }
    let dir = tempfile::tempdir().unwrap();
    let mut app = super::super::tests::file_backed_app(
        dir.path(),
        &[("one.md", "original\n"), ("two.md", "original\n")],
    );
    super::super::tests::dirty_tab(&mut app, 0, "mine ");
    super::super::tests::dirty_tab(&mut app, 1, "other ");
    app.save_observer = Some(Box::new(ChangeBeforeReplace(dir.path().join("one.md"))));
    app.run_autosave(AutosaveTrigger::Interval);
    assert_eq!(
        std::fs::read_to_string(dir.path().join("one.md")).unwrap(),
        "external during save\n"
    );
    assert_eq!(
        std::fs::read_to_string(dir.path().join("two.md")).unwrap(),
        "other original\n"
    );
    assert!(app.tabs[0].session.is_dirty());
    assert!(!app.tabs[1].session.is_dirty());
    assert!(!app.overlay.is_some());
    assert!(matches!(app.lifecycle_state, LifecycleState::Idle));
    assert!(app.transient.as_ref().unwrap().text.contains("1 failed"));
    assert_eq!(
        app.lifecycle_events
            .iter()
            .filter(|e| matches!(e, PaneEvent::Failed { .. }))
            .count(),
        1
    );
    assert_eq!(
        app.lifecycle_events
            .iter()
            .filter(|e| matches!(e, PaneEvent::Saved { .. }))
            .count(),
        1
    );
}
