//! Autosave through the public pane boundary, with injected time and real file bytes.

use oom_edit::{
    AllowAllFileAccess, AutosaveConfig, ClipboardError, ClipboardSink, Config, ConfigApplyReport,
    ConfigField, ConfigPersistError, DisplayMode, EditorPane, FileAccessPolicy, FileOperation,
    FilePolicyError, KeyCode, KeyCodeKind, KeyInput, Mode, Modifiers, OpenOptions, PaneEvent,
    PaneInit, PaneInput, PaneOptions, PaneServices, Retarget, TabId, ThemeCatalog,
    ThemePersistenceSink, ThemeSelection, ThemeSlot, Tier,
};
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use std::time::{Duration, Instant};

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
fn pane(
    base: &Path,
    paths: Vec<PathBuf>,
    autosave: AutosaveConfig,
    now: Instant,
    policy: Box<dyn FileAccessPolicy>,
) -> EditorPane {
    let mut config = Config {
        autosave,
        ..Config::default()
    };
    config.spell.enabled = false;
    let construction = EditorPane::construct(PaneInit {
        config,
        theme_catalog: ThemeCatalog::builtins(),
        theme_selection: ThemeSelection::new(None, DisplayMode::Dark, Tier::Monochrome),
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
        now,
    });
    assert!(construction.report.paths.iter().all(Result::is_ok));
    construction.pane
}
fn key(pane: &mut EditorPane, code: KeyCodeKind, now: Instant) {
    pane.handle_input(
        PaneInput::Key(KeyInput {
            code: KeyCode { kind: code },
            mods: Modifiers::default(),
        }),
        now,
    );
}
fn insert(pane: &mut EditorPane, text: &str, now: Instant) {
    key(pane, KeyCodeKind::Char('i'), now);
    pane.handle_input(PaneInput::Paste(text.into()), now);
}
fn finish(pane: &mut EditorPane, now: Instant) {
    key(pane, KeyCodeKind::Esc, now);
}
fn write(base: &Path, name: &str) -> PathBuf {
    let path = base.join(name);
    std::fs::write(&path, "original\n").unwrap();
    path
}
fn options(interval: bool, focus: bool) -> AutosaveConfig {
    AutosaveConfig {
        every_five_minutes: interval,
        on_focus_loss: focus,
    }
}
fn saved(events: &[PaneEvent]) -> Vec<TabId> {
    events
        .iter()
        .filter_map(|e| match e {
            PaneEvent::Saved { tab, .. } => Some(tab.clone()),
            _ => None,
        })
        .collect()
}
fn ex(pane: &mut EditorPane, text: &str, now: Instant) {
    key(pane, KeyCodeKind::Char(':'), now);
    for ch in text.chars() {
        key(pane, KeyCodeKind::Char(ch), now);
    }
    key(pane, KeyCodeKind::Enter, now);
}

#[test]
fn periodic_saves_all_named_dirty_tabs_at_boundary_including_unfocused_insert() {
    let dir = tempfile::tempdir().unwrap();
    let now = Instant::now();
    let first = write(dir.path(), "first.md");
    let second = write(dir.path(), "second.md");
    let clean = write(dir.path(), "clean.md");
    let mut pane = pane(
        dir.path(),
        vec![first.clone(), second.clone(), clean.clone()],
        options(true, false),
        now,
        Box::new(AllowAllFileAccess),
    );
    let ids: Vec<_> = pane.tabs().into_iter().map(|t| t.id).collect();
    pane.focus_tab(&ids[0]).unwrap();
    insert(&mut pane, "one ", now);
    finish(&mut pane, now);
    pane.focus_tab(&ids[1]).unwrap();
    insert(&mut pane, "two ", now);
    let cursor = pane.source_cursor(&ids[1]).unwrap();
    let unnamed = pane.new_buffer(OpenOptions::default()).unwrap();
    insert(&mut pane, "unnamed", now);
    finish(&mut pane, now);
    pane.focus_tab(&ids[1]).unwrap();
    let mru = pane
        .tabs()
        .into_iter()
        .map(|t| t.mru_rank)
        .collect::<Vec<_>>();
    pane.set_focused(false);
    pane.drain_events();
    pane.tick(now + Duration::from_millis(299_999));
    assert_eq!(std::fs::read_to_string(&first).unwrap(), "original\n");
    assert!(saved(&pane.drain_events()).is_empty());
    pane.tick(now + Duration::from_secs(300));
    assert_eq!(std::fs::read_to_string(&first).unwrap(), "one original\n");
    assert_eq!(std::fs::read_to_string(&second).unwrap(), "two original\n");
    assert_eq!(std::fs::read_to_string(&clean).unwrap(), "original\n");
    assert_eq!(pane.text(&unnamed).unwrap(), "unnamed");
    assert!(pane.tabs().iter().find(|t| t.id == unnamed).unwrap().dirty);
    assert!(pane
        .tabs()
        .iter()
        .filter(|t| ids.contains(&t.id))
        .all(|t| !t.dirty));
    assert_eq!(pane.active_tab(), Some(ids[1].clone()));
    assert_eq!(pane.source_cursor(&ids[1]).unwrap(), cursor);
    assert_eq!(pane.tabs()[1].mode, Mode::Insert);
    assert_eq!(
        pane.tabs()
            .into_iter()
            .map(|t| t.mru_rank)
            .collect::<Vec<_>>(),
        mru
    );
    let events = pane.drain_events();
    assert_eq!(saved(&events), ids[..2]);
    for event in &events {
        if let PaneEvent::Saved { request, .. } = event {
            assert_eq!(
                events
                    .iter()
                    .filter(
                        |e| matches!(e, PaneEvent::Completed { request: other } if request == other)
                    )
                    .count(),
                1
            );
        }
    }
    assert!(pane.drain_events().is_empty());
    pane.tick(now + Duration::from_secs(301));
    assert!(saved(&pane.drain_events()).is_empty());
    pane.set_focused(true);
    pane.handle_input(
        PaneInput::Paste("more ".into()),
        now + Duration::from_secs(301),
    );
    assert!(pane.tabs()[1].dirty);
    pane.tick(now + Duration::from_secs(599));
    assert!(saved(&pane.drain_events()).is_empty());
    pane.tick(now + Duration::from_secs(600));
    assert_eq!(saved(&pane.drain_events()), vec![ids[1].clone()]);
}

#[test]
fn switches_are_independent_focus_is_a_transition_and_combined_triggers_do_not_duplicate() {
    for interval in [false, true] {
        for focus in [false, true] {
            let dir = tempfile::tempdir().unwrap();
            let now = Instant::now();
            let path = write(dir.path(), "note.md");
            let mut pane = pane(
                dir.path(),
                vec![path.clone()],
                options(interval, focus),
                now,
                Box::new(AllowAllFileAccess),
            );
            let id = pane.active_tab().unwrap();
            insert(&mut pane, "first ", now);
            finish(&mut pane, now);
            pane.drain_events();
            pane.tick(now + Duration::from_secs(10));
            pane.set_focused(false);
            assert_eq!(saved(&pane.drain_events()).len(), usize::from(focus));
            pane.set_focused(false);
            assert!(saved(&pane.drain_events()).is_empty());
            pane.set_focused(true);
            assert!(saved(&pane.drain_events()).is_empty());
            pane.tick(now + Duration::from_secs(300));
            assert_eq!(
                saved(&pane.drain_events()).len(),
                usize::from(interval && !focus)
            );
            assert_eq!(pane.tabs()[0].dirty, !(interval || focus));
            assert_eq!(pane.text(&id).unwrap(), "first original\n");
            if interval || focus {
                assert_eq!(std::fs::read_to_string(&path).unwrap(), "first original\n");
            } else {
                assert_eq!(std::fs::read_to_string(&path).unwrap(), "original\n");
            }
            insert(&mut pane, "second ", now + Duration::from_secs(300));
            finish(&mut pane, now + Duration::from_secs(300));
            pane.tick(now + Duration::from_secs(600));
            pane.set_focused(false);
            assert_eq!(
                saved(&pane.drain_events()).len(),
                usize::from(interval || focus)
            );
        }
    }
}

#[test]
fn live_configuration_rearms_without_persistence_and_late_ticks_do_not_catch_up() {
    let dir = tempfile::tempdir().unwrap();
    let now = Instant::now();
    let mut pane = pane(
        dir.path(),
        vec![],
        options(false, false),
        now,
        Box::new(AllowAllFileAccess),
    );
    assert_eq!(pane.tick(now).next_deadline, None);
    let mut config = Config::default();
    config.spell.enabled = false;
    config.autosave = options(true, true);
    pane.tick(now + Duration::from_secs(50));
    assert_eq!(
        pane.apply_config(config.clone()).applied_now,
        vec![
            ConfigField::AutosaveEveryFiveMinutes,
            ConfigField::AutosaveOnFocusLoss
        ]
    );
    assert_eq!(
        pane.tick(now + Duration::from_secs(50)).next_deadline,
        Some(now + Duration::from_secs(350))
    );
    pane.tick(now + Duration::from_secs(100));
    assert_eq!(
        pane.apply_config(config.clone()),
        ConfigApplyReport::default()
    );
    assert_eq!(
        pane.tick(now + Duration::from_secs(100)).next_deadline,
        Some(now + Duration::from_secs(350))
    );
    config.editor.cursor_shapes = false;
    pane.apply_config(config.clone());
    assert_eq!(
        pane.tick(now + Duration::from_secs(100)).next_deadline,
        Some(now + Duration::from_secs(350))
    );
    assert_eq!(
        pane.tick(now + Duration::from_secs(1000)).next_deadline,
        Some(now + Duration::from_secs(1300))
    );
    config.autosave.every_five_minutes = false;
    pane.apply_config(config.clone());
    assert_eq!(
        pane.tick(now + Duration::from_secs(1000)).next_deadline,
        None
    );
    config.autosave.every_five_minutes = true;
    pane.apply_config(config);
    assert_eq!(
        pane.tick(now + Duration::from_secs(1000)).next_deadline,
        Some(now + Duration::from_secs(1300))
    );
    assert!(!dir.path().join("config.toml").exists());
}

#[test]
fn manual_and_focus_saves_do_not_reset_the_periodic_cadence() {
    for manual in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let now = Instant::now();
        let path = write(dir.path(), "note.md");
        let mut pane = pane(
            dir.path(),
            vec![path],
            options(true, true),
            now,
            Box::new(AllowAllFileAccess),
        );
        insert(&mut pane, "first ", now);
        finish(&mut pane, now);
        pane.drain_events();
        let at = now + Duration::from_secs(100);
        if manual {
            ex(&mut pane, "w", at);
        } else {
            pane.tick(at);
            pane.set_focused(false);
            pane.set_focused(true);
        }
        assert_eq!(saved(&pane.drain_events()).len(), 1);
        insert(&mut pane, "second ", at);
        finish(&mut pane, at);
        pane.tick(now + Duration::from_secs(299));
        assert!(saved(&pane.drain_events()).is_empty());
        pane.tick(now + Duration::from_secs(300));
        assert_eq!(saved(&pane.drain_events()).len(), 1);
    }
}

#[test]
fn focus_preserves_select_cursor_and_undo_redo_and_does_not_save_on_tab_switch() {
    let dir = tempfile::tempdir().unwrap();
    let now = Instant::now();
    let path = write(dir.path(), "note.md");
    let second = write(dir.path(), "second.md");
    let mut pane = pane(
        dir.path(),
        vec![path.clone(), second],
        options(false, true),
        now,
        Box::new(AllowAllFileAccess),
    );
    let ids = pane.tabs().into_iter().map(|t| t.id).collect::<Vec<_>>();
    pane.focus_tab(&ids[0]).unwrap();
    insert(&mut pane, "changed ", now);
    finish(&mut pane, now);
    pane.focus_tab(&ids[1]).unwrap();
    pane.focus_tab(&ids[0]).unwrap();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "original\n");
    pane.render(80, 24, now);
    key(&mut pane, KeyCodeKind::Char('v'), now);
    key(&mut pane, KeyCodeKind::Char('l'), now);
    let cursor = pane.source_cursor(&ids[0]).unwrap();
    let frame = pane.render(80, 24, now);
    pane.set_focused(false);
    pane.set_focused(true);
    assert_eq!(pane.tabs()[0].mode, Mode::Select);
    assert_eq!(pane.source_cursor(&ids[0]).unwrap(), cursor);
    let after = pane.render(80, 24, now);
    // Selection styling and note rows remain byte-identical; the status notice changes.
    for index in 80..80 * 22 {
        assert_eq!(frame.cells[index], after.cells[index], "note cell {index}");
    }
    finish(&mut pane, now);
    key(&mut pane, KeyCodeKind::Char('u'), now);
    assert_eq!(pane.text(&ids[0]).unwrap(), "original\n");
    assert!(pane.tabs()[0].dirty);
    pane.handle_input(
        PaneInput::Key(KeyInput {
            code: KeyCode {
                kind: KeyCodeKind::Char('r'),
            },
            mods: Modifiers {
                ctrl: true,
                ..Modifiers::default()
            },
        }),
        now,
    );
    assert_eq!(pane.text(&ids[0]).unwrap(), "changed original\n");
    assert!(!pane.tabs()[0].dirty);
}

#[test]
fn named_new_files_are_created_and_serialization_policy_is_preserved() {
    for original in ["one\r\ntwo\r\n", "one\ntwo"] {
        let dir = tempfile::tempdir().unwrap();
        let now = Instant::now();
        let path = dir.path().join("note.md");
        std::fs::write(&path, original).unwrap();
        let new = dir.path().join("new.md");
        let mut pane = pane(
            dir.path(),
            vec![path.clone(), new.clone()],
            options(false, true),
            now,
            Box::new(AllowAllFileAccess),
        );
        let ids = pane.tabs().into_iter().map(|t| t.id).collect::<Vec<_>>();
        pane.focus_tab(&ids[0]).unwrap();
        insert(&mut pane, "prefix ", now);
        finish(&mut pane, now);
        pane.focus_tab(&ids[1]).unwrap();
        insert(&mut pane, "new", now);
        finish(&mut pane, now);
        pane.set_focused(false);
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            format!("prefix {original}")
        );
        assert_eq!(std::fs::read_to_string(&new).unwrap(), "new");
        assert!(pane.tabs().iter().all(|t| !t.dirty && !t.is_new));
    }
}

struct DenySave {
    denied: PathBuf,
    saves: Arc<AtomicUsize>,
}
impl FileAccessPolicy for DenySave {
    fn authorize(&self, op: FileOperation, path: &Path) -> Result<(), FilePolicyError> {
        if op == FileOperation::Save {
            self.saves.fetch_add(1, Ordering::SeqCst);
            if path == self.denied {
                return Err(FilePolicyError {
                    reason: "denied by host".into(),
                });
            }
        }
        Ok(())
    }
}
#[test]
fn conflicting_missing_denied_and_invalid_paths_keep_dirty_work_without_background_prompts() {
    for failure in ["external", "deleted", "denied", "directory"] {
        let dir = tempfile::tempdir().unwrap();
        let now = Instant::now();
        let bad = write(dir.path(), "bad.md");
        let good = write(dir.path(), "good.md");
        let writes = Arc::new(AtomicUsize::new(0));
        let mut pane = pane(
            dir.path(),
            vec![bad.clone(), good.clone()],
            options(false, true),
            now,
            Box::new(DenySave {
                denied: if failure == "denied" {
                    bad.clone()
                } else {
                    dir.path().join("other")
                },
                saves: writes.clone(),
            }),
        );
        let ids = pane.tabs().into_iter().map(|t| t.id).collect::<Vec<_>>();
        for id in &ids {
            pane.focus_tab(id).unwrap();
            insert(&mut pane, "changed ", now);
            finish(&mut pane, now);
        }
        match failure {
            "external" => std::fs::write(&bad, "external content\n").unwrap(),
            "deleted" => std::fs::remove_file(&bad).unwrap(),
            "directory" => {
                std::fs::remove_file(&bad).unwrap();
                std::fs::create_dir(&bad).unwrap();
            }
            _ => {}
        }
        pane.drain_events();
        pane.set_focused(false);
        assert!(!pane.input_state().modal);
        assert!(pane.tabs()[0].dirty);
        assert!(!pane.tabs()[1].dirty);
        assert_eq!(
            std::fs::read_to_string(&good).unwrap(),
            "changed original\n"
        );
        match failure {
            "external" => assert_eq!(std::fs::read_to_string(&bad).unwrap(), "external content\n"),
            "deleted" => assert!(!bad.exists()),
            "directory" => assert!(bad.is_dir()),
            _ => assert_eq!(std::fs::read_to_string(&bad).unwrap(), "original\n"),
        }
        let events = pane.drain_events();
        assert_eq!(saved(&events), vec![ids[1].clone()]);
        assert!(events
            .iter()
            .any(|e| matches!(e, PaneEvent::Failed{tab:Some(id),..} if id==&ids[0])));
        let frame = pane.render(120, 24, now);
        let text: String = frame.cells.iter().map(|c| c.symbol.as_str()).collect();
        assert!(text.contains("Autosave: 1 saved; 1 failed"), "{text}");
        assert!(text.contains("bad.md"), "{text}");
        let count = writes.load(Ordering::SeqCst);
        pane.tick(now + Duration::from_secs(1));
        pane.set_focused(false);
        assert_eq!(writes.load(Ordering::SeqCst), count);
    }
}

#[test]
fn exclusive_prompts_and_pending_grammars_skip_without_consuming_state_or_queuing_saves() {
    for pending in ["command", "search", "palette", "g", "space"] {
        let dir = tempfile::tempdir().unwrap();
        let now = Instant::now();
        let path = write(dir.path(), "note.md");
        let mut pane = pane(
            dir.path(),
            vec![path.clone()],
            options(true, true),
            now,
            Box::new(AllowAllFileAccess),
        );
        insert(&mut pane, "changed ", now);
        finish(&mut pane, now);
        pane.render(80, 24, now);
        let start = match pending {
            "command" => ':',
            "search" => '/',
            "palette" => '?',
            "g" => 'g',
            _ => ' ',
        };
        key(&mut pane, KeyCodeKind::Char(start), now);
        if pending == "palette" {
            finish(&mut pane, now);
            key(&mut pane, KeyCodeKind::Char(' '), now);
            key(&mut pane, KeyCodeKind::Char('h'), now);
        }
        let state = pane.input_state();
        if pending == "command" || pending == "search" || pending == "palette" {
            assert!(state.text_entry, "{pending} must actually own text input");
        }
        pane.tick(now + Duration::from_secs(300));
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "original\n",
            "pending {pending}"
        );
        pane.set_focused(false);
        assert_eq!(pane.input_state(), state);
        assert!(pane.tabs()[0].dirty);
        pane.set_focused(true);
        finish(&mut pane, now + Duration::from_secs(300));
        pane.tick(now + Duration::from_secs(301));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "original\n");
        pane.tick(now + Duration::from_secs(600));
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "changed original\n"
        );
    }
}

#[test]
fn lifecycle_tokens_and_manual_confirmations_block_automatic_io() {
    for pending in ["external", "close", "retarget", "confirmation"] {
        let dir = tempfile::tempdir().unwrap();
        let now = Instant::now();
        let path = write(dir.path(), "note.md");
        let mut pane = pane(
            dir.path(),
            vec![path.clone()],
            options(true, true),
            now,
            Box::new(AllowAllFileAccess),
        );
        let id = pane.active_tab().unwrap();
        insert(&mut pane, "changed ", now);
        finish(&mut pane, now);
        match pending {
            "external" => {
                let token = pane.begin_external_change().unwrap();
                pane.tick(now + Duration::from_secs(300));
                pane.set_focused(false);
                assert_eq!(std::fs::read_to_string(&path).unwrap(), "original\n");
                pane.abort_external_change(token).unwrap();
            }
            "close" => {
                let request = pane.prepare_close(std::slice::from_ref(&id)).unwrap();
                pane.tick(now + Duration::from_secs(300));
                pane.set_focused(false);
                assert_eq!(std::fs::read_to_string(&path).unwrap(), "original\n");
                pane.cancel_request(&request).unwrap();
            }
            "retarget" => {
                let token = pane
                    .prepare_retarget(&[Retarget {
                        tab: id.clone(),
                        path: dir.path().join("new.md"),
                    }])
                    .unwrap();
                pane.tick(now + Duration::from_secs(300));
                pane.set_focused(false);
                assert_eq!(std::fs::read_to_string(&path).unwrap(), "original\n");
                pane.abort_retarget(token).unwrap();
            }
            _ => {
                std::fs::write(&path, "external\n").unwrap();
                ex(&mut pane, "w", now);
                assert!(pane.input_state().modal);
                pane.tick(now + Duration::from_secs(300));
                pane.set_focused(false);
                assert!(pane.input_state().modal);
                assert_eq!(std::fs::read_to_string(&path).unwrap(), "external\n");
            }
        }
        assert_eq!(pane.text(&id).unwrap(), "changed original\n");
        assert!(pane.tabs()[0].dirty);
        pane.tick(now + Duration::from_secs(301));
        assert!(saved(&pane.drain_events()).is_empty());
    }
}

#[test]
fn live_focus_setting_can_be_enabled_disabled_and_reenabled_without_saving_immediately() {
    let dir = tempfile::tempdir().unwrap();
    let now = Instant::now();
    let path = write(dir.path(), "note.md");
    let mut pane = pane(
        dir.path(),
        vec![path.clone()],
        options(false, false),
        now,
        Box::new(AllowAllFileAccess),
    );
    insert(&mut pane, "first ", now);
    finish(&mut pane, now);
    let mut config = Config {
        autosave: options(false, true),
        ..Config::default()
    };
    config.spell.enabled = false;
    pane.drain_events();
    let report = pane.apply_config(config.clone());
    assert_eq!(report.applied_now, [ConfigField::AutosaveOnFocusLoss]);
    assert!(saved(&pane.drain_events()).is_empty());
    pane.set_focused(false);
    assert_eq!(saved(&pane.drain_events()).len(), 1);
    pane.set_focused(true);
    insert(&mut pane, "second ", now);
    finish(&mut pane, now);
    config.autosave.on_focus_loss = false;
    pane.apply_config(config.clone());
    pane.set_focused(false);
    assert!(saved(&pane.drain_events()).is_empty());
    assert!(pane.tabs()[0].dirty);
    let previous = std::fs::read_to_string(&path).unwrap();
    config.autosave.on_focus_loss = true;
    pane.apply_config(config);
    pane.set_focused(false);
    assert!(saved(&pane.drain_events()).is_empty());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), previous);
    pane.set_focused(true);
    pane.set_focused(false);
    assert_eq!(saved(&pane.drain_events()).len(), 1);
    assert!(!pane.tabs()[0].dirty);
}

#[test]
fn an_uncreated_named_path_that_appears_before_autosave_is_not_overwritten() {
    let dir = tempfile::tempdir().unwrap();
    let now = Instant::now();
    let path = dir.path().join("new.md");
    let mut pane = pane(
        dir.path(),
        vec![path.clone()],
        options(false, true),
        now,
        Box::new(AllowAllFileAccess),
    );
    let id = pane.active_tab().unwrap();
    insert(&mut pane, "mine", now);
    std::fs::write(&path, "someone else's new file").unwrap();
    pane.drain_events();
    pane.set_focused(false);
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "someone else's new file"
    );
    assert_eq!(pane.text(&id).unwrap(), "mine");
    assert!(pane.tabs()[0].dirty);
    assert!(!pane.input_state().modal);
    let events = pane.drain_events();
    assert!(saved(&events).is_empty());
    assert!(events
        .iter()
        .any(|e| matches!(e,PaneEvent::Failed{tab:Some(tab),..} if tab==&id)));
}
