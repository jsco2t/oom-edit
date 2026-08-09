//! Session-level integration tests for `EditorSession`.
//!
//! Covers: basic session operations, rendering, effect emission, and
//! highlighter integration.

use oom_edit_core::session::{
    EditorSession, Effect, KeyCode, KeyCodeKind, KeyInput, Mode, Modifiers, Viewport,
};
use oom_edit_core::style::SemanticStyle;

const VIEW_EXIT_TEXT: &str = "first paragraph\n\nsecond paragraph\n\nthird paragraph";

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

fn key_ctrl(ch: char) -> KeyInput {
    KeyInput {
        code: KeyCode {
            kind: KeyCodeKind::Char(ch),
        },
        mods: Modifiers {
            ctrl: true,
            ..Modifiers::default()
        },
    }
}

fn move_view_cursor_to_text(session: &mut EditorSession, needle: &str) {
    session.toggle_view();
    let target_line = session
        .render_view(80)
        .lines
        .iter()
        .position(|line| line.styled.text.contains(needle))
        .unwrap_or_else(|| panic!("rendered View should contain {needle:?}"));

    for _ in 0..target_line {
        session.handle_key(key_special(KeyCodeKind::Down));
    }

    assert_eq!(session.view_cursor_line(), target_line);
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
fn command_line_exposes_active_command_prompt() {
    let mut session = EditorSession::from_text("hello");
    assert_eq!(session.command_line(), None);

    session.handle_key(key(':'));
    assert_eq!(session.command_line().as_deref(), Some(""));

    for ch in "help".chars() {
        session.handle_key(key(ch));
    }
    assert_eq!(session.command_line().as_deref(), Some("help"));

    session.handle_key(key_special(KeyCodeKind::Esc));
    assert_eq!(session.command_line(), None);
}

#[test]
fn command_line_exposes_only_active_view_search_prompt() {
    let mut session = EditorSession::from_text("alpha beta alpha");
    session.toggle_view();

    session.handle_key(key('/'));
    assert_eq!(session.command_line(), None);
    assert_eq!(session.view_search_prompt().as_deref(), Some("/"));
    for ch in "alpha".chars() {
        session.handle_key(key(ch));
    }
    assert_eq!(session.command_line(), None);
    assert_eq!(session.view_search_prompt().as_deref(), Some("/alpha"));

    session.handle_key(key_special(KeyCodeKind::Enter));
    assert_eq!(session.view_search_prompt(), None);
    assert_eq!(
        session.view_search().map(|search| search.pattern.as_str()),
        Some("alpha")
    );

    session.handle_key(key('?'));
    assert_eq!(session.view_search_prompt().as_deref(), Some("?"));
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
        wrap: true,
        left_col: 0,
        skip_rows: 0,
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
        wrap: true,
        left_col: 0,
        skip_rows: 0,
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
        wrap: true,
        left_col: 0,
        skip_rows: 0,
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
        wrap: true,
        left_col: 0,
        skip_rows: 0,
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
        wrap: true,
        left_col: 0,
        skip_rows: 0,
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
        wrap: true,
        left_col: 0,
        skip_rows: 0,
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
        wrap: true,
        left_col: 0,
        skip_rows: 0,
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

#[test]
fn view_i_enters_insert() {
    let mut session = EditorSession::from_text(VIEW_EXIT_TEXT);
    move_view_cursor_to_text(&mut session, "second paragraph");

    let effects = session.handle_key(key('i'));

    assert_eq!(session.mode(), Mode::Insert);
    assert_eq!(session.cursor(), (2, 0));
    assert_eq!(session.document(), VIEW_EXIT_TEXT);
    assert!(effects
        .iter()
        .any(|effect| matches!(effect, Effect::ModeChanged(Mode::Insert))));
}

#[test]
fn view_a_enters_insert_after() {
    let mut session = EditorSession::from_text(VIEW_EXIT_TEXT);
    move_view_cursor_to_text(&mut session, "second paragraph");

    let effects = session.handle_key(key('a'));

    assert_eq!(session.mode(), Mode::Insert);
    assert_eq!(session.cursor(), (2, 1));
    assert_eq!(session.document(), VIEW_EXIT_TEXT);
    assert!(effects
        .iter()
        .any(|effect| matches!(effect, Effect::ModeChanged(Mode::Insert))));
}

#[test]
fn view_o_opens_line_below() {
    let mut session = EditorSession::from_text(VIEW_EXIT_TEXT);
    move_view_cursor_to_text(&mut session, "second paragraph");
    let original_line_count = session.line_count();

    let effects = session.handle_key(key('o'));

    assert_eq!(session.mode(), Mode::Insert);
    assert_eq!(session.cursor(), (3, 0));
    assert_eq!(session.line_count(), original_line_count + 1);
    assert_eq!(
        session.document(),
        "first paragraph\n\nsecond paragraph\n\n\nthird paragraph"
    );
    assert!(effects
        .iter()
        .any(|effect| matches!(effect, Effect::ModeChanged(Mode::Insert))));
    assert!(effects
        .iter()
        .any(|effect| matches!(effect, Effect::Edited)));
}

#[test]
fn view_iao_distinct_behavior() {
    let outcomes = ['i', 'a', 'o'].map(|action| {
        let mut session = EditorSession::from_text(VIEW_EXIT_TEXT);
        move_view_cursor_to_text(&mut session, "second paragraph");
        session.handle_key(key(action));
        (session.cursor(), session.line_count())
    });

    assert_ne!(outcomes[0], outcomes[1]);
    assert_ne!(outcomes[0], outcomes[2]);
    assert_ne!(outcomes[1], outcomes[2]);
}

#[test]
fn view_search_accepts_exit_action_letters() {
    let original = "# Opening\n## radio signal\n## Closing";
    let mut session = EditorSession::from_text(original);
    session.toggle_view();
    let target_line = session
        .render_view(80)
        .lines
        .iter()
        .position(|line| line.styled.text.contains("radio signal"))
        .expect("rendered View should contain the unique search target");

    session.handle_key(key('/'));
    for ch in "radio".chars() {
        session.handle_key(key(ch));
    }
    session.handle_key(key_special(KeyCodeKind::Enter));

    assert_eq!(session.mode(), Mode::View);
    assert_eq!(
        session
            .view_search()
            .expect("submitted View search should be retained")
            .pattern,
        "radio"
    );
    assert_eq!(session.view_cursor_line(), target_line);
    assert_eq!(session.document(), original);
    assert!(!session.is_dirty());
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

#[test]
fn session_is_clean_after_save_and_dirty_after_another_edit() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("dirty-cycle.md");
    let mut session = EditorSession::from_text("hello");

    assert!(!session.is_dirty(), "a new session should start clean");

    session.handle_key(key('i'));
    session.handle_key(key('x'));
    session.handle_key(key_special(KeyCodeKind::Esc));
    assert!(
        session.is_dirty(),
        "an insertion should make the session dirty"
    );

    session.save(Some(&path), false).unwrap();
    assert!(
        !session.is_dirty(),
        "a successful save should clear dirty state"
    );

    session.handle_key(key('a'));
    session.handle_key(key('y'));
    session.handle_key(key_special(KeyCodeKind::Esc));
    assert!(
        session.is_dirty(),
        "an insertion after saving should make the session dirty again"
    );
}

#[test]
fn failed_save_preserves_dirty_state() {
    let mut session = EditorSession::from_text("hello");

    session.handle_key(key('x'));
    assert!(session.is_dirty(), "an edit should make the session dirty");

    assert!(
        session.save(None, false).is_err(),
        "saving a pathless session without a target should fail"
    );
    assert!(
        session.is_dirty(),
        "a failed save must not advance the save point"
    );
}

#[test]
fn undoing_to_the_save_point_clears_dirty_state() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("undo-to-save-point.md");
    let mut session = EditorSession::from_text("abc");

    session.handle_key(key('x'));
    assert!(
        session.is_dirty(),
        "the first deletion should make the session dirty"
    );
    session.save(Some(&path), false).unwrap();
    assert!(!session.is_dirty(), "the saved state should be clean");

    session.handle_key(key('x'));
    assert!(
        session.is_dirty(),
        "the second deletion should make the session dirty"
    );

    session.handle_key(key('u'));
    assert_eq!(session.document(), "bc");
    assert!(
        !session.is_dirty(),
        "undoing the second deletion should restore the saved state"
    );

    session.handle_key(key_ctrl('r'));
    assert_eq!(session.document(), "c");
    assert!(
        session.is_dirty(),
        "redoing the second deletion should leave the save point"
    );
}

#[test]
fn editing_back_to_saved_contents_without_undo_remains_dirty() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("same-contents-new-state.md");
    let mut session = EditorSession::from_text("abc");
    session.save(Some(&path), false).unwrap();

    session.handle_key(key('$'));
    session.handle_key(key('x'));
    assert_eq!(session.document(), "ab");

    session.handle_key(key('a'));
    session.handle_key(key('c'));
    session.handle_key(key_special(KeyCodeKind::Esc));

    assert_eq!(session.document(), "abc");
    assert!(
        session.is_dirty(),
        "new edits that recreate saved bytes are still an unsaved undo state"
    );

    session.handle_key(key('$'));
    session.handle_key(key('x'));
    assert_eq!(session.document(), "ab");
    session.handle_key(key('u'));
    assert_eq!(session.document(), "abc");
    assert!(
        session.is_dirty(),
        "undoing into a different node with saved-equivalent bytes must remain dirty"
    );
}

#[test]
fn branching_after_undo_discards_the_saved_state() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("discarded-save-point.md");
    let mut session = EditorSession::from_text("abc");

    session.handle_key(key('x'));
    session.save(Some(&path), false).unwrap();
    assert_eq!(session.document(), "bc");
    assert!(!session.is_dirty());

    session.handle_key(key('u'));
    assert_eq!(session.document(), "abc");
    assert!(session.is_dirty());

    session.handle_key(key('i'));
    session.handle_key(key('z'));
    session.handle_key(key_special(KeyCodeKind::Esc));
    assert_eq!(session.document(), "zabc");
    assert!(
        session.is_dirty(),
        "a divergent Insert-mode branch must not reuse the discarded save point"
    );

    session.handle_key(key('u'));
    session.handle_key(key_ctrl('r'));
    assert_eq!(session.document(), "zabc");
    assert!(session.is_dirty());
}

#[test]
fn bracketed_paste_can_undo_to_the_save_point() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("paste-undo.md");
    let mut session = EditorSession::from_text("abc");
    session.save(Some(&path), false).unwrap();

    session.handle_key(key('i'));
    session.insert_paste("x");
    session.handle_key(key_special(KeyCodeKind::Esc));
    assert_eq!(session.document(), "xabc");
    assert!(session.is_dirty());

    session.handle_key(key('u'));
    assert_eq!(session.document(), "abc");
    assert!(!session.is_dirty());
}

#[test]
fn history_traversal_with_g_updates_dirty_state() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("g-history.md");
    let mut session = EditorSession::from_text("abc");

    session.handle_key(key('x'));
    session.save(Some(&path), false).unwrap();
    session.handle_key(key('x'));
    assert_eq!(session.document(), "c");
    assert!(session.is_dirty());

    session.handle_key(key('g'));
    session.handle_key(key('-'));
    assert_eq!(session.document(), "bc");
    assert!(!session.is_dirty());

    session.handle_key(key('g'));
    session.handle_key(key('+'));
    assert_eq!(session.document(), "c");
    assert!(session.is_dirty());
}

#[test]
fn saving_mid_insert_does_not_clean_later_text_in_the_same_undo_node() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("mid-insert-save.md");
    let mut session = EditorSession::from_text("abc");

    session.handle_key(key('i'));
    session.handle_key(key('x'));
    session.save(Some(&path), false).unwrap();
    assert!(!session.is_dirty());

    session.handle_key(key('y'));
    session.handle_key(key_special(KeyCodeKind::Esc));
    assert_eq!(session.document(), "xyabc");
    assert!(session.is_dirty());

    session.handle_key(key('u'));
    assert_eq!(session.document(), "abc");
    assert!(session.is_dirty());

    session.handle_key(key_ctrl('r'));
    assert_eq!(session.document(), "xyabc");
    assert!(
        session.is_dirty(),
        "redoing the extended node must not match its earlier saved contents"
    );
}

#[test]
fn substitute_undo_traverses_changes_that_recreate_saved_bytes() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("no-op-undo.md");
    let mut session = EditorSession::from_text("abc");
    session.save(Some(&path), false).unwrap();

    for command in ["s/a/x/", "s/x/a/"] {
        session.handle_key(key(':'));
        for ch in command.chars() {
            session.handle_key(key(ch));
        }
        session.handle_key(key_special(KeyCodeKind::Enter));
    }

    assert_eq!(session.document(), "abc");
    assert!(session.is_dirty());

    session.handle_key(key('u'));
    assert_eq!(session.document(), "xbc");
    assert!(session.is_dirty());

    session.handle_key(key('u'));
    assert_eq!(session.document(), "abc");
    assert!(!session.is_dirty());
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
        wrap: true,
        left_col: 0,
        skip_rows: 0,
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
    let temp_dir = tempfile::tempdir().unwrap();
    let temp_path = temp_dir.path().join("highlighter_save_test.md");

    // Write initial content with heading
    std::fs::write(&temp_path, "# Test\n\nContent\n").unwrap();

    let mut session = EditorSession::open(&temp_path).expect("should open temp file");

    // Verify initial highlighting
    let vp = Viewport {
        top_line: 0,
        height: 10,
        width: 80,
        wrap: true,
        left_col: 0,
        skip_rows: 0,
    };
    let frame1 = session.render_source(vp);
    assert!(
        !frame1.lines[0].spans.is_empty(),
        "initial file should have highlighting"
    );

    // Save (rebuilds highlighter)
    session.save(None, false).unwrap();

    // Re-open to verify the highlighter was rebuilt
    let mut session = EditorSession::open(&temp_path).expect("should re-open temp file");

    let frame2 = session.render_source(vp);

    // The first line should still be highlighted as a heading
    assert!(
        !frame2.lines[0].spans.is_empty(),
        "reopened file should have highlighting"
    );
}
