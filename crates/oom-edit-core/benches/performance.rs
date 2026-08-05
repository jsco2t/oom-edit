//! Performance benchmarks for oom-edit-core.
//!
//! Measures the four NFR performance budgets (NFR-1..NFR-4) using a
//! hand-rolled harness — no external dependencies.
//!
//! Run with `cargo bench -p oom-edit-core --bench performance`.
//!
//! ## Benchmark groups
//!
//! | Group | NFR | Target | What it measures |
//! |-------|-----|--------|------------------|
//! | `open_to_first_frame` | NFR-1 | <150 ms | `EditorSession::from_text(large)` + `render_source(80x40)` |
//! | `keystroke_to_frame` | NFR-2 | <16 ms typical | `handle_key('x')` + `render_source` |
//! | `incremental_rehighlight` | NFR-3 | <10 ms each | `apply_edit` single-char + render |
//! | `view_build` | NFR-4 | <100 ms | `render_view(80)` cold + rebuild after width change |

use oom_edit_core::session::{EditorSession, KeyCode, KeyCodeKind, KeyInput, Modifiers, Viewport};
use std::time::{Duration, Instant};

// ── Simple benchmark harness ────────────────────────────────────────────────

fn bench_run<F>(duration: Duration, mut f: F) -> (Duration, usize)
where
    F: FnMut(),
{
    let warmup = Duration::from_millis(50);
    let mut start = Instant::now();
    let mut iterations = 0u64;

    while start.elapsed() < warmup {
        f();
        iterations += 1;
    }

    start = Instant::now();
    while start.elapsed() < duration {
        f();
        iterations += 1;
    }

    let elapsed = start.elapsed();
    (elapsed, iterations as usize)
}

fn format_duration(d: Duration) -> String {
    let ms = d.as_secs_f64() * 1000.0;
    if ms < 1.0 {
        format!("{:.2}µs", ms * 1000.0)
    } else if ms < 1000.0 {
        format!("{:.2}ms", ms)
    } else {
        format!("{:.2}s", ms / 1000.0)
    }
}

fn doc() -> String {
    String::from(
        "# Test\n\nSome content.\n\n## Section\n\nMore content.\n\n```rust\nfn main() {}\n```\n",
    )
}

fn bench_open_to_first_frame() {
    let d = doc();
    let (elapsed, iters) = bench_run(Duration::from_millis(100), || {
        let mut s = EditorSession::from_text(&d);
        let _ = s.render_source(Viewport {
            top_line: 0,
            height: 40,
            width: 80,
        });
    });
    let avg = elapsed / iters as u32;
    println!(
        "open_to_first_frame: {} bytes, {} iters, avg {}",
        d.len(),
        iters,
        format_duration(avg)
    );
}

fn bench_keystroke_to_frame() {
    let d = doc();
    let (elapsed, iters) = bench_run(Duration::from_millis(100), || {
        let mut s = EditorSession::from_text(&d);
        let k = KeyInput {
            code: KeyCode {
                kind: KeyCodeKind::Char('x'),
            },
            mods: Modifiers::default(),
        };
        let _ = s.handle_key(k);
        let _ = s.render_source(Viewport {
            top_line: 0,
            height: 40,
            width: 80,
        });
    });
    let avg = elapsed / iters as u32;
    println!(
        "keystroke_to_frame: {} bytes, {} iters, avg {}",
        d.len(),
        iters,
        format_duration(avg)
    );
}

fn bench_incremental_rehighlight() {
    let d = doc();
    let (elapsed, iters) = bench_run(Duration::from_millis(100), || {
        let mut s = EditorSession::from_text(&d);
        let k = KeyInput {
            code: KeyCode {
                kind: KeyCodeKind::Char('x'),
            },
            mods: Modifiers::default(),
        };
        let _ = s.handle_key(k);
        let _ = s.render_source(Viewport {
            top_line: 0,
            height: 40,
            width: 80,
        });
    });
    let avg = elapsed / iters as u32;
    println!(
        "incremental_rehighlight: {} bytes, {} iters, avg {}",
        d.len(),
        iters,
        format_duration(avg)
    );
}

fn bench_view_build() {
    let d = doc();
    let (elapsed, iters) = bench_run(Duration::from_millis(100), || {
        let mut s = EditorSession::from_text(&d);
        let _ = s.render_view(80);
    });
    let avg = elapsed / iters as u32;
    println!(
        "view_build: {} bytes, {} iters, avg {}",
        d.len(),
        iters,
        format_duration(avg)
    );
}

fn main() {
    bench_open_to_first_frame();
    bench_keystroke_to_frame();
    bench_incremental_rehighlight();
    bench_view_build();
}
