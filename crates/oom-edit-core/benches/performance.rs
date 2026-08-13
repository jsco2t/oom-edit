//! Hand-rolled release benchmarks for NFR-1..NFR-4.
//!
//! Run through `make bench`; every row reports its fixture, iterations,
//! average, worst observation, and exact documented target.

#[path = "../perf/fixtures.rs"]
mod fixtures;

use std::time::{Duration, Instant};

use oom_edit_core::{EditorSession, KeyCode, KeyCodeKind, KeyInput, Mode, Modifiers, Viewport};

const ONE_MIB: usize = 1024 * 1024;
const FIXTURE_SEED: u64 = 0x00_0D_D1_7E;

#[derive(Clone, Copy)]
struct Stats {
    total: Duration,
    worst: Duration,
    iterations: u32,
}

impl Stats {
    fn average(self) -> Duration {
        self.total / self.iterations
    }
}

fn bench_run(duration: Duration, mut operation: impl FnMut()) -> Stats {
    let warmup_started = Instant::now();
    while warmup_started.elapsed() < Duration::from_millis(50) {
        operation();
    }

    let mut stats = Stats {
        total: Duration::ZERO,
        worst: Duration::ZERO,
        iterations: 0,
    };
    while stats.total < duration || stats.iterations == 0 {
        let started = Instant::now();
        operation();
        let observed = started.elapsed();
        stats.total += observed;
        stats.worst = stats.worst.max(observed);
        stats.iterations += 1;
    }
    stats
}

fn format_duration(duration: Duration) -> String {
    let millis = duration.as_secs_f64() * 1_000.0;
    if millis < 1.0 {
        format!("{:.2}µs", millis * 1_000.0)
    } else {
        format!("{millis:.2}ms")
    }
}

fn report(name: &str, target: Duration, bytes: usize, lines: usize, stats: Stats) {
    println!(
        "{name}: {bytes} bytes, {lines} lines, {} iterations, avg {}, worst {}, target <{}",
        stats.iterations,
        format_duration(stats.average()),
        format_duration(stats.worst),
        format_duration(target),
    );
    assert!(
        stats.worst < target,
        "{name} worst {} exceeded target {}",
        format_duration(stats.worst),
        format_duration(target)
    );
}

fn source_fixture_1mb() -> String {
    fixtures::seeded_markdown_fixture(ONE_MIB, FIXTURE_SEED)
}

fn rendered_5000_line_fixture() -> String {
    fixtures::seeded_rendered_fixture(5_000, FIXTURE_SEED)
}

fn injection_heavy_doc() -> String {
    let mut document = String::new();
    for index in 0..10 {
        for (language, body) in [
            ("rust", format!("fn rust_{index}() {{}}")),
            ("python", format!("print({index})")),
            ("yaml", format!("value: {index}")),
            ("toml", format!("value = {index}")),
            ("javascript", format!("const value{index} = {index};")),
        ] {
            document.push_str(&format!("```{language}\n{body}\n```\n\n"));
        }
    }
    document
}

fn key(ch: char) -> KeyInput {
    KeyInput {
        code: KeyCode {
            kind: KeyCodeKind::Char(ch),
        },
        mods: Modifiers::default(),
    }
}

fn special(kind: KeyCodeKind) -> KeyInput {
    KeyInput {
        code: KeyCode { kind },
        mods: Modifiers::default(),
    }
}

fn enter_insert(session: &mut EditorSession) {
    session.handle_key(key('i'));
    assert_eq!(session.mode(), Mode::Insert);
}

fn viewport(top_line: usize) -> Viewport {
    Viewport {
        top_line,
        height: 40,
        width: 100,
        wrap: true,
        left_col: 0,
        skip_rows: 0,
    }
}

fn benchmark_open_to_first_frame(document: &str) {
    let lines = document.matches('\n').count() + 1;
    let stats = bench_run(Duration::from_millis(250), || {
        let mut session = EditorSession::from_text(document);
        assert_eq!(session.mode(), Mode::Normal);
        // T18 defines NFR-1 as construction plus the first fully highlighted
        // 80x40 source frame, independent of the interactive surface.
        let _ = session.render_source(viewport(0));
    });
    report(
        "NFR-1 open_to_first_frame",
        Duration::from_millis(150),
        document.len(),
        lines,
        stats,
    );
}

fn benchmark_insert_cursor_and_source_frame(document: &str) {
    let mut session = EditorSession::from_text(document);
    enter_insert(&mut session);
    let middle = session.line_count() / 2;
    for _ in 0..middle {
        session.handle_key(special(KeyCodeKind::Down));
    }
    let mut down = true;
    let cursor = bench_run(Duration::from_millis(250), || {
        session.handle_key(special(if down {
            KeyCodeKind::Down
        } else {
            KeyCodeKind::Up
        }));
        down = !down;
    });
    report(
        "NFR-2 insert_cursor",
        Duration::from_millis(16),
        document.len(),
        session.line_count(),
        cursor,
    );

    let mut top = middle;
    let frame = bench_run(Duration::from_millis(250), || {
        let _ = session.render_source(viewport(top));
        top = top.saturating_add(1);
    });
    report(
        "NFR-2 source_scroll_frame",
        Duration::from_millis(50),
        document.len(),
        session.line_count(),
        frame,
    );
}

fn benchmark_incremental_edit(document: &str, name: &str, target_line: usize) {
    let mut session = EditorSession::from_text(document);
    enter_insert(&mut session);
    for _ in 0..target_line {
        session.handle_key(special(KeyCodeKind::Down));
    }
    assert_eq!(session.mode(), Mode::Insert);
    let mut inserting = true;
    let stats = bench_run(Duration::from_millis(250), || {
        session.handle_key(if inserting {
            key('x')
        } else {
            special(KeyCodeKind::Backspace)
        });
        inserting = !inserting;
    });
    report(
        name,
        Duration::from_millis(10),
        document.len(),
        session.line_count(),
        stats,
    );
}

fn benchmark_edit_to_frame(document: &str) {
    let mut session = EditorSession::from_text(document);
    enter_insert(&mut session);
    let middle = session.line_count() / 2;
    for _ in 0..middle {
        session.handle_key(special(KeyCodeKind::Down));
    }
    let mut inserting = true;
    let stats = bench_run(Duration::from_millis(250), || {
        session.handle_key(if inserting {
            key('x')
        } else {
            special(KeyCodeKind::Backspace)
        });
        let _ = session.render_source(viewport(middle));
        inserting = !inserting;
    });
    report(
        "NFR-2 insert_edit_to_frame",
        Duration::from_millis(50),
        document.len(),
        session.line_count(),
        stats,
    );
}

fn benchmark_injection_heavy_edit() {
    let document = injection_heavy_doc();
    let mut session = EditorSession::from_text(&document);
    enter_insert(&mut session);
    let mut inserting = true;
    let stats = bench_run(Duration::from_millis(250), || {
        session.handle_key(if inserting {
            key('x')
        } else {
            special(KeyCodeKind::Backspace)
        });
        inserting = !inserting;
    });
    report(
        "NFR-3 injection_heavy_edit",
        Duration::from_millis(10),
        document.len(),
        document.matches('\n').count() + 1,
        stats,
    );
}

fn benchmark_rendered(document: &str) {
    let lines = document.matches('\n').count() + 1;
    assert_eq!(lines, 5_000);

    let mut cold_session = EditorSession::from_text(document);
    assert_eq!(cold_session.mode(), Mode::Normal);
    let started = Instant::now();
    cold_session.render_layout(100);
    let cold = started.elapsed();
    report(
        "NFR-4 rendered_cold_build",
        Duration::from_millis(100),
        document.len(),
        lines,
        Stats {
            total: cold,
            worst: cold,
            iterations: 1,
        },
    );

    let started = Instant::now();
    cold_session.render_layout(72);
    let resize = started.elapsed();
    report(
        "NFR-4 rendered_resize_rebuild",
        Duration::from_millis(100),
        document.len(),
        lines,
        Stats {
            total: resize,
            worst: resize,
            iterations: 1,
        },
    );

    for digit in "2500".chars() {
        cold_session.handle_key(key(digit));
    }
    cold_session.handle_key(key('G'));
    assert!(cold_session.rendered_cursor_line() > 1_000);
    let mut down = true;
    let navigation = bench_run(Duration::from_millis(250), || {
        cold_session.handle_key(key(if down { 'j' } else { 'k' }));
        cold_session.render_layout(72);
        down = !down;
    });
    report(
        "NFR-2 rendered_mid_file_navigation",
        Duration::from_millis(16),
        document.len(),
        lines,
        navigation,
    );
}

fn main() {
    let source = source_fixture_1mb();
    let line_count = source.matches('\n').count() + 1;
    benchmark_open_to_first_frame(&source);
    benchmark_insert_cursor_and_source_frame(&source);
    benchmark_incremental_edit(&source, "NFR-3 edit_beginning", 0);
    benchmark_incremental_edit(&source, "NFR-3 edit_middle", line_count / 2);
    benchmark_incremental_edit(&source, "NFR-3 edit_end", line_count.saturating_sub(1));
    benchmark_edit_to_frame(&source);
    benchmark_injection_heavy_edit();
    benchmark_rendered(&rendered_5000_line_fixture());
}
