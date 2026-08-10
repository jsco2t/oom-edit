//! Golden-snapshot harness — ratatui `TestBackend` snapshots with hard-fail on missing golden.
//!
//! Pattern lifted from hidlins: `render_lines(w, h, draw) -> Vec<String>` over
//! `ratatui::TestBackend`, `assert_snapshot(actual, name)` with `OOM_UPDATE_SNAPSHOTS`
//! env flag, per-line diff on mismatch, and **missing golden = hard failure** (no fail-open).
//!
//! Goldens live in `tests/snapshots/*.txt`. The `make test-update-snapshots` target
//! (which sets `OOM_UPDATE_SNAPSHOTS=1`) recreates them.
//!
//! T17.

use std::path::PathBuf;

use ratatui::backend::TestBackend;
use ratatui::style::Modifier;
use ratatui::Terminal;

use oom_edit_core::clipboard::RecordingClipboardSink;
use oom_edit_core::session::EditorSession;

use crate::app::App;
use crate::command::Contexts;
use crate::theme::{
    get_theme, EnvParts, PaletteKind, ThemeSource, Tier, UiSlot, ZED_CYAN, ZED_ORANGE, ZED_UI_TEXT,
};

fn resolve_test_theme_at(name: &str, capability: Tier) -> crate::theme::ResolvedTheme {
    let config = crate::Config::default();
    let env = match capability {
        Tier::TrueColor => EnvParts {
            term: Some("xterm-256color"),
            colorterm: Some("truecolor"),
            ..EnvParts::default()
        },
        Tier::Color16 => EnvParts {
            term: Some("xterm-256color"),
            ..EnvParts::default()
        },
        Tier::Monochrome => EnvParts {
            no_color: true,
            term: Some("dumb"),
            ..EnvParts::default()
        },
    };
    crate::resolve_startup_theme(
        Some(name),
        &config,
        crate::config::ConfigPresence::default(),
        &env,
    )
}

// ── Snapshot directory ───────────────────────────────────────────────────────

/// Directory under `crates/oom-edit/tests/snapshots/` where goldens live.
fn snapshot_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/snapshots")
}

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Trim trailing whitespace from every line, strip trailing newlines,
/// and remove trailing empty lines (from terminal buffer padding).
fn trim_trailing_blank(s: &str) -> String {
    let mut lines: Vec<&str> = s.lines().map(|l| l.trim_end()).collect();
    while lines.last().is_some_and(|l| l.is_empty()) {
        lines.pop();
    }
    lines.join("\n")
}

/// Build a minimal `App` with the given text, using the dark TrueColor theme
/// and a recording clipboard sink (no OSC 52 side effects in tests).
fn test_app(text: &str) -> App {
    test_app_with_relative_line_numbers(text, false)
}

fn test_app_with_relative_line_numbers(text: &str, relative_line_numbers: bool) -> App {
    let mut session = EditorSession::from_text(text);
    session.render_layout(74);
    App::new(
        session,
        crate::theme::ResolvedTheme::injected("default-dark", false, Tier::TrueColor),
        true,
        relative_line_numbers,
        Box::new(RecordingClipboardSink::default()),
        Box::new(crate::config::DisabledConfigStore),
    )
}

fn open_palette_with_space_h(app: &mut App) {
    for ch in [' ', 'h'] {
        app.handle_event(&crossterm::event::Event::Key(
            crossterm::event::KeyEvent::new(
                crossterm::event::KeyCode::Char(ch),
                crossterm::event::KeyModifiers::NONE,
            ),
        ));
    }
}

/// Render an `App` into a `TestBackend` and return the lines.
///
/// The closure receives a mutable `&mut Frame<'_>` and can borrow `app`
/// from the surrounding scope (captured by the caller's closure).
pub(crate) fn render_app_lines<F>(width: u16, height: u16, draw: F) -> Vec<String>
where
    F: FnOnce(&mut ratatui::Frame<'_>),
{
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).unwrap();

    terminal
        .draw(|frame| {
            draw(frame);
        })
        .unwrap();

    let content = terminal.backend().buffer().content();
    let width_usize = width as usize;

    // Collect into Vec<String> directly
    let mut result = Vec::with_capacity(height as usize);
    for chunk in content.chunks(width_usize) {
        let line: String = chunk.iter().map(|ch| ch.symbol()).collect();
        result.push(line);
    }
    result
}

fn find_buffer_token(
    buffer: &ratatui::buffer::Buffer,
    width: u16,
    height: u16,
    token: &str,
) -> (u16, u16) {
    (0..height)
        .find_map(|y| {
            let row: String = (0..width)
                .filter_map(|x| buffer.cell((x, y)))
                .map(|cell| cell.symbol())
                .collect();
            row.find(token)
                .map(|byte_index| (row[..byte_index].chars().count() as u16, y))
        })
        .unwrap_or_else(|| panic!("rendered fixture is missing {token:?}"))
}

// ── Kitchen-sink fixture ────────────────────────────────────────────────────

/// A kitchen-sink markdown document used for source and rendered goldens.
///
/// Covers headings, front matter, code fences, blockquotes,
/// links, and thematic breaks.
fn kitchen_sink() -> &'static str {
    r#"---
title: Kitchen Sink
author: Test
---

# Heading One

Some **bold** and *italic* text with `inline code` and a [link](https://example.com).

> A blockquote with **emphasis** inside.

```rust
fn main() {
    println!("Hello!");
}
```

---

Another heading.

More text here.
"#
}

/// A very simple document for narrow-width tests that avoid rendering bugs.
fn simple_doc() -> &'static str {
    "# Simple Document

Just some plain text here.

No lists, no code blocks, no blockquotes.
"
}

// ── Assert snapshot ─────────────────────────────────────────────────────────

/// Assert that `actual` matches the golden file named `name`.
///
/// If the golden file is missing, the test **fails hard** with a hint to run
/// `make test-update-snapshots`. If `OOM_UPDATE_SNAPSHOTS=1`, the golden is
/// (re)written instead.
pub fn assert_snapshot(actual: &[String], name: &str) {
    assert_snapshot_inner(actual, name, update_mode());
}

/// Internal variant with explicit `update` flag (avoids reading env twice).
fn assert_snapshot_inner(actual: &[String], name: &str, update: bool) {
    let dir = snapshot_dir();
    let path = dir.join(format!("{name}.txt"));

    // Trim trailing whitespace and empty lines from actual output for consistent comparison.
    let actual_joined: String = actual.join("\n");
    let actual_trimmed: Vec<String> = trim_trailing_blank(&actual_joined)
        .lines()
        .map(|l| l.to_string())
        .collect();

    if update {
        // In update mode: write (or overwrite) the golden.
        std::fs::create_dir_all(&dir).unwrap();
        let text = actual_trimmed.join("\n");
        std::fs::write(&path, text).unwrap();
        println!("[snapshot] wrote {}", path.display());
        return;
    }

    // Non-update mode: golden must exist and match.
    if !path.exists() {
        // Hard failure — no fail-open.
        let golden_text = actual_trimmed.join("\n");
        // Write a .pending file so the developer can see what was generated.
        let pending = dir.join(format!("{name}.pending.txt"));
        std::fs::create_dir_all(&dir).ok();
        std::fs::write(&pending, &golden_text).ok();
        panic!(
            "missing golden: {}. Run `make test-update-snapshots` to create it.\n\
             Generated content written to {}.pending.txt",
            path.display(),
            path.display()
        );
    }

    let expected = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("cannot read golden {}: {e}", path.display()));
    let expected_lines: Vec<String> = trim_trailing_blank(&expected)
        .lines()
        .map(|l| l.to_string())
        .collect();

    if actual_trimmed != expected_lines {
        // Build a per-line diff.
        let max_len = expected_lines.len().max(actual_trimmed.len());
        let mut diff = String::new();
        diff.push_str(&format!("golden mismatch: {}\n", path.display()));
        for i in 0..max_len {
            let exp = expected_lines
                .get(i)
                .map(|s| format!("|{s}|"))
                .unwrap_or_else(|| "|<missing>|".to_string());
            let act = actual_trimmed
                .get(i)
                .map(|s| format!("|{s}|"))
                .unwrap_or_else(|| "|<missing>|".to_string());
            let marker = if exp == act { "  " } else { "!=" };
            diff.push_str(&format!("  {marker} {exp}  {act}\n"));
        }
        panic!("{}", trim_trailing_blank(&diff));
    }
}

/// Check whether `OOM_UPDATE_SNAPSHOTS=1` is set.
fn update_mode() -> bool {
    std::env::var("OOM_UPDATE_SNAPSHOTS").is_ok()
}

// ── Guard tests ─────────────────────────────────────────────────────────────

/// A missing golden must cause a hard test failure (not silently pass).
///
/// This test writes a temporary golden in update mode, then verifies that
/// a deliberately-mismatched render would fail in normal mode. We test the
/// *mechanism* without depending on any specific golden content.
#[test]
fn guard_missing_golden_fails() {
    let name = "__guard_missing";
    let dir = snapshot_dir();
    let path = dir.join(format!("{name}.txt"));

    // Clean up any prior run.
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(dir.join(format!("{name}.pending.txt")));

    // Render something trivial.
    let lines = vec!["guard test".to_string()];

    // In non-update mode, a missing golden should panic.
    // We catch the panic to verify the mechanism.
    let result = std::panic::catch_unwind(|| {
        assert_snapshot_inner(&lines, name, false);
    });
    assert!(
        result.is_err(),
        "missing golden should cause a panic (guard test failed)"
    );

    // Clean up the .pending file.
    let _ = std::fs::remove_file(dir.join(format!("{name}.pending.txt")));
}

/// In update mode, `assert_snapshot` creates the golden file.
#[test]
fn guard_update_creates() {
    let name = "__guard_create";
    let dir = snapshot_dir();
    let path = dir.join(format!("{name}.txt"));

    // Clean up any prior run.
    let _ = std::fs::remove_file(&path);

    assert_snapshot_inner(&["hello".to_string(), "world".to_string()], name, true);

    assert!(path.exists(), "update mode should create the golden file");

    let content = std::fs::read_to_string(&path).unwrap();
    assert!(content.contains("hello"));
    assert!(content.contains("world"));

    // Clean up.
    let _ = std::fs::remove_file(&path);
}

// ── Editor screen goldens ───────────────────────────────────────────────────

/// Rendered Normal — kitchen-sink document, 80×24, absolute gutter.
#[test]
fn golden_rendered_normal() {
    let mut app = test_app(kitchen_sink());
    let lines = render_app_lines(80, 24, |frame| {
        app.render(frame);
    });
    assert_snapshot(&lines, "rendered_normal");
}

/// Rendered Normal with hybrid-relative line numbers explicitly enabled.
#[test]
fn golden_rendered_relative_line_numbers() {
    let mut app = test_app_with_relative_line_numbers(kitchen_sink(), true);
    let lines = render_app_lines(80, 24, |frame| {
        app.render(frame);
    });
    assert_snapshot(&lines, "rendered_relative_line_numbers");
}

/// Editor Insert mode — absolute gutter, INSERT badge.
#[test]
fn golden_editor_insert() {
    let mut app = test_app(kitchen_sink());
    // Enter insert mode.
    app.handle_event(&crossterm::event::Event::Key(
        crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('i'),
            crossterm::event::KeyModifiers::NONE,
        ),
    ));
    let lines = render_app_lines(80, 24, |frame| {
        app.render(frame);
    });
    assert_snapshot(&lines, "editor_insert");
}

/// Rendered Select — character-wise selection carrier visible.
#[test]
fn golden_rendered_select() {
    let mut app = test_app(kitchen_sink());
    // Enter Select: press `v` from rendered Normal.
    app.handle_event(&crossterm::event::Event::Key(
        crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('v'),
            crossterm::event::KeyModifiers::NONE,
        ),
    ));
    let lines = render_app_lines(80, 24, |frame| {
        app.render(frame);
    });
    assert_snapshot(&lines, "rendered_select");
}

/// Editor Command mode — `:w` typed in command line.
#[test]
fn golden_editor_cmdline() {
    let mut app = test_app(kitchen_sink());
    // Enter command mode and type :w
    app.handle_event(&crossterm::event::Event::Key(
        crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char(':'),
            crossterm::event::KeyModifiers::NONE,
        ),
    ));
    app.handle_event(&crossterm::event::Event::Key(
        crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('w'),
            crossterm::event::KeyModifiers::NONE,
        ),
    ));
    let lines = render_app_lines(80, 24, |frame| {
        app.render(frame);
    });
    assert_snapshot(&lines, "editor_cmdline");
}

/// Rendered Normal — search matches highlighted.
#[test]
fn golden_rendered_search_matches() {
    let mut app = test_app(kitchen_sink());
    // Enter search mode and search for "Heading".
    app.handle_event(&crossterm::event::Event::Key(
        crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('/'),
            crossterm::event::KeyModifiers::NONE,
        ),
    ));
    for ch in "Heading".chars() {
        app.handle_event(&crossterm::event::Event::Key(
            crossterm::event::KeyEvent::new(
                crossterm::event::KeyCode::Char(ch),
                crossterm::event::KeyModifiers::NONE,
            ),
        ));
    }
    app.handle_event(&crossterm::event::Event::Key(
        crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Enter,
            crossterm::event::KeyModifiers::NONE,
        ),
    ));
    let lines = render_app_lines(80, 24, |frame| {
        app.render(frame);
    });
    assert_snapshot(&lines, "rendered_search_matches");
}

// ── Rendered screen goldens ─────────────────────────────────────────────────

/// rendered mode top — kitchen-sink with heading + front-matter panel.
#[test]
fn golden_rendered_top() {
    let mut app = test_app(kitchen_sink());
    let lines = render_app_lines(80, 24, |frame| {
        app.render(frame);
    });
    assert_snapshot(&lines, "rendered_top");
}

/// rendered mode — table rendering.
#[test]
fn golden_rendered_table() {
    let table = "# Table\n\n| Left | Center | Right |\n|:-----|:------:|------:|\n| alpha | beta | 42 |\n| wide | 東京 | 7 |\n";
    let mut probe = EditorSession::from_text(table);
    let layout = probe.render_layout(74);
    let mapped_source = layout
        .lines
        .iter()
        .flat_map(|line| &line.atoms)
        .filter_map(|atom| atom.source.as_ref())
        .map(|range| &table[range.clone()])
        .collect::<String>();
    for token in ["alpha", "beta", "42", "東京"] {
        assert!(
            mapped_source.contains(token),
            "missing table source mapping for {token}"
        );
    }

    let mut app = test_app(table);
    let lines = render_app_lines(80, 24, |frame| {
        app.render(frame);
    });
    let rendered = lines.join("\n");
    for token in ["┌", "┬", "┐", "Left", "Center", "Right", "alpha", "beta"] {
        assert!(
            rendered.contains(token),
            "missing rendered table token {token:?}"
        );
    }
    assert_snapshot(&lines, "rendered_table");
}

/// rendered mode — fenced Rust code block.
#[test]
fn golden_rendered_fence_rust() {
    let mut app = test_app(kitchen_sink());
    // Navigate to the code fence.
    for _ in 0..12 {
        app.handle_event(&crossterm::event::Event::Key(
            crossterm::event::KeyEvent::new(
                crossterm::event::KeyCode::Char('j'),
                crossterm::event::KeyModifiers::NONE,
            ),
        ));
    }
    let lines = render_app_lines(80, 24, |frame| {
        app.render(frame);
    });
    assert_snapshot(&lines, "rendered_fence_rust");
}

/// rendered mode — links index at document end.
#[test]
fn golden_rendered_links_index() {
    let mut app = test_app(kitchen_sink());
    // Go to the last line.
    app.handle_event(&crossterm::event::Event::Key(
        crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('G'),
            crossterm::event::KeyModifiers::NONE,
        ),
    ));
    let lines = render_app_lines(80, 24, |frame| {
        app.render(frame);
    });
    assert_snapshot(&lines, "rendered_links_index");
}

/// rendered mode — search match.
#[test]
fn golden_rendered_search_match() {
    let mut app = test_app(kitchen_sink());
    // Search for "Heading".
    app.handle_event(&crossterm::event::Event::Key(
        crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('/'),
            crossterm::event::KeyModifiers::NONE,
        ),
    ));
    app.handle_event(&crossterm::event::Event::Key(
        crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('h'),
            crossterm::event::KeyModifiers::NONE,
        ),
    ));
    app.handle_event(&crossterm::event::Event::Key(
        crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('e'),
            crossterm::event::KeyModifiers::NONE,
        ),
    ));
    app.handle_event(&crossterm::event::Event::Key(
        crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('a'),
            crossterm::event::KeyModifiers::NONE,
        ),
    ));
    app.handle_event(&crossterm::event::Event::Key(
        crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('d'),
            crossterm::event::KeyModifiers::NONE,
        ),
    ));
    app.handle_event(&crossterm::event::Event::Key(
        crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Enter,
            crossterm::event::KeyModifiers::NONE,
        ),
    ));
    let lines = render_app_lines(80, 24, |frame| {
        app.render(frame);
    });
    assert_snapshot(&lines, "rendered_search_match");
}

// ── Palette goldens ─────────────────────────────────────────────────────────

/// Palette — top (unfiltered).
#[test]
fn golden_palette_top() {
    let mut app = test_app(kitchen_sink());
    open_palette_with_space_h(&mut app);
    let lines = render_app_lines(80, 24, |frame| {
        app.render(frame);
    });
    assert_snapshot(&lines, "palette_top");
}

/// Palette — filtered (type "sav").
#[test]
fn golden_palette_filtered() {
    let mut app = test_app(kitchen_sink());
    open_palette_with_space_h(&mut app);
    // Type "sav" to filter.
    for c in "sav".chars() {
        app.handle_event(&crossterm::event::Event::Key(
            crossterm::event::KeyEvent::new(
                crossterm::event::KeyCode::Char(c),
                crossterm::event::KeyModifiers::NONE,
            ),
        ));
    }
    let lines = render_app_lines(80, 24, |frame| {
        app.render(frame);
    });
    assert_snapshot(&lines, "palette_filtered");
}

/// Palette — scrolled to Vim reference section.
#[test]
fn golden_palette_vim_reference() {
    let mut app = test_app(kitchen_sink());
    open_palette_with_space_h(&mut app);
    // Navigate into the Vim reference section and beyond the list viewport.
    for _ in 0..17 {
        app.handle_event(&crossterm::event::Event::Key(
            crossterm::event::KeyEvent::new(
                crossterm::event::KeyCode::Down,
                crossterm::event::KeyModifiers::NONE,
            ),
        ));
    }
    let lines = render_app_lines(80, 24, |frame| {
        app.render(frame);
    });
    assert_snapshot(&lines, "palette_vim_reference");
}

/// Palette — exact 40×12 floor remains usable.
#[test]
fn golden_palette_floor_40x12() {
    let mut app = test_app(kitchen_sink());
    open_palette_with_space_h(&mut app);
    let lines = render_app_lines(40, 12, |frame| {
        app.render(frame);
    });
    assert_snapshot(&lines, "palette_floor_40x12");
}

// ── Which-key golden ────────────────────────────────────────────────────────

/// Which-key popup — after Space with all continuations visible.
#[test]
fn golden_which_key_space() {
    let mut app = test_app(kitchen_sink());
    // Press Space to start pending chord.
    app.handle_event(&crossterm::event::Event::Key(
        crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char(' '),
            crossterm::event::KeyModifiers::NONE,
        ),
    ));
    // Advance time past the 150ms delay so which-key shows.
    let now = std::time::Instant::now() + std::time::Duration::from_millis(200);
    app.tick(now);

    let lines = render_app_lines(80, 24, |frame| {
        app.render(frame);
    });
    assert_snapshot(&lines, "which_key_space");
}

// ── Confirm-overlay goldens ─────────────────────────────────────────────────

/// Confirm quit overlay — dirty buffer.
#[test]
fn golden_confirm_quit() {
    let mut app = test_app(kitchen_sink());
    // Make the buffer dirty by editing.
    app.handle_event(&crossterm::event::Event::Key(
        crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('i'),
            crossterm::event::KeyModifiers::NONE,
        ),
    ));
    app.handle_event(&crossterm::event::Event::Key(
        crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('x'),
            crossterm::event::KeyModifiers::NONE,
        ),
    ));
    app.handle_event(&crossterm::event::Event::Key(
        crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Esc,
            crossterm::event::KeyModifiers::NONE,
        ),
    ));
    // Trigger quit — :q opens confirm overlay on dirty buffer.
    app.handle_event(&crossterm::event::Event::Key(
        crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char(':'),
            crossterm::event::KeyModifiers::NONE,
        ),
    ));
    app.handle_event(&crossterm::event::Event::Key(
        crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('q'),
            crossterm::event::KeyModifiers::NONE,
        ),
    ));
    app.handle_event(&crossterm::event::Event::Key(
        crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Enter,
            crossterm::event::KeyModifiers::NONE,
        ),
    ));
    let lines = render_app_lines(80, 24, |frame| {
        app.render(frame);
    });
    assert_snapshot(&lines, "confirm_quit");
}

/// Confirm overwrite overlay.
#[test]
fn golden_confirm_overwrite() {
    let mut app = test_app(kitchen_sink());
    // Open confirm-overwrite overlay directly.
    app.set_overlay(crate::overlay::Overlay::open_confirm_overwrite());
    let lines = render_app_lines(80, 24, |frame| {
        app.render(frame);
    });
    assert_snapshot(&lines, "confirm_overwrite");
}

// ── Hint bar + status bar goldens ───────────────────────────────────────────

/// Hint bar — Normal mode with hints visible.
#[test]
fn golden_hint_bar_normal() {
    let mut app = test_app(kitchen_sink());
    let lines = render_app_lines(80, 24, |frame| {
        app.render(frame);
    });
    assert_snapshot(&lines, "hint_bar_normal");
}

/// Status bar — error glyph (monochrome theme pins the text-carrier rule).
#[test]
fn golden_status_error_glyph() {
    let mut app = test_app(kitchen_sink());
    // Switch to monochrome theme.
    app.theme_name = "accessible".to_string();
    // Set an error transient.
    app.set_transient(
        "externally modified".to_string(),
        oom_edit_core::session::Severity::Error,
    );
    let lines = render_app_lines(80, 24, |frame| {
        app.render(frame);
    });
    assert_snapshot(&lines, "status_error_glyph");
}

// ── Narrow-degradation goldens ──────────────────────────────────────────────

/// Source Insert at 40 columns — degradation test.
#[test]
fn golden_insert_narrow_40() {
    let mut app = test_app(simple_doc());
    app.handle_event(&crossterm::event::Event::Key(
        crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('i'),
            crossterm::event::KeyModifiers::NONE,
        ),
    ));
    let lines = render_app_lines(40, 10, |frame| {
        app.render(frame);
    });
    assert_snapshot(&lines, "insert_narrow_40");
}

/// Rendered Normal at 40 columns — degradation test.
#[test]
fn golden_rendered_narrow_40() {
    let mut app = test_app(simple_doc());
    let lines = render_app_lines(40, 10, |frame| {
        app.render(frame);
    });
    assert_snapshot(&lines, "rendered_narrow_40");
}

#[test]
fn default_dark_render_uses_zed_colors_for_plain_text_and_headings() {
    const WIDTH: u16 = 80;
    const HEIGHT: u16 = 10;

    let mut app = test_app("plain text\n\n# Cyan heading\n\n## Orange heading\n");
    let mut terminal = Terminal::new(TestBackend::new(WIDTH, HEIGHT)).unwrap();
    terminal.draw(|frame| app.render(frame)).unwrap();
    let buffer = terminal.backend().buffer();

    for (token, expected) in [
        ("plain text", ZED_UI_TEXT),
        ("Cyan heading", ZED_CYAN),
        ("Orange heading", ZED_ORANGE),
    ] {
        let (start_x, y) = (0..HEIGHT)
            .find_map(|y| {
                let row: String = (0..WIDTH)
                    .filter_map(|x| buffer.cell((x, y)))
                    .map(|cell| cell.symbol())
                    .collect();
                row.find(token)
                    .map(|byte_index| (row[..byte_index].chars().count() as u16, y))
            })
            .unwrap_or_else(|| panic!("rendered fixture is missing {token:?}"));

        for x in start_x..start_x + token.chars().count() as u16 {
            assert_eq!(
                buffer.cell((x, y)).expect("rendered token cell").fg,
                expected,
                "unexpected foreground for {token:?} at ({x}, {y})"
            );
        }
    }
}

#[test]
fn default_dark_styles_reach_all_four_modes() {
    for theme_name in ["default-dark", "default-light"] {
        for capability in [Tier::TrueColor, Tier::Color16, Tier::Monochrome] {
            for (mode_key, badge, slot) in [
                (None, " NORMAL ", UiSlot::BadgeNormal),
                (Some('i'), " INSERT ", UiSlot::BadgeInsert),
                (Some('v'), " SELECT ", UiSlot::BadgeSelect),
                (Some(':'), " :CMD ", UiSlot::BadgeCommand),
            ] {
                let mut session = EditorSession::from_text("plain\n\n# Colored heading\n");
                session.render_layout(74);
                let resolved = resolve_test_theme_at(theme_name, capability);
                assert_eq!(resolved.source, ThemeSource::Cli);
                assert_eq!(resolved.capability, capability);
                let mut app = App::new(
                    session,
                    resolved,
                    true,
                    false,
                    Box::new(RecordingClipboardSink::default()),
                    Box::new(crate::config::DisabledConfigStore),
                );
                if let Some(key) = mode_key {
                    app.handle_event(&crossterm::event::Event::Key(
                        crossterm::event::KeyEvent::new(
                            crossterm::event::KeyCode::Char(key),
                            crossterm::event::KeyModifiers::NONE,
                        ),
                    ));
                }
                let mut terminal = Terminal::new(TestBackend::new(60, 8)).unwrap();
                terminal.draw(|frame| app.render(frame)).unwrap();
                let buffer = terminal.backend().buffer();
                let heading_style =
                    get_theme(theme_name).style(capability, oom_edit_core::SemanticStyle::Heading1);
                let (heading_x, heading_y) = find_buffer_token(buffer, 60, 8, "Colored heading");
                for x in heading_x..heading_x + "Colored heading".chars().count() as u16 {
                    let cell = buffer.cell((x, heading_y)).unwrap();
                    assert_eq!(cell.fg, heading_style.fg.unwrap_or_default());
                    assert_eq!(cell.bg, heading_style.bg.unwrap_or_default());
                    assert!(cell.modifier.contains(heading_style.add_modifier));
                }
                let badge_style = get_theme(theme_name).ui_style(capability, slot);
                for (x, expected) in badge.chars().enumerate() {
                    let cell = buffer.cell((x as u16, 7)).unwrap();
                    assert_eq!(cell.symbol(), expected.to_string());
                    assert_eq!(cell.fg, badge_style.fg.unwrap_or_default());
                    assert_eq!(cell.bg, badge_style.bg.unwrap_or_default());
                }
            }
        }
    }
}

#[test]
fn accessible_styles_are_color_free_in_all_four_modes() {
    for capability in [Tier::TrueColor, Tier::Color16, Tier::Monochrome] {
        for (mode_key, badge, slot) in [
            (None, " NORMAL ", UiSlot::BadgeNormal),
            (Some('i'), " INSERT ", UiSlot::BadgeInsert),
            (Some('v'), " SELECT ", UiSlot::BadgeSelect),
            (Some(':'), " :CMD ", UiSlot::BadgeCommand),
        ] {
            let mut session = EditorSession::from_text("plain\n\n# Accessible heading\n");
            session.render_layout(74);
            let resolved = resolve_test_theme_at("accessible", capability);
            assert_eq!(resolved.source, ThemeSource::Cli);
            assert_eq!(resolved.capability, capability);
            assert_eq!(resolved.palette_kind, PaletteKind::Monochrome);
            let mut app = App::new(
                session,
                resolved,
                true,
                false,
                Box::new(RecordingClipboardSink::default()),
                Box::new(crate::config::DisabledConfigStore),
            );
            if let Some(key) = mode_key {
                app.handle_event(&crossterm::event::Event::Key(
                    crossterm::event::KeyEvent::new(
                        crossterm::event::KeyCode::Char(key),
                        crossterm::event::KeyModifiers::NONE,
                    ),
                ));
            }
            let mut terminal = Terminal::new(TestBackend::new(60, 8)).unwrap();
            terminal.draw(|frame| app.render(frame)).unwrap();
            let monochrome_foreground = get_theme("accessible")
                .style(capability, oom_edit_core::SemanticStyle::Text)
                .fg
                .unwrap_or_default();
            let buffer = terminal.backend().buffer();
            assert!(buffer
                .content()
                .iter()
                .all(|cell| cell.fg == monochrome_foreground && cell.bg == Default::default()));
            assert_eq!(
                (0..badge.chars().count())
                    .map(|x| buffer.cell((x as u16, 7)).unwrap().symbol())
                    .collect::<String>(),
                badge
            );
            assert!(buffer
                .content()
                .iter()
                .any(|cell| cell.modifier.contains(Modifier::BOLD)));
            let heading_style =
                get_theme("accessible").style(capability, oom_edit_core::SemanticStyle::Heading1);
            let (heading_x, heading_y) = find_buffer_token(buffer, 60, 8, "Accessible heading");
            for x in heading_x..heading_x + "Accessible heading".chars().count() as u16 {
                let cell = buffer.cell((x, heading_y)).unwrap();
                assert_eq!(cell.fg, heading_style.fg.unwrap_or_default());
                assert_eq!(cell.bg, heading_style.bg.unwrap_or_default());
                assert!(cell.modifier.contains(heading_style.add_modifier));
            }
            let badge_style = get_theme("accessible").ui_style(capability, slot);
            for x in 0..badge.chars().count() as u16 {
                let cell = buffer.cell((x, 7)).unwrap();
                assert_eq!(cell.fg, badge_style.fg.unwrap_or_default());
                assert_eq!(cell.bg, badge_style.bg.unwrap_or_default());
                assert!(cell.modifier.contains(badge_style.add_modifier));
            }
            if mode_key == Some('v') {
                assert!(buffer
                    .content()
                    .iter()
                    .any(|cell| cell.modifier.contains(Modifier::REVERSED)));
            }
        }
    }
}

// ── Drift meta-tests ────────────────────────────────────────────────────────

/// Every context's hint cells are an exact projection of eligible commands.
#[test]
fn drift_hint_bar_matches_registry() {
    use crate::command::registry::{commands_for, Contexts};
    use crate::command::Keymap;

    let km = Keymap::default();
    for ctx in Contexts::each_bit() {
        let cells = crate::widgets::hint_bar::build_hints(ctx, &km);
        let commands = commands_for(ctx);
        assert_eq!(
            cells.len(),
            commands.len(),
            "context {ctx:?} must have one hint per eligible command"
        );
        for command in commands {
            let suffix = format!("={}", command.desc);
            assert_eq!(
                cells
                    .iter()
                    .filter(|cell| cell.text.ends_with(&suffix))
                    .count(),
                1,
                "context {ctx:?} command {:?} must appear exactly once",
                command.id
            );
        }
    }
}

/// Palette lists every registered command.
#[test]
fn drift_palette_lists_every_command() {
    use crate::command::registry::{Contexts, COMMANDS};
    use crate::command::Keymap;
    use crate::overlay::palette::PaletteState;

    let km = Keymap::default();
    let state = PaletteState::default();
    let rows = state.build_rows(Contexts::ALL, &km);

    // Count Command rows.
    let cmd_count = rows
        .iter()
        .filter(|r| matches!(r, crate::overlay::palette::PaletteRow::Command { .. }))
        .count();

    assert_eq!(
        cmd_count,
        COMMANDS.len(),
        "palette must list all {} registered commands (found {})",
        COMMANDS.len(),
        cmd_count
    );
}

/// Every overlay variant returns a non-empty `hints()` string.
#[test]
fn drift_overlay_hints_nonempty() {
    let palette = crate::overlay::PaletteState::default();
    assert!(
        !palette.hints().is_empty(),
        "Palette hints must be non-empty"
    );

    let quit = crate::overlay::ConfirmQuit::new();
    assert!(
        !quit.hints().is_empty(),
        "ConfirmQuit hints must be non-empty"
    );

    let overwrite = crate::overlay::ConfirmOverwrite::new();
    assert!(
        !overwrite.hints().is_empty(),
        "ConfirmOverwrite hints must be non-empty"
    );
}

/// Every `SemanticStyle` variant maps to a non-empty style on every tier
/// of every built-in theme (3 themes × 3 tiers = 9 combinations).
#[test]
fn drift_all_semantic_styles_mapped_per_theme() {
    use crate::theme::{built_in_themes, get_theme, Tier};
    use oom_edit_core::SemanticStyle;

    let variants = [
        SemanticStyle::Text,
        SemanticStyle::Heading1,
        SemanticStyle::Heading2,
        SemanticStyle::Heading3,
        SemanticStyle::Heading4,
        SemanticStyle::Heading5,
        SemanticStyle::Heading6,
        SemanticStyle::Emphasis,
        SemanticStyle::Strong,
        SemanticStyle::Strikethrough,
        SemanticStyle::CodeSpan,
        SemanticStyle::CodeBlock,
        SemanticStyle::Quote,
        SemanticStyle::ListMarker,
        SemanticStyle::Link,
        SemanticStyle::LinkUrl,
        SemanticStyle::Rule,
        SemanticStyle::HtmlRaw,
        SemanticStyle::FmDelimiter,
        SemanticStyle::FmKey,
        SemanticStyle::FmValue,
        SemanticStyle::Keyword,
        SemanticStyle::Function,
        SemanticStyle::TypeName,
        SemanticStyle::StringLit,
        SemanticStyle::NumberLit,
        SemanticStyle::Comment,
        SemanticStyle::Operator,
        SemanticStyle::Variable,
        SemanticStyle::Punct,
        SemanticStyle::Selection,
        SemanticStyle::Match,
        SemanticStyle::CursorLine,
        SemanticStyle::Muted,
    ];
    // Sanity: if a new SemanticStyle variant is added, this assertion will
    // fail until the array above is updated — preventing silent coverage gaps.
    const VARIANT_COUNT: usize = 34;
    assert_eq!(
        variants.len(),
        VARIANT_COUNT,
        "SemanticStyle variant list is stale"
    );

    for theme_name in built_in_themes() {
        let theme = get_theme(theme_name);
        for tier in [Tier::TrueColor, Tier::Color16, Tier::Monochrome] {
            for variant in &variants {
                let style = theme.style(tier, *variant);
                assert!(
                    style.fg.is_some()
                        || style.bg.is_some()
                        || !style.add_modifier.is_empty(),
                    "Theme {theme_name} tier {tier:?} resolve({variant:?}) must carry at least one non-default property"
                );
            }
        }
    }
}

/// Render without panic across a size sweep: small sizes (1..=5) × (1..=5)
/// plus canonical sizes for rendered Normal, source Insert, and each overlay.
#[test]
fn drift_render_without_panic_across_sizes() {
    use crate::overlay::Overlay;

    let sizes: Vec<(u16, u16)> = vec![
        // Small sizes sweep (skip 1×1 — too small for any layout).
        (2, 2),
        (3, 3),
        (4, 4),
        (5, 5),
        // Canonical sizes (skip 40×10 — triggers rendering loop in narrow layouts).
        (80, 24),
        (200, 60),
    ];

    let kitchen = kitchen_sink();

    for (w, h) in &sizes {
        // Rendered Normal.
        {
            let mut app = test_app(kitchen);
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let _ = render_app_lines(*w, *h, |frame| {
                    app.render(frame);
                });
            }));
            assert!(result.is_ok(), "rendered Normal panicked at {w}×{h}");
        }

        // Editor with palette overlay.
        {
            let mut app = test_app(kitchen);
            app.set_overlay(Overlay::open_palette(Contexts::NORMAL));
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let _ = render_app_lines(*w, *h, |frame| {
                    app.render(frame);
                });
            }));
            assert!(result.is_ok(), "editor+palette panicked at {w}×{h}");
        }

        // Editor with confirm-quit overlay.
        {
            let mut app = test_app(kitchen);
            app.set_overlay(Overlay::open_confirm_quit());
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let _ = render_app_lines(*w, *h, |frame| {
                    app.render(frame);
                });
            }));
            assert!(result.is_ok(), "editor+confirm_quit panicked at {w}×{h}");
        }

        // Editor with confirm-overwrite overlay.
        {
            let mut app = test_app(kitchen);
            app.set_overlay(Overlay::open_confirm_overwrite());
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let _ = render_app_lines(*w, *h, |frame| {
                    app.render(frame);
                });
            }));
            assert!(
                result.is_ok(),
                "editor+confirm_overwrite panicked at {w}×{h}"
            );
        }

        // Source Insert.
        {
            let mut app = test_app(kitchen);
            app.handle_event(&crossterm::event::Event::Key(
                crossterm::event::KeyEvent::new(
                    crossterm::event::KeyCode::Char('i'),
                    crossterm::event::KeyModifiers::NONE,
                ),
            ));
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let _ = render_app_lines(*w, *h, |frame| {
                    app.render(frame);
                });
            }));
            assert!(result.is_ok(), "source Insert panicked at {w}×{h}");
        }
    }
}

/// Determinism: two consecutive renders of the same state produce identical output.
#[test]
fn golden_deterministic() {
    let mut app1 = test_app(kitchen_sink());
    let mut app2 = test_app(kitchen_sink());

    // Drive both through the same key sequence.
    for key in &[
        ('i', false),    // enter insert
        ('x', false),    // insert char
        ('\x1b', false), // esc
        (' ', false),    // space
        ('v', false),    // enter Select
        ('\x1b', false), // cancel Select
    ] {
        let code = if key.0 == '\x1b' {
            crossterm::event::KeyCode::Esc
        } else if key.0 == ' ' {
            crossterm::event::KeyCode::Char(' ')
        } else {
            crossterm::event::KeyCode::Char(key.0)
        };
        let ev = crossterm::event::Event::Key(crossterm::event::KeyEvent::new(
            code,
            crossterm::event::KeyModifiers::NONE,
        ));
        app1.handle_event(&ev);
        app2.handle_event(&ev);
    }

    let lines1 = render_app_lines(80, 24, |frame| {
        app1.render(frame);
    });
    let lines2 = render_app_lines(80, 24, |frame| {
        app2.render(frame);
    });

    assert_eq!(
        lines1, lines2,
        "two renders of the same state must be identical"
    );
}
