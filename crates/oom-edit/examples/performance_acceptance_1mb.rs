//! Exact-document interaction probe used by `make bench-acceptance-1mb-record`.

use std::error::Error;
use std::fs;
use std::path::Path;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use oom_edit::{
    AllowAllFileAccess, ClipboardError, ClipboardSink, Config, ConfigPersistError, DisplayMode,
    EditorPane, KeyCode, KeyCodeKind, KeyInput, Modifiers, OpenCursor, OpenOptions, PaneInit,
    PaneInput, PaneOptions, PaneServices, ThemeCatalog, ThemePersistenceSink, ThemeSelection,
    ThemeSlot, Tier,
};
use oom_edit_core::Mode;

const WIDTH: u16 = 100;
const HEIGHT: u16 = 41;

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

fn pane(base: &Path) -> EditorPane {
    let mut config = Config::default();
    config.spell.enabled = false;
    let constructed = EditorPane::construct(PaneInit {
        config,
        theme_catalog: ThemeCatalog::builtins(),
        theme_selection: ThemeSelection::new(None, DisplayMode::Dark, Tier::TrueColor),
        services: PaneServices {
            clipboard_sink: Box::new(Silent),
            theme_sink: Box::new(Silent),
            file_access_policy: Box::new(AllowAllFileAccess),
            config_base_directory: base.to_path_buf(),
            personal_dictionary_path: base.join("personal.txt"),
            working_directory: base.to_path_buf(),
        },
        options: PaneOptions::default(),
        initial_paths: Vec::new(),
        now: Instant::now(),
    });
    assert!(constructed.report.warnings.is_empty());
    constructed.pane
}

fn key(editor: &mut EditorPane, kind: KeyCodeKind) {
    editor.handle_input(
        PaneInput::Key(KeyInput {
            code: KeyCode { kind },
            mods: Modifiers::default(),
        }),
        Instant::now(),
    );
}

fn key_kind(name: &str) -> KeyCodeKind {
    match name {
        "j" => KeyCodeKind::Char('j'),
        "k" => KeyCodeKind::Char('k'),
        "down" => KeyCodeKind::Down,
        "up" => KeyCodeKind::Up,
        _ => panic!("unknown key: {name}"),
    }
}

fn rss_bytes(field: &str) -> u64 {
    let Ok(status) = fs::read_to_string("/proc/self/status") else {
        return 0;
    };
    status
        .lines()
        .find_map(|line| {
            line.strip_prefix(field)
                .and_then(|value| value.split_whitespace().next())
                .and_then(|value| value.parse::<u64>().ok())
                .map(|kib| kib * 1024)
        })
        .unwrap_or(0)
}

fn hash(text: &str) -> u64 {
    text.as_bytes()
        .iter()
        .fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
            (hash ^ u64::from(*byte)).wrapping_mul(0x100_0000_01b3)
        })
}

fn main() -> Result<(), Box<dyn Error>> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    assert_eq!(
        args.len(),
        6,
        "usage: performance_acceptance_1mb FIXTURE nav|select-delete|select-change|select-delete-cycles|select-change-cycles LINE j|k|down|up flat|retained COUNT"
    );
    let fixture = Path::new(&args[0]);
    let case = args[1].as_str();
    let start_line = args[2].parse::<usize>()?;
    let motion = args[3].as_str();
    let state = args[4].as_str();
    let count = args[5].parse::<usize>()?;
    assert!(matches!(
        case,
        "nav" | "select-delete" | "select-change" | "select-delete-cycles" | "select-change-cycles"
    ));
    assert!(matches!(state, "flat" | "retained"));
    assert!(count > 0);
    let source = fs::read_to_string(fixture)?;
    assert_eq!(
        source.len(),
        1_048_722,
        "fixture must be the approved large note"
    );
    assert!(
        source.lines().nth(start_line).is_some(),
        "start line is outside note"
    );
    let fixture_hash = hash(&source);

    let base = std::env::temp_dir().join(format!(
        "oom-edit-acceptance-{}-{}",
        std::process::id(),
        SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
    ));
    fs::create_dir(&base)?;
    let path = base.join("note.md");
    fs::copy(fixture, &path)?;
    let mut editor = pane(&base);
    editor.open_existing(
        &path,
        OpenOptions {
            cursor: OpenCursor::SourceLine(start_line),
            enter_insert: false,
        },
    )?;
    editor.set_focused(true);
    let _ = editor.drain_events();
    let opened = Instant::now();
    let mut frame = editor.render(WIDTH, HEIGHT, Instant::now());
    let cold_ns = opened.elapsed().as_nanos();
    assert_eq!((frame.width, frame.height), (WIDTH, HEIGHT));
    assert_eq!(editor.tabs()[0].mode, Mode::Normal);

    if state == "retained" {
        key(&mut editor, KeyCodeKind::Char('i'));
        key(&mut editor, KeyCodeKind::Char('x'));
        key(&mut editor, KeyCodeKind::Backspace);
        key(&mut editor, KeyCodeKind::Esc);
        frame = editor.render(WIDTH, HEIGHT, Instant::now());
        assert_eq!(editor.text(&editor.tabs()[0].id)?, source);
    }
    let tab = editor.tabs()[0].id.clone();
    let initial_line = editor.source_cursor(&tab)?.0;
    if case.ends_with("-cycles") {
        let change = case == "select-change-cycles";
        let original_lines = source.lines().count();
        let mut expected_changed = None;
        let mut expected_body = None;
        let mut records = Vec::with_capacity(count);
        for cycle in 0..count {
            if editor.source_cursor(&tab)?.0 != initial_line {
                key(&mut editor, KeyCodeKind::Char(':'));
                for digit in (initial_line + 1).to_string().chars() {
                    key(&mut editor, KeyCodeKind::Char(digit));
                }
                key(&mut editor, KeyCodeKind::Enter);
            }
            assert_eq!(editor.source_cursor(&tab)?.0, initial_line);
            assert_eq!(editor.tabs()[0].mode, Mode::Normal);
            key(&mut editor, KeyCodeKind::Char('v'));
            for _ in 0..15 {
                key(&mut editor, key_kind(motion));
            }
            assert_eq!(editor.tabs()[0].mode, Mode::Select);
            let selected_frame = editor.render(WIDTH, HEIGHT, Instant::now());
            assert!(selected_frame.cursor.is_some());

            let started = Instant::now();
            key(
                &mut editor,
                KeyCodeKind::Char(if change { 'c' } else { 'd' }),
            );
            let input_ns = started.elapsed().as_nanos();
            let started = Instant::now();
            let edited_frame = editor.render(WIDTH, HEIGHT, Instant::now());
            let render_ns = started.elapsed().as_nanos();
            assert_eq!((edited_frame.width, edited_frame.height), (WIDTH, HEIGHT));
            assert!(edited_frame.cursor.is_some());
            assert_eq!(
                editor.tabs()[0].mode,
                if change { Mode::Insert } else { Mode::Normal }
            );
            let changed = editor.text(&tab)?;
            assert_ne!(changed, source);
            assert!((10..=20).contains(&(original_lines - changed.lines().count())));
            if let Some(expected) = &expected_changed {
                assert_eq!(changed, *expected, "cycle {cycle} changed different bytes");
            } else {
                expected_changed = Some(changed.to_string());
            }
            let body_cells = &edited_frame.cells[..usize::from(WIDTH) * usize::from(HEIGHT - 1)];
            if let Some(expected) = &expected_body {
                assert_eq!(
                    body_cells, expected,
                    "cycle {cycle} changed different cells"
                );
            } else {
                expected_body = Some(body_cells.to_vec());
            }
            if change {
                key(&mut editor, KeyCodeKind::Esc);
            }
            let started = Instant::now();
            key(&mut editor, KeyCodeKind::Char('u'));
            let undo_input_ns = started.elapsed().as_nanos();
            let started = Instant::now();
            let restored = editor.render(WIDTH, HEIGHT, Instant::now());
            let undo_render_ns = started.elapsed().as_nanos();
            assert_eq!(editor.text(&tab)?, source, "cycle {cycle} undo differed");
            assert_eq!(editor.tabs()[0].mode, Mode::Normal);
            records.push(format!(
                "CYCLE\t{fixture_hash:016x}\t{case}\t{start_line}\t{cycle}\t{input_ns}\t{render_ns}\t{:016x}\t{}\t{}\t{undo_input_ns}\t{undo_render_ns}",
                hash(&changed),
                changed.lines().count(),
                if change { "insert" } else { "normal" }
            ));
            std::hint::black_box(restored);
        }
        println!(
            "RUN-CYCLES\t{fixture_hash:016x}\t{case}\t{start_line}\t{motion}\t{state}\t{count}\t{initial_line}"
        );
        for record in records {
            println!("{record}");
        }
        return Ok(());
    }
    let origin = Instant::now();
    let mut records = Vec::with_capacity(count + 2);
    let mut previous_line = initial_line;
    for index in 0..count {
        let pause_ns = if index == count / 2 && count >= 100 {
            let pause = Instant::now();
            std::thread::sleep(Duration::from_millis(300));
            pause.elapsed().as_nanos()
        } else {
            0
        };
        let start_ns = origin.elapsed().as_nanos();
        let started = Instant::now();
        if index == 0 && case != "nav" {
            key(&mut editor, KeyCodeKind::Char('v'));
            assert_eq!(editor.tabs()[0].mode, Mode::Select);
        }
        key(&mut editor, key_kind(motion));
        let input_ns = started.elapsed().as_nanos();
        let started = Instant::now();
        let next = editor.render(WIDTH, HEIGHT, Instant::now());
        let render_ns = started.elapsed().as_nanos();
        let end_ns = origin.elapsed().as_nanos();
        assert_eq!((next.width, next.height), (WIDTH, HEIGHT));
        assert!(next
            .cursor
            .is_some_and(|cursor| cursor.column < WIDTH && cursor.row < HEIGHT));
        let line = editor.source_cursor(&tab)?.0;
        if matches!(motion, "j" | "down") {
            assert!(line >= previous_line, "downward motion moved up");
        } else {
            assert!(line <= previous_line, "upward motion moved down");
        }
        assert_eq!(
            editor.tabs()[0].mode,
            if case == "nav" {
                Mode::Normal
            } else {
                Mode::Select
            }
        );
        assert!(
            !next.visually_equals(&frame),
            "every measured key must update the owned frame"
        );
        records.push(format!(
            "KEY\t{fixture_hash:016x}\t{case}\t{start_line}\t{motion}\t{state}\t{index}\t{start_ns}\t{input_ns}\t{render_ns}\t{end_ns}\t{pause_ns}\t{line}\t{}\t{}\t{}\t{}",
            next.cursor.expect("visible cursor").row,
            next.cursor.expect("visible cursor").column,
            if case == "nav" { "normal" } else { "select" },
            !next.visually_equals(&frame),
        ));
        previous_line = line;
        frame = next;
    }

    if case != "nav" {
        let started = Instant::now();
        key(
            &mut editor,
            KeyCodeKind::Char(if case == "select-delete" { 'd' } else { 'c' }),
        );
        let input_ns = started.elapsed().as_nanos();
        let started = Instant::now();
        let edited_frame = editor.render(WIDTH, HEIGHT, Instant::now());
        let render_ns = started.elapsed().as_nanos();
        assert_eq!((edited_frame.width, edited_frame.height), (WIDTH, HEIGHT));
        std::hint::black_box(edited_frame);
        let changed = editor.text(&tab)?;
        let changed_lines = changed.lines().count();
        let original_lines = source.lines().count();
        assert_ne!(changed, source, "selection edit must change text");
        let expected_mode = if case == "select-delete" {
            Mode::Normal
        } else {
            Mode::Insert
        };
        assert_eq!(editor.tabs()[0].mode, expected_mode);
        records.push(format!(
            "EDIT\t{fixture_hash:016x}\t{case}\t{start_line}\t{motion}\t{state}\t{count}\t{input_ns}\t{render_ns}\t{original_lines}\t{changed_lines}\t{:016x}\t{}",
            hash(&changed),
            if expected_mode == Mode::Normal { "normal" } else { "insert" }
        ));
        if expected_mode == Mode::Insert {
            key(&mut editor, KeyCodeKind::Esc);
        }
        key(&mut editor, KeyCodeKind::Char('u'));
        frame = editor.render(WIDTH, HEIGHT, Instant::now());
        assert_eq!(
            editor.text(&tab)?,
            source,
            "one undo must restore exact bytes"
        );
        assert_eq!(editor.tabs()[0].mode, Mode::Normal);
    }
    std::hint::black_box(frame);
    println!(
        "RUN\t{fixture_hash:016x}\t{case}\t{start_line}\t{motion}\t{state}\t{count}\t{initial_line}\t{cold_ns}\t{}\t{}",
        rss_bytes("VmRSS:"),
        rss_bytes("VmHWM:")
    );
    for record in records {
        println!("{record}");
    }
    fs::remove_file(&path)?;
    fs::remove_dir(&base)?;
    Ok(())
}
