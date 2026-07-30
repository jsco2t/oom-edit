//! Hand-rolled release benchmarks for NFR-1..NFR-4.
//!
//! Run through `make bench`; every row reports its fixture, iterations,
//! average, worst observation, and exact documented target.

#[path = "../perf/fixtures.rs"]
mod fixtures;

use std::time::{Duration, Instant};

use oom_edit_core::{EditorSession, KeyCode, KeyCodeKind, KeyInput, Mode, Modifiers, Viewport};
use oom_spell::{BuildProgress, SpellEngine, SpellEngineBuilder};

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

    fn record(&mut self, observed: Duration) {
        self.total += observed;
        self.worst = self.worst.max(observed);
        self.iterations += 1;
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

fn sampled_run(samples: u32, mut operation: impl FnMut() -> Duration) -> Stats {
    assert!(samples > 1);
    let _ = operation();

    let mut stats = Stats {
        total: Duration::ZERO,
        worst: Duration::ZERO,
        iterations: 0,
    };
    for _ in 0..samples {
        stats.record(operation());
    }
    stats
}

fn measure(operation: impl FnOnce()) -> Duration {
    let started = Instant::now();
    operation();
    started.elapsed()
}

fn format_duration(duration: Duration) -> String {
    let millis = duration.as_secs_f64() * 1_000.0;
    if millis < 1.0 {
        format!("{:.2}µs", millis * 1_000.0)
    } else {
        format!("{millis:.2}ms")
    }
}

fn duration_gate_passes(observed: Duration, target: Duration) -> bool {
    observed < target
}

fn report(name: &str, target: Duration, bytes: usize, lines: usize, stats: Stats) {
    println!(
        "{name}: {bytes} bytes, {lines} lines, {} iterations, avg {}, worst {}, target <{}",
        stats.iterations,
        format_duration(stats.average()),
        format_duration(stats.worst),
        format_duration(target),
    );
    assert!(!stats.worst.is_zero(), "{name} measured no work");
    let lowered = stats.worst.saturating_sub(Duration::from_nanos(1));
    assert!(
        !duration_gate_passes(stats.worst, lowered),
        "{name} must reject its intentionally lowered threshold {}",
        format_duration(lowered)
    );
    assert!(
        duration_gate_passes(stats.worst, target),
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

fn spell_engine() -> SpellEngine {
    let mut builder = SpellEngineBuilder::new(vec!["hello\nworld\ngood\ntext\n".to_string()]);
    while builder.step(4096) != BuildProgress::Complete {}
    builder.finish().unwrap()
}

fn spell_fixture_1mb() -> String {
    let row = "hello world good text `ignored misspeling`\n";
    let mut text = row.repeat(ONE_MIB / row.len() + 1);
    text.truncate(ONE_MIB);
    text
}

fn spell_dirty_fixture_1mb() -> String {
    let mut text = "[hello](https://example.invalid/path)\n`ignored`\n".to_string();
    let stress = "wrng `ignored`\n".repeat(1_000);
    let ending = format!("\nhelo\n{stress}");
    let row = "hello world good text\n";
    while text.len() + row.len() + ending.len() <= ONE_MIB {
        text.push_str(row);
    }
    text.push_str(&" ".repeat(ONE_MIB - text.len() - ending.len()));
    text.push_str(&ending);
    text
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

fn benchmark_spell() {
    let engine = spell_engine();
    let document = spell_fixture_1mb();
    let bytes = document.len();
    let lines = document.matches('\n').count() + 1;
    let tick = sampled_run(20, || {
        let mut session = EditorSession::from_text(&document);
        let started = Instant::now();
        assert!(session.spell_tick(&engine, 4096));
        started.elapsed()
    });
    report(
        "NFR-10 spell_tick_4k",
        Duration::from_millis(2),
        bytes,
        lines,
        tick,
    );

    let full = sampled_run(5, || {
        let mut session = EditorSession::from_text(&document);
        let started = Instant::now();
        while session.diagnostics_pending() {
            assert!(session.spell_tick(&engine, 4096));
        }
        started.elapsed()
    });
    report(
        "NFR-10 spell_full_scan_1mb",
        Duration::from_millis(300),
        bytes,
        lines,
        full,
    );

    let pathological = "hello ".repeat(ONE_MIB / 6);
    let pathological_tick = sampled_run(20, || {
        let mut session = EditorSession::from_text(&pathological);
        let started = Instant::now();
        assert!(session.spell_tick(&engine, 4096));
        started.elapsed()
    });
    report(
        "NFR-10 pathological_single_line_tick_4k",
        Duration::from_millis(2),
        pathological.len(),
        1,
        pathological_tick,
    );

    let dirty_document = spell_dirty_fixture_1mb();
    let dirty = sampled_run(8, || {
        const PAIRS: u32 = 16;
        let edit_offset = dirty_document.rfind("helo").unwrap();
        let mut pairs = Vec::new();
        for _ in 0..PAIRS {
            let mut baseline = EditorSession::from_text(&dirty_document);
            baseline.jump_to_offset(edit_offset).unwrap();
            enter_insert(&mut baseline);

            let mut session = EditorSession::from_text(&dirty_document);
            while session.diagnostics_pending() {
                assert!(session.spell_tick(&engine, 4096));
            }
            assert_eq!(session.diagnostics().len(), 1_001);
            session.jump_to_offset(edit_offset).unwrap();
            enter_insert(&mut session);
            let last_range = session.diagnostics().last().unwrap().range.clone();
            pairs.push((baseline, session, last_range));
        }

        let mut baseline_total = Duration::ZERO;
        let mut dirty_total = Duration::ZERO;
        for (index, (baseline, session, last_range)) in pairs.iter_mut().enumerate() {
            if index % 2 == 0 {
                baseline_total += measure(|| {
                    baseline.insert_paste("x");
                });
                dirty_total += measure(|| {
                    session.insert_paste("x");
                });
            } else {
                dirty_total += measure(|| {
                    session.insert_paste("x");
                });
                baseline_total += measure(|| {
                    baseline.insert_paste("x");
                });
            }
            assert_eq!(session.diagnostics().len(), 1_000);
            assert_eq!(session.diagnostics()[0].source_text, "wrng");
            assert_eq!(
                session.diagnostics().last().unwrap().range,
                last_range.start + 1..last_range.end + 1
            );
        }
        dirty_total.saturating_sub(baseline_total) / PAIRS
    });
    report(
        "NFR-10 spell_dirty_mark_and_shift",
        Duration::from_micros(100),
        dirty_document.len(),
        dirty_document.matches('\n').count() + 1,
        dirty,
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
    benchmark_spell();
}
