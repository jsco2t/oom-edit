//! External-consumer contract for the curated headless pane facade.

use std::path::{Path, PathBuf};
use std::time::Instant;

use oom_edit::{
    AllowAllFileAccess, ClipboardSink, CommandPolicy, Config, ConfigPersistError, DisplayMode,
    EditorPane, FileAccessPolicy, FileOperation, FilePolicyError, InputDisposition, KeyCode,
    KeyCodeKind, KeyInput, Mode, Modifiers, OpenCursor, OpenOptions, OpenOutcome, PaneConstruction,
    PaneErrorKind, PaneEvent, PaneInit, PaneInput, PaneMouse, PaneMouseKind, PaneOptions,
    PaneServices, ThemeCatalog, ThemePersistenceSink, ThemeSelection, ThemeSlot, Tier,
};

struct SilentClipboard;

impl ClipboardSink for SilentClipboard {
    fn copy(&mut self, _text: &str) -> Result<(), oom_edit::ClipboardError> {
        Ok(())
    }
}

struct SilentThemeSink;

impl ThemePersistenceSink for SilentThemeSink {
    fn persist_theme(&mut self, _slot: ThemeSlot, _name: &str) -> Result<(), ConfigPersistError> {
        Ok(())
    }
}

fn construct(base: &Path, initial_paths: Vec<PathBuf>) -> PaneConstruction {
    construct_with_policy(base, initial_paths, Box::new(AllowAllFileAccess))
}

fn construct_with_policy(
    base: &Path,
    initial_paths: Vec<PathBuf>,
    policy: Box<dyn FileAccessPolicy>,
) -> PaneConstruction {
    construct_with_options(
        base,
        initial_paths,
        policy,
        PaneOptions {
            command_policy: CommandPolicy::Embedded,
            inline_hints: false,
            always_tab_bar: true,
            empty_state_lines: vec!["Choose a note".to_string()],
        },
    )
}

fn construct_with_options(
    base: &Path,
    initial_paths: Vec<PathBuf>,
    policy: Box<dyn FileAccessPolicy>,
    options: PaneOptions,
) -> PaneConstruction {
    let services = PaneServices {
        clipboard_sink: Box::new(SilentClipboard),
        theme_sink: Box::new(SilentThemeSink),
        file_access_policy: policy,
        config_base_directory: base.to_path_buf(),
        personal_dictionary_path: base.join("personal.txt"),
        working_directory: base.to_path_buf(),
    };
    EditorPane::construct(PaneInit {
        config: Config::default(),
        theme_catalog: ThemeCatalog::builtins(),
        theme_selection: ThemeSelection::new(None, DisplayMode::Dark, Tier::Color16),
        services,
        options,
        initial_paths,
        now: Instant::now(),
    })
}

fn ex(pane: &mut EditorPane, command: &str, now: Instant) {
    pane.handle_input(PaneInput::Key(key(':')), now);
    for ch in command.chars() {
        pane.handle_input(PaneInput::Key(key(ch)), now);
    }
    pane.handle_input(PaneInput::Key(special(KeyCodeKind::Enter)), now);
}

fn special(kind: KeyCodeKind) -> KeyInput {
    KeyInput {
        code: KeyCode { kind },
        mods: Modifiers::default(),
    }
}

fn key(ch: char) -> KeyInput {
    KeyInput {
        code: KeyCode {
            kind: KeyCodeKind::Char(ch),
        },
        mods: Modifiers::default(),
    }
}

#[test]
fn embedded_pane_characterwise_delete_removes_the_complete_source_line() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("note.md");
    std::fs::write(&path, "# café\n# next\n").unwrap();
    let mut pane = construct(directory.path(), vec![path]).pane;
    let tab = pane.active_tab().unwrap();
    let now = Instant::now();
    pane.render(80, 24, now);
    for ch in ['v', '$', 'd'] {
        pane.handle_input(PaneInput::Key(key(ch)), now);
    }
    assert_eq!(pane.text(&tab).unwrap(), "# next\n");
    let frame = pane.render(80, 24, now);
    let visible = frame
        .cells
        .iter()
        .map(|cell| cell.symbol.as_str())
        .collect::<String>();
    assert!(visible.contains("next"));
    assert!(!visible.contains("café"));
}

#[test]
fn host_dictionary_roots_load_and_persist_only_the_supplied_personal_path() {
    let directory = tempfile::tempdir().unwrap();
    let config_base = directory.path().join("host-config");
    let document_base = directory.path().join("vault");
    let personal_path = directory.path().join("host-data/personal.words");
    std::fs::create_dir(&config_base).unwrap();
    std::fs::create_dir(&document_base).unwrap();
    std::fs::create_dir(personal_path.parent().unwrap()).unwrap();
    std::fs::write(config_base.join("team.words"), "qzxextra\n").unwrap();
    std::fs::write(&personal_path, "qzxpersonal\n").unwrap();
    let path = document_base.join("note.md");
    std::fs::write(&path, "qzxhostword qzxextra qzxpersonal\n").unwrap();
    let mut config = Config::default();
    config.spell.additional_dictionaries = vec!["team.words".into()];
    let now = Instant::now();
    let construction = EditorPane::construct(PaneInit {
        config,
        theme_catalog: ThemeCatalog::builtins(),
        theme_selection: ThemeSelection::new(
            Some("accessible".into()),
            DisplayMode::Dark,
            Tier::Monochrome,
        ),
        services: PaneServices {
            clipboard_sink: Box::new(SilentClipboard),
            theme_sink: Box::new(SilentThemeSink),
            file_access_policy: Box::new(AllowAllFileAccess),
            config_base_directory: config_base.clone(),
            personal_dictionary_path: personal_path.clone(),
            working_directory: document_base.clone(),
        },
        options: PaneOptions::default(),
        initial_paths: vec![path.clone()],
        now,
    });
    assert!(construction.report.warnings.is_empty());
    let mut pane = construction.pane;
    let tab = pane.active_tab().unwrap();
    pane.render(80, 24, now);
    let drain = |pane: &mut EditorPane| {
        for _ in 0..10_000 {
            if !pane.idle_unit(4096).worked {
                return;
            }
        }
        panic!("bounded idle work must become quiescent");
    };
    drain(&mut pane);
    assert_eq!(pane.status().unwrap().spell.unwrap().count, 1);
    let diagnostic = pane.render(80, 24, now);
    assert!(diagnostic
        .cells
        .iter()
        .any(|cell| cell.symbol == "•" && cell.style.modifiers.bold));
    assert!(diagnostic.cells.iter().all(|cell| [
        cell.style.foreground,
        cell.style.background,
        cell.style.underline_color
    ]
    .iter()
    .all(|color| matches!(color, None | Some(oom_edit::ColorValue::Default)))));
    let saved_dictionary = personal_path.with_extension("saved");
    std::fs::rename(&personal_path, &saved_dictionary).unwrap();
    std::fs::create_dir(&personal_path).unwrap();
    pane.handle_input(PaneInput::Key(key(' ')), now);
    pane.handle_input(PaneInput::Key(key('a')), now);
    let failed = pane.render(80, 24, now);
    let visible = failed
        .cells
        .iter()
        .map(|cell| cell.symbol.as_str())
        .collect::<String>();
    assert!(visible.contains("failed to add word:"));
    assert_eq!(
        std::fs::read_to_string(&saved_dictionary).unwrap(),
        "qzxpersonal\n"
    );
    assert_eq!(pane.status().unwrap().spell.unwrap().count, 1);
    std::fs::remove_dir(&personal_path).unwrap();
    std::fs::rename(&saved_dictionary, &personal_path).unwrap();
    pane.handle_input(PaneInput::Key(key(' ')), now);
    pane.handle_input(PaneInput::Key(key('a')), now);
    assert_eq!(
        std::fs::read_to_string(&personal_path).unwrap(),
        "qzxhostword\nqzxpersonal\n"
    );
    assert_eq!(
        std::fs::read_to_string(config_base.join("team.words")).unwrap(),
        "qzxextra\n"
    );
    assert!(!config_base.join("dictionary.txt").exists());
    assert!(!document_base.join("dictionary.txt").exists());
    assert_eq!(
        pane.text(&tab).unwrap(),
        "qzxhostword qzxextra qzxpersonal\n"
    );
    drain(&mut pane);
    assert_eq!(pane.status().unwrap().spell.unwrap().count, 0);
    assert_eq!(
        std::fs::read_to_string(path).unwrap(),
        pane.text(&tab).unwrap()
    );
}

#[test]
fn public_tab_bar_empty_state_cursor_and_compact_layout_matrix() {
    let directory = tempfile::tempdir().unwrap();
    let first_path = directory.path().join("first.md");
    let second_path = directory.path().join("second.md");
    std::fs::write(&first_path, "# Heading\n\n界e\u{301}🙂\n").unwrap();
    std::fs::write(&second_path, "second\n").unwrap();
    let now = Instant::now();
    for always_tab_bar in [false, true] {
        let mut pane = construct_with_options(
            directory.path(),
            vec![],
            Box::new(AllowAllFileAccess),
            PaneOptions {
                always_tab_bar,
                inline_hints: false,
                empty_state_lines: vec!["Choose a note".into(), "Or create one".into()],
                ..PaneOptions::default()
            },
        )
        .pane;
        let empty = pane.render(80, 24, now);
        let row_text = |frame: &oom_edit::PaneFrame, row| {
            (0..frame.width)
                .map(|column| frame.cell(column, row).unwrap().symbol.as_str())
                .collect::<String>()
        };
        let body_row = u16::from(always_tab_bar);
        assert!(row_text(&empty, body_row).starts_with("Choose a note"));
        assert!(row_text(&empty, body_row + 1).starts_with("Or create one"));
        assert!(empty.cursor.is_none());
        assert!(pane.status().is_none());
        let OpenOutcome::Opened(first) = pane
            .open_existing(&first_path, OpenOptions::default())
            .unwrap()
        else {
            panic!("first path must create a tab");
        };
        assert_eq!(pane.render(80, 24, now).cursor.unwrap().row, body_row);
        let OpenOutcome::Opened(second) = pane
            .open_existing(&second_path, OpenOptions::default())
            .unwrap()
        else {
            panic!("second path must create a tab");
        };
        pane.focus_tab(&first).unwrap();
        assert_eq!(pane.render(80, 24, now).cursor.unwrap().row, 1);
        pane.close(&second).unwrap();
        assert_eq!(pane.tabs()[0].id, first);
        assert_eq!(pane.render(80, 24, now).cursor.unwrap().row, body_row);
        for (mode_key, expected_mode, expected_shape) in [
            (None, Mode::Normal, oom_edit::PaneCursorShape::Block),
            (Some('i'), Mode::Insert, oom_edit::PaneCursorShape::Bar),
            (
                Some('v'),
                Mode::Select,
                oom_edit::PaneCursorShape::Underscore,
            ),
            (Some(':'), Mode::Command, oom_edit::PaneCursorShape::Bar),
        ] {
            pane.handle_input(PaneInput::Key(special(KeyCodeKind::Esc)), now);
            if let Some(ch) = mode_key {
                pane.handle_input(PaneInput::Key(key(ch)), now);
            }
            assert_eq!(pane.status().unwrap().mode, expected_mode);
            for (width, height) in [
                (0, 0),
                (0, 5),
                (20, 0),
                (19, 4),
                (20, 5),
                (80, 24),
                (200, 60),
            ] {
                let frame = pane.render(width, height, now);
                assert_eq!(frame.cells.len(), usize::from(width) * usize::from(height));
                if let Some(cursor) = frame.cursor {
                    assert!(cursor.column < width && cursor.row < height);
                    assert_eq!(cursor.shape, expected_shape);
                }
                if width >= 20 && height >= 5 {
                    assert!(frame.cursor.is_some());
                }
                assert!(!pane.render(width, height, now).changed);
                pane.set_focused(false);
                assert!(pane.render(width, height, now).cursor.is_none());
                pane.set_focused(true);
            }
        }
        let mut config = Config::default();
        config.editor.cursor_shapes = false;
        pane.apply_config(config);
        assert_eq!(
            pane.render(80, 24, now).cursor.unwrap().shape,
            oom_edit::PaneCursorShape::Block
        );
        pane.handle_input(PaneInput::Key(special(KeyCodeKind::Esc)), now);
        assert_eq!(pane.text(&first).unwrap(), "# Heading\n\n界e\u{301}🙂\n");
        pane.close(&first).unwrap();
        assert!(pane.tabs().is_empty());
        assert!(row_text(&pane.render(80, 24, now), body_row).starts_with("Choose a note"));
    }
}

#[test]
fn cached_presentation_tracks_time_tab_focus_and_lifecycle_changes() {
    let directory = tempfile::tempdir().unwrap();
    let mut pane = construct_with_options(
        directory.path(),
        vec![],
        Box::new(AllowAllFileAccess),
        PaneOptions {
            inline_hints: true,
            always_tab_bar: true,
            ..PaneOptions::default()
        },
    )
    .pane;
    let first = pane.new_buffer(OpenOptions::default()).unwrap();
    let now = Instant::now();
    pane.render(80, 24, now);
    pane.handle_input(PaneInput::Key(key(' ')), now);
    let early = pane.render(80, 24, now);
    assert!(
        !pane
            .render(80, 24, now + std::time::Duration::from_millis(149))
            .changed
    );
    let visible = pane.render(80, 24, now + std::time::Duration::from_millis(150));
    assert!(
        visible.changed,
        "render observes the which-key boundary without requiring tick"
    );
    assert!(!visible.visually_equals(&early));
    assert!(
        !pane
            .render(80, 24, now + std::time::Duration::from_millis(151))
            .changed
    );
    pane.handle_input(PaneInput::Key(special(KeyCodeKind::Esc)), now);
    pane.handle_input(PaneInput::Key(key('i')), now);
    pane.handle_input(PaneInput::Paste("First".into()), now);
    pane.handle_input(PaneInput::Key(special(KeyCodeKind::Esc)), now);
    let before_open = pane.render(80, 24, now);
    let second = pane.new_buffer(OpenOptions::default()).unwrap();
    assert!(!pane.render(80, 24, now).visually_equals(&before_open));
    pane.focus_tab(&first).unwrap();
    let focused = pane.render(80, 24, now);
    assert!(focused
        .cells
        .iter()
        .map(|cell| cell.symbol.as_str())
        .collect::<String>()
        .contains("First"));
    pane.focus_tab(&second).unwrap();
    assert!(!pane.render(80, 24, now).visually_equals(&focused));
    pane.close(&second).unwrap();
    assert_eq!(pane.active_tab(), Some(first));
    assert!(pane.render(80, 24, now).visually_equals(&before_open));
}

#[test]
fn coalesced_resize_preserves_host_requested_tab_bar_geometry() {
    let directory = tempfile::tempdir().unwrap();
    let mut pane = construct(directory.path(), vec![]).pane;
    let tab = pane.new_buffer(OpenOptions::default()).unwrap();
    let now = Instant::now();
    pane.handle_input(PaneInput::Key(key('i')), now);
    pane.handle_input(PaneInput::Paste("one\ntwo\nthree".into()), now);
    pane.render(80, 24, now);
    let cursor = pane.source_cursor(&tab).unwrap();
    pane.resize(40, 10, now);
    pane.handle_input(
        PaneInput::Mouse(PaneMouse {
            kind: PaneMouseKind::LeftDown,
            column: 5,
            row: 0,
            modifiers: Modifiers::default(),
        }),
        now,
    );
    assert_eq!(
        pane.source_cursor(&tab).unwrap(),
        cursor,
        "tab bar input cannot move the document cursor before painting"
    );
}

#[test]
fn owned_frames_share_unchanged_cells_without_mutating_prior_snapshots() {
    let directory = tempfile::tempdir().unwrap();
    let mut pane = construct(directory.path(), vec![]).pane;
    pane.new_buffer(OpenOptions::default()).unwrap();
    let now = Instant::now();
    pane.handle_input(PaneInput::Key(key('i')), now);
    pane.handle_input(PaneInput::Paste("界 e\u{301} 👩\u{200d}💻".into()), now);
    let first = pane.render(100, 30, now);
    let saved = first.clone();
    let repeated = pane.render(100, 30, now);
    assert!(!repeated.changed);
    assert!(std::sync::Arc::ptr_eq(&first.cells, &repeated.cells));
    pane.handle_input(PaneInput::Key(key('X')), now);
    let edited = pane.render(100, 30, now);
    assert!(edited.changed);
    assert!(!std::sync::Arc::ptr_eq(&first.cells, &edited.cells));
    assert_eq!(
        first, saved,
        "returned snapshots remain independent of edits"
    );
    assert!(!edited.visually_equals(&first));
    pane.set_focused(false);
    let unfocused = pane.render(100, 30, now);
    assert!(unfocused.changed);
    assert!(unfocused.cursor.is_none());
    let tiny = pane.render(19, 4, now);
    assert_eq!(tiny.cells.len(), 76);
    let restored = pane.render(100, 30, now);
    assert!(restored.changed);
    assert!(restored.visually_equals(&unfocused));
    assert!(!pane.render(100, 30, now).changed);
    assert_eq!(first, saved);
}

#[test]
fn live_theme_cycle_has_owned_change_notification() {
    let directory = tempfile::tempdir().unwrap();
    let mut pane = construct(directory.path(), vec![]).pane;
    pane.new_buffer(OpenOptions::default()).unwrap();
    pane.drain_events();
    let now = Instant::now();
    let before = pane.render(80, 24, now);
    pane.handle_input(PaneInput::Key(key(' ')), now);
    pane.handle_input(PaneInput::Key(key('t')), now);
    let events = pane.drain_events();
    assert_eq!(
        events.len(),
        1,
        "one successful theme change needs one notification: {events:?}"
    );
    assert!(
        matches!(&events[0], PaneEvent::ThemeChanged { theme } if theme.name != "default-dark")
    );
    assert!(!pane.render(80, 24, now).visually_equals(&before));
}

#[test]
fn live_theme_and_config_apply_through_public_facade_without_persistence() {
    use oom_edit::{ConfigField, PaneCursorShape};
    use std::cell::Cell;
    use std::rc::Rc;
    struct CountSink(Rc<Cell<usize>>);
    impl ThemePersistenceSink for CountSink {
        fn persist_theme(&mut self, _: ThemeSlot, _: &str) -> Result<(), ConfigPersistError> {
            self.0.set(self.0.get() + 1);
            Ok(())
        }
    }
    let directory = tempfile::tempdir().unwrap();
    let writes = Rc::new(Cell::new(0));
    let now = Instant::now();
    let mut pane = EditorPane::construct(PaneInit {
        config: Config::default(),
        theme_catalog: ThemeCatalog::builtins(),
        theme_selection: ThemeSelection::new(None, DisplayMode::Dark, Tier::Color16),
        services: PaneServices {
            clipboard_sink: Box::new(SilentClipboard),
            theme_sink: Box::new(CountSink(Rc::clone(&writes))),
            file_access_policy: Box::new(AllowAllFileAccess),
            config_base_directory: directory.path().into(),
            personal_dictionary_path: directory.path().join("personal.txt"),
            working_directory: directory.path().into(),
        },
        options: PaneOptions::default(),
        initial_paths: vec![],
        now,
    })
    .pane;
    let tab = pane.new_buffer(OpenOptions::default()).unwrap();
    pane.drain_events();
    let dark = pane.render(80, 24, now);
    pane.set_theme("nord").unwrap();
    assert_eq!(writes.get(), 0);
    assert!(
        matches!(pane.drain_events().as_slice(), [PaneEvent::ThemeChanged { theme }] if theme.name == "nord")
    );
    let nord = pane.render(80, 24, now);
    assert!(!nord.visually_equals(&dark));
    pane.set_theme("nord").unwrap();
    assert!(pane.drain_events().is_empty());
    assert_eq!(
        pane.set_theme("does-not-exist").unwrap_err().kind,
        PaneErrorKind::InvalidTheme
    );
    assert!(pane.drain_events().is_empty());
    assert!(nord.visually_equals(&pane.render(80, 24, now)));

    let mut config = Config::default();
    config.theme.mode = Some("light".into());
    config.editor.wrap = false;
    config.editor.cursor_shapes = false;
    config.relative_line_numbers = true;
    config.clipboard.copy_format = oom_edit::ClipboardCopyFormat::PlainText;
    config.spell.enabled = false;
    config.editor.wrap_width = 72;
    config.spell.language = "en_CA".into();
    config.spell.additional_dictionaries = vec![directory.path().join("not-loaded.txt")];
    let report = pane.apply_config(config.clone());
    assert_eq!(
        report.applied_now,
        [
            ConfigField::ThemeMode,
            ConfigField::Wrap,
            ConfigField::RelativeLineNumbers,
            ConfigField::CursorShapes,
            ConfigField::ClipboardCopyFormat,
            ConfigField::SpellEnabled
        ]
    );
    assert_eq!(
        report.requires_new_pane,
        [
            ConfigField::WrapWidth,
            ConfigField::SpellLanguage,
            ConfigField::AdditionalDictionaries
        ]
    );
    assert!(
        matches!(pane.drain_events().as_slice(), [PaneEvent::ThemeChanged { theme }] if theme.name == "default-light")
    );
    pane.handle_input(PaneInput::Key(key('i')), now);
    assert_eq!(
        pane.render(80, 24, now).cursor.unwrap().shape,
        PaneCursorShape::Block
    );
    assert_eq!(pane.text(&tab).unwrap(), "");
    assert!(!directory.path().join("not-loaded.txt").exists());
    pane.drain_events();
    let repeat = pane.apply_config(config);
    assert!(repeat.applied_now.is_empty());
    assert_eq!(repeat.requires_new_pane, report.requires_new_pane);
    assert!(pane.drain_events().is_empty());
    assert_eq!(writes.get(), 0);
    assert_eq!(pane.tabs()[0].mode, Mode::Insert);
    assert_eq!(pane.text(&tab).unwrap(), "");
}

#[test]
fn zero_tab_host_renders_and_old_tab_ids_never_address_a_recreated_pane() {
    let dir = tempfile::tempdir().unwrap();
    let PaneConstruction { mut pane, report } = construct(dir.path(), vec![]);
    assert!(report.paths.is_empty());
    assert!(pane.tabs().is_empty());
    let frame = pane.render(20, 5, Instant::now());
    assert!(frame.cursor.is_none());
    assert_eq!(frame.cell(0, 1).unwrap().symbol, "C");

    let path = dir.path().join("note.md");
    std::fs::write(&path, "hello\n").unwrap();
    let old_id = match pane.open(&path, OpenOptions::default()).unwrap() {
        OpenOutcome::Opened(id) => id,
        other => panic!("unexpected open result: {other:?}"),
    };
    drop(pane);
    let mut recreated = construct(dir.path(), vec![path]).pane;
    assert_ne!(recreated.tabs()[0].id, old_id);
    assert!(recreated.focus_tab(&old_id).is_err());
}

#[test]
fn initial_paths_report_each_success_and_failure_without_discarding_open_tabs() {
    let dir = tempfile::tempdir_in(std::env::temp_dir().canonicalize().unwrap()).unwrap();
    let good = dir.path().join("good.md");
    let invalid = dir.path().join("invalid.md");
    let later = dir.path().join("later.md");
    std::fs::write(&good, "good\n").unwrap();
    std::fs::write(&invalid, [0xff]).unwrap();
    std::fs::write(&later, "Café\n").unwrap();
    let result = construct(
        dir.path(),
        vec![good.clone(), invalid.clone(), later.clone()],
    );
    assert_eq!(result.report.paths.len(), 3);
    assert!(result.report.paths[0].is_ok());
    let error = result.report.paths[1].as_ref().unwrap_err();
    assert_eq!(error.kind, PaneErrorKind::NotUtf8);
    assert_eq!(error.path.as_ref(), Some(&invalid));
    assert!(!error.detail.is_empty());
    assert!(result.report.paths[2].is_ok());
    let tabs = result.pane.tabs();
    assert_eq!(tabs.len(), 2);
    assert_eq!(tabs[0].path.as_ref(), Some(&good));
    assert_eq!(tabs[1].path.as_ref(), Some(&later));
    assert_eq!(result.pane.text(&tabs[0].id).unwrap(), "good\n");
    assert_eq!(result.pane.text(&tabs[1].id).unwrap(), "Café\n");
}

#[test]
fn core_keys_and_paste_are_owned_input_with_focus_gating() {
    let dir = tempfile::tempdir().unwrap();
    let mut pane = construct(dir.path(), vec![]).pane;
    let path = dir.path().join("new.md");
    pane.open(&path, OpenOptions::default()).unwrap();
    let now = Instant::now();
    assert_eq!(
        pane.handle_input(PaneInput::Key(key('i')), now),
        InputDisposition::Consumed
    );
    assert_eq!(
        pane.handle_input(PaneInput::Paste("hello".to_string()), now),
        InputDisposition::Consumed
    );
    let id = pane.active_tab().unwrap();
    assert_eq!(pane.text(&id).unwrap(), "hello");
    pane.set_focused(false);
    assert_eq!(
        pane.handle_input(PaneInput::Key(key('x')), now),
        InputDisposition::NotConsumed
    );
    assert!(pane.render(80, 24, now).cursor.is_none());
    pane.set_focused(true);
    assert_eq!(pane.text(&id).unwrap(), "hello");
}

#[test]
fn open_focus_and_mru_events_are_ordered_and_idempotent() {
    let dir = tempfile::tempdir().unwrap();
    let first = dir.path().join("first.md");
    let second = dir.path().join("second.md");
    std::fs::write(&first, "first\n").unwrap();
    std::fs::write(&second, "second\n").unwrap();
    let mut pane = construct(dir.path(), vec![first.clone()]).pane;
    let initial_events = pane.drain_events();
    assert!(matches!(
        initial_events.as_slice(),
        [
            PaneEvent::Opened { .. },
            PaneEvent::ActiveTabChanged { .. },
            PaneEvent::Completed { .. }
        ]
    ));
    let first_id = pane.active_tab().unwrap();
    let second_id = match pane.open(&second, OpenOptions::default()).unwrap() {
        OpenOutcome::Opened(id) => id,
        other => panic!("unexpected open result: {other:?}"),
    };
    let events = pane.drain_events();
    assert!(matches!(
        events.as_slice(),
        [
            PaneEvent::Opened { .. },
            PaneEvent::ActiveTabChanged { .. },
            PaneEvent::Completed { .. }
        ]
    ));
    assert_eq!(pane.tabs()[0].mru_rank, 1);
    assert_eq!(pane.tabs()[1].mru_rank, 0);

    assert_eq!(
        pane.open(&first, OpenOptions::default()).unwrap(),
        OpenOutcome::FocusedExisting(first_id.clone())
    );
    assert!(matches!(
        &pane.drain_events()[..],
        [
            PaneEvent::ActiveTabChanged { .. },
            PaneEvent::Completed { .. }
        ]
    ));
    assert_eq!(pane.tabs()[0].mru_rank, 0);
    pane.focus_tab(&first_id).unwrap();
    assert!(pane.drain_events().is_empty());
    assert_eq!(pane.tabs()[0].mru_rank, 0);
    assert_eq!(pane.tabs()[1].id, second_id);
    pane.focus_tab(&second_id).unwrap();
    let events = pane.drain_events();
    assert!(
        matches!(events.as_slice(), [PaneEvent::ActiveTabChanged { request, tab }, PaneEvent::Completed { request: complete }] if request == complete && tab == &second_id)
    );
}

#[test]
fn open_existing_rejects_missing_even_if_the_tab_was_previously_open() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("vanished.md");
    std::fs::write(&path, "saved\n").unwrap();
    let mut pane = construct(dir.path(), vec![path.clone()]).pane;
    std::fs::remove_file(&path).unwrap();
    let error = pane
        .open_existing(&path, OpenOptions::default())
        .unwrap_err();
    assert_eq!(error.kind, PaneErrorKind::Missing);
    assert_eq!(pane.tabs().len(), 1);
}

#[test]
fn open_cursor_options_only_apply_to_new_tabs() {
    let dir = tempfile::tempdir().unwrap();
    let first = dir.path().join("first.md");
    let second = dir.path().join("second.md");
    std::fs::write(&first, "---\ntitle: Test\n---\nbody\nsecond\n").unwrap();
    std::fs::write(&second, "other\n").unwrap();
    let mut pane = construct(dir.path(), vec![]).pane;
    let first_id = match pane
        .open(
            &first,
            OpenOptions {
                cursor: OpenCursor::AfterFrontMatter,
                enter_insert: false,
            },
        )
        .unwrap()
    {
        OpenOutcome::Opened(id) => id,
        other => panic!("unexpected open result: {other:?}"),
    };
    assert_eq!(pane.source_cursor(&first_id).unwrap(), (3, 0));
    pane.open(&second, OpenOptions::default()).unwrap();
    pane.focus_tab(&first_id).unwrap();
    let outcome = pane
        .open(
            &first,
            OpenOptions {
                cursor: OpenCursor::SourceLine(4),
                enter_insert: true,
            },
        )
        .unwrap();
    assert_eq!(outcome, OpenOutcome::FocusedExisting(first_id.clone()));
    assert_eq!(pane.source_cursor(&first_id).unwrap(), (3, 0));
    assert_eq!(pane.tabs()[0].mode, Mode::Normal);
}

#[test]
fn focus_loss_suspends_pending_g_but_switching_tabs_clears_it() {
    let dir = tempfile::tempdir().unwrap();
    let first = dir.path().join("first.md");
    let second = dir.path().join("second.md");
    std::fs::write(&first, "one\ntwo\nthree\n").unwrap();
    std::fs::write(&second, "other\n").unwrap();
    let mut pane = construct(dir.path(), vec![]).pane;
    let first_id = match pane
        .open(
            &first,
            OpenOptions {
                cursor: OpenCursor::SourceLine(2),
                enter_insert: false,
            },
        )
        .unwrap()
    {
        OpenOutcome::Opened(id) => id,
        other => panic!("unexpected open result: {other:?}"),
    };
    let now = Instant::now();
    pane.render(80, 24, now);
    assert_eq!(pane.source_cursor(&first_id).unwrap(), (2, 0));
    pane.handle_input(PaneInput::Key(key('g')), now);
    pane.set_focused(false);
    pane.set_focused(true);
    pane.handle_input(PaneInput::Key(key('g')), now);
    assert_eq!(pane.source_cursor(&first_id).unwrap(), (0, 0));

    pane.handle_input(PaneInput::Key(key('j')), now);
    assert_eq!(pane.source_cursor(&first_id).unwrap(), (1, 0));
    pane.handle_input(PaneInput::Key(key('g')), now);
    pane.open(&second, OpenOptions::default()).unwrap();
    pane.focus_tab(&first_id).unwrap();
    pane.handle_input(PaneInput::Key(key('g')), now);
    assert_eq!(pane.source_cursor(&first_id).unwrap(), (1, 0));
    pane.handle_input(PaneInput::Key(key('g')), now);
    assert_eq!(pane.source_cursor(&first_id).unwrap(), (0, 0));
    pane.handle_input(PaneInput::Key(special(KeyCodeKind::Esc)), now);
}

struct DenyAll;

impl FileAccessPolicy for DenyAll {
    fn authorize(&self, _operation: FileOperation, _path: &Path) -> Result<(), FilePolicyError> {
        Err(FilePolicyError {
            reason: "host denied this path".to_string(),
        })
    }
}

#[test]
fn policy_denial_precedes_initial_and_later_file_reads() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("invalid.md");
    std::fs::write(&path, [0xff]).unwrap();
    let mut result = construct_with_policy(dir.path(), vec![path.clone()], Box::new(DenyAll));
    assert_eq!(
        result.report.paths[0].as_ref().unwrap_err().kind,
        PaneErrorKind::Denied
    );
    assert!(result.pane.tabs().is_empty());
    assert_eq!(
        result
            .pane
            .open(&path, OpenOptions::default())
            .unwrap_err()
            .kind,
        PaneErrorKind::Denied
    );
    let events = result.pane.drain_events();
    assert_eq!(events.len(), 2);
    assert!(events.iter().all(|event| matches!(event, PaneEvent::Failed { error, .. } if error.kind == PaneErrorKind::Denied)));
}

#[test]
fn tick_exposes_host_idle_deadline_and_mouse_uses_local_cells() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("long.md");
    let text = (0..50)
        .map(|row| format!("# row {row:02}\n"))
        .collect::<String>();
    std::fs::write(&path, text).unwrap();
    let mut pane = construct(dir.path(), vec![path]).pane;
    let now = Instant::now();
    pane.handle_input(PaneInput::Key(key('j')), now);
    let early = pane.tick(now + std::time::Duration::from_secs(4));
    assert!(!early.idle_due);
    assert_eq!(
        early.next_deadline,
        Some(now + std::time::Duration::from_secs(5))
    );
    let due = pane.tick(now + std::time::Duration::from_secs(5));
    assert!(due.idle_due);
    let _ = pane.idle_unit(4096);

    let id = pane.active_tab().unwrap();
    pane.render(40, 8, now);
    for _ in 0..10 {
        assert_eq!(
            pane.handle_input(
                PaneInput::Mouse(PaneMouse {
                    kind: PaneMouseKind::ScrollDown,
                    column: 4,
                    row: 2,
                    modifiers: Modifiers::default(),
                }),
                now,
            ),
            InputDisposition::Consumed
        );
    }
    assert_eq!(pane.source_cursor(&id).unwrap(), (15, 2));
}

#[test]
fn host_open_deduplicates_even_when_commands_use_standalone_policy() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("note.md");
    std::fs::write(&path, "hello\n").unwrap();
    let mut pane = construct_with_options(
        dir.path(),
        vec![path.clone()],
        Box::new(AllowAllFileAccess),
        PaneOptions {
            command_policy: CommandPolicy::Standalone,
            ..PaneOptions::default()
        },
    )
    .pane;
    let id = pane.active_tab().unwrap();
    assert_eq!(
        pane.open(&path, OpenOptions::default()).unwrap(),
        OpenOutcome::FocusedExisting(id)
    );
    assert_eq!(pane.tabs().len(), 1);
    pane.render(80, 24, Instant::now());
    ex(&mut pane, "tabnew note.md", Instant::now());
    assert_eq!(pane.tabs().len(), 2);
    assert_ne!(pane.tabs()[0].id, pane.tabs()[1].id);
}

#[cfg(unix)]
#[test]
fn canonical_symlink_alias_focuses_the_same_tab_and_preserves_dirty_text() {
    let dir = tempfile::tempdir_in(std::env::temp_dir().canonicalize().unwrap()).unwrap();
    let path = dir.path().join("note.md");
    let alias = dir.path().join("alias.md");
    std::fs::write(&path, "hello\n").unwrap();
    std::os::unix::fs::symlink(&path, &alias).unwrap();
    let mut pane = construct(dir.path(), vec![path.clone()]).pane;
    let id = pane.active_tab().unwrap();
    pane.handle_input(PaneInput::Key(key('i')), Instant::now());
    pane.handle_input(PaneInput::Paste("changed ".into()), Instant::now());
    assert_eq!(
        pane.open(&alias, OpenOptions::default()).unwrap(),
        OpenOutcome::FocusedExisting(id.clone())
    );
    assert_eq!(pane.text(&id).unwrap(), "changed hello\n");
    assert_eq!(pane.tabs()[0].path.as_deref(), Some(path.as_path()));
    assert!(pane.tabs()[0].dirty);
}

#[test]
fn save_repaints_status_without_changing_rendered_note_content() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("note.md");
    std::fs::write(&path, "# Heading\n\nA paragraph.\n").unwrap();
    let mut pane = construct(dir.path(), vec![path.clone()]).pane;
    let now = Instant::now();
    pane.render(80, 24, now);
    pane.handle_input(PaneInput::Key(key('i')), now);
    pane.handle_input(PaneInput::Key(key('X')), now);
    pane.handle_input(PaneInput::Key(special(KeyCodeKind::Esc)), now);
    let edited_text = pane.text(&pane.active_tab().unwrap()).unwrap();
    assert!(pane.tabs()[0].dirty);
    let before = pane.render(80, 24, now);

    ex(&mut pane, "w", now);
    let after = pane.render(80, 24, now);
    assert!(!pane.tabs()[0].dirty);
    assert_eq!(pane.text(&pane.active_tab().unwrap()).unwrap(), edited_text);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), edited_text);
    let before_text = before
        .cells
        .iter()
        .map(|cell| cell.symbol.as_str())
        .collect::<String>();
    let after_text = after
        .cells
        .iter()
        .map(|cell| cell.symbol.as_str())
        .collect::<String>();
    assert!(before_text.contains("Heading"));
    assert!(after_text.contains("Heading"));
    assert!(after_text.contains("Saved note.md"));
}

#[test]
fn owned_noops_include_pending_grammars_and_text_prompts() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("note.md");
    std::fs::write(&path, "hello\n").unwrap();
    let mut pane = construct(dir.path(), vec![path]).pane;
    let now = Instant::now();
    pane.render(80, 24, now);
    assert_eq!(
        pane.handle_input(PaneInput::Key(special(KeyCodeKind::Noop)), now),
        InputDisposition::NotConsumed
    );
    for prefix in [' ', 'g', '2', '"', '/'] {
        pane.handle_input(PaneInput::Key(key(prefix)), now);
        assert_eq!(
            pane.handle_input(PaneInput::Key(special(KeyCodeKind::Noop)), now),
            InputDisposition::Consumed,
            "prefix {prefix}"
        );
        pane.handle_input(PaneInput::Key(special(KeyCodeKind::Esc)), now);
    }
    pane.handle_input(PaneInput::Key(key(':')), now);
    assert!(pane.input_state().text_entry);
    assert_eq!(
        pane.handle_input(PaneInput::Key(special(KeyCodeKind::Noop)), now),
        InputDisposition::Consumed
    );
    pane.handle_input(PaneInput::Key(special(KeyCodeKind::Esc)), now);
    assert_eq!(
        pane.handle_input(PaneInput::Key(special(KeyCodeKind::F(1))), now),
        InputDisposition::NotConsumed
    );
}

#[test]
fn paste_is_one_undo_operation_and_modes_emit_once() {
    let dir = tempfile::tempdir().unwrap();
    let mut pane = construct(dir.path(), vec![dir.path().join("new.md")]).pane;
    let id = pane.active_tab().unwrap();
    let now = Instant::now();
    pane.drain_events();
    pane.handle_input(PaneInput::Key(key('i')), now);
    assert_eq!(
        pane.drain_events(),
        vec![PaneEvent::ModeChanged {
            tab: id.clone(),
            mode: Mode::Insert
        }]
    );
    assert!(pane.input_state().text_entry);
    pane.handle_input(PaneInput::Paste("first\nsecond\n".into()), now);
    assert_eq!(pane.text(&id).unwrap(), "first\nsecond\n");
    assert!(pane.drain_events().is_empty());
    pane.handle_input(PaneInput::Key(special(KeyCodeKind::Esc)), now);
    pane.handle_input(PaneInput::Key(key('u')), now);
    assert_eq!(pane.text(&id).unwrap(), "");
    assert!(!pane.tabs()[0].dirty);
    assert!(pane.tabs()[0].is_new);
}

#[test]
fn focus_loss_preserves_modal_state_and_cancels_space_deadline() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("note.md");
    std::fs::write(&path, "hello\n").unwrap();
    let mut pane = construct(dir.path(), vec![path]).pane;
    let now = Instant::now();
    pane.render(80, 24, now);
    pane.handle_input(PaneInput::Key(key(' ')), now);
    assert_eq!(
        pane.tick(now).next_deadline,
        Some(now + std::time::Duration::from_millis(150))
    );
    pane.set_focused(false);
    assert_ne!(
        pane.tick(now).next_deadline,
        Some(now + std::time::Duration::from_millis(150))
    );
    pane.set_focused(true);
    pane.handle_input(PaneInput::Key(key('?')), now);
    assert!(pane.input_state().modal);
    assert!(pane.input_state().text_entry);
    pane.set_focused(false);
    assert!(pane.input_state().modal);
    assert!(pane.render(20, 5, now).cursor.is_none());
    pane.set_focused(true);
    let id = pane.active_tab().unwrap();
    let before = pane.source_cursor(&id).unwrap();
    pane.handle_input(
        PaneInput::Mouse(PaneMouse {
            kind: PaneMouseKind::LeftDown,
            column: 8,
            row: 2,
            modifiers: Modifiers::default(),
        }),
        now,
    );
    pane.handle_input(PaneInput::Paste("ignored".into()), now);
    assert_eq!(pane.source_cursor(&id).unwrap(), before);
    assert_eq!(pane.text(&id).unwrap(), "hello\n");
    pane.handle_input(PaneInput::Key(special(KeyCodeKind::Esc)), now);
    assert!(!pane.input_state().modal);
    assert!(!pane.input_state().text_entry);
}

#[test]
fn decision_overlay_has_no_text_entry_owner_and_survives_focus_loss() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("note.md");
    std::fs::write(&path, "hello\n").unwrap();
    let mut pane = construct(dir.path(), vec![path]).pane;
    let now = Instant::now();
    pane.render(80, 24, now);
    pane.handle_input(PaneInput::Key(key('i')), now);
    pane.handle_input(PaneInput::Paste("changed".into()), now);
    pane.handle_input(PaneInput::Key(special(KeyCodeKind::Esc)), now);
    pane.handle_input(PaneInput::Key(key(' ')), now);
    pane.handle_input(PaneInput::Key(key('q')), now);
    assert!(pane.input_state().modal);
    assert!(!pane.input_state().text_entry);
    pane.set_focused(false);
    pane.set_focused(true);
    pane.handle_input(PaneInput::Key(special(KeyCodeKind::Esc)), now);
    assert_eq!(pane.tabs().len(), 1);
    assert!(pane.tabs()[0].dirty);
}

#[test]
fn resize_relayouts_on_next_render_without_changing_source_cursor() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("long.md");
    std::fs::write(
        &path,
        "a long prose paragraph that wraps onto several rows at a narrow width\n",
    )
    .unwrap();
    let mut pane = construct(dir.path(), vec![path]).pane;
    let now = Instant::now();
    let id = pane.active_tab().unwrap();
    pane.render(80, 24, now);
    pane.handle_input(PaneInput::Key(key('l')), now);
    let cursor = pane.source_cursor(&id).unwrap();
    let narrow = pane.render(20, 5, now);
    assert_eq!(pane.source_cursor(&id).unwrap(), cursor);
    assert!(narrow.changed);
    assert!(!pane.render(20, 5, now).changed);
    assert!(pane.render(80, 24, now).changed);
    assert_eq!(pane.source_cursor(&id).unwrap(), cursor);
}

#[test]
fn native_register_prompt_suspends_its_timeout_while_unfocused() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("note.md");
    std::fs::write(&path, "hello\n").unwrap();
    let mut pane = construct(dir.path(), vec![path]).pane;
    let now = Instant::now();
    pane.handle_input(PaneInput::Key(key('i')), now);
    pane.handle_input(
        PaneInput::Key(KeyInput {
            mods: Modifiers {
                ctrl: true,
                ..Modifiers::default()
            },
            ..key('r')
        }),
        now,
    );
    pane.set_focused(false);
    pane.tick(now + std::time::Duration::from_secs(10));
    pane.set_focused(true);
    let id = pane.active_tab().unwrap();
    pane.handle_input(
        PaneInput::Key(key('a')),
        now + std::time::Duration::from_secs(10),
    );
    assert_eq!(
        pane.text(&id).unwrap(),
        "hello\n",
        "a selects an empty register; it is not inserted after a hidden timeout"
    );
    pane.handle_input(
        PaneInput::Key(key('a')),
        now + std::time::Duration::from_secs(10),
    );
    assert_eq!(pane.text(&id).unwrap(), "ahello\n");
}

#[test]
fn construction_probe_has_no_process_or_terminal_side_effects() {
    if std::env::var_os("OOM_PANE_CONSTRUCTION_PROBE").is_none() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let config_file = dir.path().join("config.toml");
    std::fs::write(&config_file, "this is not valid TOML [").unwrap();
    let before_directory = std::env::current_dir().unwrap();
    let before_environment = std::env::vars_os().collect::<std::collections::BTreeMap<_, _>>();
    let panics = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let capture = std::sync::Arc::clone(&panics);
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |_| {
        capture.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    }));
    for _ in 0..3 {
        let result = construct(dir.path(), vec![]);
        assert!(result.report.warnings.is_empty());
        assert!(result.pane.tabs().is_empty());
    }
    assert_eq!(std::env::current_dir().unwrap(), before_directory);
    assert_eq!(
        std::env::vars_os().collect::<std::collections::BTreeMap<_, _>>(),
        before_environment
    );
    assert_eq!(
        std::fs::read_to_string(config_file).unwrap(),
        "this is not valid TOML ["
    );
    assert!(std::panic::catch_unwind(|| panic!("hook probe")).is_err());
    assert_eq!(panics.load(std::sync::atomic::Ordering::SeqCst), 1);
    std::panic::set_hook(original_hook);
}

#[test]
fn construction_is_silent_in_a_subprocess_with_conflicting_environment() {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "construction_probe_has_no_process_or_terminal_side_effects",
            "--nocapture",
        ])
        .env("OOM_PANE_CONSTRUCTION_PROBE", "1")
        .env("OOM_EDIT_THEME", "not-a-real-theme")
        .env("NO_COLOR", "1")
        .stdin(std::process::Stdio::null())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    let lines = stdout
        .lines()
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>();
    assert_eq!(lines.len(), 3, "unexpected pane output: {stdout:?}");
    assert_eq!(lines[0], "running 1 test");
    assert_eq!(
        lines[1],
        "test construction_probe_has_no_process_or_terminal_side_effects ... ok"
    );
    assert!(lines[2].starts_with("test result: ok. 1 passed;"));
}

#[test]
fn unnamed_tab_metadata_and_open_events_preserve_optional_paths() {
    let dir = tempfile::tempdir_in(std::env::temp_dir().canonicalize().unwrap()).unwrap();
    let mut pane = construct(dir.path(), vec![]).pane;
    let id = pane
        .new_buffer(OpenOptions {
            enter_insert: true,
            ..OpenOptions::default()
        })
        .unwrap();
    let tab = &pane.tabs()[0];
    assert_eq!(tab.id, id);
    assert_eq!(tab.title, "[No Name]");
    assert_eq!(tab.path, None);
    assert!(!tab.dirty);
    assert!(!tab.is_new);
    assert!(tab.active);
    assert!(!tab.changed_on_disk);
    assert_eq!(tab.mru_rank, 0);
    assert_eq!(tab.mode, Mode::Insert);
    let events = pane.drain_events();
    let [PaneEvent::Opened { request, tab, path }, PaneEvent::ActiveTabChanged {
        request: active_request,
        tab: active,
    }, PaneEvent::Completed {
        request: completed_request,
    }] = events.as_slice()
    else {
        panic!("unexpected events: {events:?}");
    };
    assert_eq!(tab, &id);
    assert_eq!(active, &id);
    assert_eq!(request, active_request);
    assert_eq!(request, completed_request);
    assert_eq!(path, &None);
    pane.handle_input(PaneInput::Paste("retained".into()), Instant::now());
    assert!(pane.tabs()[0].dirty);
    assert_eq!(pane.text(&id).unwrap(), "retained");
    assert!(std::fs::read_dir(dir.path()).unwrap().next().is_none());

    let existing_path = dir.path().join("existing.md");
    let new_path = dir.path().join("new.md");
    std::fs::write(&existing_path, "original\n").unwrap();
    let OpenOutcome::Opened(existing) = pane
        .open_existing(&existing_path, OpenOptions::default())
        .unwrap()
    else {
        panic!("expected new existing-file tab");
    };
    let OpenOutcome::Opened(new) = pane.open(&new_path, OpenOptions::default()).unwrap() else {
        panic!("expected never-created tab");
    };
    pane.focus_tab(&id).unwrap();
    pane.set_focused(false);
    std::fs::write(&existing_path, "replacement\n").unwrap();
    pane.notify_paths_changed(std::slice::from_ref(&existing_path))
        .unwrap();
    let tabs = pane.tabs();
    assert_eq!(
        tabs.iter().map(|tab| &tab.id).collect::<Vec<_>>(),
        [&id, &existing, &new]
    );
    for (tab, path, title, dirty, is_new, changed_on_disk, active, mru_rank, mode) in [
        (
            &tabs[0],
            None,
            "[No Name]",
            true,
            false,
            false,
            true,
            0,
            Mode::Insert,
        ),
        (
            &tabs[1],
            Some(&existing_path),
            "existing.md",
            false,
            false,
            true,
            false,
            2,
            Mode::Normal,
        ),
        (
            &tabs[2],
            Some(&new_path),
            "new.md",
            false,
            true,
            false,
            false,
            1,
            Mode::Normal,
        ),
    ] {
        assert_eq!(tab.path.as_ref(), path);
        assert_eq!(tab.title, title);
        assert_eq!(tab.dirty, dirty);
        assert_eq!(tab.is_new, is_new);
        assert_eq!(tab.changed_on_disk, changed_on_disk);
        assert_eq!(tab.active, active);
        assert_eq!(tab.mru_rank, mru_rank);
        assert_eq!(tab.mode, mode);
    }
    assert_eq!(pane.text(&existing).unwrap(), "original\n");
    pane.close(&existing).unwrap();
    let remaining = pane.tabs();
    assert_eq!(
        remaining.iter().map(|tab| &tab.id).collect::<Vec<_>>(),
        [&id, &new]
    );
    assert_eq!(pane.text(&new).unwrap(), "");
    assert_eq!(pane.text(&id).unwrap(), "retained");
    assert!(!new_path.exists());
    assert_eq!(
        std::fs::read_to_string(existing_path).unwrap(),
        "replacement\n"
    );
}
