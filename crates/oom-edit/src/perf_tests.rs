//! Private TUI performance harness used only by make-owned test commands.

use std::time::{Duration, Instant};

use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use oom_edit_core::{EditorSession, RecordingClipboardSink};
use ratatui::backend::TestBackend;
use ratatui::Terminal;

use crate::app::{App, AppServices, AppStartupOptions};
use crate::config::{ClipboardCopyFormat, DisabledConfigStore};
use crate::spell_host::SpellHost;
use crate::theme::{ResolvedTheme, ThemeCatalog, Tier};

#[path = "../../oom-edit-core/perf/fixtures.rs"]
mod fixtures;

const WIDTH: u16 = 100;
const HEIGHT: u16 = 41;
const FIXTURE_VERSION: &str = "oom-edit-tui-v2";
const MARKER_DOCUMENT_LINES: usize = 50_100;
const SPARSE_MARKER_LINES: [usize; 4] = [0, 7, 20, 39];

fn source_fixture_bytes() -> usize {
    if cfg!(debug_assertions) {
        64 * 1024
    } else {
        1024 * 1024
    }
}

fn rendered_fixture_lines() -> usize {
    if cfg!(debug_assertions) {
        500
    } else {
        5_000
    }
}

fn minimum_measured() -> Duration {
    if cfg!(debug_assertions) {
        Duration::from_millis(25)
    } else {
        Duration::from_millis(250)
    }
}

#[derive(Clone, Copy, Debug)]
struct Measurement {
    iterations: u64,
    measured: Duration,
    average: Duration,
    worst: Duration,
    snapshot_heap_bytes: usize,
}

fn test_app(text: &str, spell_enabled: bool) -> App {
    App::new_with_spell(
        EditorSession::from_text(text),
        ThemeCatalog::builtins(),
        ResolvedTheme::injected("default-dark", false, Tier::TrueColor),
        AppStartupOptions::new(true, false, ClipboardCopyFormat::Markdown, spell_enabled),
        AppServices::new(
            Box::new(RecordingClipboardSink::default()),
            Box::new(DisabledConfigStore),
            SpellHost::testing("known\ntext\nword\n"),
            std::path::PathBuf::from("/"),
        ),
        Instant::now(),
    )
}

fn key(character: char) -> Event {
    Event::Key(KeyEvent::new(KeyCode::Char(character), KeyModifiers::NONE))
}

fn special(code: KeyCode) -> Event {
    Event::Key(KeyEvent::new(code, KeyModifiers::NONE))
}

fn draw(app: &mut App, terminal: &mut Terminal<TestBackend>) {
    terminal.draw(|frame| app.render(frame)).unwrap();
}

fn measure(mut operation: impl FnMut()) -> Measurement {
    for _ in 0..3 {
        operation();
    }

    let started = Instant::now();
    let mut iterations = 0_u64;
    let mut worst = Duration::ZERO;
    let mut measured = Duration::ZERO;
    while started.elapsed() < minimum_measured() || iterations < 1 {
        let iteration_started = Instant::now();
        operation();
        let elapsed = iteration_started.elapsed();
        measured += elapsed;
        worst = worst.max(elapsed);
        iterations += 1;
    }
    Measurement {
        iterations,
        measured,
        average: measured / u32::try_from(iterations).unwrap_or(u32::MAX),
        worst,
        snapshot_heap_bytes: 0,
    }
}

fn measure_once(operation: impl FnOnce() -> usize) -> Measurement {
    let started = Instant::now();
    let snapshot_heap_bytes = operation();
    let elapsed = started.elapsed();
    Measurement {
        iterations: 1,
        measured: elapsed,
        average: elapsed,
        worst: elapsed,
        snapshot_heap_bytes,
    }
}

fn source_render() -> Measurement {
    let text = fixtures::seeded_markdown_fixture(source_fixture_bytes(), 0x0a11_ce01);
    let mut app = test_app(&text, false);
    app.handle_event(&key('i'));
    let mut terminal = Terminal::new(TestBackend::new(WIDTH, HEIGHT)).unwrap();
    measure(|| draw(&mut app, &mut terminal))
}

fn rendered_render() -> Measurement {
    let text = fixtures::seeded_rendered_fixture(rendered_fixture_lines(), 0x0a11_ce02);
    let mut app = test_app(&text, false);
    let mut terminal = Terminal::new(TestBackend::new(WIDTH, HEIGHT)).unwrap();
    measure(|| draw(&mut app, &mut terminal))
}

fn gutter_render(rendered: bool, source_lines: &[usize]) -> Measurement {
    let text = "word\n".repeat(MARKER_DOCUMENT_LINES);
    let mut app = test_app(&text, false);
    if !rendered {
        app.handle_event(&key('i'));
    }
    app.performance_set_gutter_snapshot(source_lines);
    let snapshot_heap_bytes = app.performance_gutter_snapshot_heap_bytes();
    let mut terminal = Terminal::new(TestBackend::new(WIDTH, HEIGHT)).unwrap();
    draw(&mut app, &mut terminal);
    let mut measurement = measure(|| draw(&mut app, &mut terminal));
    measurement.snapshot_heap_bytes = snapshot_heap_bytes;
    measurement
}

fn gutter_sparse(rendered: bool) -> Measurement {
    gutter_render(rendered, &SPARSE_MARKER_LINES)
}

fn gutter_dense(rendered: bool, marked_lines: usize) -> Measurement {
    let source_lines = (0..marked_lines).collect::<Vec<_>>();
    gutter_render(rendered, &source_lines)
}

fn drain_background_work(app: &mut App) {
    for _ in 0..100_000 {
        let worked = app.on_idle_unit(crate::event::SPELL_WORK_UNIT_BYTES);
        if app.spell_host_phase() == "Ready"
            && !app.performance_diagnostics_pending()
            && !app.gutter_projection_pending()
            && !worked
        {
            return;
        }
    }
    panic!("performance fixture background work did not become quiescent");
}

fn gutter_projection(cancel: bool) -> Measurement {
    let mut app = test_app(&"misspelledd\n".repeat(5_000), true);
    drain_background_work(&mut app);
    assert_eq!(app.gutter_snapshot_len(), 5_000);
    let snapshot_heap_bytes = app.performance_gutter_snapshot_heap_bytes();
    let mut measurement = if cancel {
        measure(|| {
            app.performance_restart_gutter_projection();
            app.performance_cancel_gutter_projection();
            assert!(!app.gutter_projection_pending());
        })
    } else {
        measure(|| {
            app.performance_restart_gutter_projection();
            assert!(app.on_idle_unit(crate::event::SPELL_WORK_UNIT_BYTES));
            assert!(app.gutter_projection_pending());
        })
    };
    measurement.snapshot_heap_bytes = snapshot_heap_bytes;
    measurement
}

fn source_first_frame() -> Measurement {
    let text = fixtures::seeded_markdown_fixture(source_fixture_bytes(), 0x0a11_ce03);
    measure_once(|| {
        let mut app = test_app(&text, false);
        app.handle_event(&key('i'));
        let mut terminal = Terminal::new(TestBackend::new(WIDTH, HEIGHT)).unwrap();
        draw(&mut app, &mut terminal);
        0
    })
}

fn rendered_first_frame() -> Measurement {
    let text = fixtures::seeded_markdown_fixture(source_fixture_bytes(), 0x0a11_ce03);
    measure_once(|| {
        let mut app = test_app(&text, false);
        let mut terminal = Terminal::new(TestBackend::new(WIDTH, HEIGHT)).unwrap();
        draw(&mut app, &mut terminal);
        app.performance_rendered_layout_heap_bytes()
    })
}

fn edit_to_frame() -> Measurement {
    let text = fixtures::seeded_markdown_fixture(source_fixture_bytes(), 0x0a11_ce04);
    let mut app = test_app(&text, false);
    app.handle_event(&key('i'));
    let mut terminal = Terminal::new(TestBackend::new(WIDTH, HEIGHT)).unwrap();
    measure(|| {
        app.handle_event(&key('x'));
        draw(&mut app, &mut terminal);
        app.handle_event(&special(KeyCode::Backspace));
    })
}

fn source_scroll_frame() -> Measurement {
    let text = fixtures::seeded_markdown_fixture(source_fixture_bytes(), 0x0a11_ce05);
    let mut app = test_app(&text, false);
    app.handle_event(&key('i'));
    let mut terminal = Terminal::new(TestBackend::new(WIDTH, HEIGHT)).unwrap();
    measure(|| {
        app.handle_event(&special(KeyCode::Down));
        draw(&mut app, &mut terminal);
        app.handle_event(&special(KeyCode::Up));
    })
}

fn idle_quiescent() -> Measurement {
    let mut app = test_app("known text\n", false);
    measure(|| assert!(!app.on_idle_unit(crate::event::SPELL_WORK_UNIT_BYTES)))
}

fn run_case(case: &str) -> Measurement {
    match case {
        "source-render-empty" => source_render(),
        "rendered-render-empty" => rendered_render(),
        "source-gutter-sparse" => gutter_sparse(false),
        "rendered-gutter-sparse" => gutter_sparse(true),
        "source-gutter-dense-5000" => gutter_dense(false, 5_000),
        "source-gutter-dense-50000" => gutter_dense(false, 50_000),
        "rendered-gutter-dense-5000" => gutter_dense(true, 5_000),
        "rendered-gutter-dense-50000" => gutter_dense(true, 50_000),
        "idle-gutter-project" => gutter_projection(false),
        "idle-gutter-cancel" => gutter_projection(true),
        "source-first-frame" => source_first_frame(),
        "rendered-first-frame" => rendered_first_frame(),
        "edit-frame" => edit_to_frame(),
        "source-scroll-frame" => source_scroll_frame(),
        "idle-quiescent" => idle_quiescent(),
        other => panic!("unknown TUI performance case: {other}"),
    }
}

fn absolute_status(case: &str, measurement: Measurement) -> &'static str {
    absolute_status_for_profile(case, measurement, cfg!(debug_assertions))
}

fn absolute_status_for_profile(
    case: &str,
    measurement: Measurement,
    debug_assertions: bool,
) -> &'static str {
    let limit = if debug_assertions {
        match case {
            "source-first-frame" | "rendered-first-frame" => Duration::from_secs(5),
            "edit-frame" | "source-scroll-frame" => Duration::from_secs(2),
            "source-render-empty"
            | "rendered-render-empty"
            | "source-gutter-sparse"
            | "rendered-gutter-sparse"
            | "source-gutter-dense-5000"
            | "source-gutter-dense-50000"
            | "rendered-gutter-dense-5000"
            | "rendered-gutter-dense-50000" => Duration::from_secs(2),
            "idle-quiescent" | "idle-gutter-project" | "idle-gutter-cancel" => {
                Duration::from_millis(10)
            }
            _ => return "fail-unknown-case",
        }
    } else {
        match case {
            "source-first-frame" => Duration::from_millis(150),
            "rendered-first-frame" => Duration::from_millis(350),
            "edit-frame" | "source-scroll-frame" => Duration::from_millis(50),
            "source-render-empty"
            | "rendered-render-empty"
            | "source-gutter-sparse"
            | "rendered-gutter-sparse"
            | "source-gutter-dense-5000"
            | "source-gutter-dense-50000"
            | "rendered-gutter-dense-5000"
            | "rendered-gutter-dense-50000" => Duration::from_millis(50),
            "idle-quiescent" | "idle-gutter-project" | "idle-gutter-cancel" => {
                Duration::from_millis(1)
            }
            _ => return "fail-unknown-case",
        }
    };
    if measurement.worst >= limit {
        return "fail-absolute-limit";
    }
    let unique_marked_lines = match case {
        "source-gutter-sparse" | "rendered-gutter-sparse" => Some(SPARSE_MARKER_LINES.len()),
        "source-gutter-dense-5000" | "rendered-gutter-dense-5000" => Some(5_000),
        "source-gutter-dense-50000" | "rendered-gutter-dense-50000" => Some(50_000),
        _ => None,
    };
    if unique_marked_lines
        .is_some_and(|lines| measurement.snapshot_heap_bytes > lines * 24 + 4 * 1024)
    {
        return "fail-snapshot-memory-limit";
    }
    "pass"
}

#[test]
#[ignore = "run by make bench-check"]
fn tui_gutter_debug_performance_smoke() {
    for case in [
        "source-render-empty",
        "rendered-render-empty",
        "source-gutter-sparse",
        "rendered-gutter-sparse",
        "source-gutter-dense-5000",
        "source-gutter-dense-50000",
        "rendered-gutter-dense-5000",
        "rendered-gutter-dense-50000",
        "idle-gutter-project",
        "idle-gutter-cancel",
        "source-first-frame",
        "rendered-first-frame",
        "edit-frame",
        "source-scroll-frame",
        "idle-quiescent",
    ] {
        let measured = run_case(case);
        eprintln!(
            "{case}: average {:?}, worst {:?}, {} iterations",
            measured.average, measured.worst, measured.iterations
        );
        assert_eq!(absolute_status(case, measured), "pass", "{case}");
    }

    let mut app = test_app("one\ntwo\nthree\n", false);
    app.handle_event(&key('i'));
    let mut terminal = Terminal::new(TestBackend::new(WIDTH, HEIGHT)).unwrap();
    draw(&mut app, &mut terminal);
    let app_shape = app.performance_state_shape();
    let buffer_cells = terminal.backend().buffer().content().len();
    for _ in 0..100 {
        draw(&mut app, &mut terminal);
    }
    assert_eq!(app.performance_state_shape(), app_shape);
    assert_eq!(terminal.backend().buffer().content().len(), buffer_cells);
    assert!(!app.on_idle_unit(crate::event::SPELL_WORK_UNIT_BYTES));
}

#[test]
#[ignore = "run by scripts/tui-performance.py"]
fn tui_release_performance_case() {
    let case = std::env::var("OOM_TUI_PERF_CASE")
        .expect("OOM_TUI_PERF_CASE must select one make-owned performance case");
    let measured = run_case(&case);
    let status = absolute_status(&case, measured);
    println!(
        "OOM_TUI_METRIC\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
        FIXTURE_VERSION,
        case,
        measured.iterations,
        measured.measured.as_nanos(),
        measured.average.as_nanos(),
        measured.worst.as_nanos(),
        measured.snapshot_heap_bytes,
        status,
    );
}

#[test]
fn absolute_limits_reject_lowered_thresholds() {
    let failing = Measurement {
        iterations: 1,
        measured: Duration::from_millis(151),
        average: Duration::from_millis(151),
        worst: Duration::from_millis(151),
        snapshot_heap_bytes: 0,
    };
    assert_eq!(
        absolute_status_for_profile("source-first-frame", failing, false),
        "fail-absolute-limit"
    );
    assert_eq!(
        absolute_status_for_profile(
            "rendered-first-frame",
            Measurement {
                worst: Duration::from_millis(351),
                ..failing
            },
            false,
        ),
        "fail-absolute-limit"
    );
    assert_eq!(
        absolute_status_for_profile("unknown", failing, false),
        "fail-unknown-case"
    );
    for (case, limit) in [
        ("source-first-frame", Duration::from_millis(150)),
        ("rendered-first-frame", Duration::from_millis(350)),
    ] {
        assert_eq!(
            absolute_status_for_profile(
                case,
                Measurement {
                    measured: limit,
                    average: limit,
                    worst: limit,
                    ..failing
                },
                false,
            ),
            "fail-absolute-limit"
        );
    }

    let marker_limit = SPARSE_MARKER_LINES.len() * 24 + 4 * 1024;
    let passing_marker = Measurement {
        measured: Duration::from_millis(49),
        average: Duration::from_millis(49),
        worst: Duration::from_millis(49),
        snapshot_heap_bytes: marker_limit,
        ..failing
    };
    assert_eq!(
        absolute_status_for_profile("source-gutter-sparse", passing_marker, false),
        "pass"
    );
    assert_eq!(
        absolute_status_for_profile(
            "source-gutter-sparse",
            Measurement {
                snapshot_heap_bytes: marker_limit + 1,
                ..passing_marker
            },
            false,
        ),
        "fail-snapshot-memory-limit"
    );
    assert_eq!(
        absolute_status_for_profile(
            "idle-gutter-project",
            Measurement {
                measured: Duration::from_millis(1),
                average: Duration::from_millis(1),
                worst: Duration::from_millis(1),
                snapshot_heap_bytes: 0,
                ..failing
            },
            false,
        ),
        "fail-absolute-limit"
    );
}

#[test]
fn performance_contract_docs_scripts_and_make_targets_stay_synchronized() {
    let readme = include_str!("../../../README.md");
    let documentation = include_str!("../../../docs/performance.md");
    let makefile = include_str!("../../../Makefile");
    let evidence_tool = include_str!("../../../scripts/tui_performance.py");

    assert!(readme.contains("[docs/performance.md](docs/performance.md)"));
    for contract in [
        "less than 150 ms worst case",
        "less than 250 ms worst",
        "less than 350 ms worst",
        "at most 64 MiB",
        "at most 192 MiB",
        "2.25 times",
        "1 MiB",
        "96 text columns",
        FIXTURE_VERSION,
        "at most 24 bytes per unique marked line plus 4 KiB",
        "at most 25%",
        "below 1 ms worst in release",
    ] {
        assert!(
            documentation.contains(contract),
            "performance documentation is missing {contract:?}"
        );
    }
    for case in [
        "source-render-empty",
        "rendered-render-empty",
        "source-gutter-sparse",
        "rendered-gutter-sparse",
        "source-gutter-dense-5000",
        "source-gutter-dense-50000",
        "rendered-gutter-dense-5000",
        "rendered-gutter-dense-50000",
        "idle-gutter-project",
        "idle-gutter-cancel",
        "source-first-frame",
        "rendered-first-frame",
        "edit-frame",
        "source-scroll-frame",
        "idle-quiescent",
    ] {
        assert!(documentation.contains(&format!("`{case}`")), "{case}");
        assert!(evidence_tool.contains(&format!("\"{case}\"")), "{case}");
    }
    assert!(evidence_tool.contains(&format!("FIXTURE_VERSION = \"{FIXTURE_VERSION}\"")));
    assert!(evidence_tool.contains("RENDERED_FIRST_FRAME_RSS_LIMIT = 192 * 1024 * 1024"));

    for (target, description) in [
        ("bench", "Run exact asserting release performance gates"),
        ("bench-check", "Run asserting debug performance smoke gates"),
        (
            "tui-perf-record",
            "Record TUI performance TSV (BRANCH_ROLE, OUTPUT, TRIALS)",
        ),
        (
            "tui-perf-compare",
            "Compare baseline/candidate TUI TSV evidence",
        ),
    ] {
        assert!(
            makefile.contains(&format!("{target}: ## {description}")),
            "make help metadata for {target} drifted"
        );
        assert!(
            documentation.contains(&format!("make {target}")),
            "{target}"
        );
    }
}
