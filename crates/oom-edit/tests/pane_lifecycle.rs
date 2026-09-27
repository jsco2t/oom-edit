use std::path::{Path, PathBuf};
use std::time::Instant;

use oom_edit::{
    ClipboardError, ClipboardSink, Config, ConfigPersistError, DisplayMode, EditorPane,
    FileAccessPolicy, FileOperation, FilePolicyError, KeyCode, KeyCodeKind, KeyInput, Modifiers,
    OpenOptions, PaneInit, PaneInput, PaneOptions, PaneServices, ThemeCatalog,
    ThemePersistenceSink, ThemeSelection, ThemeSlot, Tier,
};

struct Silent;
impl ClipboardSink for Silent {
    fn copy(&mut self, _: &str) -> Result<(), ClipboardError> {
        Ok(())
    }
}
impl ThemePersistenceSink for Silent {
    fn persist_theme(&mut self, _: ThemeSlot, _: &str) -> Result<(), ConfigPersistError> {
        Ok(())
    }
}

struct OpenOnly;
impl FileAccessPolicy for OpenOnly {
    fn authorize(&self, operation: FileOperation, _: &Path) -> Result<(), FilePolicyError> {
        if matches!(operation, FileOperation::Open | FileOperation::OpenExisting) {
            Ok(())
        } else {
            Err(FilePolicyError {
                reason: "host disallows this operation".into(),
            })
        }
    }
}

struct Within(PathBuf);
impl FileAccessPolicy for Within {
    fn authorize(&self, _: FileOperation, path: &Path) -> Result<(), FilePolicyError> {
        if path.starts_with(&self.0) {
            Ok(())
        } else {
            Err(FilePolicyError {
                reason: "outside host root".into(),
            })
        }
    }
}

fn terminal_request(event: &oom_edit::PaneEvent) -> Option<&oom_edit::RequestId> {
    match event {
        oom_edit::PaneEvent::Completed { request }
        | oom_edit::PaneEvent::Cancelled { request }
        | oom_edit::PaneEvent::Failed { request, .. }
        | oom_edit::PaneEvent::DiskWriteCommitted { request, .. } => Some(request),
        _ => None,
    }
}

#[test]
fn compact_close_and_overwrite_choices_have_distinct_outcomes() {
    for (choice, closes, saved) in [
        ('y', true, true),
        ('n', true, false),
        ('\u{1b}', false, false),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("界e\u{301}.md");
        std::fs::write(&path, "original\n").unwrap();
        let mut editor = pane(
            dir.path(),
            vec![path.clone()],
            Box::new(oom_edit::AllowAllFileAccess),
        );
        let tab = editor.active_tab().unwrap();
        edit(&mut editor, "memory ");
        editor.close(&tab).unwrap();
        let frame = editor.render(20, 5, Instant::now());
        let text = frame
            .cells
            .iter()
            .map(|cell| cell.symbol.as_str())
            .collect::<String>();
        for label in ["Save [y/w]", "Discard [n]", "Cancel [Esc]"] {
            assert!(text.contains(label), "{text:?} missing {label}");
        }
        if choice == '\u{1b}' {
            esc(&mut editor);
        } else {
            input(&mut editor, key(choice));
        }
        assert_eq!(editor.tabs().is_empty(), closes);
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            if saved {
                "memory original\n"
            } else {
                "original\n"
            }
        );
        if !closes {
            assert_eq!(editor.text(&tab).unwrap(), "memory original\n");
            input(&mut editor, key('u'));
            assert_eq!(editor.text(&tab).unwrap(), "original\n");
        }
    }
    for (choice, expected_disk, expected_buffer) in [
        ('o', "memory original\n", "memory original\n"),
        ('r', "outside\n", "outside\n"),
        ('\u{1b}', "outside\n", "memory original\n"),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("note.md");
        std::fs::write(&path, "original\n").unwrap();
        let mut editor = pane(
            dir.path(),
            vec![path.clone()],
            Box::new(oom_edit::AllowAllFileAccess),
        );
        let tab = editor.active_tab().unwrap();
        edit(&mut editor, "memory ");
        std::fs::write(&path, "outside\n").unwrap();
        ex(&mut editor, "w");
        let frame = editor.render(20, 5, Instant::now());
        let text = frame
            .cells
            .iter()
            .map(|cell| cell.symbol.as_str())
            .collect::<String>();
        for label in ["Overwrite [o]", "Reload [r]", "Cancel [Esc]"] {
            assert!(text.contains(label), "{text:?} missing {label}");
        }
        if choice == '\u{1b}' {
            esc(&mut editor);
        } else {
            input(&mut editor, key(choice));
        }
        assert_eq!(std::fs::read_to_string(path).unwrap(), expected_disk);
        assert_eq!(editor.text(&tab).unwrap(), expected_buffer);
    }
}

#[test]
fn close_all_publishes_captured_order_then_all_closed_and_one_terminal() {
    let dir = tempfile::tempdir().unwrap();
    let (mut pane, ids, paths) = dirty_three(dir.path());
    let request = pane.close_all().unwrap();
    input(&mut pane, key('y'));
    input(&mut pane, key('n'));
    input(&mut pane, key('n'));
    assert!(pane.tabs().is_empty());
    assert_eq!(std::fs::read_to_string(&paths[0]).unwrap(), "0 original\n");
    let events = pane.drain_events();
    let closed = events
        .iter()
        .filter_map(|event| match event {
            oom_edit::PaneEvent::Closed {
                request: id, tab, ..
            } => {
                assert_eq!(id, &request);
                Some(tab.clone())
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(closed, ids);
    let last_closed = events
        .iter()
        .rposition(|event| matches!(event, oom_edit::PaneEvent::Closed { .. }))
        .unwrap();
    let all = events.iter().position(|event| matches!(event, oom_edit::PaneEvent::AllClosed { request: id } if id == &request)).unwrap();
    assert!(last_closed < all);
    assert_eq!(
        events
            .iter()
            .filter_map(terminal_request)
            .filter(|id| *id == &request)
            .count(),
        1
    );
    assert!(
        matches!(events.last(), Some(oom_edit::PaneEvent::Completed { request: id }) if id == &request)
    );
    assert!(pane.drain_events().is_empty());
}

#[test]
fn explicit_close_abort_retains_discarded_buffers_and_undo() {
    let dir = tempfile::tempdir().unwrap();
    let (mut pane, ids, _) = dirty_three(dir.path());
    let request = pane.prepare_close(&ids).unwrap();
    for _ in 0..3 {
        input(&mut pane, key('n'));
    }
    let token = pane.take_prepared_close(&request).unwrap();
    pane.abort_close(token).unwrap();
    assert_eq!(pane.tabs().len(), 3);
    let events = pane.drain_events();
    assert_eq!(
        events
            .iter()
            .filter_map(terminal_request)
            .filter(|id| *id == &request)
            .count(),
        1
    );
    assert!(
        matches!(events.last(), Some(oom_edit::PaneEvent::Cancelled { request: id }) if id == &request)
    );
    for (i, id) in ids.iter().enumerate() {
        pane.focus_tab(id).unwrap();
        assert_eq!(pane.text(id).unwrap(), format!("{i} original\n"));
        input(&mut pane, key('u'));
        assert_eq!(pane.text(id).unwrap(), "original\n");
    }
}

#[test]
fn foreign_and_dropped_tokens_never_complete_another_panes_transaction() {
    let dir = tempfile::tempdir().unwrap();
    let mut first = pane(dir.path(), vec![], Box::new(oom_edit::AllowAllFileAccess));
    let mut second = pane(dir.path(), vec![], Box::new(oom_edit::AllowAllFileAccess));
    let foreign = first.begin_external_change().unwrap();
    let original_request = foreign.request_id().clone();
    let own = second.begin_external_change().unwrap();
    let own_request = own.request_id().clone();
    assert_eq!(
        second
            .commit_external_change(foreign, &[])
            .unwrap_err()
            .kind,
        oom_edit::PaneErrorKind::Stale
    );
    assert_eq!(
        second.new_buffer(OpenOptions::default()).unwrap_err().kind,
        oom_edit::PaneErrorKind::Busy
    );
    assert!(
        matches!(&first.drain_events()[..], [oom_edit::PaneEvent::Cancelled { request }] if request == &original_request)
    );
    second.abort_external_change(own).unwrap();
    assert!(
        matches!(&second.drain_events()[..], [oom_edit::PaneEvent::Cancelled { request }] if request == &own_request)
    );
    assert!(first.drain_events().is_empty());
}

#[test]
fn aborted_and_dropped_retargets_keep_paths_text_and_cursor() {
    let dir = tempfile::tempdir().unwrap();
    let (mut pane, ids, _) = dirty_three(dir.path());
    let before = pane.tabs();
    let mapping = oom_edit::Retarget {
        tab: ids[0].clone(),
        path: dir.path().join("renamed.md"),
    };
    let token = pane
        .prepare_retarget(std::slice::from_ref(&mapping))
        .unwrap();
    let request = token.request_id().clone();
    pane.abort_retarget(token).unwrap();
    assert_eq!(pane.tabs(), before);
    assert!(
        matches!(pane.drain_events().last(), Some(oom_edit::PaneEvent::Cancelled { request: id }) if id == &request)
    );
    let token = pane.prepare_retarget(&[mapping]).unwrap();
    let request = token.request_id().clone();
    drop(token);
    pane.tick(Instant::now());
    assert_eq!(pane.tabs(), before);
    assert!(
        matches!(pane.drain_events().last(), Some(oom_edit::PaneEvent::Cancelled { request: id }) if id == &request)
    );
    input(&mut pane, key('u'));
    assert_eq!(pane.text(&ids[2]).unwrap(), "original\n");
}

#[test]
fn host_quit_all_is_a_request_not_a_close_or_process_exit() {
    let dir = tempfile::tempdir().unwrap();
    let (mut pane, ids, paths) = dirty_three(dir.path());
    ex(&mut pane, "qa");
    assert_eq!(pane.tabs().len(), 3);
    assert!(pane.drain_events().iter().any(|event| matches!(event, oom_edit::PaneEvent::Failed { error, .. } if error.kind == oom_edit::PaneErrorKind::Dirty)));
    ex(&mut pane, "qa!");
    let events = pane.drain_events();
    assert!(events.iter().any(|event| matches!(
        event,
        oom_edit::PaneEvent::QuitAllRequested { force: true, .. }
    )));
    assert!(!events
        .iter()
        .any(|event| matches!(event, oom_edit::PaneEvent::Closed { .. })));
    assert_eq!(pane.tabs().len(), 3);
    ex(&mut pane, "wqa");
    for (i, id) in ids.iter().enumerate() {
        assert_eq!(pane.text(id).unwrap(), format!("{i} original\n"));
        assert_eq!(std::fs::read_to_string(&paths[i]).unwrap(), "original\n");
    }
}

#[cfg(unix)]
#[test]
fn every_command_path_rejects_outside_symlinks_and_bang_targets() {
    use std::os::unix::fs::symlink;
    let dir = tempfile::tempdir_in(std::env::temp_dir().canonicalize().unwrap()).unwrap();
    let inside = dir.path().join("inside");
    std::fs::create_dir(&inside).unwrap();
    let note = inside.join("safe.md");
    let sentinel = dir.path().join("sentinel.md");
    std::fs::write(&note, "original\n").unwrap();
    std::fs::write(&sentinel, "protected\n").unwrap();
    symlink(&sentinel, inside.join("alias.md")).unwrap();
    symlink(dir.path(), inside.join("outside")).unwrap();
    let mut pane = pane(
        &inside,
        vec![note.clone()],
        Box::new(Within(inside.clone())),
    );
    let id = pane.active_tab().unwrap();
    edit(&mut pane, "memory ");
    for command in [
        "e! alias.md",
        "tabnew alias.md",
        "w! alias.md",
        "saveas! alias.md",
        "w! outside/new.md",
        "saveas! ../sentinel.md",
    ] {
        ex(&mut pane, command);
        let events = pane.drain_events();
        assert!(events.iter().any(|event| matches!(event, oom_edit::PaneEvent::Failed { error, .. } if error.kind == oom_edit::PaneErrorKind::Denied)), "{command}: {events:?}");
        assert_eq!(std::fs::read_to_string(&sentinel).unwrap(), "protected\n");
        assert_eq!(pane.text(&id).unwrap(), "memory original\n");
        assert_eq!(pane.tabs().len(), 1);
    }
    assert!(!dir.path().join("new.md").exists());
    assert_eq!(
        pane.open(&inside.join("alias.md"), OpenOptions::default())
            .unwrap_err()
            .kind,
        oom_edit::PaneErrorKind::Denied
    );
    assert_eq!(
        pane.prepare_retarget(&[oom_edit::Retarget {
            tab: id.clone(),
            path: inside.join("alias.md")
        }])
        .unwrap_err()
        .kind,
        oom_edit::PaneErrorKind::Denied
    );
    assert_eq!(pane.tabs()[0].path.as_deref(), Some(note.as_path()));
}

#[test]
fn retarget_revalidates_policy_at_commit_before_changing_any_identity() {
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    };
    struct Switch(Arc<AtomicBool>);
    impl FileAccessPolicy for Switch {
        fn authorize(&self, _: FileOperation, _: &Path) -> Result<(), FilePolicyError> {
            if self.0.load(Ordering::SeqCst) {
                Err(FilePolicyError {
                    reason: "policy changed".into(),
                })
            } else {
                Ok(())
            }
        }
    }
    let dir = tempfile::tempdir().unwrap();
    let denied = Arc::new(AtomicBool::new(false));
    let from = dir.path().join("from.md");
    let to = dir.path().join("to.md");
    std::fs::write(&from, "original\n").unwrap();
    let mut pane = pane(
        dir.path(),
        vec![from.clone()],
        Box::new(Switch(Arc::clone(&denied))),
    );
    let before = pane.tabs();
    let token = pane
        .prepare_retarget(&[oom_edit::Retarget {
            tab: before[0].id.clone(),
            path: to.clone(),
        }])
        .unwrap();
    std::fs::rename(&from, &to).unwrap();
    denied.store(true, Ordering::SeqCst);
    assert_eq!(
        pane.commit_retarget(token).unwrap_err().kind,
        oom_edit::PaneErrorKind::Denied
    );
    assert_eq!(pane.tabs(), before);
    assert_eq!(std::fs::read_to_string(&to).unwrap(), "original\n");
}

#[cfg(unix)]
#[test]
fn path_revalidation_rejects_a_parent_symlink_swapped_during_authorization() {
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Barrier,
    };
    struct PausedPolicy {
        root: PathBuf,
        barrier: Arc<Barrier>,
        pause: AtomicBool,
    }
    impl FileAccessPolicy for PausedPolicy {
        fn authorize(&self, operation: FileOperation, path: &Path) -> Result<(), FilePolicyError> {
            if !path.starts_with(&self.root) {
                return Err(FilePolicyError {
                    reason: "outside root".into(),
                });
            }
            if operation == FileOperation::Save && self.pause.swap(false, Ordering::SeqCst) {
                self.barrier.wait();
                self.barrier.wait();
            }
            Ok(())
        }
    }
    let dir = tempfile::tempdir_in(std::env::temp_dir().canonicalize().unwrap()).unwrap();
    let inside = dir.path().join("inside");
    let docs = inside.join("docs");
    let outside = dir.path().join("outside");
    std::fs::create_dir_all(&docs).unwrap();
    std::fs::create_dir(&outside).unwrap();
    std::fs::write(docs.join("note.md"), "original\n").unwrap();
    std::fs::write(outside.join("note.md"), "sentinel\n").unwrap();
    let barrier = Arc::new(Barrier::new(2));
    let mut pane = pane(
        &inside,
        vec![docs.join("note.md")],
        Box::new(PausedPolicy {
            root: inside.clone(),
            barrier: Arc::clone(&barrier),
            pause: AtomicBool::new(true),
        }),
    );
    let id = pane.active_tab().unwrap();
    edit(&mut pane, "memory ");
    let worker_barrier = Arc::clone(&barrier);
    let worker_docs = docs.clone();
    let worker_outside = outside.clone();
    let moved = inside.join("retained");
    let worker = std::thread::spawn(move || {
        worker_barrier.wait();
        std::fs::rename(&worker_docs, moved).unwrap();
        std::os::unix::fs::symlink(worker_outside, worker_docs).unwrap();
        worker_barrier.wait();
    });
    ex(&mut pane, "w!");
    worker.join().unwrap();
    assert_eq!(
        std::fs::read_to_string(outside.join("note.md")).unwrap(),
        "sentinel\n"
    );
    assert_eq!(
        std::fs::read_to_string(inside.join("retained/note.md")).unwrap(),
        "original\n"
    );
    assert_eq!(pane.text(&id).unwrap(), "memory original\n");
    assert!(pane.drain_events().iter().any(|event| matches!(event, oom_edit::PaneEvent::Failed { error, .. } if error.kind == oom_edit::PaneErrorKind::Denied)));
}

#[test]
fn prepared_close_blocks_writes_and_edits_while_host_move_is_barrier_paused() {
    use std::sync::{Arc, Barrier};
    let dir = tempfile::tempdir().unwrap();
    let from = dir.path().join("from.md");
    let moved = dir.path().join("trash.md");
    std::fs::write(&from, "original\n").unwrap();
    let mut pane = pane(
        dir.path(),
        vec![from.clone()],
        Box::new(oom_edit::AllowAllFileAccess),
    );
    let id = pane.active_tab().unwrap();
    edit(&mut pane, "memory ");
    let request = pane.prepare_close(std::slice::from_ref(&id)).unwrap();
    input(&mut pane, key('n'));
    let token = pane.take_prepared_close(&request).unwrap();
    let barrier = Arc::new(Barrier::new(2));
    let worker_barrier = Arc::clone(&barrier);
    let worker_from = from.clone();
    let worker = std::thread::spawn(move || {
        std::fs::rename(worker_from, moved).unwrap();
        worker_barrier.wait();
        worker_barrier.wait();
    });
    barrier.wait();
    pane.tick(Instant::now() + std::time::Duration::from_secs(10));
    edit(&mut pane, "forbidden ");
    ex(&mut pane, "w!");
    assert_eq!(pane.text(&id).unwrap(), "memory original\n");
    assert!(!from.exists());
    assert_eq!(
        pane.close_all().unwrap_err().kind,
        oom_edit::PaneErrorKind::Busy
    );
    barrier.wait();
    worker.join().unwrap();
    pane.commit_close(token).unwrap();
    assert!(pane.tabs().is_empty());
    let events = pane.drain_events();
    assert_eq!(
        events
            .iter()
            .filter_map(terminal_request)
            .filter(|id| *id == &request)
            .count(),
        1
    );
}

fn pane(base: &Path, paths: Vec<PathBuf>, policy: Box<dyn FileAccessPolicy>) -> EditorPane {
    EditorPane::construct(PaneInit {
        config: Config::default(),
        theme_catalog: ThemeCatalog::builtins(),
        theme_selection: ThemeSelection::new(None, DisplayMode::Dark, Tier::Color16),
        services: PaneServices {
            clipboard_sink: Box::new(Silent),
            theme_sink: Box::new(Silent),
            file_access_policy: policy,
            config_base_directory: base.into(),
            personal_dictionary_path: base.join("personal.txt"),
            working_directory: base.into(),
        },
        options: PaneOptions::default(),
        initial_paths: paths,
        now: Instant::now(),
    })
    .pane
}

fn key(ch: char) -> KeyInput {
    KeyInput {
        code: KeyCode {
            kind: KeyCodeKind::Char(ch),
        },
        mods: Modifiers::default(),
    }
}

fn input(pane: &mut EditorPane, key: KeyInput) {
    pane.handle_input(PaneInput::Key(key), Instant::now());
}

fn esc(pane: &mut EditorPane) {
    input(
        pane,
        KeyInput {
            code: KeyCode {
                kind: KeyCodeKind::Esc,
            },
            mods: Modifiers::default(),
        },
    );
}

fn ex(pane: &mut EditorPane, text: &str) {
    input(pane, key(':'));
    for ch in text.chars() {
        input(pane, key(ch));
    }
    input(
        pane,
        KeyInput {
            code: KeyCode {
                kind: KeyCodeKind::Enter,
            },
            mods: Modifiers::default(),
        },
    );
}

fn edit(pane: &mut EditorPane, text: &str) {
    input(pane, key('i'));
    pane.handle_input(PaneInput::Paste(text.into()), Instant::now());
    esc(pane);
}

#[test]
fn overwrite_choice_is_bound_to_the_displayed_version() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("note.md");
    std::fs::write(&path, "original\n").unwrap();
    let mut pane = pane(
        dir.path(),
        vec![path.clone()],
        Box::new(oom_edit::AllowAllFileAccess),
    );
    let id = pane.active_tab().unwrap();
    edit(&mut pane, "memory ");
    std::fs::write(&path, "version A\n").unwrap();
    ex(&mut pane, "w");
    assert!(pane.input_state().modal);
    std::fs::write(&path, "version B\n").unwrap();
    input(&mut pane, key('o'));
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "version B\n");
    assert_eq!(pane.text(&id).unwrap(), "memory original\n");
    assert!(pane.input_state().modal);
    input(&mut pane, key('o'));
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "memory original\n");
}

#[test]
fn missing_recreation_choice_cannot_overwrite_a_later_replacement() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("note.md");
    std::fs::write(&path, "original\n").unwrap();
    let mut pane = pane(
        dir.path(),
        vec![path.clone()],
        Box::new(oom_edit::AllowAllFileAccess),
    );
    let id = pane.active_tab().unwrap();
    edit(&mut pane, "memory ");
    std::fs::remove_file(&path).unwrap();
    ex(&mut pane, "w");
    assert!(!path.exists());
    assert!(pane.input_state().modal);
    std::fs::write(&path, "replacement\n").unwrap();
    input(&mut pane, key('o'));
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "replacement\n");
    assert_eq!(pane.text(&id).unwrap(), "memory original\n");
    assert!(pane.input_state().modal);
    esc(&mut pane);
    input(&mut pane, key('u'));
    assert_eq!(pane.text(&id).unwrap(), "original\n");
}

#[test]
fn batch_retarget_preserves_ids_dirty_cursor_and_undo() {
    let dir = tempfile::tempdir_in(std::env::temp_dir().canonicalize().unwrap()).unwrap();
    let (mut pane, ids, paths) = dirty_three(dir.path());
    let before = ids
        .iter()
        .map(|id| (pane.text(id).unwrap(), pane.source_cursor(id).unwrap()))
        .collect::<Vec<_>>();
    let mappings = ids
        .iter()
        .enumerate()
        .map(|(i, tab)| oom_edit::Retarget {
            tab: tab.clone(),
            path: dir.path().join(format!("moved-{i}.md")),
        })
        .collect::<Vec<_>>();
    let token = pane.prepare_retarget(&mappings).unwrap();
    let request = token.request_id().clone();
    assert_eq!(
        pane.close_all().unwrap_err().kind,
        oom_edit::PaneErrorKind::Busy
    );
    for (path, mapping) in paths.iter().zip(&mappings) {
        std::fs::rename(path, &mapping.path).unwrap();
    }
    pane.commit_retarget(token).unwrap();
    for ((snapshot, mapping), (text, cursor)) in pane.tabs().iter().zip(&mappings).zip(before) {
        assert_eq!(snapshot.id, mapping.tab);
        assert_eq!(snapshot.path.as_deref(), Some(mapping.path.as_path()));
        assert!(snapshot.dirty);
        assert_eq!(pane.text(&mapping.tab).unwrap(), text);
        assert_eq!(pane.source_cursor(&mapping.tab).unwrap(), cursor);
    }
    let events = pane.drain_events();
    let retargeted = events
        .iter()
        .filter_map(|event| match event {
            oom_edit::PaneEvent::Retargeted {
                request: id, tab, ..
            } => {
                assert_eq!(id, &request);
                Some(tab.clone())
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(retargeted, ids);
    for id in ids {
        pane.focus_tab(&id).unwrap();
        input(&mut pane, key('u'));
        assert_eq!(pane.text(&id).unwrap(), "original\n");
    }
}

#[test]
fn failed_last_retarget_validation_changes_no_bindings() {
    let dir = tempfile::tempdir().unwrap();
    let (mut pane, ids, paths) = dirty_three(dir.path());
    let before = pane.tabs();
    let mappings = ids
        .iter()
        .enumerate()
        .map(|(i, tab)| oom_edit::Retarget {
            tab: tab.clone(),
            path: dir.path().join(format!("moved-{i}.md")),
        })
        .collect::<Vec<_>>();
    let token = pane.prepare_retarget(&mappings).unwrap();
    let request = token.request_id().clone();
    for (path, mapping) in paths.iter().zip(&mappings) {
        std::fs::rename(path, &mapping.path).unwrap();
    }
    std::fs::write(&mappings[2].path, "different bytes\n").unwrap();
    assert_eq!(
        pane.commit_retarget(token).unwrap_err().kind,
        oom_edit::PaneErrorKind::Stale
    );
    assert_eq!(pane.tabs(), before);
    let events = pane.drain_events();
    assert_eq!(events.iter().filter(|event| matches!(event, oom_edit::PaneEvent::Failed { request: id, .. } if id == &request)).count(), 1);
    assert!(!events.iter().any(|event| matches!(
        event,
        oom_edit::PaneEvent::Retargeted { .. } | oom_edit::PaneEvent::PreparedCommitted { .. }
    )));
    for id in ids {
        pane.focus_tab(&id).unwrap();
        input(&mut pane, key('u'));
        assert_eq!(pane.text(&id).unwrap(), "original\n");
    }
}

#[test]
fn retarget_follows_the_observed_modified_file_without_accepting_its_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let from = dir.path().join("from.md");
    let to = dir.path().join("to.md");
    std::fs::write(&from, "original\n").unwrap();
    let mut pane = pane(
        dir.path(),
        vec![from.clone()],
        Box::new(oom_edit::AllowAllFileAccess),
    );
    let id = pane.active_tab().unwrap();
    edit(&mut pane, "memory ");
    std::fs::write(&from, "outside\n").unwrap();
    let token = pane
        .prepare_retarget(&[oom_edit::Retarget {
            tab: id.clone(),
            path: to.clone(),
        }])
        .unwrap();
    std::fs::rename(&from, &to).unwrap();
    pane.commit_retarget(token).unwrap();
    assert!(pane.tabs()[0].changed_on_disk);
    assert_eq!(pane.text(&id).unwrap(), "memory original\n");
    ex(&mut pane, "w");
    assert_eq!(std::fs::read_to_string(&to).unwrap(), "outside\n");
    assert!(pane.input_state().modal);
    esc(&mut pane);
    input(&mut pane, key('u'));
    assert_eq!(pane.text(&id).unwrap(), "original\n");
}

#[test]
fn bang_save_and_save_copy_cannot_override_host_policy() {
    let dir = tempfile::tempdir_in(std::env::temp_dir().canonicalize().unwrap()).unwrap();
    let path = dir.path().join("note.md");
    let copy = dir.path().join("copy.md");
    std::fs::write(&path, "original\n").unwrap();
    let mut pane = pane(dir.path(), vec![path.clone()], Box::new(OpenOnly));
    pane.render(80, 24, Instant::now());
    let id = pane.active_tab().unwrap();
    edit(&mut pane, "changed ");
    ex(&mut pane, "w!");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "original\n");
    assert_eq!(pane.text(&id).unwrap(), "changed original\n");
    assert!(pane.tabs()[0].dirty);
    ex(&mut pane, "w! copy.md");
    assert!(!copy.exists());
    ex(&mut pane, "saveas! copy.md");
    assert!(!copy.exists());
    assert_eq!(pane.tabs()[0].path.as_deref(), Some(path.as_path()));
}

#[test]
fn denied_reload_preserves_the_stable_id_buffer_cursor_and_undo() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("note.md");
    std::fs::write(&path, "original\n").unwrap();
    let mut pane = pane(dir.path(), vec![path.clone()], Box::new(OpenOnly));
    pane.render(80, 24, Instant::now());
    let id = pane.active_tab().unwrap();
    edit(&mut pane, "changed ");
    let cursor = pane.source_cursor(&id).unwrap();
    std::fs::write(&path, "outside\n").unwrap();
    ex(&mut pane, "e!");
    assert_eq!(pane.tabs()[0].id, id);
    assert_eq!(pane.text(&id).unwrap(), "changed original\n");
    assert_eq!(pane.source_cursor(&id).unwrap(), cursor);
    input(&mut pane, key('u'));
    assert_eq!(pane.text(&id).unwrap(), "original\n");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "outside\n");
}

#[test]
fn embedded_last_tab_close_leaves_an_empty_reusable_pane() {
    let dir = tempfile::tempdir_in(std::env::temp_dir().canonicalize().unwrap()).unwrap();
    let path = dir.path().join("note.md");
    std::fs::write(&path, "original\n").unwrap();
    let mut pane = pane(
        dir.path(),
        vec![path.clone()],
        Box::new(oom_edit::AllowAllFileAccess),
    );
    ex(&mut pane, "q");
    assert!(pane.tabs().is_empty());
    pane.open(&path, OpenOptions::default()).unwrap();
    assert_eq!(pane.tabs().len(), 1);
    assert_eq!(pane.tabs()[0].path.as_deref(), Some(path.as_path()));
}

fn dirty_three(base: &Path) -> (EditorPane, Vec<oom_edit::TabId>, Vec<PathBuf>) {
    let paths = (0..3)
        .map(|i| base.join(format!("{i}.md")))
        .collect::<Vec<_>>();
    for path in &paths {
        std::fs::write(path, "original\n").unwrap();
    }
    let mut pane = pane(base, paths.clone(), Box::new(oom_edit::AllowAllFileAccess));
    let ids = pane
        .tabs()
        .into_iter()
        .map(|tab| tab.id)
        .collect::<Vec<_>>();
    for (i, id) in ids.iter().enumerate() {
        pane.focus_tab(id).unwrap();
        edit(&mut pane, &format!("{i} "));
    }
    pane.drain_events();
    (pane, ids, paths)
}

#[test]
fn cancel_third_retains_all_tabs_saved_first_and_discarded_second_undo() {
    let dir = tempfile::tempdir().unwrap();
    let (mut pane, ids, paths) = dirty_three(dir.path());
    let request = pane.prepare_close(&ids).unwrap();
    assert!(
        matches!(pane.drain_events().last(), Some(oom_edit::PaneEvent::AttentionRequired { request: event_request, tab }) if event_request == &request && tab == &ids[0])
    );
    input(&mut pane, key('y'));
    input(&mut pane, key('n'));
    esc(&mut pane);
    assert_eq!(pane.tabs().len(), 3);
    assert_eq!(std::fs::read_to_string(&paths[0]).unwrap(), "0 original\n");
    assert!(!pane.tabs()[0].dirty);
    assert!(pane.tabs()[1].dirty);
    assert!(pane.tabs()[2].dirty);
    let events = pane.drain_events();
    assert_eq!(events.iter().filter(|event| matches!(event, oom_edit::PaneEvent::Cancelled { request: id } if id == &request)).count(), 1);
    assert!(!events.iter().any(|event| matches!(
        event,
        oom_edit::PaneEvent::Closed { .. } | oom_edit::PaneEvent::PreparedClose { .. }
    )));
    assert!(pane.take_prepared_close(&request).is_err());
    pane.focus_tab(&ids[1]).unwrap();
    assert_eq!(pane.text(&ids[1]).unwrap(), "1 original\n");
    input(&mut pane, key('u'));
    assert_eq!(pane.text(&ids[1]).unwrap(), "original\n");
}

#[test]
fn prepared_close_freezes_targets_and_commits_closed_events_in_tab_order() {
    let dir = tempfile::tempdir().unwrap();
    let (mut pane, ids, _) = dirty_three(dir.path());
    let request = pane
        .prepare_close(&[ids[2].clone(), ids[0].clone(), ids[1].clone()])
        .unwrap();
    input(&mut pane, key('y'));
    input(&mut pane, key('n'));
    input(&mut pane, key('n'));
    assert_eq!(pane.tabs().len(), 3);
    let token = pane.take_prepared_close(&request).unwrap();
    assert!(pane.take_prepared_close(&request).is_err());
    pane.focus_tab(&ids[1]).unwrap();
    let before = pane.text(&ids[1]).unwrap();
    edit(&mut pane, "must not edit");
    assert_eq!(pane.text(&ids[1]).unwrap(), before);
    assert_eq!(
        pane.open(&dir.path().join("other.md"), OpenOptions::default())
            .unwrap_err()
            .kind,
        oom_edit::PaneErrorKind::Busy
    );
    pane.commit_close(token).unwrap();
    assert!(pane.tabs().is_empty());
    let events = pane.drain_events();
    let closed = events
        .iter()
        .filter_map(|event| {
            if let oom_edit::PaneEvent::Closed {
                request: event_request,
                tab,
                ..
            } = event
            {
                assert_eq!(event_request, &request);
                Some(tab.clone())
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(closed, ids);
    let saved = events
        .iter()
        .position(|event| matches!(event, oom_edit::PaneEvent::Saved { .. }))
        .unwrap();
    let first_close = events
        .iter()
        .position(|event| matches!(event, oom_edit::PaneEvent::Closed { .. }))
        .unwrap();
    assert!(saved < first_close);
    assert!(
        matches!(events.last(), Some(oom_edit::PaneEvent::Completed { request: id }) if id == &request)
    );
}

#[test]
fn dropped_prepared_close_cancels_once_and_restores_editability() {
    let dir = tempfile::tempdir().unwrap();
    let (mut pane, ids, _) = dirty_three(dir.path());
    let request = pane.prepare_close(&ids).unwrap();
    for _ in 0..3 {
        input(&mut pane, key('n'));
    }
    let token = pane.take_prepared_close(&request).unwrap();
    drop(token);
    pane.focus_tab(&ids[0]).unwrap();
    let cursor = pane.source_cursor(&ids[0]).unwrap();
    edit(&mut pane, "restored ");
    let mut expected = "0 original\n".to_string();
    expected.insert_str(cursor.1, "restored ");
    assert_eq!(pane.text(&ids[0]).unwrap(), expected);
    let events = pane.drain_events();
    assert_eq!(events.iter().filter(|event| matches!(event, oom_edit::PaneEvent::Cancelled { request: id } if id == &request)).count(), 1);
    assert!(!events
        .iter()
        .any(|event| matches!(event, oom_edit::PaneEvent::Closed { .. })));
    assert!(pane.drain_events().is_empty());
}

#[test]
fn external_lease_allows_edits_blocks_io_and_abort_restores_requests() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("note.md");
    std::fs::write(&path, "original\n").unwrap();
    let mut pane = pane(
        dir.path(),
        vec![path.clone()],
        Box::new(oom_edit::AllowAllFileAccess),
    );
    let id = pane.active_tab().unwrap();
    let token = pane.begin_external_change().unwrap();
    edit(&mut pane, "memory ");
    ex(&mut pane, "w!");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "original\n");
    assert_eq!(pane.text(&id).unwrap(), "memory original\n");
    assert_eq!(
        pane.open(&path, OpenOptions::default()).unwrap_err().kind,
        oom_edit::PaneErrorKind::Busy
    );
    assert_eq!(
        pane.begin_external_change().unwrap_err().kind,
        oom_edit::PaneErrorKind::Busy
    );
    assert_eq!(
        pane.close(&id).unwrap_err().kind,
        oom_edit::PaneErrorKind::Busy
    );
    pane.abort_external_change(token).unwrap();
    ex(&mut pane, "w");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "memory original\n");
}

#[test]
fn unnamed_close_requires_explicit_host_path_and_cancel_invents_nothing() {
    let dir = tempfile::tempdir_in(std::env::temp_dir().canonicalize().unwrap()).unwrap();
    let mut pane = pane(dir.path(), vec![], Box::new(oom_edit::AllowAllFileAccess));
    let id = pane.new_buffer(OpenOptions::default()).unwrap();
    edit(&mut pane, "unnamed");
    let request = pane.prepare_close(std::slice::from_ref(&id)).unwrap();
    input(&mut pane, key('y'));
    let events = pane.drain_events();
    assert!(
        matches!(events.last(), Some(oom_edit::PaneEvent::SavePathRequested { request: event_request, tab }) if event_request == &request && tab == &id)
    );
    assert!(std::fs::read_dir(dir.path()).unwrap().next().is_none());
    pane.provide_save_path(&request, None).unwrap();
    assert_eq!(pane.text(&id).unwrap(), "unnamed");
    assert_eq!(pane.tabs()[0].path, None);
    let request = pane.prepare_close(std::slice::from_ref(&id)).unwrap();
    input(&mut pane, key('y'));
    let path = dir.path().join("chosen.md");
    pane.provide_save_path(&request, Some(&path)).unwrap();
    let token = pane.take_prepared_close(&request).unwrap();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "unnamed");
    pane.abort_close(token).unwrap();
    assert_eq!(pane.tabs().len(), 1);
    assert!(!pane.tabs()[0].dirty);
    assert_eq!(pane.tabs()[0].path.as_deref(), Some(path.as_path()));
}

#[test]
fn unnamed_save_path_wait_can_discard_without_inventing_or_writing_a_path() {
    let dir = tempfile::tempdir().unwrap();
    let mut pane = pane(dir.path(), vec![], Box::new(oom_edit::AllowAllFileAccess));
    let id = pane.new_buffer(OpenOptions::default()).unwrap();
    edit(&mut pane, "unnamed");
    let request = pane.prepare_close(std::slice::from_ref(&id)).unwrap();
    input(&mut pane, key('y'));
    input(&mut pane, key('n'));
    let token = pane.take_prepared_close(&request).unwrap();
    assert_eq!(pane.text(&id).unwrap(), "unnamed");
    assert_eq!(pane.tabs()[0].path, None);
    assert!(std::fs::read_dir(dir.path()).unwrap().next().is_none());
    pane.abort_close(token).unwrap();
    input(&mut pane, key('u'));
    assert_eq!(pane.text(&id).unwrap(), "");
}

#[test]
fn denied_host_save_path_is_a_typed_failure_and_retains_the_unnamed_buffer() {
    let dir = tempfile::tempdir().unwrap();
    let mut pane = pane(dir.path(), vec![], Box::new(OpenOnly));
    let id = pane.new_buffer(OpenOptions::default()).unwrap();
    edit(&mut pane, "unnamed");
    let request = pane.prepare_close(std::slice::from_ref(&id)).unwrap();
    input(&mut pane, key('y'));
    let path = dir.path().join("chosen.md");
    assert_eq!(
        pane.provide_save_path(&request, Some(&path))
            .unwrap_err()
            .kind,
        oom_edit::PaneErrorKind::Denied
    );
    assert_eq!(pane.text(&id).unwrap(), "unnamed");
    assert_eq!(pane.tabs()[0].path, None);
    assert!(!path.exists());
    let events = pane.drain_events();
    assert_eq!(
        events
            .iter()
            .filter_map(terminal_request)
            .filter(|id| *id == &request)
            .count(),
        1
    );
    assert!(!events.iter().any(|event| matches!(
        event,
        oom_edit::PaneEvent::PreparedClose { .. } | oom_edit::PaneEvent::Closed { .. }
    )));
}

#[test]
fn host_cancellation_while_unfocused_preserves_insert_mode_and_text() {
    let dir = tempfile::tempdir().unwrap();
    let mut pane = pane(dir.path(), vec![], Box::new(oom_edit::AllowAllFileAccess));
    let id = pane
        .new_buffer(OpenOptions {
            enter_insert: true,
            ..OpenOptions::default()
        })
        .unwrap();
    pane.handle_input(PaneInput::Paste("retained".into()), Instant::now());
    let request = pane.prepare_close(std::slice::from_ref(&id)).unwrap();
    pane.set_focused(false);
    pane.cancel_request(&request).unwrap();
    assert_eq!(pane.text(&id).unwrap(), "retained");
    assert_eq!(pane.tabs()[0].mode, oom_edit::Mode::Insert);
    assert!(!pane.input_state().modal);
    assert!(pane.render(80, 24, Instant::now()).cursor.is_none());
    assert_eq!(
        pane.cancel_request(&request).unwrap_err().kind,
        oom_edit::PaneErrorKind::Stale
    );
    let events = pane.drain_events();
    assert_eq!(
        events
            .iter()
            .filter_map(terminal_request)
            .filter(|id| *id == &request)
            .count(),
        1
    );
}

#[test]
fn denied_prerequisite_save_never_prepares_or_closes_a_tab() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("note.md");
    std::fs::write(&path, "original\n").unwrap();
    let mut pane = pane(dir.path(), vec![path.clone()], Box::new(OpenOnly));
    edit(&mut pane, "retained ");
    let id = pane.active_tab().unwrap();
    let request = pane.prepare_close(std::slice::from_ref(&id)).unwrap();
    input(&mut pane, key('y'));
    let events = pane.drain_events();
    assert!(events.iter().any(|event| matches!(event, oom_edit::PaneEvent::Failed { request: event_request, error, .. } if event_request == &request && error.kind == oom_edit::PaneErrorKind::Denied)));
    assert!(!events.iter().any(|event| matches!(
        event,
        oom_edit::PaneEvent::PreparedClose { .. } | oom_edit::PaneEvent::Closed { .. }
    )));
    assert!(pane.take_prepared_close(&request).is_err());
    assert_eq!(pane.text(&id).unwrap(), "retained original\n");
    assert_eq!(std::fs::read_to_string(path).unwrap(), "original\n");
    input(&mut pane, key('u'));
    assert_eq!(pane.text(&id).unwrap(), "original\n");
}
