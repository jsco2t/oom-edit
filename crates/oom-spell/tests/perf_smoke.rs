//! Relaxed debug-profile performance guards for the spell engine.

use oom_spell::{BuildProgress, SpellEngine, SpellEngineBuilder, MAX_CHECKED_WORD_BYTES};
use std::time::{Duration, Instant};

const SYNTHETIC_WORDS: usize = 30_000;

fn encoded_word(mut value: usize, len: usize) -> String {
    let mut bytes = vec![b'a'; len];
    for byte in bytes.iter_mut().rev() {
        *byte = b'a' + (value % 26) as u8;
        value /= 26;
    }
    String::from_utf8(bytes).expect("generated words are ASCII")
}

fn synthetic_list(words: usize) -> String {
    let mut list = String::with_capacity(words * 9);
    for index in 0..words {
        list.push_str(&encoded_word(index, 8));
        list.push('\n');
    }
    list.push_str("the\n");
    list.push_str(&"a".repeat(MAX_CHECKED_WORD_BYTES));
    list.push('\n');
    list
}

fn build_engine(list: String) -> SpellEngine {
    let mut builder = SpellEngineBuilder::new(vec![list]);
    while builder.step(4 * 1024) == BuildProgress::Pending {}
    builder.finish().expect("builder should be complete")
}

#[derive(Clone, Copy)]
struct Measurements {
    check_average: Duration,
    worst_step: Duration,
    total_build: Duration,
    common_suggestion: Duration,
    boundary_suggestion: Duration,
}

#[derive(Clone, Copy)]
struct Limits {
    check_average: Duration,
    worst_step: Duration,
    total_build: Duration,
    common_suggestion: Duration,
    boundary_suggestion: Duration,
}

fn gate_failures(measured: Measurements, limits: Limits) -> Vec<&'static str> {
    [
        (
            "check average",
            measured.check_average,
            limits.check_average,
        ),
        ("4 KiB build step", measured.worst_step, limits.worst_step),
        ("synthetic build", measured.total_build, limits.total_build),
        (
            "common suggestion",
            measured.common_suggestion,
            limits.common_suggestion,
        ),
        (
            "64-byte suggestion",
            measured.boundary_suggestion,
            limits.boundary_suggestion,
        ),
    ]
    .into_iter()
    .filter_map(|(label, observed, limit)| (observed >= limit).then_some(label))
    .collect()
}

#[test]
fn debug_engine_performance_smoke() {
    let list = synthetic_list(SYNTHETIC_WORDS);
    let mut builder = SpellEngineBuilder::new(vec![list]);
    let mut worst_step = Duration::ZERO;
    let build_started = Instant::now();
    loop {
        let step_started = Instant::now();
        let progress = builder.step(4 * 1024);
        worst_step = worst_step.max(step_started.elapsed());
        if progress == BuildProgress::Complete {
            break;
        }
    }
    let engine = builder.finish().expect("builder should be complete");
    let build_elapsed = build_started.elapsed();

    let check_started = Instant::now();
    for index in 0..100_000 {
        let word = if index % 2 == 0 { "the" } else { "missing" };
        std::hint::black_box(engine.check(std::hint::black_box(word)));
    }
    let check_average = check_started.elapsed() / 100_000;

    let common_started = Instant::now();
    let _ = std::hint::black_box(engine.suggest("teh", 9));
    let common = common_started.elapsed();
    let long = format!("{}b", "a".repeat(MAX_CHECKED_WORD_BYTES - 1));
    let boundary_started = Instant::now();
    let _ = std::hint::black_box(engine.suggest(&long, 9));
    let boundary = boundary_started.elapsed();

    let measured = Measurements {
        check_average,
        worst_step,
        total_build: build_elapsed,
        common_suggestion: common,
        boundary_suggestion: boundary,
    };
    eprintln!(
        "oom-spell debug smoke: check avg {check_average:?}, step worst {worst_step:?}, build {build_elapsed:?}, suggest {common:?}/{boundary:?}"
    );
    let lowered = Limits {
        check_average: Duration::ZERO,
        worst_step: Duration::ZERO,
        total_build: Duration::ZERO,
        common_suggestion: Duration::ZERO,
        boundary_suggestion: Duration::ZERO,
    };
    assert_eq!(
        gate_failures(measured, lowered),
        [
            "check average",
            "4 KiB build step",
            "synthetic build",
            "common suggestion",
            "64-byte suggestion",
        ],
        "every gate must reject its intentionally lowered threshold"
    );
    let restored = Limits {
        check_average: Duration::from_micros(10),
        worst_step: Duration::from_millis(20),
        total_build: Duration::from_secs(2),
        common_suggestion: Duration::from_millis(100),
        boundary_suggestion: Duration::from_millis(100),
    };
    assert!(
        gate_failures(measured, restored).is_empty(),
        "restored debug performance thresholds must pass"
    );
}

#[test]
fn tiny_builder_fixture_is_not_vacuous() {
    let engine = build_engine("the\nword\n".into());
    assert!(engine.check("word"));
}
