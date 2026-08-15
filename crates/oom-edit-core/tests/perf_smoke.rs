//! Performance-smoke assertions for NFR-1..NFR-4.
//!
//! These relaxed debug-build ceilings are paired with deterministic unit
//! invariants. Exact release-profile budgets are asserted by `make bench`.

#[path = "../perf/fixtures.rs"]
mod fixtures;

mod perf_assertions {
    use std::time::{Duration, Instant};

    use oom_edit_core::{EditorSession, KeyCode, KeyCodeKind, KeyInput, Mode, Modifiers, Viewport};
    use oom_spell::{BuildProgress, SpellEngine, SpellEngineBuilder};

    const ONE_MIB: usize = 1024 * 1024;
    const FIXTURE_SEED: u64 = 0x00_0D_D1_7E;

    fn source_fixture_1mb() -> String {
        super::fixtures::seeded_markdown_fixture(ONE_MIB, FIXTURE_SEED)
    }

    fn rendered_5000_line_fixture() -> String {
        super::fixtures::seeded_rendered_fixture(5_000, FIXTURE_SEED)
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

    fn source_viewport(top_line: usize) -> Viewport {
        Viewport {
            top_line,
            height: 40,
            width: 100,
            wrap: true,
            left_col: 0,
            skip_rows: 0,
        }
    }

    #[test]
    fn perf_smoke_nfr1_open_to_first_frame() {
        let doc = source_fixture_1mb();
        let start = Instant::now();
        let mut session = EditorSession::from_text(&doc);
        assert_eq!(session.mode(), Mode::Normal);
        // Keep this paired with T18's source-frame definition of NFR-1.
        let _frame = session.render_source(source_viewport(0));
        let elapsed = start.elapsed();
        println!(
            "NFR-1 smoke: {} bytes, {} lines, {:?}",
            doc.len(),
            session.line_count(),
            elapsed
        );
        assert!(elapsed < Duration::from_secs(2));
    }

    #[test]
    fn perf_smoke_nfr2_insert_scroll_frame_1mb() {
        let doc = source_fixture_1mb();
        let mut session = EditorSession::from_text(&doc);
        enter_insert(&mut session);
        let middle = session.line_count() / 2;
        for _ in 0..middle {
            session.handle_key(special(KeyCodeKind::Down));
        }

        let iterations = 20u32;
        let cursor_start = Instant::now();
        for iteration in 0..iterations {
            let kind = if iteration % 2 == 0 {
                KeyCodeKind::Down
            } else {
                KeyCodeKind::Up
            };
            session.handle_key(special(kind));
        }
        let cursor_avg = cursor_start.elapsed() / iterations;

        let frame_start = Instant::now();
        for row in 0..iterations as usize {
            let _ = session.render_source(source_viewport(middle + row));
        }
        let frame_avg = frame_start.elapsed() / iterations;
        println!(
            "NFR-2 smoke: {} bytes, cursor avg {:?}, source-frame avg {:?}",
            doc.len(),
            cursor_avg,
            frame_avg
        );
        assert!(cursor_avg < Duration::from_millis(50));
        assert!(frame_avg < Duration::from_millis(100));
    }

    #[test]
    fn perf_smoke_nfr3_insert_edit_1mb() {
        let doc = source_fixture_1mb();
        let mut session = EditorSession::from_text(&doc);
        enter_insert(&mut session);
        let line_count = session.line_count();
        let locations = [0, line_count / 2, line_count.saturating_sub(1)];
        let mut current = 0;
        let mut durations = Vec::new();

        for target in locations {
            for _ in current..target {
                session.handle_key(special(KeyCodeKind::Down));
            }
            current = target;
            let started = Instant::now();
            session.handle_key(key('x'));
            durations.push(started.elapsed());
        }

        let worst = durations.iter().copied().max().unwrap_or_default();
        println!(
            "NFR-3 smoke: {} bytes, begin/middle/end {:?}, worst {:?}",
            doc.len(),
            durations,
            worst
        );
        assert!(worst < Duration::from_millis(250));
    }

    #[test]
    fn perf_smoke_nfr2_edit_to_frame_1mb() {
        let doc = source_fixture_1mb();
        let mut session = EditorSession::from_text(&doc);
        enter_insert(&mut session);
        let middle = session.line_count() / 2;
        for _ in 0..middle {
            session.handle_key(special(KeyCodeKind::Down));
        }
        let started = Instant::now();
        session.handle_key(key('x'));
        let _ = session.render_source(source_viewport(middle));
        let elapsed = started.elapsed();
        println!("NFR-2 edit-to-frame smoke: {elapsed:?}");
        assert!(elapsed < Duration::from_millis(350));
    }

    #[test]
    fn perf_smoke_nfr4_rendered_build() {
        let doc = rendered_5000_line_fixture();
        let mut session = EditorSession::from_text(&doc);
        assert_eq!(session.line_count(), 5_000);
        let cold_start = Instant::now();
        let _ = session.render_layout(100);
        let cold = cold_start.elapsed();
        let resize_start = Instant::now();
        let _ = session.render_layout(72);
        let resize = resize_start.elapsed();
        println!("NFR-4 smoke: 5000 lines, cold {cold:?}, resize {resize:?}");
        assert!(cold < Duration::from_secs(2));
        assert!(resize < Duration::from_secs(2));
    }

    #[test]
    fn perf_smoke_nfr2_rendered_navigation_to_frame() {
        let doc = rendered_5000_line_fixture();
        let mut session = EditorSession::from_text(&doc);
        session.render_layout(120);
        for digit in "2500".chars() {
            session.handle_key(key(digit));
        }
        session.handle_key(key('G'));
        assert!(session.rendered_cursor_line() > 1_000);

        let iterations = 40u32;
        let start = Instant::now();
        for iteration in 0..iterations {
            session.handle_key(key(if iteration % 2 == 0 { 'j' } else { 'k' }));
            session.render_layout(120);
        }
        let average = start.elapsed() / iterations;
        println!("NFR-2 rendered mid-file navigation avg {average:?}");
        assert!(average < Duration::from_millis(100));
    }

    #[test]
    fn perf_smoke_spell_tick_and_full_scan_1mb() {
        let engine = spell_engine();
        let document = spell_fixture_1mb();
        let mut session = EditorSession::from_text(&document);
        let tick_started = Instant::now();
        assert!(session.spell_tick(&engine, 4096));
        let tick = tick_started.elapsed();
        assert!(
            tick < Duration::from_millis(50),
            "4 KiB spell tick took {tick:?}"
        );

        let full_started = Instant::now();
        while session.diagnostics_pending() {
            assert!(session.spell_tick(&engine, 4096));
        }
        let full = full_started.elapsed();
        println!("spell smoke: 4 KiB {tick:?}, remaining 1 MiB {full:?}");
        assert!(full < Duration::from_secs(5));
        assert!(session.diagnostics().is_empty());

        let mut dirty_session = EditorSession::from_text(&"helo\n".repeat(1_000));
        while dirty_session.diagnostics_pending() {
            dirty_session.spell_tick(&engine, 4096);
        }
        let diagnostic = dirty_session.diagnostics()[0].clone();
        let dirty_started = Instant::now();
        dirty_session.apply_spell_replacement(&diagnostic, "hello");
        let dirty = dirty_started.elapsed();
        assert!(
            dirty < Duration::from_millis(10),
            "spell dirty-mark and shift took {dirty:?}"
        );

        let pathological = "hello ".repeat(ONE_MIB / 6);
        let mut pathological_session = EditorSession::from_text(&pathological);
        let pathological_started = Instant::now();
        assert!(pathological_session.spell_tick(&engine, 4096));
        let pathological_tick = pathological_started.elapsed();
        assert!(pathological_session.diagnostics_pending());
        assert!(
            pathological_tick < Duration::from_millis(50),
            "pathological 4 KiB spell tick took {pathological_tick:?}"
        );
    }
}
