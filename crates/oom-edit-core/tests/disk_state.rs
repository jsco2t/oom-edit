use std::fs::{self, File, FileTimes};

use oom_edit_core::{
    DiskHint, DiskState, EditorSession, KeyCode, KeyCodeKind, KeyInput, Modifiers, ReloadError,
    RetargetError, SaveError,
};

fn key(kind: KeyCodeKind) -> KeyInput {
    KeyInput {
        code: KeyCode { kind },
        mods: Modifiers::default(),
    }
}

#[test]
fn disk_state_distinguishes_unbacked_new_existing_modified_missing_and_io_error() {
    assert!(matches!(
        EditorSession::from_text("untitled").disk_state(),
        DiskState::Unbacked
    ));
    let dir = tempfile::tempdir().unwrap();
    let new = dir.path().join("new.md");
    let session = EditorSession::open(&new).unwrap();
    assert!(matches!(
        session.disk_state(),
        DiskState::NeverCreated { .. }
    ));

    let path = dir.path().join("file.md");
    fs::write(&path, "old\n").unwrap();
    let session = EditorSession::open(&path).unwrap();
    assert!(matches!(session.disk_state(), DiskState::Unchanged { .. }));
    fs::write(&path, "new\n").unwrap();
    assert!(matches!(session.disk_state(), DiskState::Modified { .. }));
    fs::remove_file(&path).unwrap();
    assert!(matches!(session.disk_state(), DiskState::Missing { .. }));
    fs::create_dir(&path).unwrap();
    assert!(matches!(session.disk_state(), DiskState::IoError(_)));
}

#[test]
fn missing_backed_file_requires_version_bound_recreation() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("file.md");
    fs::write(&path, "original\n").unwrap();
    let mut session = EditorSession::open(&path).unwrap();
    fs::remove_file(&path).unwrap();
    assert!(matches!(
        session.save(None, false),
        Err(SaveError::Missing(_))
    ));
    assert!(!path.exists());
    let DiskState::Missing { version } = session.disk_state() else {
        panic!("expected Missing")
    };
    session.authorize_recreation(&version).unwrap();
    fs::write(&path, "replacement\n").unwrap();
    assert!(session.save(None, false).is_err());
    assert_eq!(fs::read(&path).unwrap(), b"replacement\n");
    fs::remove_file(&path).unwrap();
    session.authorize_recreation(&version).unwrap();
    session.save(None, false).unwrap();
    assert_eq!(fs::read(&path).unwrap(), b"original\n");
}

#[test]
fn validated_keep_mine_is_invalidated_by_a_later_change() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("file.md");
    fs::write(&path, "original\n").unwrap();
    let mut session = EditorSession::open(&path).unwrap();
    fs::write(&path, "change 1\n").unwrap();
    let DiskState::Modified { version } = session.disk_state() else {
        panic!("expected Modified")
    };
    session.acknowledge_keep_mine(&version).unwrap();
    fs::write(&path, "change 2\n").unwrap();
    assert!(matches!(
        session.save(None, false),
        Err(SaveError::ExternallyModified(_))
    ));
    assert_eq!(fs::read(&path).unwrap(), b"change 2\n");
}

#[test]
fn identical_reload_preserves_insert_mode_undo_and_cursor() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("file.md");
    fs::write(&path, "one\r\ntwo\r\n").unwrap();
    let mut session = EditorSession::open(&path).unwrap();
    session.handle_key(key(KeyCodeKind::Char('i')));
    session.handle_key(key(KeyCodeKind::Char('X')));
    let before = session.cursor();
    let DiskState::Unchanged { version } = session.disk_state() else {
        panic!("expected Unchanged")
    };
    fs::write(&path, "Xone\ntwo\n").unwrap();
    let DiskState::Modified { version: current } = session.disk_state() else {
        panic!("expected Modified")
    };
    assert_ne!(version, current);
    session.reload_from_disk(&current).unwrap();
    assert_eq!(session.mode(), oom_edit_core::Mode::Insert);
    assert_eq!(session.cursor(), before);
    assert_eq!(session.document(), "Xone\ntwo\n");
    assert!(!session.is_dirty());
    session.handle_key(key(KeyCodeKind::Esc));
    session.handle_key(key(KeyCodeKind::Char('u')));
    assert_eq!(session.document(), "one\ntwo\n");
}

#[test]
fn changed_reload_resets_undo_and_clamps_source_cursor() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("file.md");
    fs::write(&path, "first\nsecond\nthird\n").unwrap();
    let mut session = EditorSession::open(&path).unwrap();
    session.jump_to_offset(17).unwrap();
    fs::write(&path, "x\n").unwrap();
    let DiskState::Modified { version } = session.disk_state() else {
        panic!("expected Modified")
    };
    session.reload_from_disk(&version).unwrap();
    assert_eq!(session.document(), "x\n");
    assert_eq!(session.mode(), oom_edit_core::Mode::Normal);
    assert_eq!(session.cursor().0, 1);
    assert!(!session.is_dirty());
    session.handle_key(key(KeyCodeKind::Char('u')));
    assert_eq!(session.document(), "x\n");
}

#[test]
fn stale_or_invalid_reload_leaves_editor_untouched() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("file.md");
    fs::write(&path, "original\n").unwrap();
    let mut session = EditorSession::open(&path).unwrap();
    session.handle_key(key(KeyCodeKind::Char('i')));
    session.handle_key(key(KeyCodeKind::Char('X')));
    session.handle_key(key(KeyCodeKind::Esc));
    fs::write(&path, "candidate\n").unwrap();
    let DiskState::Modified { version } = session.disk_state() else {
        panic!("expected Modified")
    };
    fs::write(&path, b"\xff").unwrap();
    assert!(matches!(
        session.reload_from_disk(&version),
        Err(ReloadError::StaleVersion)
    ));
    let DiskState::Modified { version } = session.disk_state() else {
        panic!("expected Modified")
    };
    assert!(matches!(
        session.reload_from_disk(&version),
        Err(ReloadError::NotUtf8(0))
    ));
    assert_eq!(session.document(), "Xoriginal\n");
    assert!(session.is_dirty());
    fs::remove_file(&path).unwrap();
    fs::create_dir(&path).unwrap();
    assert!(matches!(
        session.reload_from_disk(&version),
        Err(ReloadError::Io(_))
    ));
    assert_eq!(session.document(), "Xoriginal\n");
    session.handle_key(key(KeyCodeKind::Char('u')));
    assert_eq!(session.document(), "original\n");
}

#[test]
fn same_metadata_content_replacement_is_detected_by_validated_state() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("file.md");
    fs::write(&path, "first\n").unwrap();
    let session = EditorSession::open(&path).unwrap();
    let original_modified = fs::metadata(&path).unwrap().modified().unwrap();
    fs::write(&path, "other\n").unwrap();
    File::options()
        .write(true)
        .open(&path)
        .unwrap()
        .set_times(FileTimes::new().set_modified(original_modified))
        .unwrap();
    assert_eq!(session.disk_hint(), DiskHint::Unchanged);
    assert!(matches!(session.disk_state(), DiskState::Modified { .. }));
}

#[test]
fn guarded_retarget_follows_a_move_without_writing_or_losing_undo() {
    let dir = tempfile::tempdir().unwrap();
    let old = dir.path().join("old.md");
    let moved = dir.path().join("moved.md");
    fs::write(&old, "original\n").unwrap();
    let mut session = EditorSession::open(&old).unwrap();
    session.handle_key(key(KeyCodeKind::Char('i')));
    session.handle_key(key(KeyCodeKind::Char('X')));
    session.handle_key(key(KeyCodeKind::Esc));
    let cursor = session.cursor();
    fs::rename(&old, &moved).unwrap();
    let DiskState::Missing { .. } = session.disk_state() else {
        panic!("expected Missing")
    };
    // The destination version can be obtained through a separate file-backed session.
    let DiskState::Unchanged { version } = EditorSession::open(&moved).unwrap().disk_state() else {
        panic!("expected Unchanged")
    };
    session.retarget(&moved, &version).unwrap();
    assert_eq!(session.path(), Some(moved.as_path()));
    assert_eq!(fs::read(&moved).unwrap(), b"original\n");
    assert_eq!(session.cursor(), cursor);
    assert!(session.is_dirty());
    assert!(matches!(session.disk_state(), DiskState::Unchanged { .. }));
    session.handle_key(key(KeyCodeKind::Char('u')));
    assert_eq!(session.document(), "original\n");
}

#[test]
fn prepared_binding_preserves_external_conflict_and_rejects_document_generation_changes() {
    let dir = tempfile::tempdir().unwrap();
    let old = dir.path().join("old.md");
    let moved = dir.path().join("moved.md");
    fs::write(&old, "original\n").unwrap();
    let mut session = EditorSession::open(&old).unwrap();
    fs::write(&old, "outside\n").unwrap();
    let source = oom_edit_core::DiskVersion::observe(&old).unwrap();
    let preparation = session.prepare_retarget(&moved, &source).unwrap();
    assert!(preparation.destination_version().is_missing());
    fs::rename(&old, &moved).unwrap();
    let binding = session.validate_retarget(&preparation).unwrap();
    assert!(matches!(binding.disk_state(), DiskState::Modified { .. }));
    assert!(session.can_commit_retarget(&binding));
    session.commit_retarget(binding).unwrap();
    assert_eq!(session.document(), "original\n");
    assert!(matches!(session.disk_state(), DiskState::Modified { .. }));

    let second = dir.path().join("second.md");
    let source = oom_edit_core::DiskVersion::observe(&moved).unwrap();
    let preparation = session.prepare_retarget(&second, &source).unwrap();
    fs::rename(&moved, &second).unwrap();
    let binding = session.validate_retarget(&preparation).unwrap();
    session.save(None, true).unwrap();
    assert!(!session.can_commit_retarget(&binding));
    assert!(matches!(
        session.commit_retarget(binding),
        Err(RetargetError::StaleVersion)
    ));
    assert_eq!(session.path(), Some(moved.as_path()));
}

#[test]
fn writer_at_pre_replace_barrier_is_never_overwritten_even_by_force() {
    struct Writer;
    impl oom_edit_core::SaveObserver for Writer {
        fn observe(
            &mut self,
            boundary: oom_edit_core::SaveBoundary,
            path: &std::path::Path,
        ) -> Result<(), oom_edit_core::DiskIoError> {
            if boundary == oom_edit_core::SaveBoundary::BeforeReplace {
                fs::write(path, "intervening writer\n").unwrap();
            }
            Ok(())
        }
    }
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("note.md");
    fs::write(&path, "original\n").unwrap();
    let mut session = EditorSession::open(&path).unwrap();
    session.handle_key(key(KeyCodeKind::Char('i')));
    session.handle_key(key(KeyCodeKind::Char('X')));
    assert!(session.save_with_observer(None, true, &mut Writer).is_err());
    assert_eq!(fs::read_to_string(&path).unwrap(), "intervening writer\n");
    assert_eq!(session.document(), "Xoriginal\n");
    assert!(session.is_dirty());
}

#[test]
fn conflicting_or_stale_retarget_preserves_identity_and_live_state() {
    let dir = tempfile::tempdir().unwrap();
    let old = dir.path().join("old.md");
    let destination = dir.path().join("destination.md");
    fs::write(&old, "original\n").unwrap();
    fs::write(&destination, "conflict\n").unwrap();
    let mut session = EditorSession::open(&old).unwrap();
    let DiskState::Unchanged { version } = EditorSession::open(&destination).unwrap().disk_state()
    else {
        panic!("expected Unchanged")
    };
    assert!(matches!(
        session.retarget(&destination, &version),
        Err(RetargetError::ConflictingDestination)
    ));
    assert_eq!(session.path(), Some(old.as_path()));
    assert_eq!(session.document(), "original\n");
    fs::write(&destination, "changed!\n").unwrap();
    assert!(matches!(
        session.retarget(&destination, &version),
        Err(RetargetError::StaleVersion)
    ));
    assert_eq!(fs::read(&destination).unwrap(), b"changed!\n");
}

#[test]
fn never_created_save_is_safe_but_a_new_occupant_requires_force() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("new.md");
    let mut session = EditorSession::open(&path).unwrap();
    assert!(session.is_new());
    fs::write(&path, "another writer\n").unwrap();
    assert!(matches!(
        session.save(None, false),
        Err(SaveError::ExternallyModified(_))
    ));
    assert_eq!(fs::read(&path).unwrap(), b"another writer\n");
    session.save(None, true).unwrap();
    assert_eq!(fs::read(&path).unwrap(), b"");
    assert!(!session.is_new());

    let other = dir.path().join("fresh.md");
    let mut fresh = EditorSession::open(&other).unwrap();
    fresh.save(None, false).unwrap();
    assert_eq!(fs::read(&other).unwrap(), b"");
}

#[test]
fn keep_mine_allows_only_the_exact_validated_version() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("file.md");
    fs::write(&path, "original\n").unwrap();
    let mut session = EditorSession::open(&path).unwrap();
    fs::write(&path, "external\n").unwrap();
    let DiskState::Modified { version } = session.disk_state() else {
        panic!("expected Modified")
    };
    session.acknowledge_keep_mine(&version).unwrap();
    session.save(None, false).unwrap();
    assert_eq!(fs::read(&path).unwrap(), b"original\n");
    assert!(matches!(session.disk_state(), DiskState::Unchanged { .. }));
}

#[test]
fn same_metadata_overwrite_is_refused_even_when_poll_hint_is_unchanged() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("file.md");
    fs::write(&path, "first\n").unwrap();
    let mut session = EditorSession::open(&path).unwrap();
    let modified = fs::metadata(&path).unwrap().modified().unwrap();
    fs::write(&path, "other\n").unwrap();
    File::options()
        .write(true)
        .open(&path)
        .unwrap()
        .set_times(FileTimes::new().set_modified(modified))
        .unwrap();
    assert_eq!(session.disk_hint(), DiskHint::Unchanged);
    assert!(matches!(
        session.save(None, false),
        Err(SaveError::ExternallyModified(_))
    ));
    assert_eq!(fs::read(&path).unwrap(), b"other\n");
}
