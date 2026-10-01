//! Exact-document, public-pane current-RSS probe for repeated editor use.

use std::error::Error;
use std::fs;
use std::path::Path;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use oom_edit::{
    AllowAllFileAccess, ClipboardError, ClipboardSink, Config, ConfigPersistError, DisplayMode,
    EditorPane, KeyCode, KeyCodeKind, KeyInput, Modifiers, OpenCursor, OpenOptions, PaneEvent,
    PaneFrame, PaneInit, PaneInput, PaneOptions, PaneServices, ThemeCatalog, ThemePersistenceSink,
    ThemeSelection, ThemeSlot, Tier,
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

fn hash(text: &str) -> u64 {
    text.as_bytes()
        .iter()
        .fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
            (hash ^ u64::from(*byte)).wrapping_mul(0x100_0000_01b3)
        })
}

fn rss_bytes() -> Result<u64, Box<dyn Error>> {
    let status = fs::read_to_string("/proc/self/status")?;
    let kib = status
        .lines()
        .find_map(|line| {
            line.strip_prefix("VmRSS:")
                .and_then(|value| value.split_whitespace().next())
        })
        .ok_or("VmRSS is unavailable")?
        .parse::<u64>()?;
    Ok(kib * 1024)
}

fn render(editor: &mut EditorPane) -> PaneFrame {
    let frame = editor.render(WIDTH, HEIGHT, Instant::now());
    assert_eq!((frame.width, frame.height), (WIDTH, HEIGHT));
    assert_eq!(frame.cells.len(), usize::from(WIDTH) * usize::from(HEIGHT));
    assert!(frame.cursor.is_some());
    frame
}

fn same_frame(expected: &mut Option<PaneFrame>, frame: &PaneFrame) {
    if let Some(previous) = expected {
        assert!(
            previous.visually_equals(frame),
            "frame drifted across cycles"
        );
    } else {
        *expected = Some(frame.clone());
    }
}

fn goto(editor: &mut EditorPane, line: usize) {
    key(editor, KeyCodeKind::Char(':'));
    for digit in (line + 1).to_string().chars() {
        key(editor, KeyCodeKind::Char(digit));
    }
    key(editor, KeyCodeKind::Enter);
}

fn reload(editor: &mut EditorPane, path: &Path) -> Result<PaneFrame, Box<dyn Error>> {
    editor.notify_paths_changed(&[path.to_path_buf()])?;
    let _ = editor.tick(Instant::now());
    let frame = render(editor);
    let events = editor.drain_events();
    assert!(events
        .iter()
        .any(|event| matches!(event, PaneEvent::ReloadedFromDisk { .. })));
    Ok(frame)
}

fn changed_language(source: &str, language: &str) -> String {
    let marker = format!("```{language}");
    let offset = source
        .match_indices(&marker)
        .nth(1)
        .expect("the exact fixture has two fences of each language")
        .0;
    let replacement = if language == "rust" {
        "```go"
    } else {
        "```rust"
    };
    let mut changed = source.to_string();
    changed.replace_range(offset..offset + marker.len(), replacement);
    changed
}

fn main() -> Result<(), Box<dyn Error>> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    assert_eq!(
        args.len(),
        4,
        "usage: performance_rss_stability FIXTURE SCENARIO LINE COUNT"
    );
    let fixture = Path::new(&args[0]);
    let scenario = args[1].as_str();
    let line = args[2].parse::<usize>()?;
    let count = args[3].parse::<usize>()?;
    assert!(matches!(count, 100 | 200 | 300 | 400));
    let language = scenario.split('-').next().unwrap();
    let operation = scenario.split('-').nth(1).unwrap();
    assert!(matches!(language, "rust" | "go"));
    assert!(matches!(operation, "delete" | "change" | "mode" | "reload"));
    assert_eq!(line, if language == "rust" { 3000 } else { 20000 });
    let source = fs::read_to_string(fixture)?;
    assert_eq!(source.len(), 1_048_722, "fixture bytes changed");
    let source_hash = hash(&source);
    let base = std::env::temp_dir().join(format!(
        "oom-edit-rss-{}-{}",
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
            cursor: OpenCursor::SourceLine(line),
            enter_insert: false,
        },
    )?;
    editor.set_focused(true);
    let _ = editor.drain_events();
    let opened = render(&mut editor);
    assert_eq!(editor.tabs()[0].mode, Mode::Normal);
    let tab = editor.tabs()[0].id.clone();
    let initial_line = editor.source_cursor(&tab)?.0;
    assert_eq!(initial_line, line);
    let mut expected_restored = None;
    let mut expected_intermediate = None;
    let alternate = if operation == "reload" {
        Some(changed_language(&source, language))
    } else {
        None
    };
    println!(
        "RSS-RUN\t{source_hash:016x}\t{scenario}\t{count}\t{line}\t{}",
        opened.cells.len()
    );
    for cycle in 1..=count {
        if editor.source_cursor(&tab)?.0 != initial_line {
            goto(&mut editor, initial_line);
        }
        assert_eq!(editor.source_cursor(&tab)?.0, initial_line);
        match operation {
            "delete" | "change" => {
                key(&mut editor, KeyCodeKind::Char('v'));
                for _ in 0..15 {
                    key(&mut editor, KeyCodeKind::Char('j'));
                }
                assert_eq!(editor.tabs()[0].mode, Mode::Select);
                let selection = render(&mut editor);
                assert!(selection.cursor.is_some());
                let started = Instant::now();
                key(
                    &mut editor,
                    KeyCodeKind::Char(if operation == "change" { 'c' } else { 'd' }),
                );
                let edited = render(&mut editor);
                if cycle == 1 {
                    println!(
                        "FIRST-EDIT\t{source_hash:016x}\t{scenario}\t{}",
                        started.elapsed().as_nanos()
                    );
                }
                assert_ne!(editor.text(&tab)?, source);
                assert_eq!(
                    editor.tabs()[0].mode,
                    if operation == "change" {
                        Mode::Insert
                    } else {
                        Mode::Normal
                    }
                );
                same_frame(&mut expected_intermediate, &edited);
                if operation == "change" {
                    key(&mut editor, KeyCodeKind::Esc);
                }
                key(&mut editor, KeyCodeKind::Char('u'));
            }
            "mode" => {
                key(&mut editor, KeyCodeKind::Char('i'));
                assert_eq!(editor.tabs()[0].mode, Mode::Insert);
                let source_frame = render(&mut editor);
                same_frame(&mut expected_intermediate, &source_frame);
                key(&mut editor, KeyCodeKind::Esc);
            }
            "reload" => {
                let alternate = alternate.as_ref().unwrap();
                fs::write(&path, alternate)?;
                let changed = reload(&mut editor, &path)?;
                assert_eq!(editor.text(&tab)?, *alternate);
                same_frame(&mut expected_intermediate, &changed);
                fs::write(&path, &source)?;
                let restored = reload(&mut editor, &path)?;
                assert_eq!(editor.text(&tab)?, source);
                same_frame(&mut expected_restored, &restored);
            }
            _ => unreachable!(),
        }
        let restored = render(&mut editor);
        assert_eq!(
            editor.text(&tab)?,
            source,
            "cycle {cycle} did not restore text"
        );
        assert_eq!(editor.tabs()[0].mode, Mode::Normal);
        same_frame(&mut expected_restored, &restored);
        if cycle % 100 == 0 {
            println!(
                "RSS\t{source_hash:016x}\t{scenario}\t{cycle}\t{}\t{:016x}\tnormal\t{}",
                rss_bytes()?,
                hash(&editor.text(&tab)?),
                restored.cells.len(),
            );
        }
    }
    drop(editor);
    fs::remove_dir_all(&base)?;
    Ok(())
}
