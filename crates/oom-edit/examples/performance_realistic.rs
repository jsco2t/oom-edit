//! Cold public-pane performance probe. Run through `make bench-realistic`.

#[path = "../perf/realistic_fixtures.rs"]
mod fixtures;
#[allow(dead_code)]
#[path = "../../oom-edit-core/perf/layout_metrics.rs"]
mod layout_metrics;

use std::error::Error;
use std::fs;
use std::path::Path;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use oom_edit::{
    AllowAllFileAccess, ClipboardError, ClipboardSink, Config, ConfigPersistError, DisplayMode,
    EditorPane, KeyCode, KeyCodeKind, KeyInput, Modifiers, OpenCursor, OpenOptions, PaneFrame,
    PaneInit, PaneInput, PaneMouse, PaneMouseKind, PaneOptions, PaneServices, ThemeCatalog,
    ThemePersistenceSink, ThemeSelection, ThemeSlot, Tier,
};
use oom_edit_core::{EditorSession, Mode};

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

fn peak_rss_bytes() -> u64 {
    let Ok(status) = fs::read_to_string("/proc/self/status") else {
        return 0;
    };
    status
        .lines()
        .find_map(|line| {
            line.strip_prefix("VmHWM:")
                .and_then(|value| value.split_whitespace().next())
                .and_then(|value| value.parse::<u64>().ok())
                .map(|kib| kib * 1024)
        })
        .unwrap_or(0)
}

fn input(editor: &mut EditorPane, kind: KeyCodeKind) {
    editor.handle_input(
        PaneInput::Key(KeyInput {
            code: KeyCode { kind },
            mods: Modifiers::default(),
        }),
        Instant::now(),
    );
}

fn scroll(editor: &mut EditorPane, kind: PaneMouseKind) {
    editor.handle_input(
        PaneInput::Mouse(PaneMouse {
            kind,
            column: 10,
            row: 10,
            modifiers: Modifiers::default(),
        }),
        Instant::now(),
    );
}

fn trigger(
    editor: &mut EditorPane,
    name: &str,
    path: &Path,
    text: &str,
    width: u16,
    height: u16,
) -> Result<u128, Box<dyn Error>> {
    let started = Instant::now();
    let render_width = match name {
        "edit" => {
            input(editor, KeyCodeKind::Char('i'));
            input(editor, KeyCodeKind::Char('x'));
            input(editor, KeyCodeKind::Esc);
            width
        }
        "save" => {
            input(editor, KeyCodeKind::Char(':'));
            input(editor, KeyCodeKind::Char('w'));
            input(editor, KeyCodeKind::Enter);
            width
        }
        "width" => {
            let next = width.saturating_sub(1);
            editor.resize(next, height, Instant::now());
            next
        }
        "reload" => {
            fs::write(path, format!("{text}\n"))?;
            editor.notify_paths_changed(&[path.to_path_buf()])?;
            let _ = editor.tick(Instant::now());
            width
        }
        _ => panic!("unknown trigger: {name}"),
    };
    let frame = editor.render(render_width, height, Instant::now());
    let host_cells = frame.cells.iter().cloned().collect::<Vec<_>>();
    std::hint::black_box(host_cells);
    Ok(started.elapsed().as_nanos())
}

fn interaction_step<F>(
    editor: &mut EditorPane,
    identity: &str,
    case: &str,
    step: &str,
    width: u16,
    height: u16,
    action: F,
) -> Result<PaneFrame, Box<dyn Error>>
where
    F: FnOnce(&mut EditorPane) -> Result<(), Box<dyn Error>>,
{
    let total_started = Instant::now();
    let started = Instant::now();
    action(editor)?;
    let input_ns = started.elapsed().as_nanos();
    let started = Instant::now();
    let frame = editor.render(width, height, Instant::now());
    let host_cells = frame.cells.iter().cloned().collect::<Vec<_>>();
    std::hint::black_box(host_cells);
    let render_ns = started.elapsed().as_nanos();
    let total_ns = total_started.elapsed().as_nanos();
    let rss_bytes = peak_rss_bytes();
    assert_eq!((frame.width, frame.height), (width, height));
    let mode = editor.tabs()[0].mode;
    let expected_mode = if case.starts_with("source-") || case.starts_with("fence-") {
        Mode::Insert
    } else {
        Mode::Normal
    };
    assert_eq!(mode, expected_mode, "interaction step changed mode");
    let mode_name = if mode == Mode::Insert {
        "insert"
    } else {
        "normal"
    };
    println!(
        "INTERACTION\t{identity}\t{case}\t{step}\t{width}\t{height}\t{mode_name}\t{input_ns}\t{render_ns}\t{total_ns}\t{rss_bytes}"
    );
    Ok(frame)
}

fn run_interaction(args: &[String]) -> Result<(), Box<dyn Error>> {
    assert_eq!(
        args.len(),
        6,
        "usage: performance_realistic interaction CLASS BYTES WIDTH HEIGHT CASE ITERATIONS"
    );
    let class = &args[0];
    assert!(fixtures::CLASSES.contains(&class.as_str()));
    let bytes = args[1].parse::<usize>()?;
    let width = args[2].parse::<u16>()?;
    let height = args[3].parse::<u16>()?;
    let case = args[4].as_str();
    let iterations = args[5].parse::<usize>()?;
    assert!(iterations > 0);
    assert_eq!(
        class,
        if case == "source-reference" {
            "mixed-reference"
        } else {
            "mixed"
        }
    );
    let source = matches!(
        case,
        "source-type"
            | "source-type-mid"
            | "source-structural"
            | "source-structural-mid"
            | "source-line"
            | "source-line-mid"
            | "source-frontmatter"
            | "source-reference"
            | "fence-language"
            | "fence-language-mid"
            | "fence-unknown"
            | "fence-unknown-mid"
            | "source-scroll"
            | "source-scroll-mid"
            | "exit-insert"
            | "exit-insert-mid"
    );
    assert!(matches!(
        case,
        "source-type"
            | "source-type-mid"
            | "source-structural"
            | "source-structural-mid"
            | "source-line"
            | "source-line-mid"
            | "source-frontmatter"
            | "source-reference"
            | "fence-language"
            | "fence-language-mid"
            | "fence-unknown"
            | "fence-unknown-mid"
            | "source-scroll"
            | "source-scroll-mid"
            | "rendered-scroll"
            | "rendered-scroll-mid"
            | "rendered-edit"
            | "rendered-edit-mid"
            | "exit-insert"
            | "exit-insert-mid"
            | "resize"
            | "reload"
            | "save"
    ));
    let text = fixtures::generate(class, bytes);
    let identity = fixtures::identity(class, &text);
    let middle_heading = case.ends_with("-mid").then(|| {
        text.lines()
            .enumerate()
            .skip(text.lines().count() / 2)
            .find_map(|(line, content)| content.starts_with("## ").then_some(line))
            .expect("mixed fixture has a heading after its midpoint")
    });
    let fence_line = if case.starts_with("fence-language") || case.starts_with("fence-unknown") {
        let first = if case.ends_with("-mid") {
            text.lines().count() / 2
        } else {
            0
        };
        Some(
            text.lines()
                .enumerate()
                .skip(first)
                .find_map(|(line, content)| (content == "```rust").then_some(line))
                .expect("mixed fixture has a Rust fence at the selected position"),
        )
    } else {
        None
    };
    let line = if case == "source-frontmatter" {
        1
    } else if case == "source-reference" {
        4
    } else if let Some(fence_line) = fence_line {
        fence_line
    } else if case == "reload" || case == "resize" {
        0
    } else if case == "source-structural" {
        4
    } else if case == "source-structural-mid" {
        middle_heading.expect("middle heading was selected")
    } else if case.ends_with("-mid") {
        middle_heading.expect("middle heading was selected") + 2
    } else {
        6
    };
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let base = std::env::temp_dir().join(format!(
        "oom-edit-interaction-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir(&base)?;
    let path = base.join("note.md");
    fs::write(&path, &text)?;
    let mut editor = pane(&base);
    editor.open_existing(
        &path,
        OpenOptions {
            cursor: OpenCursor::SourceLine(line),
            enter_insert: source,
        },
    )?;
    editor.set_focused(true);
    let _ = editor.drain_events();
    std::hint::black_box(editor.render(width, height, Instant::now()));
    if case.starts_with("fence-language") || case.starts_with("fence-unknown") {
        input(&mut editor, KeyCodeKind::End);
        input(&mut editor, KeyCodeKind::Right);
        std::hint::black_box(editor.render(width, height, Instant::now()));
    }

    for index in 0..iterations {
        match case {
            "source-type"
            | "source-type-mid"
            | "source-structural"
            | "source-structural-mid"
            | "source-frontmatter"
            | "source-reference" => {
                let character = if case.starts_with("source-type") {
                    'x'
                } else {
                    '#'
                };
                interaction_step(
                    &mut editor,
                    &identity,
                    case,
                    "insert",
                    width,
                    height,
                    |pane| {
                        input(pane, KeyCodeKind::Char(character));
                        Ok(())
                    },
                )?;
                let active = editor.active_tab().expect("interaction tab remains open");
                assert_ne!(
                    editor.text(&active)?,
                    text,
                    "source insert must change text"
                );
                interaction_step(
                    &mut editor,
                    &identity,
                    case,
                    "delete",
                    width,
                    height,
                    |pane| {
                        input(pane, KeyCodeKind::Backspace);
                        Ok(())
                    },
                )?;
                assert_eq!(
                    editor.text(&active)?,
                    text,
                    "source delete must restore text"
                );
            }
            "source-line" | "source-line-mid" => {
                interaction_step(
                    &mut editor,
                    &identity,
                    case,
                    "insert",
                    width,
                    height,
                    |pane| {
                        input(pane, KeyCodeKind::Enter);
                        Ok(())
                    },
                )?;
                let active = editor.active_tab().expect("interaction tab remains open");
                assert_ne!(
                    editor.text(&active)?,
                    text,
                    "source line insert must change text"
                );
                interaction_step(
                    &mut editor,
                    &identity,
                    case,
                    "delete",
                    width,
                    height,
                    |pane| {
                        input(pane, KeyCodeKind::Backspace);
                        Ok(())
                    },
                )?;
                assert_eq!(
                    editor.text(&active)?,
                    text,
                    "source line delete must restore text"
                );
            }
            "fence-language" | "fence-language-mid" | "fence-unknown" | "fence-unknown-mid" => {
                let replacement = if case.starts_with("fence-unknown") {
                    "unknown"
                } else {
                    ""
                };
                interaction_step(
                    &mut editor,
                    &identity,
                    case,
                    if replacement.is_empty() {
                        "plain"
                    } else {
                        "unknown"
                    },
                    width,
                    height,
                    |pane| {
                        for _ in 0..4 {
                            input(pane, KeyCodeKind::Backspace);
                        }
                        for character in replacement.chars() {
                            input(pane, KeyCodeKind::Char(character));
                        }
                        Ok(())
                    },
                )?;
                let active = editor.active_tab().expect("interaction tab remains open");
                assert_ne!(
                    editor.text(&active)?,
                    text,
                    "fence label removal must change text"
                );
                interaction_step(
                    &mut editor,
                    &identity,
                    case,
                    "highlighted",
                    width,
                    height,
                    |pane| {
                        for _ in replacement.chars() {
                            input(pane, KeyCodeKind::Backspace);
                        }
                        for character in "rust".chars() {
                            input(pane, KeyCodeKind::Char(character));
                        }
                        Ok(())
                    },
                )?;
                let current = editor.text(&active)?;
                assert_eq!(
                    current.lines().nth(line),
                    text.lines().nth(line),
                    "fence label edit must restore the selected line"
                );
                assert_eq!(current, text, "fence language cycle must restore text");
            }
            "source-scroll" | "source-scroll-mid" => {
                for (step, kind) in [
                    ("down", PaneMouseKind::ScrollDown),
                    ("up", PaneMouseKind::ScrollUp),
                ] {
                    let before = editor.render(width, height, Instant::now());
                    let after = interaction_step(
                        &mut editor,
                        &identity,
                        case,
                        step,
                        width,
                        height,
                        |pane| {
                            scroll(pane, kind);
                            Ok(())
                        },
                    )?;
                    assert!(
                        !after.visually_equals(&before),
                        "source scroll must change view"
                    );
                }
            }
            "rendered-scroll" | "rendered-scroll-mid" => {
                for (step, kind) in [
                    ("down", PaneMouseKind::ScrollDown),
                    ("up", PaneMouseKind::ScrollUp),
                ] {
                    let before = editor.render(width, height, Instant::now());
                    let after = interaction_step(
                        &mut editor,
                        &identity,
                        case,
                        step,
                        width,
                        height,
                        |pane| {
                            scroll(pane, kind);
                            Ok(())
                        },
                    )?;
                    assert!(
                        !after.visually_equals(&before),
                        "rendered scroll must change view"
                    );
                }
            }
            "rendered-edit" | "rendered-edit-mid" => {
                for step in ["delete", "undo"] {
                    interaction_step(&mut editor, &identity, case, step, width, height, |pane| {
                        if step == "delete" {
                            input(pane, KeyCodeKind::Char('V'));
                            input(pane, KeyCodeKind::Char('d'));
                        } else {
                            input(pane, KeyCodeKind::Char('u'));
                        }
                        Ok(())
                    })?;
                    let active = editor.active_tab().expect("interaction tab remains open");
                    let current = editor.text(&active)?;
                    if step == "delete" {
                        assert_ne!(current, text, "rendered delete must change text");
                    } else {
                        assert_eq!(current, text, "rendered undo must restore text");
                    }
                }
            }
            "exit-insert" | "exit-insert-mid" => {
                let before = editor.text(&editor.tabs()[0].id)?;
                input(&mut editor, KeyCodeKind::Char('x'));
                std::hint::black_box(editor.render(width, height, Instant::now()));
                let after_edit = editor.text(&editor.tabs()[0].id)?;
                assert_ne!(after_edit, before, "Insert edit must change source text");
                interaction_step(
                    &mut editor,
                    &identity,
                    case,
                    "escape",
                    width,
                    height,
                    |pane| {
                        input(pane, KeyCodeKind::Esc);
                        Ok(())
                    },
                )?;
                assert_eq!(editor.tabs()[0].mode, Mode::Normal);
                assert_eq!(editor.text(&editor.tabs()[0].id)?, after_edit);
                input(&mut editor, KeyCodeKind::Char('i'));
                std::hint::black_box(editor.render(width, height, Instant::now()));
                assert_eq!(editor.tabs()[0].mode, Mode::Insert);
            }
            "resize" => {
                let narrow = width.saturating_sub(1);
                for (step, next) in [("narrow", narrow), ("wide", width)] {
                    let after = interaction_step(
                        &mut editor,
                        &identity,
                        case,
                        step,
                        next,
                        height,
                        |pane| {
                            pane.resize(next, height, Instant::now());
                            Ok(())
                        },
                    )?;
                    assert_eq!(after.width, next);
                    assert_eq!(after.height, height);
                }
            }
            "reload" => {
                let changed = if index % 2 == 0 {
                    format!("{text}\n")
                } else {
                    text.clone()
                };
                fs::write(&path, &changed)?;
                interaction_step(
                    &mut editor,
                    &identity,
                    case,
                    "reload",
                    width,
                    height,
                    |pane| {
                        pane.notify_paths_changed(std::slice::from_ref(&path))?;
                        let _ = pane.tick(Instant::now());
                        Ok(())
                    },
                )?;
                let active = editor.active_tab().expect("interaction tab remains open");
                assert_eq!(
                    editor.text(&active)?,
                    changed,
                    "reload must publish disk text"
                );
            }
            "save" => {
                interaction_step(
                    &mut editor,
                    &identity,
                    case,
                    "save",
                    width,
                    height,
                    |pane| {
                        input(pane, KeyCodeKind::Char(':'));
                        input(pane, KeyCodeKind::Char('w'));
                        input(pane, KeyCodeKind::Enter);
                        Ok(())
                    },
                )?;
                assert_eq!(fs::read_to_string(&path)?, text);
            }
            _ => unreachable!(),
        }
        assert_eq!(
            editor.tabs()[0].mode,
            if source { Mode::Insert } else { Mode::Normal },
            "interaction must preserve its expected mode"
        );
    }
    if matches!(
        case,
        "source-type"
            | "source-type-mid"
            | "source-structural"
            | "source-structural-mid"
            | "source-line"
            | "source-line-mid"
            | "source-frontmatter"
            | "source-reference"
            | "fence-language"
            | "fence-language-mid"
            | "fence-unknown"
            | "fence-unknown-mid"
            | "rendered-edit"
            | "rendered-edit-mid"
    ) {
        let active = editor.active_tab().expect("interaction tab remains open");
        assert!(
            editor
                .text(&active)
                .expect("interaction text remains available")
                == text,
            "interaction must restore the original text"
        );
    }
    fs::remove_file(&path)?;
    fs::remove_dir(&base)?;
    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.first().is_some_and(|arg| arg == "interaction") {
        return run_interaction(&args[1..]);
    }
    assert!(
        matches!(args.len(), 6 | 7),
        "usage: performance_realistic CLASS BYTES WIDTH HEIGHT normal|source|layout start|last [edit|save|width|reload]"
    );
    let class = &args[0];
    assert!(fixtures::CLASSES.contains(&class.as_str()));
    let bytes = args[1].parse::<usize>()?;
    let width = args[2].parse::<u16>()?;
    let height = args[3].parse::<u16>()?;
    let text = fixtures::generate(class, bytes);
    let identity = fixtures::identity(class, &text);
    if args[4] == "layout" {
        let mut session = EditorSession::from_text(&text);
        let started = Instant::now();
        let layout = session.render_layout(width);
        let elapsed = started.elapsed();
        let heap = layout_metrics::rendered_layout_heap_bytes(layout);
        println!(
            "LAYOUT\t{identity}\t{width}\t{}\t{}\t{heap}\t{}",
            elapsed.as_nanos(),
            layout.lines.len(),
            peak_rss_bytes()
        );
        return Ok(());
    }
    let source = match args[4].as_str() {
        "normal" => false,
        "source" => true,
        _ => panic!("mode must be normal, source or layout"),
    };
    let cursor = match args[5].as_str() {
        "start" => OpenCursor::Start,
        "last" => OpenCursor::SourceLine(text.bytes().filter(|byte| *byte == b'\n').count()),
        _ => panic!("cursor must be start or last"),
    };
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let base =
        std::env::temp_dir().join(format!("oom-edit-realistic-{}-{nonce}", std::process::id()));
    fs::create_dir(&base)?;
    let path = base.join("note.md");
    fs::write(&path, &text)?;

    let total_started = Instant::now();
    let started = Instant::now();
    let mut editor = pane(&base);
    let construct = started.elapsed();
    let started = Instant::now();
    editor.open_existing(
        &path,
        OpenOptions {
            cursor,
            enter_insert: source,
        },
    )?;
    let open = started.elapsed();
    editor.set_focused(true);
    let _ = editor.drain_events();
    let started = Instant::now();
    let frame = editor.render(width, height, Instant::now());
    let render = started.elapsed();
    assert_eq!(frame.width, width);
    assert_eq!(frame.height, height);
    let started = Instant::now();
    let host_cells = frame.cells.iter().cloned().collect::<Vec<_>>();
    std::hint::black_box(&host_cells);
    let copy = started.elapsed();
    assert_eq!(host_cells.len(), usize::from(width) * usize::from(height));
    assert!(host_cells.iter().any(|cell| cell.symbol != " "));
    let total = total_started.elapsed();
    let rss = peak_rss_bytes();

    println!(
        "MEASURE\t{identity}\t{width}\t{height}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{rss}",
        args[4],
        args[5],
        construct.as_nanos(),
        open.as_nanos(),
        render.as_nanos(),
        copy.as_nanos(),
        total.as_nanos()
    );
    if let Some(name) = args.get(6) {
        let elapsed = trigger(&mut editor, name, &path, &text, width, height)?;
        println!("TRIGGER\t{identity}\t{name}\t{elapsed}");
    }
    fs::remove_file(&path)?;
    fs::remove_dir(&base)?;
    Ok(())
}
