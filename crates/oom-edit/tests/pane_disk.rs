//! Disk reconciliation through the same public boundary used by both hosts.

use std::cell::Cell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant};

use oom_edit::{
    AllowAllFileAccess, ClipboardError, ClipboardSink, CommandPolicy, Config, ConfigPersistError,
    DisplayMode, EditorPane, KeyCode, KeyCodeKind, KeyInput, Mode, Modifiers, PaneEvent, PaneInit,
    PaneInput, PaneOptions, PaneServices, ThemeCatalog, ThemePersistenceSink, ThemeSelection,
    ThemeSlot, Tier,
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

fn pane(base: &Path, paths: Vec<PathBuf>, policy: CommandPolicy, now: Instant) -> EditorPane {
    pane_with_access(base, paths, policy, now, Box::new(AllowAllFileAccess))
}

fn pane_with_access(
    base: &Path,
    paths: Vec<PathBuf>,
    policy: CommandPolicy,
    now: Instant,
    access: Box<dyn oom_edit::FileAccessPolicy>,
) -> EditorPane {
    let construction = EditorPane::construct(PaneInit {
        config: Config::default(),
        theme_catalog: ThemeCatalog::builtins(),
        theme_selection: ThemeSelection::new(None, DisplayMode::Dark, Tier::Monochrome),
        services: PaneServices {
            clipboard_sink: Box::new(Silent),
            theme_sink: Box::new(Silent),
            file_access_policy: access,
            config_base_directory: base.into(),
            personal_dictionary_path: base.join("personal.txt"),
            working_directory: base.into(),
        },
        options: PaneOptions {
            command_policy: policy,
            always_tab_bar: true,
            ..PaneOptions::default()
        },
        initial_paths: paths,
        now,
    });
    assert!(construction.report.paths.iter().all(Result::is_ok));
    construction.pane
}

struct CountInspections(Rc<Cell<usize>>);

impl oom_edit::FileAccessPolicy for CountInspections {
    fn authorize(
        &self,
        operation: oom_edit::FileOperation,
        _: &Path,
    ) -> Result<(), oom_edit::FilePolicyError> {
        if operation == oom_edit::FileOperation::Inspect {
            self.0.set(self.0.get() + 1);
        }
        Ok(())
    }
}

#[test]
fn nfr_003_poll_count() {
    let directory = tempfile::tempdir().unwrap();
    let paths = (0..50)
        .map(|index| {
            let path = directory.path().join(format!("{index}.md"));
            std::fs::write(&path, "original\n").unwrap();
            path
        })
        .collect::<Vec<_>>();
    let now = Instant::now();
    let inspections = Rc::new(Cell::new(0));
    let mut editor = pane_with_access(
        directory.path(),
        paths.clone(),
        CommandPolicy::Embedded,
        now,
        Box::new(CountInspections(Rc::clone(&inspections))),
    );
    assert_eq!(inspections.get(), 0);
    for milliseconds in [0, 50, 1000, 1999] {
        editor.tick(now + Duration::from_millis(milliseconds));
    }
    assert_eq!(inspections.get(), 0);
    editor.tick(now + Duration::from_secs(2));
    assert_eq!(inspections.get(), 50);
    for milliseconds in [2000, 2050, 3000, 3999] {
        editor.tick(now + Duration::from_millis(milliseconds));
    }
    assert_eq!(inspections.get(), 50);
    editor.tick(now + Duration::from_secs(4));
    assert_eq!(inspections.get(), 100);
    let token = editor.begin_external_change().unwrap();
    for second in [6, 8, 10, 100] {
        editor.tick(now + Duration::from_secs(second));
    }
    assert_eq!(inspections.get(), 100);
    assert_eq!(
        editor.notify_paths_changed(&paths).unwrap_err().kind,
        oom_edit::PaneErrorKind::Busy
    );
    assert_eq!(inspections.get(), 100);
    editor.abort_external_change(token).unwrap();
    assert_eq!(inspections.get(), 150);
    editor.tick(now + Duration::from_secs(100));
    assert_eq!(inspections.get(), 150);
    editor.tick(now + Duration::from_secs(102));
    assert_eq!(inspections.get(), 200);
}

#[test]
fn fr_054_poll_and_notify() {
    for policy in [CommandPolicy::Standalone, CommandPolicy::Embedded] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("transaction.md");
        std::fs::write(&path, "original\n").unwrap();
        let now = Instant::now();
        let mut editor = pane(directory.path(), vec![path.clone()], policy, now);
        let id = editor.active_tab().unwrap();
        let token = editor.begin_external_change().unwrap();
        dirty(&mut editor, now);
        let mine = editor.text(&id).unwrap();
        editor.set_focused(false);
        editor.drain_events();
        std::fs::write(&path, "intermediate A\n").unwrap();
        editor.tick(now + Duration::from_secs(20));
        assert_eq!(
            editor
                .notify_paths_changed(std::slice::from_ref(&path))
                .unwrap_err()
                .kind,
            oom_edit::PaneErrorKind::Busy
        );
        assert!(!editor.tabs()[0].changed_on_disk);
        assert!(editor.drain_events().is_empty());
        std::fs::write(&path, "final B\n").unwrap();
        editor
            .commit_external_change(token, std::slice::from_ref(&path))
            .unwrap();
        let events = editor.drain_events();
        assert_eq!(events.len(), 3);
        assert!(
            matches!(&events[0], PaneEvent::DiskChangePending { tab, change: oom_edit::DiskChange::Modified(_) } if tab == &id)
        );
        assert!(
            matches!((&events[1], &events[2]), (PaneEvent::ExternalChangeCommitted { request, paths }, PaneEvent::Completed { request: completed }) if request == completed && paths == std::slice::from_ref(&path))
        );
        assert_eq!(editor.text(&id).unwrap(), mine);
        assert_eq!(std::fs::read(&path).unwrap(), b"final B\n");
        editor.tick(now + Duration::from_secs(20));
        assert!(editor.drain_events().is_empty());
        editor.set_focused(true);
        editor.tick(now + Duration::from_secs(20));
        assert!(editor.input_state().modal);
        key(&mut editor, KeyCodeKind::Esc, now + Duration::from_secs(20));
        let token = editor.begin_external_change().unwrap();
        std::fs::write(&path, "abort retains C\n").unwrap();
        editor.set_focused(false);
        editor.drain_events();
        editor.abort_external_change(token).unwrap();
        let events = editor.drain_events();
        assert_eq!(events.len(), 2);
        assert!(matches!(&events[0], PaneEvent::Cancelled { .. }));
        assert!(matches!(&events[1], PaneEvent::DiskChangePending { .. }));
        assert_eq!(editor.text(&id).unwrap(), mine);
        assert_eq!(std::fs::read(&path).unwrap(), b"abort retains C\n");
    }
}

#[test]
fn fr_056_safe_point() {
    for policy in [CommandPolicy::Standalone, CommandPolicy::Embedded] {
        for is_dirty in [false, true] {
            for prefix in ["i", "v", ":", "/", " ", "g~", "2", "g", "\"", " ?"] {
                let directory = tempfile::tempdir().unwrap();
                let path = directory.path().join("safe.md");
                std::fs::write(&path, "original\n").unwrap();
                let now = Instant::now();
                let mut editor = pane(directory.path(), vec![path.clone()], policy, now);
                let id = editor.active_tab().unwrap();
                if is_dirty {
                    dirty(&mut editor, now);
                }
                editor.render(80, 24, now);
                keys(&mut editor, prefix, now);
                let mine = editor.text(&id).unwrap();
                let mode = editor.tabs()[0].mode;
                let cursor = editor.source_cursor(&id).unwrap();
                editor.drain_events();
                std::fs::write(&path, "external replacement\n").unwrap();
                editor
                    .notify_paths_changed(std::slice::from_ref(&path))
                    .unwrap();
                assert_eq!(
                    editor.text(&id).unwrap(),
                    mine,
                    "notification during {prefix:?}"
                );
                editor.tick(now + Duration::from_secs(2));
                assert_eq!(editor.text(&id).unwrap(), mine, "{is_dirty} {prefix:?}");
                assert_eq!(editor.tabs()[0].mode, mode);
                assert_eq!(editor.source_cursor(&id).unwrap(), cursor);
                let events = editor.drain_events();
                assert_eq!(
                    events.len(),
                    1,
                    "unsafe states emit only one pending observation"
                );
                assert!(matches!(&events[0], PaneEvent::DiskChangePending { .. }));
                key(&mut editor, KeyCodeKind::Esc, now + Duration::from_secs(2));
                editor.tick(now + Duration::from_secs(2));
                if is_dirty {
                    assert_eq!(editor.text(&id).unwrap(), mine);
                    assert!(editor.input_state().modal);
                } else {
                    assert_eq!(editor.text(&id).unwrap(), "external replacement\n");
                    assert!(!editor.input_state().modal);
                }
            }
        }
    }
}

fn key(pane: &mut EditorPane, kind: KeyCodeKind, now: Instant) {
    pane.handle_input(
        PaneInput::Key(KeyInput {
            code: KeyCode { kind },
            mods: Modifiers::default(),
        }),
        now,
    );
}

fn keys(pane: &mut EditorPane, text: &str, now: Instant) {
    for ch in text.chars() {
        key(pane, KeyCodeKind::Char(ch), now);
    }
}

fn dirty(pane: &mut EditorPane, now: Instant) {
    keys(pane, "imylocal ", now);
    key(pane, KeyCodeKind::Esc, now);
    assert!(pane.tabs().iter().find(|tab| tab.active).unwrap().dirty);
}

fn ex(pane: &mut EditorPane, command: &str, now: Instant) {
    keys(pane, ":", now);
    keys(pane, command, now);
    key(pane, KeyCodeKind::Enter, now);
}

fn frame_text(pane: &mut EditorPane, now: Instant) -> String {
    pane.render(80, 24, now)
        .cells
        .iter()
        .map(|cell| cell.symbol.as_str())
        .collect()
}

#[test]
fn inactive_tabs_and_unrelated_lifecycle_decisions_delay_reloads() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("active.md");
    std::fs::write(&path, "original\n").unwrap();
    let now = Instant::now();
    let mut editor = pane(
        directory.path(),
        vec![path.clone()],
        CommandPolicy::Embedded,
        now,
    );
    let backed = editor.active_tab().unwrap();
    let other = editor.new_buffer(oom_edit::OpenOptions::default()).unwrap();
    dirty(&mut editor, now);
    let other_text = editor.text(&other).unwrap();
    editor.drain_events();
    std::fs::write(&path, "while inactive\n").unwrap();
    editor
        .notify_paths_changed(std::slice::from_ref(&path))
        .unwrap();
    assert_eq!(editor.text(&backed).unwrap(), "original\n");
    let events = editor.drain_events();
    assert_eq!(events.len(), 1);
    assert!(matches!(&events[0], PaneEvent::DiskChangePending { tab, .. } if tab == &backed));
    let request = editor.close(&other).unwrap();
    editor.focus_tab(&backed).unwrap();
    editor.drain_events();
    editor.tick(now + Duration::from_secs(2));
    assert_eq!(editor.text(&backed).unwrap(), "original\n");
    assert!(editor.input_state().modal);
    assert!(editor.drain_events().is_empty());
    editor.cancel_request(&request).unwrap();
    editor.tick(now + Duration::from_secs(2));
    assert_eq!(editor.text(&backed).unwrap(), "while inactive\n");
    assert_eq!(editor.text(&other).unwrap(), other_text);
    assert_eq!(editor.tabs().len(), 2);
    let events = editor.drain_events();
    assert_eq!(events.len(), 3);
    assert!(
        matches!(&events[0], PaneEvent::Cancelled { request: cancelled } if cancelled == &request)
    );
    assert!(
        matches!((&events[1], &events[2]), (PaneEvent::ReloadedFromDisk { request, tab, .. }, PaneEvent::Completed { request: completed }) if request == completed && tab == &backed)
    );
}

#[test]
fn identical_normalized_reload_preserves_cursor_and_undo() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("identical.md");
    std::fs::write(&path, "original\nsecond\n").unwrap();
    let now = Instant::now();
    let mut editor = pane(
        directory.path(),
        vec![path.clone()],
        CommandPolicy::Embedded,
        now,
    );
    let id = editor.active_tab().unwrap();
    dirty(&mut editor, now);
    ex(&mut editor, "w", now);
    let text = editor.text(&id).unwrap();
    let cursor = editor.source_cursor(&id).unwrap();
    let crlf = text.replace('\n', "\r\n");
    std::fs::write(&path, crlf.as_bytes()).unwrap();
    editor.drain_events();
    editor
        .notify_paths_changed(std::slice::from_ref(&path))
        .unwrap();
    assert_eq!(editor.text(&id).unwrap(), text);
    assert_eq!(editor.source_cursor(&id).unwrap(), cursor);
    assert_eq!(editor.tabs()[0].mode, Mode::Normal);
    assert!(!editor.tabs()[0].dirty);
    assert!(editor
        .drain_events()
        .iter()
        .any(|event| matches!(event, PaneEvent::ReloadedFromDisk { .. })));
    key(&mut editor, KeyCodeKind::Char('u'), now);
    assert_eq!(editor.text(&id).unwrap(), "original\nsecond\n");
    assert_eq!(std::fs::read(&path).unwrap(), crlf.as_bytes());
}

#[test]
fn automatic_reload_restarts_quiescent_idle_work() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("idle.md");
    std::fs::write(&path, "hello world\n").unwrap();
    let now = Instant::now();
    let mut editor = pane(
        directory.path(),
        vec![path.clone()],
        CommandPolicy::Embedded,
        now,
    );
    let mut quiescent = false;
    for _ in 0..10000 {
        if !editor.idle_unit(4096).worked {
            quiescent = true;
            break;
        }
    }
    assert!(quiescent);
    assert!(!editor.tick(now + Duration::from_secs(6)).idle_due);
    std::fs::write(&path, "a changed note with helo\n").unwrap();
    let tick = editor.tick(now + Duration::from_secs(8));
    assert!(tick.redraw);
    assert!(
        tick.idle_due,
        "disk reload must not leave spell work marked quiescent"
    );
    assert!(editor.idle_unit(4096).worked);
}

#[test]
fn stale_reload_and_recreation_choices_preserve_later_bytes() {
    for policy in [CommandPolicy::Standalone, CommandPolicy::Embedded] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("stale.md");
        std::fs::write(&path, "original\n").unwrap();
        let now = Instant::now();
        let mut editor = pane(directory.path(), vec![path.clone()], policy, now);
        let id = editor.active_tab().unwrap();
        dirty(&mut editor, now);
        let mine = editor.text(&id).unwrap();
        std::fs::write(&path, "version A\n").unwrap();
        editor
            .notify_paths_changed(std::slice::from_ref(&path))
            .unwrap();
        std::fs::write(&path, "version B\n").unwrap();
        editor.drain_events();
        key(&mut editor, KeyCodeKind::Char('r'), now);
        assert_eq!(editor.text(&id).unwrap(), mine);
        assert_eq!(std::fs::read(&path).unwrap(), b"version B\n");
        assert!(editor.drain_events().iter().any(|event| matches!(event, PaneEvent::Failed { error, .. } if error.kind == oom_edit::PaneErrorKind::Stale)));
        editor.tick(now);
        key(&mut editor, KeyCodeKind::Esc, now);
        std::fs::remove_file(&path).unwrap();
        editor
            .notify_paths_changed(std::slice::from_ref(&path))
            .unwrap();
        ex(&mut editor, "w", now);
        assert!(editor.input_state().modal);
        std::fs::write(&path, "new occupant C\n").unwrap();
        key(&mut editor, KeyCodeKind::Char('o'), now);
        assert_eq!(std::fs::read(&path).unwrap(), b"new occupant C\n");
        assert_eq!(editor.text(&id).unwrap(), mine);
        assert!(
            editor.input_state().modal,
            "recreation permission cannot overwrite a later occupant"
        );
        key(&mut editor, KeyCodeKind::Esc, now);
        key(&mut editor, KeyCodeKind::Char('u'), now);
        assert_eq!(editor.text(&id).unwrap(), "original\n");
    }
}

#[test]
fn failed_decode_and_non_file_observations_retain_the_buffer() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("failure.md");
    std::fs::write(&path, "original\n").unwrap();
    let now = Instant::now();
    let mut editor = pane(
        directory.path(),
        vec![path.clone()],
        CommandPolicy::Embedded,
        now,
    );
    let id = editor.active_tab().unwrap();
    dirty(&mut editor, now);
    let mine = editor.text(&id).unwrap();
    std::fs::write(&path, [0xff, 0xfe, b'\n']).unwrap();
    editor
        .notify_paths_changed(std::slice::from_ref(&path))
        .unwrap();
    editor.drain_events();
    key(&mut editor, KeyCodeKind::Char('r'), now);
    assert_eq!(editor.text(&id).unwrap(), mine);
    assert_eq!(std::fs::read(&path).unwrap(), [0xff, 0xfe, b'\n']);
    let events = editor.drain_events();
    assert_eq!(events.len(), 1);
    assert!(
        matches!(&events[0], PaneEvent::Failed { error, .. } if error.kind == oom_edit::PaneErrorKind::NotUtf8)
    );
    editor.tick(now);
    assert!(
        !editor.input_state().modal,
        "failed version must not immediately loop a prompt"
    );
    std::fs::remove_file(&path).unwrap();
    std::fs::create_dir(&path).unwrap();
    editor
        .notify_paths_changed(std::slice::from_ref(&path))
        .unwrap();
    let events = editor.drain_events();
    assert_eq!(events.len(), 1);
    assert!(
        matches!(&events[0], PaneEvent::DiskChangePending { change: oom_edit::DiskChange::IoError(error), .. } if error.kind == oom_edit::PaneErrorKind::Io)
    );
    assert!(frame_text(&mut editor, now).contains("[disk error]"));
    assert!(!frame_text(&mut editor, now).contains("[missing]"));
    key(&mut editor, KeyCodeKind::Char('u'), now);
    assert_eq!(editor.text(&id).unwrap(), "original\n");
    assert!(path.is_dir());
}

#[test]
fn compact_disk_prompt_has_reachable_choices_and_non_color_markers() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("界e\u{301}.md");
    std::fs::write(&path, "original\n").unwrap();
    let now = Instant::now();
    let mut editor = pane(
        directory.path(),
        vec![path.clone()],
        CommandPolicy::Embedded,
        now,
    );
    dirty(&mut editor, now);
    std::fs::write(&path, "external\n").unwrap();
    editor
        .notify_paths_changed(std::slice::from_ref(&path))
        .unwrap();
    for size in [(20, 5), (80, 24)] {
        let frame = editor.render(size.0, size.1, now);
        let text = frame
            .cells
            .iter()
            .map(|cell| cell.symbol.as_str())
            .collect::<String>();
        for label in ["Keep mine [k]", "Reload disk [r]", "Cancel [Esc]"] {
            assert!(text.contains(label), "{size:?} missing {label}");
        }
    }
    key(&mut editor, KeyCodeKind::Down, now);
    key(&mut editor, KeyCodeKind::Down, now);
    key(&mut editor, KeyCodeKind::Enter, now);
    assert!(!editor.input_state().modal);
    editor.tick(now + Duration::from_secs(2));
    assert!(!editor.input_state().modal);
    assert!(frame_text(&mut editor, now).contains("[disk changed]"));
}

#[test]
fn fr_054_notification_content_validation() {
    for policy in [CommandPolicy::Standalone, CommandPolicy::Embedded] {
        let directory = tempfile::tempdir_in(std::env::temp_dir().canonicalize().unwrap()).unwrap();
        let path = directory.path().join("equal.md");
        std::fs::write(&path, "before\n").unwrap();
        let reference = oom_edit_core::EditorSession::open_existing(&path).unwrap();
        let metadata = std::fs::metadata(&path).unwrap();
        let now = Instant::now();
        let mut editor = pane(directory.path(), vec![path.clone()], policy, now);
        let id = editor.active_tab().unwrap();
        editor.set_focused(false);
        editor.drain_events();
        std::fs::write(&path, "after!\n").unwrap();
        std::fs::File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_times(std::fs::FileTimes::new().set_modified(metadata.modified().unwrap()))
            .unwrap();
        assert_eq!(std::fs::metadata(&path).unwrap().len(), metadata.len());
        assert_eq!(
            std::fs::metadata(&path).unwrap().modified().unwrap(),
            metadata.modified().unwrap()
        );
        assert_eq!(reference.disk_hint(), oom_edit_core::DiskHint::Unchanged);
        editor.tick(now + Duration::from_secs(2));
        assert!(!editor.tabs()[0].changed_on_disk);
        let unrelated = directory.path().join("unrelated.md");
        std::fs::write(&unrelated, "unrelated").unwrap();
        editor.notify_paths_changed(&[unrelated]).unwrap();
        assert!(editor.drain_events().is_empty());
        editor
            .notify_paths_changed(std::slice::from_ref(&path))
            .unwrap();
        editor
            .notify_paths_changed(std::slice::from_ref(&path))
            .unwrap();
        assert_eq!(editor.text(&id).unwrap(), "before\n");
        let events = editor.drain_events();
        assert_eq!(events.len(), 1);
        assert!(
            matches!(&events[0], PaneEvent::DiskChangePending { tab, change: oom_edit::DiskChange::Modified(version) } if tab == &id && !version.is_missing())
        );
        assert!(editor.tabs()[0].changed_on_disk);
        editor.set_focused(true);
        editor.tick(now + Duration::from_secs(2));
        assert_eq!(editor.text(&id).unwrap(), "after!\n");
        let events = editor.drain_events();
        assert_eq!(events.len(), 2);
        assert!(
            matches!((&events[0], &events[1]), (PaneEvent::ReloadedFromDisk { request, tab, path: loaded }, PaneEvent::Completed { request: completed }) if request == completed && tab == &id && loaded == &path)
        );
    }
}

#[test]
fn fr_055_keep_mine_revalidation() {
    for policy in [CommandPolicy::Standalone, CommandPolicy::Embedded] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("dirty.md");
        std::fs::write(&path, "original\n").unwrap();
        let now = Instant::now();
        let mut editor = pane(directory.path(), vec![path.clone()], policy, now);
        let id = editor.active_tab().unwrap();
        dirty(&mut editor, now);
        let mine = editor.text(&id).unwrap();
        editor.drain_events();
        std::fs::write(&path, "version A\n").unwrap();
        editor
            .notify_paths_changed(std::slice::from_ref(&path))
            .unwrap();
        assert!(editor.input_state().modal);
        editor.drain_events();
        std::fs::write(&path, "version B\n").unwrap();
        key(&mut editor, KeyCodeKind::Char('k'), now);
        assert_eq!(editor.text(&id).unwrap(), mine);
        assert_eq!(std::fs::read(&path).unwrap(), b"version B\n");
        assert!(editor.tabs()[0].dirty);
        let events = editor.drain_events();
        assert!(events.iter().any(|event| matches!(event, PaneEvent::Failed { error, .. } if error.kind == oom_edit::PaneErrorKind::Stale)));
        assert_eq!(
            events
                .iter()
                .filter(|event| matches!(event, PaneEvent::DiskChangePending { .. }))
                .count(),
            1
        );
        editor.tick(now);
        assert!(editor.input_state().modal);
        key(&mut editor, KeyCodeKind::Char('k'), now);
        assert!(!editor.input_state().modal);
        assert!(!editor.tabs()[0].changed_on_disk);
        editor.tick(now + Duration::from_secs(2));
        assert!(!editor.input_state().modal);
        ex(&mut editor, "w", now + Duration::from_secs(2));
        assert!(
            !editor.input_state().modal,
            "acknowledged version must not prompt a second time"
        );
        assert_eq!(std::fs::read_to_string(&path).unwrap(), mine);
        assert!(!editor.tabs()[0].dirty);
        key(
            &mut editor,
            KeyCodeKind::Char('u'),
            now + Duration::from_secs(2),
        );
        assert_eq!(
            editor.text(&id).unwrap(),
            "original\n",
            "keep mine and save must preserve undo"
        );
        std::fs::write(&path, "version C after the save\n").unwrap();
        editor
            .notify_paths_changed(std::slice::from_ref(&path))
            .unwrap();
        assert!(
            editor.input_state().modal,
            "later bytes require a fresh decision"
        );
        key(
            &mut editor,
            KeyCodeKind::Char('r'),
            now + Duration::from_secs(2),
        );
        assert_eq!(editor.text(&id).unwrap(), "version C after the save\n");
        key(
            &mut editor,
            KeyCodeKind::Char('u'),
            now + Duration::from_secs(2),
        );
        assert_eq!(
            editor.text(&id).unwrap(),
            "version C after the save\n",
            "changed reload resets undo"
        );
    }
}

#[test]
fn fr_055_missing_retention() {
    for policy in [CommandPolicy::Standalone, CommandPolicy::Embedded] {
        for saved in [false, true] {
            let directory = tempfile::tempdir().unwrap();
            let path = directory.path().join("missing.md");
            std::fs::write(&path, "original\n").unwrap();
            let now = Instant::now();
            let mut editor = pane(directory.path(), vec![path.clone()], policy, now);
            let id = editor.active_tab().unwrap();
            dirty(&mut editor, now);
            if saved {
                ex(&mut editor, "w", now);
            }
            let mine = editor.text(&id).unwrap();
            let cursor = editor.source_cursor(&id).unwrap();
            editor.drain_events();
            std::fs::remove_file(&path).unwrap();
            editor
                .notify_paths_changed(std::slice::from_ref(&path))
                .unwrap();
            assert_eq!(editor.text(&id).unwrap(), mine);
            assert_eq!(editor.source_cursor(&id).unwrap(), cursor);
            assert_eq!(editor.tabs().len(), 1);
            assert!(!editor.input_state().modal);
            assert!(editor.tabs()[0].changed_on_disk);
            assert_eq!(editor.tabs()[0].dirty, !saved);
            assert!(frame_text(&mut editor, now).contains("[missing]"));
            ex(&mut editor, "w", now);
            assert!(editor.input_state().modal);
            assert!(!path.exists());
            key(&mut editor, KeyCodeKind::Esc, now);
            assert!(!path.exists());
            key(&mut editor, KeyCodeKind::Char('u'), now);
            assert_eq!(editor.text(&id).unwrap(), "original\n");
            ex(&mut editor, "w", now);
            key(&mut editor, KeyCodeKind::Char('o'), now);
            assert_eq!(std::fs::read_to_string(&path).unwrap(), "original\n");
            assert!(!editor.tabs()[0].changed_on_disk);
        }
    }
}

#[test]
fn fr_055_dirty_clean_disk() {
    for policy in [CommandPolicy::Standalone, CommandPolicy::Embedded] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("note.md");
        std::fs::write(&path, "original\n").unwrap();
        let now = Instant::now();
        let mut editor = pane(directory.path(), vec![path.clone()], policy, now);
        let id = editor.active_tab().unwrap();
        editor.render(80, 24, now);
        editor.drain_events();
        std::fs::write(&path, "external clean replacement\n").unwrap();
        let later = now + Duration::from_secs(2);
        editor.tick(later);
        assert_eq!(editor.text(&id).unwrap(), "external clean replacement\n");
        assert!(!editor.tabs()[0].dirty);
        assert_eq!(editor.tabs()[0].mode, Mode::Normal);
        assert!(!editor.tabs()[0].changed_on_disk);
        assert_eq!(
            std::fs::read(&path).unwrap(),
            b"external clean replacement\n"
        );
        assert!(!editor
            .drain_events()
            .iter()
            .any(|event| matches!(event, PaneEvent::AttentionRequired { .. })));

        dirty(&mut editor, later);
        let mine = editor.text(&id).unwrap();
        editor.drain_events();
        std::fs::write(&path, "external dirty replacement\n").unwrap();
        editor.tick(later + Duration::from_secs(2));
        assert_eq!(editor.text(&id).unwrap(), mine);
        assert_eq!(
            std::fs::read(&path).unwrap(),
            b"external dirty replacement\n"
        );
        assert!(editor.input_state().modal);
        assert!(editor.tabs()[0].changed_on_disk);
        let events = editor.drain_events();
        assert_eq!(
            events
                .iter()
                .filter(|event| matches!(event, PaneEvent::DiskChangePending { .. }))
                .count(),
            1
        );
        assert!(!events
            .iter()
            .any(|event| matches!(event, PaneEvent::AttentionRequired { .. })));
    }
}

#[test]
fn fr_056_background_no_attention() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("background.md");
    std::fs::write(&path, "before\n").unwrap();
    let now = Instant::now();
    let mut editor = pane(
        directory.path(),
        vec![path.clone()],
        CommandPolicy::Embedded,
        now,
    );
    let id = editor.active_tab().unwrap();
    keys(&mut editor, "i", now);
    editor.set_focused(false);
    editor.drain_events();
    std::fs::write(&path, "after change\n").unwrap();
    for second in [2, 3, 4, 6] {
        editor.tick(now + Duration::from_secs(second));
        assert_eq!(editor.text(&id).unwrap(), "before\n");
        assert_eq!(editor.tabs()[0].mode, Mode::Insert);
        assert!(!editor.input_state().modal);
    }
    assert!(editor.tabs()[0].changed_on_disk);
    let events = editor.drain_events();
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event, PaneEvent::DiskChangePending { .. }))
            .count(),
        1
    );
    assert!(!events
        .iter()
        .any(|event| matches!(event, PaneEvent::AttentionRequired { .. })));
    assert_eq!(std::fs::read(&path).unwrap(), b"after change\n");
}
