use super::*;
use std::cell::RefCell;
use std::rc::Rc;

use oom_edit_core::{
    DiskIoError, DiskIoErrorKind, KeyCode, KeyCodeKind, KeyInput, Modifiers, RecordingClipboardSink,
};

#[derive(Default)]
struct Counts {
    metadata: Vec<PathBuf>,
    content: Vec<PathBuf>,
}

struct CountedProbe {
    counts: Rc<RefCell<Counts>>,
    error: bool,
}

impl DiskProbe for CountedProbe {
    fn hint(&mut self, session: &EditorSession) -> DiskHint {
        self.counts
            .borrow_mut()
            .metadata
            .push(session.path().unwrap().into());
        if self.error {
            DiskHint::IoError
        } else {
            session.disk_hint()
        }
    }
    fn state(&mut self, session: &EditorSession) -> DiskState {
        self.counts
            .borrow_mut()
            .content
            .push(session.path().unwrap().into());
        if self.error {
            DiskState::IoError(DiskIoError::new(
                DiskIoErrorKind::PermissionDenied,
                "injected permission denial",
            ))
        } else {
            session.disk_state()
        }
    }
}

fn app(path: &Path, now: Instant) -> App {
    App::new(
        EditorSession::open_existing(path).unwrap(),
        ResolvedTheme::injected("accessible", false, Tier::Monochrome),
        true,
        false,
        Box::new(RecordingClipboardSink::default()),
        Box::new(crate::config::DisabledConfigStore),
        now,
    )
}

fn key(app: &mut App, kind: KeyCodeKind, now: Instant) {
    app.handle_key_input(
        KeyInput {
            code: KeyCode { kind },
            mods: Modifiers::default(),
        },
        now,
    );
}

fn input(app: &mut App, text: &str, now: Instant) {
    for ch in text.chars() {
        key(app, KeyCodeKind::Char(ch), now);
    }
}

#[test]
fn nfr_003_fifty_tab_metadata_and_content_counters() {
    let directory = tempfile::tempdir_in(std::env::temp_dir().canonicalize().unwrap()).unwrap();
    let now = Instant::now();
    let paths = (0..50)
        .map(|index| {
            let path = directory.path().join(format!("{index}.md"));
            std::fs::write(&path, "original\n").unwrap();
            path
        })
        .collect::<Vec<_>>();
    let mut editor = app(&paths[0], now);
    for path in &paths[1..] {
        editor.append_tab(EditorSession::open_existing(path).unwrap());
    }
    editor.append_tab(EditorSession::from_text("unnamed"));
    editor.set_focused(false);
    let counts = Rc::new(RefCell::new(Counts::default()));
    editor.disk_probe = Box::new(CountedProbe {
        counts: Rc::clone(&counts),
        error: false,
    });
    for millis in [0, 1, 50, 1999] {
        editor.tick(now + Duration::from_millis(millis));
    }
    assert!(counts.borrow().metadata.is_empty());
    editor.tick(now + Duration::from_secs(2));
    assert_eq!(counts.borrow().metadata, paths);
    assert!(counts.borrow().content.is_empty());
    for millis in [2000, 2001, 2050, 3999] {
        editor.tick(now + Duration::from_millis(millis));
    }
    assert_eq!(counts.borrow().metadata.len(), 50);
    editor.tick(now + Duration::from_secs(4));
    assert_eq!(counts.borrow().metadata.len(), 100);
    let token = match editor.execute_lifecycle(LifecycleAction::BeginExternalChange) {
        LifecycleOutcome::External(token) => token,
        _ => panic!("expected external-change token"),
    };
    editor.drain_lifecycle_events();
    std::fs::write(&paths[0], "intermediate A").unwrap();
    for second in [6, 8, 30, 100] {
        editor.tick(now + Duration::from_secs(second));
    }
    assert_eq!(counts.borrow().metadata.len(), 100);
    assert!(counts.borrow().content.is_empty());
    assert_eq!(
        editor.notify_disk_paths(&paths).unwrap_err().kind,
        PaneErrorKind::Busy
    );
    assert!(counts.borrow().content.is_empty());
    std::fs::write(&paths[0], "final B").unwrap();
    editor.execute_lifecycle(LifecycleAction::AbortToken {
        request: token.request.clone(),
    });
    assert_eq!(
        counts.borrow().content.len(),
        50,
        "release reconciles once without acknowledging"
    );
    assert_eq!(counts.borrow().metadata.len(), 100);
    assert_eq!(editor.tabs[0].session.document(), "original\n");
    let events = editor.drain_lifecycle_events();
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event, PaneEvent::DiskChangePending { .. }))
            .count(),
        1
    );
    editor.tick(now + Duration::from_secs(100));
    assert_eq!(counts.borrow().metadata.len(), 100);
    let token = match editor.execute_lifecycle(LifecycleAction::BeginExternalChange) {
        LifecycleOutcome::External(token) => token,
        _ => panic!("expected token"),
    };
    editor.execute_lifecycle(LifecycleAction::CommitExternalChange {
        request: token.request.clone(),
        paths: vec![directory.path().into()],
    });
    assert_eq!(counts.borrow().content.len(), 100);
    editor.tick(now + Duration::from_secs(100));
    assert_eq!(counts.borrow().metadata.len(), 100);
    let token = match editor.execute_lifecycle(LifecycleAction::BeginExternalChange) {
        LifecycleOutcome::External(token) => token,
        _ => panic!("expected token"),
    };
    drop(token);
    editor.drain_lifecycle_events();
    assert_eq!(counts.borrow().content.len(), 150);
    editor.drain_lifecycle_events();
    assert_eq!(
        counts.borrow().content.len(),
        150,
        "abandonment is observed once"
    );
    editor.tick(now + Duration::from_secs(102));
    assert_eq!(counts.borrow().metadata.len(), 150);
}

#[derive(Clone, Copy, Debug)]
enum Surface {
    Safe,
    Hidden,
    OtherTab,
    Insert,
    Select,
    Command,
    Search,
    Space,
    CorePending,
    Overlay,
    Lifecycle,
}

#[test]
fn automatic_reload_preserves_then_clamps_viewport_and_source_cursor() {
    let directory = tempfile::tempdir_in(std::env::temp_dir().canonicalize().unwrap()).unwrap();
    let path = directory.path().join("viewport.md");
    let original = (0..100)
        .map(|index| format!("line {index}\n\n"))
        .collect::<String>();
    std::fs::write(&path, &original).unwrap();
    let now = Instant::now();
    let mut editor = app(&path, now);
    editor.render_owned(80, 24, now, AppRenderOptions::standalone(), None);
    input(&mut editor, "50G", now);
    editor.render_owned(80, 24, now, AppRenderOptions::standalone(), None);
    let top = editor.tabs[0].rendered_top;
    let cursor = editor.tabs[0].session.cursor();
    assert!(top > 0);
    std::fs::write(&path, original.replace("line", "row ")).unwrap();
    editor
        .notify_disk_paths(std::slice::from_ref(&path))
        .unwrap();
    assert_eq!(editor.tabs[0].rendered_top, top);
    assert_eq!(editor.tabs[0].session.cursor(), cursor);
    editor.tabs[0].top_line = 50;
    editor.tabs[0].left_col = 100;
    editor.tabs[0].skip_rows = 100;
    editor.tabs[0].rendered_left_col = 100;
    std::fs::write(&path, "short\n").unwrap();
    editor
        .notify_disk_paths(std::slice::from_ref(&path))
        .unwrap();
    let entry = &editor.tabs[0];
    assert_eq!(entry.session.cursor(), (1, 0));
    assert_eq!(entry.top_line, 0);
    assert_eq!(entry.left_col, 0);
    assert_eq!(entry.skip_rows, 0);
    assert_eq!(entry.rendered_top, 0);
    assert_eq!(entry.rendered_left_col, 0);
    assert!(!entry.session.is_dirty());
}

#[derive(Clone, Copy, Debug)]
enum Observation {
    Modified,
    Missing,
    Error,
}

#[test]
fn clean_dirty_observation_and_safe_point_matrix() {
    for dirty in [false, true] {
        for observation in [
            Observation::Modified,
            Observation::Missing,
            Observation::Error,
        ] {
            for surface in [
                Surface::Safe,
                Surface::Hidden,
                Surface::OtherTab,
                Surface::Insert,
                Surface::Select,
                Surface::Command,
                Surface::Search,
                Surface::Space,
                Surface::CorePending,
                Surface::Overlay,
                Surface::Lifecycle,
            ] {
                let directory =
                    tempfile::tempdir_in(std::env::temp_dir().canonicalize().unwrap()).unwrap();
                let path = directory.path().join("matrix.md");
                std::fs::write(&path, "original\nsecond\n").unwrap();
                let now = Instant::now();
                let mut editor = app(&path, now);
                if dirty {
                    input(&mut editor, "ilocal ", now);
                    key(&mut editor, KeyCodeKind::Esc, now);
                }
                editor.render_owned(80, 24, now, AppRenderOptions::standalone(), None);
                let before = editor.tabs[0].session.document();
                let cursor = editor.tabs[0].session.cursor();
                match surface {
                    Surface::Safe => {}
                    Surface::Hidden => editor.set_focused(false),
                    Surface::OtherTab => {
                        editor.append_tab(EditorSession::from_text("other"));
                    }
                    Surface::Insert => input(&mut editor, "i", now),
                    Surface::Select => input(&mut editor, "v", now),
                    Surface::Command => input(&mut editor, ":", now),
                    Surface::Search => {
                        input(&mut editor, "/", now);
                        assert!(editor.tabs[0].session.rendered_search_prompt().is_some());
                    }
                    Surface::Space => input(&mut editor, " ", now),
                    Surface::CorePending => {
                        input(&mut editor, "g~", now);
                        assert!(editor.input_grammar_pending());
                    }
                    Surface::Overlay => {
                        editor.overlay = Overlay::open_palette(crate::command::Contexts::NORMAL);
                    }
                    Surface::Lifecycle => {
                        editor.lifecycle_state = LifecycleState::Executing(LifecycleContext {
                            request: editor.next_request(),
                            target: editor.active_tab_id(),
                        });
                    }
                }
                let mode = editor.tabs[0].session.mode();
                let counts = Rc::new(RefCell::new(Counts::default()));
                editor.disk_probe = Box::new(CountedProbe {
                    counts: Rc::clone(&counts),
                    error: matches!(observation, Observation::Error),
                });
                editor.drain_lifecycle_events();
                match observation {
                    Observation::Modified => {
                        std::fs::write(&path, "external replacement\n").unwrap()
                    }
                    Observation::Missing => std::fs::remove_file(&path).unwrap(),
                    Observation::Error => {}
                }
                let tick = editor.tick(now + POLL_INTERVAL);
                assert!(tick.redraw, "{dirty} {observation:?} {surface:?}");
                let reload = !dirty
                    && matches!(observation, Observation::Modified)
                    && matches!(surface, Surface::Safe);
                if reload {
                    assert_eq!(editor.tabs[0].session.document(), "external replacement\n");
                    assert_eq!(editor.tabs[0].session.mode(), Mode::Normal);
                    assert!(editor.tabs[0].disk_change.marker().is_none());
                } else {
                    assert_eq!(
                        editor.tabs[0].session.document(),
                        before,
                        "{dirty} {observation:?} {surface:?}"
                    );
                    assert_eq!(editor.tabs[0].session.cursor(), cursor);
                    assert_eq!(editor.tabs[0].session.mode(), mode);
                    assert!(editor.tabs[0].disk_change.marker().is_some());
                }
                let events = editor.drain_lifecycle_events();
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
                assert_eq!(
                    events
                        .iter()
                        .filter(|event| matches!(event, PaneEvent::ReloadedFromDisk { .. }))
                        .count(),
                    usize::from(reload)
                );
                let disk_prompt = dirty
                    && matches!(observation, Observation::Modified)
                    && matches!(surface, Surface::Safe);
                assert_eq!(
                    matches!(editor.overlay, Overlay::DiskChange(_)),
                    disk_prompt
                );
                if matches!(observation, Observation::Missing) {
                    assert!(!path.exists());
                }
                if matches!(observation, Observation::Modified) {
                    assert_eq!(std::fs::read(&path).unwrap(), b"external replacement\n");
                }
                if matches!(observation, Observation::Error) {
                    assert_eq!(std::fs::read(&path).unwrap(), b"original\nsecond\n");
                    assert!(
                        matches!(&editor.tabs[0].disk_change, DiskObservation::Pending(DiskChange::IoError(error)) | DiskObservation::Deferred(DiskChange::IoError(error)) if error.kind == PaneErrorKind::Io)
                    );
                }
            }
        }
    }
}
