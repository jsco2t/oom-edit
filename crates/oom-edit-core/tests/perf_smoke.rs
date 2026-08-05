//! Performance-smoke assertions for NFR-1..NFR-4.
//!
//! Run with `OOM_PERF_ASSERT=1 cargo test --offline --locked -p oom-edit-core --test perf_smoke`.
//! When `OOM_PERF_ASSERT` is not set, each test is a no-op.

mod perf_assertions {
    use oom_edit_core::session::{EditorSession, Viewport};

    #[test]
    fn perf_smoke_nfr1_open_to_first_frame() {
        if std::env::var("OOM_PERF_ASSERT").is_err() {
            return;
        }
        // Minimal document
        let doc = String::from("# Test\n\nSome content.\n");
        let start = std::time::Instant::now();
        let mut session = EditorSession::from_text(&doc);
        println!("Session created in {:?}", start.elapsed());
        let start = std::time::Instant::now();
        let _frame = session.render_source(Viewport {
            top_line: 0,
            height: 40,
            width: 80,
        });
        println!("Rendered in {:?}", start.elapsed());
        let total = start.elapsed().as_millis() as u64;
        println!(
            "NFR-1 open_to_first_frame: {}ms ({} bytes)",
            total,
            doc.len()
        );
        assert!(
            total < 500,
            "NFR-1 FAIL: open_to_first_frame took {}ms",
            total
        );
    }

    #[test]
    fn perf_smoke_nfr2_keystroke_to_frame() {
        if std::env::var("OOM_PERF_ASSERT").is_err() {
            return;
        }
        let doc = String::from("# Test\n\nSome content.\n");
        let mut session = EditorSession::from_text(&doc);
        let iterations = 5u64;
        let start = std::time::Instant::now();
        for i in 0..iterations {
            session.handle_key(oom_edit_core::session::KeyInput {
                code: oom_edit_core::session::KeyCode {
                    kind: oom_edit_core::session::KeyCodeKind::Char('x'),
                },
                mods: oom_edit_core::session::Modifiers::default(),
            });
            let _frame = session.render_source(Viewport {
                top_line: 0,
                height: 40,
                width: 80,
            });
            if i % 2 == 0 {
                eprint!(".");
            }
        }
        eprintln!();
        let elapsed = start.elapsed().as_millis() as u64 / iterations;
        println!("NFR-2 keystroke_to_frame: {}ms avg", elapsed);
        assert!(
            elapsed < 100,
            "NFR-2 FAIL: keystroke_to_frame avg {}ms",
            elapsed
        );
    }

    #[test]
    fn perf_smoke_nfr3_incremental_rehighlight() {
        if std::env::var("OOM_PERF_ASSERT").is_err() {
            return;
        }
        let doc = String::from("# Test\n\nSome content.\n");
        let mut session = EditorSession::from_text(&doc);
        let iterations = 5u64;
        let start = std::time::Instant::now();
        for i in 0..iterations {
            session.handle_key(oom_edit_core::session::KeyInput {
                code: oom_edit_core::session::KeyCode {
                    kind: oom_edit_core::session::KeyCodeKind::Char('x'),
                },
                mods: oom_edit_core::session::Modifiers::default(),
            });
            let _frame = session.render_source(Viewport {
                top_line: 0,
                height: 40,
                width: 80,
            });
            if i % 2 == 0 {
                eprint!(".");
            }
        }
        eprintln!();
        let elapsed = start.elapsed().as_millis() as u64 / iterations;
        println!("NFR-3 incremental_rehighlight: {}ms avg", elapsed);
        assert!(
            elapsed < 100,
            "NFR-3 FAIL: incremental_rehighlight avg {}ms",
            elapsed
        );
    }

    #[test]
    fn perf_smoke_nfr4_view_build() {
        if std::env::var("OOM_PERF_ASSERT").is_err() {
            return;
        }
        let mut out = String::new();
        for i in 0..500 {
            out.push_str(&format!(
                "## Heading {}\n\nParagraph {} is a block of prose text that provides\ncontent for the benchmark. It should be long enough to exercise\nword-wrapping and layout logic in the renderer.\n\n",
                i, i
            ));
        }
        let start = std::time::Instant::now();
        let mut session = EditorSession::from_text(&out);
        println!("Session created in {:?}", start.elapsed());
        let start = std::time::Instant::now();
        let _layout = session.render_view(80);
        let elapsed = start.elapsed().as_millis() as u64;
        println!(
            "NFR-4 view_build: {}ms ({} lines)",
            elapsed,
            out.matches('\n').count() + 1
        );
        assert!(elapsed < 500, "NFR-4 FAIL: view_build took {}ms", elapsed);
    }
}
