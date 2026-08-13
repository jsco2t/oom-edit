//! Asserting release-profile benchmarks for the engine-owned performance budgets.

#![allow(unsafe_code)]

use oom_spell::{BuildProgress, SpellEngine, SpellEngineBuilder, MAX_CHECKED_WORD_BYTES};
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

const SYNTHETIC_UNIQUE_WORDS: usize = 110_000;
const CHECK_ITERATIONS: u32 = 100_000;
const CHECK_SAMPLES: usize = 10;
const BUILD_SAMPLES: usize = 5;
const SUGGEST_SAMPLES: usize = 20;
const MAX_ENGINE_HEAP: usize = 25 * 1024 * 1024;
const MAX_SUGGEST_HEAP: usize = 16 * 1024;

struct CountingAllocator;

static LIVE_BYTES: AtomicUsize = AtomicUsize::new(0);
static PEAK_BYTES: AtomicUsize = AtomicUsize::new(0);

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc(layout) };
        if !pointer.is_null() {
            record_allocation(layout.size());
        }
        pointer
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { System.dealloc(pointer, layout) };
        LIVE_BYTES.fetch_sub(layout.size(), Ordering::Relaxed);
    }

    unsafe fn realloc(&self, pointer: *mut u8, old: Layout, new_size: usize) -> *mut u8 {
        let new_pointer = unsafe { System.realloc(pointer, old, new_size) };
        if !new_pointer.is_null() {
            if new_size >= old.size() {
                record_allocation(new_size - old.size());
            } else {
                LIVE_BYTES.fetch_sub(old.size() - new_size, Ordering::Relaxed);
            }
        }
        new_pointer
    }
}

fn record_allocation(bytes: usize) {
    let current = LIVE_BYTES.fetch_add(bytes, Ordering::Relaxed) + bytes;
    let mut peak = PEAK_BYTES.load(Ordering::Relaxed);
    while current > peak {
        match PEAK_BYTES.compare_exchange_weak(peak, current, Ordering::Relaxed, Ordering::Relaxed)
        {
            Ok(_) => break,
            Err(observed) => peak = observed,
        }
    }
}

fn encoded_word(mut value: usize, len: usize) -> String {
    let mut bytes = vec![b'a'; len];
    for byte in bytes.iter_mut().rev() {
        *byte = b'a' + (value % 26) as u8;
        value /= 26;
    }
    String::from_utf8(bytes).expect("generated words are ASCII")
}

fn synthetic_dialect() -> String {
    let mut list = String::with_capacity(SYNTHETIC_UNIQUE_WORDS * 10);
    for index in 0..SYNTHETIC_UNIQUE_WORDS {
        let length = 4 + index % 11;
        list.push_str(&encoded_word(index / 11, length));
        list.push('\n');
    }
    list.push_str("the\n");
    list.push_str(&"a".repeat(MAX_CHECKED_WORD_BYTES));
    list.push('\n');
    list
}

fn single_length_dialect() -> String {
    let mut list = String::with_capacity(SYNTHETIC_UNIQUE_WORDS * 9);
    for index in (0..SYNTHETIC_UNIQUE_WORDS).rev() {
        list.push_str(&encoded_word(index, 8));
        list.push('\n');
    }
    list
}

fn duration_gate_passes(observed: Duration, target: Duration) -> bool {
    observed < target
}

fn memory_gate_passes(observed: usize, target: usize) -> bool {
    observed <= target
}

fn assert_duration_gate(label: &str, observed: Duration, target: Duration) {
    println!("{label}: {observed:?}, target <{target:?}");
    assert!(
        !duration_gate_passes(observed, Duration::ZERO),
        "{label} should reject an intentionally lowered zero threshold"
    );
    assert!(
        duration_gate_passes(observed, target),
        "{label} exceeded its performance target"
    );
}

fn assert_memory_gate(label: &str, observed: usize, target: usize) {
    println!("{label}: {observed} bytes, target <={target}");
    assert!(
        !memory_gate_passes(observed, 0),
        "{label} should reject a lowered zero threshold"
    );
    assert!(
        memory_gate_passes(observed, target),
        "{label} exceeded its memory target"
    );
}

fn reset_peak() -> usize {
    let baseline = LIVE_BYTES.load(Ordering::Relaxed);
    PEAK_BYTES.store(baseline, Ordering::Relaxed);
    baseline
}

fn peak_since(baseline: usize) -> usize {
    PEAK_BYTES.load(Ordering::Relaxed).saturating_sub(baseline)
}

fn build_and_measure(lists: Vec<String>) -> (SpellEngine, Duration, Duration) {
    let mut builder = SpellEngineBuilder::new(lists);
    let started = Instant::now();
    let mut worst_step = Duration::ZERO;
    loop {
        let step_started = Instant::now();
        let progress = builder.step(4 * 1024);
        worst_step = worst_step.max(step_started.elapsed());
        if progress == BuildProgress::Complete {
            break;
        }
    }
    let engine = builder.finish().expect("builder should be complete");
    let total = started.elapsed();
    (engine, worst_step, total)
}

fn build_lists(dialect: &str) -> Vec<String> {
    vec![dialect.to_owned(), dialect.to_owned(), dialect.to_owned()]
}

fn dense_suggestion_list() -> String {
    let mut list = String::new();
    let base = b"aaaaaaaa";
    for first in 0..base.len() {
        for first_letter in b'b'..=b'z' {
            let mut candidate = *base;
            candidate[first] = first_letter;
            list.push_str(std::str::from_utf8(&candidate).expect("candidate is ASCII"));
            list.push('\n');
        }
        for second in first + 1..base.len() {
            for first_letter in b'b'..=b'z' {
                for second_letter in b'b'..=b'z' {
                    let mut candidate = *base;
                    candidate[first] = first_letter;
                    candidate[second] = second_letter;
                    list.push_str(std::str::from_utf8(&candidate).expect("candidate is ASCII"));
                    list.push('\n');
                }
            }
        }
    }
    list
}

fn measure_build(dialect: &str) -> (SpellEngine, Duration, Duration, usize) {
    let warm_engine = build_and_measure(build_lists(dialect)).0;
    assert!(warm_engine.word_count() >= SYNTHETIC_UNIQUE_WORDS);
    drop(warm_engine);

    let mut retained_engine = None;
    let mut worst_step = Duration::ZERO;
    let mut worst_total = Duration::ZERO;
    let mut peak_heap = 0;
    for sample in 0..BUILD_SAMPLES {
        let lists = build_lists(dialect);
        let baseline = reset_peak();
        let (engine, sample_step, sample_total) = build_and_measure(lists);
        worst_step = worst_step.max(sample_step);
        worst_total = worst_total.max(sample_total);
        peak_heap = peak_heap.max(peak_since(baseline));
        if sample + 1 == BUILD_SAMPLES {
            retained_engine = Some(engine);
        } else {
            drop(engine);
        }
    }
    (
        retained_engine.expect("one measured engine should be retained"),
        worst_step,
        worst_total,
        peak_heap,
    )
}

fn measure_check(engine: &SpellEngine) -> Duration {
    for _ in 0..10_000 {
        std::hint::black_box(engine.check(std::hint::black_box("the")));
    }
    let mut worst_average = Duration::ZERO;
    for _ in 0..CHECK_SAMPLES {
        let started = Instant::now();
        for index in 0..CHECK_ITERATIONS {
            let word = if index % 2 == 0 { "the" } else { "missing" };
            std::hint::black_box(engine.check(std::hint::black_box(word)));
        }
        worst_average = worst_average.max(started.elapsed() / CHECK_ITERATIONS);
    }
    worst_average
}

fn measure_pathological_step(list: &str) -> Duration {
    let mut worst = Duration::ZERO;
    for _ in 0..BUILD_SAMPLES {
        let mut builder = SpellEngineBuilder::new(vec![list.to_owned()]);
        loop {
            let started = Instant::now();
            let progress = builder.step(4 * 1024);
            worst = worst.max(started.elapsed());
            if progress == BuildProgress::Complete {
                break;
            }
        }
        assert_eq!(
            builder
                .finish()
                .expect("builder should complete")
                .word_count(),
            SYNTHETIC_UNIQUE_WORDS
        );
    }
    worst
}

fn measure_suggestion(engine: &SpellEngine, word: &str) -> (Vec<String>, Duration) {
    for _ in 0..10 {
        std::hint::black_box(engine.suggest(std::hint::black_box(word), 9));
    }
    let mut worst = Duration::ZERO;
    let mut result = Vec::new();
    for _ in 0..SUGGEST_SAMPLES {
        let started = Instant::now();
        result = std::hint::black_box(engine.suggest(std::hint::black_box(word), 9));
        worst = worst.max(started.elapsed());
    }
    (result, worst)
}

fn assert_suggestion_storage_is_capped() {
    let dense_engine = build_and_measure(vec![dense_suggestion_list()]).0;
    assert!(dense_engine.word_count() > 10_000);
    let baseline = reset_peak();
    let suggestions = dense_engine.suggest("aaaaaaaa", 9);
    let peak_heap = peak_since(baseline);
    assert_eq!(suggestions.len(), 9);
    assert_memory_gate(
        "dense suggestion incremental heap",
        peak_heap,
        MAX_SUGGEST_HEAP,
    );
}

fn main() {
    let dialect = synthetic_dialect();
    let (engine, worst_step, total_build, peak_heap) = measure_build(&dialect);

    assert_duration_gate("4 KiB build step", worst_step, Duration::from_millis(2));
    assert_duration_gate(
        "three-list synthetic build",
        total_build,
        Duration::from_millis(150),
    );
    assert_memory_gate("engine peak live heap", peak_heap, MAX_ENGINE_HEAP);
    let pathological_step = measure_pathological_step(&single_length_dialect());
    assert_duration_gate(
        "single-length finalization step",
        pathological_step,
        Duration::from_millis(2),
    );

    let check_average = measure_check(&engine);
    assert_duration_gate("check average", check_average, Duration::from_micros(1));

    let (common, common_elapsed) = measure_suggestion(&engine, "teh");
    assert_eq!(common.first().map(String::as_str), Some("the"));
    assert_duration_gate(
        "common suggestion",
        common_elapsed,
        Duration::from_millis(10),
    );

    let long_miss = format!("{}b", "a".repeat(MAX_CHECKED_WORD_BYTES - 1));
    let (boundary, boundary_elapsed) = measure_suggestion(&engine, &long_miss);
    assert!(!boundary.is_empty());
    assert_duration_gate(
        "64-byte suggestion",
        boundary_elapsed,
        Duration::from_millis(10),
    );
    assert_suggestion_storage_is_capped();
}
