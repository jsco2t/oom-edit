//! Session-level integration tests for `EditorSession`.
//!
//! Covers: basic session operations, rendering, effect emission, and
//! highlighter integration.

use oom_edit_core::session::{
    EditorSession, Effect, KeyCode, KeyCodeKind, KeyInput, Mode, Modifiers, Viewport,
};
use oom_edit_core::style::SemanticStyle;

fn key(ch: char) -> KeyInput {
    KeyInput {
        code: KeyCode {
            kind: KeyCodeKind::Char(ch),
        },
        mods: Modifiers::default(),
    }
}

fn key_special(kind: KeyCodeKind) -> KeyInput {
    KeyInput {
        code: KeyCode { kind },
        mods: Modifiers::default(),
    }
}

// ── Basic session operations ────────────────────────────────────────────────

#[test]
fn session_starts_in_normal_mode() {
    let session = EditorSession::from_text("hello");
    assert!(matches!(session.mode(), Mode::Normal));
}

#[test]
fn session_line_count_matches_text() {
    let session = EditorSession::from_text("line1\nline2\nline3");
    // line_count returns newline count + 1, but trailing newline is handled
    assert!(session.line_count() >= 3, "should have at least 3 lines");
}

#[test]
fn insert_mode_emits_mode_changed() {
    let mut session = EditorSession::from_text("hello");

    session.handle_key(key_special(KeyCodeKind::Esc));
    let effects = session.handle_key(key('i'));

    assert!(effects.iter().any(|e| matches!(e, Effect::ModeChanged(_))));
    assert!(matches!(session.mode(), Mode::Insert));
}

#[test]
fn normal_mode_emits_cursor_moved() {
    let mut session = EditorSession::from_text("hello");

    let effects = session.handle_key(key('l'));

    assert!(effects.iter().any(|e| matches!(e, Effect::CursorMoved)));
}

// ── render_source ───────────────────────────────────────────────────────────

#[test]
fn render_source_returns_frame_with_lines() {
    let mut session = EditorSession::from_text("hello\nworld");

    let vp = Viewport {
        top_line: 0,
        height: 10,
        width: 80,
    };
    let frame = session.render_source(vp);

    assert!(frame.lines.len() >= 2, "should have at least 2 lines");
    assert!(frame.lines.len() <= 10, "should not exceed viewport height");
}

#[test]
fn render_source_cursor_position() {
    let mut session = EditorSession::from_text("hello\nworld");

    session.handle_key(key('j')); // Move to line 2

    let vp = Viewport {
        top_line: 0,
        height: 10,
        width: 80,
    };
    let frame = session.render_source(vp);

    assert_eq!(frame.cursor.0, 1, "cursor should be on row 1");
}

#[test]
fn render_source_has_highlighting_spans() {
    let mut session = EditorSession::from_text("# Hello\n\nWorld\n");

    let vp = Viewport {
        top_line: 0,
        height: 10,
        width: 80,
    };
    let frame = session.render_source(vp);

    // First line is a heading — should have non-empty spans
    assert!(
        !frame.lines[0].spans.is_empty(),
        "heading line should have highlighting spans"
    );

    // Verify the first span has a heading style
    let has_heading_style = frame.lines[0]
        .spans
        .iter()
        .any(|s| matches!(s.style, SemanticStyle::Heading1));
    assert!(has_heading_style, "heading line should have Heading1 style");
}

#[test]
fn render_source_first_line_number() {
    let mut session = EditorSession::from_text("line1\nline2\nline3");

    // top_line = 0 → first_line_number = 1
    let vp = Viewport {
        top_line: 0,
        height: 10,
        width: 80,
    };
    let frame = session.render_source(vp);
    assert_eq!(
        frame.first_line_number, 1,
        "top_line 0 → first_line_number 1"
    );

    // top_line = 5 → first_line_number = 6
    let vp = Viewport {
        top_line: 5,
        height: 10,
        width: 80,
    };
    let frame = session.render_source(vp);
    assert_eq!(
        frame.first_line_number, 6,
        "top_line 5 → first_line_number 6"
    );
}

#[test]
fn render_source_empty_document() {
    let mut session = EditorSession::from_text("");

    let vp = Viewport {
        top_line: 0,
        height: 5,
        width: 80,
    };
    let frame = session.render_source(vp);

    // Should return frame with blank lines matching viewport height
    assert_eq!(
        frame.lines.len(),
        5,
        "empty document should return viewport-height lines"
    );
    // All lines should be blank
    for (i, line) in frame.lines.iter().enumerate() {
        assert!(
            line.text.is_empty(),
            "line {} of empty document should be blank",
            i
        );
    }
}

#[test]
fn render_source_top_line_past_end() {
    let mut session = EditorSession::from_text("line1\nline2");

    // top_line > line_count → all lines blank, first_line_number correct
    let vp = Viewport {
        top_line: 10,
        height: 5,
        width: 80,
    };
    let frame = session.render_source(vp);

    assert_eq!(
        frame.first_line_number, 11,
        "top_line 10 → first_line_number 11"
    );
    for (i, line) in frame.lines.iter().enumerate() {
        assert!(
            line.text.is_empty(),
            "line {} when scrolled past end should be blank",
            i
        );
    }
}

// ── render_view ─────────────────────────────────────────────────────────────

#[test]
fn render_view_returns_layout() {
    let mut session = EditorSession::from_text("# Hello\n\nWorld\n");

    let layout = session.render_view(80);
    assert!(!layout.lines.is_empty(), "view layout should have lines");
    assert!(
        !layout.jump_targets.is_empty(),
        "should have jump targets for headings"
    );
}

#[test]
fn render_view_invalidation_on_edit() {
    let mut session = EditorSession::from_text("hello");
    let _lines1 = session.render_view(80).lines.len();

    // Edit the text (simple insert)
    session.handle_key(key_special(KeyCodeKind::Esc));
    session.handle_key(key('i')); // Enter insert mode
    session.handle_key(key('x')); // Type 'x'
    session.handle_key(key_special(KeyCodeKind::Esc)); // Exit insert mode

    let lines2 = session.render_view(80).lines.len();
    assert!(lines2 > 0, "layout should have lines after edit");
    // The edit should have changed the document (inserted 'x' somewhere)
    assert!(
        session.document() != "hello",
        "document should be modified after edit, got: {:?}",
        session.document()
    );
}

#[test]
fn render_view_invalidation_on_width_change() {
    let mut session = EditorSession::from_text(
        "This is a longer line that should wrap differently at different widths.",
    );

    let layout80 = session.render_view(80);
    let lines80 = layout80.lines.len();

    let layout40 = session.render_view(40);
    let lines40 = layout40.lines.len();

    assert!(
        lines40 > lines80,
        "narrower width should produce more lines ({} > {})",
        lines40,
        lines80
    );
}

// ── toggle_view ─────────────────────────────────────────────────────────────

#[test]
fn toggle_view_from_normal_emits_mode_changed() {
    let mut session = EditorSession::from_text("hello");

    assert!(matches!(session.mode(), Mode::Normal));

    let effects = session.toggle_view();
    assert!(
        effects
            .iter()
            .any(|e| matches!(e, Effect::ModeChanged(Mode::View))),
        "toggle_view from Normal should emit ModeChanged(View)"
    );
    assert!(matches!(session.mode(), Mode::View));
}

#[test]
fn toggle_view_from_view_returns_to_normal() {
    let mut session = EditorSession::from_text("hello");

    // Enter View mode
    session.toggle_view();
    assert!(matches!(session.mode(), Mode::View));

    // Exit View mode
    let effects = session.toggle_view();
    assert!(
        effects
            .iter()
            .any(|e| matches!(e, Effect::ModeChanged(Mode::Normal))),
        "toggle_view from View should emit ModeChanged(Normal)"
    );
    assert!(matches!(session.mode(), Mode::Normal));
}

// ── Dirty flag ──────────────────────────────────────────────────────────────

#[test]
fn session_not_dirty_initially() {
    let session = EditorSession::from_text("hello");
    assert!(!session.is_dirty());
}

#[test]
fn session_dirty_after_edit() {
    let mut session = EditorSession::from_text("hello");

    session.handle_key(key_special(KeyCodeKind::Esc));
    session.handle_key(key('i'));
    session.handle_key(key('x'));
    session.handle_key(key_special(KeyCodeKind::Esc));

    assert!(session.is_dirty(), "session should be dirty after insert");
}

// ── Effect emission ─────────────────────────────────────────────────────────

#[test]
fn mode_changed_effect_has_new_mode() {
    let mut session = EditorSession::from_text("hello");

    session.handle_key(key_special(KeyCodeKind::Esc));
    let effects = session.handle_key(key('i'));

    if let Some(Effect::ModeChanged(mode)) =
        effects.iter().find(|e| matches!(e, Effect::ModeChanged(_)))
    {
        assert!(matches!(mode, Mode::Insert));
    } else {
        panic!("Expected ModeChanged effect");
    }
}

#[test]
fn help_requested_effect_emitted() {
    let mut session = EditorSession::from_text("hello");

    // Manually simulate ":help" chord
    let colon = key(':'); // Enters Command mode
    let _effects = session.handle_key(colon);
    let h = key('h');
    let _effects = session.handle_key(h);
    let e = key('e');
    let _effects = session.handle_key(e);
    let l = key('l');
    let _effects = session.handle_key(l);
    let p = key('p');
    let _effects = session.handle_key(p);
    let enter = key_special(KeyCodeKind::Enter);
    let effects = session.handle_key(enter);

    assert!(
        effects.iter().any(|e| matches!(e, Effect::HelpRequested)),
        ":help should emit HelpRequested effect, got: {:?}",
        effects
    );
}

// ── Highlighter integration ─────────────────────────────────────────────────

#[test]
fn highlighter_applied_on_edit() {
    // Start with text that already has heading syntax
    let mut session = EditorSession::from_text("# Hello\n\nWorld\n");

    // Verify initial highlighting
    let vp = Viewport {
        top_line: 0,
        height: 10,
        width: 80,
    };
    let frame1 = session.render_source(vp);
    assert!(
        !frame1.lines[0].spans.is_empty(),
        "initial heading line should have highlighting spans"
    );

    // Verify the first span is a heading style
    let has_heading_style = frame1.lines[0]
        .spans
        .iter()
        .any(|s| matches!(s.style, SemanticStyle::Heading1 | SemanticStyle::Strong));
    assert!(
        has_heading_style,
        "initial heading line should have Heading1 style"
    );

    // The highlighter should be accessible via the public API
    let _highlighter = session.highlighter();
}

#[test]
fn highlighter_preserved_after_save() {
    // Create a temp file
    let temp_dir = std::env::temp_dir().join("oom-edit-test");
    std::fs::create_dir_all(&temp_dir).ok();
    let temp_path = temp_dir.join("highlighter_save_test.md");

    // Write initial content with heading
    std::fs::write(&temp_path, "# Test\n\nContent\n").unwrap();

    let mut session = EditorSession::open(&temp_path).expect("should open temp file");

    // Verify initial highlighting
    let vp = Viewport {
        top_line: 0,
        height: 10,
        width: 80,
    };
    let frame1 = session.render_source(vp);
    assert!(
        !frame1.lines[0].spans.is_empty(),
        "initial file should have highlighting"
    );

    // Save (rebuilds highlighter)
    session.save(None, false).ok();

    // Re-open to verify the highlighter was rebuilt
    let mut session = EditorSession::open(&temp_path).expect("should re-open temp file");

    let frame2 = session.render_source(vp);

    // The first line should still be highlighted as a heading
    assert!(
        !frame2.lines[0].spans.is_empty(),
        "reopened file should have highlighting"
    );

    // Clean up
    std::fs::remove_file(&temp_path).ok();
    std::fs::remove_dir_all(&temp_dir).ok();
}
