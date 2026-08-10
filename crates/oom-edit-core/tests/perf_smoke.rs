//! Performance-smoke assertions for NFR-1..NFR-4.
//!
//! Run with `cargo test --offline --locked -p oom-edit-core --test perf_smoke`.
//! These relaxed regression thresholds do not replace the exact NFR benchmarks.

mod perf_assertions {
    use oom_edit_core::session::{
        EditorSession, KeyCode, KeyCodeKind, KeyInput, Modifiers, Viewport,
    };

    fn large_rendered_document() -> String {
        let mut out = String::new();
        for i in 0..500 {
            out.push_str(&format!(
                "## Heading {}\n\nParagraph {} is a block of prose text that provides\ncontent for the benchmark. It should be long enough to exercise\nword-wrapping and layout logic in the renderer.\n\n",
                i, i
            ));
        }
        out
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
    fn perf_smoke_nfr1_open_to_first_frame() {
        // Minimal document
        let doc = String::from("# Test\n\nSome content.\n");
        let start = std::time::Instant::now();
        let mut session = EditorSession::from_text(&doc);
        let after_session = start.elapsed();
        let _frame = session.render_layout(80);
        let total = start.elapsed().as_millis() as u64;
        println!(
            "open_to_first_frame smoke: {}ms (session={:?}, {} bytes)",
            total,
            after_session,
            doc.len()
        );
        assert!(
            total < 500,
            "NFR-1 smoke regression: open_to_first_frame took {}ms",
            total
        );
    }

    #[test]
    fn perf_smoke_nfr2_keystroke_to_frame() {
        let doc = String::from("# Test\n\nSome content.\n");
        let mut session = EditorSession::from_text(&doc);
        session.render_layout(80);
        session.handle_key(key('i'));
        let iterations = 5u64;
        let start = std::time::Instant::now();
        for _ in 0..iterations {
            session.handle_key(key('x'));
            let _frame = session.render_source(Viewport {
                top_line: 0,
                height: 40,
                width: 80,
                wrap: true,
                left_col: 0,
                skip_rows: 0,
            });
        }
        let elapsed = start.elapsed().as_millis() as u64 / iterations;
        println!("NFR-2 keystroke_to_frame: {}ms avg", elapsed);
        assert!(
            elapsed < 100,
            "NFR-2 smoke regression: keystroke_to_frame avg {}ms",
            elapsed
        );
    }

    #[test]
    fn perf_smoke_nfr3_incremental_rehighlight() {
        let doc = String::from("# Test\n\nSome content.\n");
        let mut session = EditorSession::from_text(&doc);
        session.render_layout(80);
        session.handle_key(key('i'));
        let iterations = 5u64;
        let start = std::time::Instant::now();
        for _ in 0..iterations {
            session.handle_key(key('x'));
            let _frame = session.render_source(Viewport {
                top_line: 0,
                height: 40,
                width: 80,
                wrap: true,
                left_col: 0,
                skip_rows: 0,
            });
        }
        let elapsed = start.elapsed().as_millis() as u64 / iterations;
        println!("NFR-3 incremental_rehighlight: {}ms avg", elapsed);
        assert!(
            elapsed < 100,
            "NFR-3 smoke regression: incremental_rehighlight avg {}ms",
            elapsed
        );
    }

    #[test]
    fn perf_smoke_nfr4_rendered_build() {
        let out = large_rendered_document();
        let start = std::time::Instant::now();
        let mut session = EditorSession::from_text(&out);
        println!("Session created in {:?}", start.elapsed());
        let start = std::time::Instant::now();
        let _layout = session.render_layout(80);
        let elapsed = start.elapsed().as_millis() as u64;
        println!(
            "NFR-4 rendered_build: {}ms ({} lines)",
            elapsed,
            out.matches('\n').count() + 1
        );
        assert!(
            elapsed < 500,
            "NFR-4 smoke regression: rendered_build took {}ms",
            elapsed
        );
    }

    #[test]
    fn perf_smoke_nfr2_rendered_navigation_to_frame() {
        let doc = large_rendered_document();
        let mut session = EditorSession::from_text(&doc);
        session.render_layout(120);

        let iterations = 20;
        let start = std::time::Instant::now();
        for _ in 0..iterations {
            session.handle_key(key('j'));
            session.render_layout(120);
        }
        let elapsed_ms = start.elapsed().as_secs_f64() * 1_000.0 / f64::from(iterations);
        println!("NFR-2 rendered navigation_to_frame: {elapsed_ms:.2}ms avg");
        assert!(
            elapsed_ms < 100.0,
            "NFR-2 smoke regression: rendered navigation_to_frame averaged {elapsed_ms:.2}ms"
        );
    }

    #[test]
    fn perf_smoke_rendered_resize_remaps_without_rebuilding_source_state() {
        let doc = large_rendered_document();
        let mut session = EditorSession::from_text(&doc);
        session.render_layout(120);
        for _ in 0..100 {
            session.handle_key(key('j'));
        }
        let source_anchor = session.cursor();

        let start = std::time::Instant::now();
        for width in [40, 80, 120, 60, 100] {
            session.render_layout(width);
            assert_eq!(session.cursor(), source_anchor);
        }
        let elapsed = start.elapsed().as_millis() as u64;
        let average = elapsed / 5;
        assert!(
            average < 500,
            "rendered resize/remap smoke averaged {average}ms ({elapsed}ms total)"
        );
    }
}
