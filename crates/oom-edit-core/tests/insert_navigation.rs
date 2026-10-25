//! Insert whitespace and visual-row navigation regressions through the core.
use oom_edit_core::{EditorSession, KeyCode, KeyCodeKind, KeyInput, Modifiers, Viewport};
use unicode_width::UnicodeWidthStr;

fn key(kind: KeyCodeKind) -> KeyInput {
    KeyInput {
        code: KeyCode { kind },
        mods: Modifiers::default(),
    }
}
fn send(session: &mut EditorSession, kind: KeyCodeKind) {
    session.handle_key(key(kind));
}
fn vp(width: u16, wrap: bool) -> Viewport {
    Viewport {
        top_line: 0,
        height: 20,
        width,
        wrap,
        left_col: 0,
        skip_rows: 0,
    }
}
fn insert(source: &str, width: u16, column: usize) -> EditorSession {
    let mut session = EditorSession::from_text(source);
    session.render_layout(80);
    send(&mut session, KeyCodeKind::Char('i'));
    for _ in 0..column {
        send(&mut session, KeyCodeKind::Right);
    }
    session.render_source(vp(width, true));
    session
}

#[test]
fn repeated_enter_preserves_exact_empty_source_lines_and_zero_column() {
    for source in [
        "This is the first line of text",
        "A sentence.",
        "café 界",
        "Sentence\nTail",
    ] {
        let mut session = EditorSession::from_text(source);
        session.render_layout(80);
        send(&mut session, KeyCodeKind::Char('A'));
        let first_line = source.split('\n').next().unwrap();
        let suffix = source.strip_prefix(first_line).unwrap();
        for count in 1..=3 {
            send(&mut session, KeyCodeKind::Enter);
            let frame = session.render_source(vp(12, true));
            assert_eq!(
                session.document(),
                format!("{first_line}{}{suffix}", "\n".repeat(count))
            );
            assert_eq!(session.cursor(), (count, 0));
            assert_eq!(
                frame.lines.iter().filter(|line| line.text == " ").count(),
                0
            );
        }
        send(&mut session, KeyCodeKind::Char('X'));
        assert_eq!(session.line(3).as_deref(), Some("X"));
        send(&mut session, KeyCodeKind::Esc);
        send(&mut session, KeyCodeKind::Char('u'));
        assert_eq!(session.document(), source);
    }
}

#[test]
fn intentional_code_indentation_survives_newline_insertion() {
    for prefix in ["    ", "\t"] {
        let source = format!("{prefix}let value = 1;");
        let mut session = EditorSession::from_text(&source);
        session.render_layout(80);
        send(&mut session, KeyCodeKind::Char('A'));
        send(&mut session, KeyCodeKind::Enter);
        assert_eq!(session.document(), format!("{source}\n{prefix}"));
        assert_eq!(session.cursor(), (1, prefix.chars().count()));
    }
}

#[test]
fn insert_arrows_move_by_wrapped_rows_and_retain_the_visual_goal_across_short_lines() {
    let source = "abcdefghijklmnop\nxy\nABCDEFGHIJKLMNOP";
    let mut session = insert(source, 5, 7);
    for (motion, expected) in [
        (KeyCodeKind::Up, (0, 2)),
        (KeyCodeKind::Down, (0, 7)),
        (KeyCodeKind::Down, (0, 12)),
        (KeyCodeKind::Down, (0, 16)),
        (KeyCodeKind::Down, (1, 2)),
        (KeyCodeKind::Down, (2, 2)),
        (KeyCodeKind::Down, (2, 7)),
        (KeyCodeKind::Up, (2, 2)),
        (KeyCodeKind::Up, (1, 2)),
        (KeyCodeKind::Up, (0, 16)),
        (KeyCodeKind::Up, (0, 12)),
    ] {
        send(&mut session, motion);
        session.render_source(vp(5, true));
        assert_eq!(session.cursor(), expected, "{motion:?}");
        assert_eq!(session.document(), source);
        assert!(!session.is_dirty());
    }
}

#[test]
fn unicode_tabs_and_combining_scalars_use_the_rendered_cell_mapping_for_vertical_motion() {
    for source in [
        "a\tbcdefghijklmno",
        "界界界abcdefghijklmno",
        "ae\u{301}bcdefghijklmnop",
    ] {
        let mut session = insert(source, 5, 8);
        let viewport = vp(5, true);
        let before = session.render_source(viewport);
        let row = usize::from(before.cursor.0);
        assert!(row > 0);
        let column: String = before.lines[row]
            .text
            .chars()
            .take(usize::from(before.cursor.1))
            .collect();
        let goal = column.width();
        let expected_offset = session
            .source_offset_at_viewport_cell(viewport, row - 1, goal)
            .unwrap();
        let expected = session.position_for_offset(expected_offset).unwrap();
        let original = session.cursor();
        send(&mut session, KeyCodeKind::Up);
        assert_eq!(
            session.cursor(),
            (expected.line, expected.column),
            "{source:?}"
        );
        send(&mut session, KeyCodeKind::Down);
        assert_eq!(session.cursor(), original, "{source:?}");
        assert_eq!(session.document(), source);
    }
}

#[test]
fn horizontal_keys_edits_and_resize_reanchor_the_visual_column() {
    let mut session = insert("abcdefghijklmnop", 5, 7);
    send(&mut session, KeyCodeKind::Up);
    send(&mut session, KeyCodeKind::Home);
    send(&mut session, KeyCodeKind::Down);
    assert_eq!(session.cursor(), (0, 5));
    send(&mut session, KeyCodeKind::Right);
    send(&mut session, KeyCodeKind::Char('X'));
    send(&mut session, KeyCodeKind::Up);
    assert_eq!(session.cursor(), (0, 2));
    send(&mut session, KeyCodeKind::End);
    session.render_source(vp(4, true));
    send(&mut session, KeyCodeKind::Up);
    assert_eq!(session.cursor(), (0, 13));
    send(&mut session, KeyCodeKind::Esc);
    send(&mut session, KeyCodeKind::Char('u'));
    assert_eq!(session.document(), "abcdefghijklmnop");
}

#[test]
fn exact_width_eof_caret_can_move_up_from_its_blank_continuation() {
    let mut session = insert("abcdefghij", 5, 10);
    assert_eq!(session.render_source(vp(5, true)).cursor, (2, 0));
    send(&mut session, KeyCodeKind::Up);
    assert_eq!(session.cursor(), (0, 5));
    send(&mut session, KeyCodeKind::Up);
    assert_eq!(session.cursor(), (0, 0));
    send(&mut session, KeyCodeKind::Up);
    assert_eq!(session.cursor(), (0, 0));
}

#[test]
fn nowrap_keeps_logical_motion_and_tiny_viewports_do_not_mutate_text() {
    let mut session = insert("abcdefghijklmnop\nxy", 5, 7);
    session.render_source(vp(5, false));
    send(&mut session, KeyCodeKind::Down);
    assert_eq!(session.cursor(), (1, 2));
    send(&mut session, KeyCodeKind::Up);
    assert_eq!(session.cursor(), (0, 7));
    for width in [0, 1] {
        session.render_source(vp(width, true));
        for kind in [
            KeyCodeKind::Up,
            KeyCodeKind::Down,
            KeyCodeKind::Down,
            KeyCodeKind::Up,
        ] {
            send(&mut session, kind);
            let (row, column) = session.cursor();
            assert!(row < session.line_count());
            assert!(column <= session.line(row).unwrap().chars().count());
        }
    }
    assert_eq!(session.document(), "abcdefghijklmnop\nxy");
}

#[test]
fn all_plain_insert_entries_and_fresh_typing_keep_blank_lines_unindented() {
    for entry in ['i', 'a', 'A', 'o', 'O'] {
        for retained in [false, true] {
            let mut session = EditorSession::from_text("This is the first line of text");
            if retained {
                session.rendered_viewport(12, 0, 20);
            } else {
                session.render_layout(12);
            }
            send(&mut session, KeyCodeKind::Char(entry));
            for _ in 0..3 {
                session.render_source(vp(12, true));
                send(&mut session, KeyCodeKind::Enter);
                assert_eq!(session.cursor().1, 0, "{entry} retained={retained}");
                assert!(!session.line(session.cursor().0).unwrap().starts_with(' '));
            }
        }
    }
    let mut session = EditorSession::from_text("");
    session.render_layout(12);
    send(&mut session, KeyCodeKind::Char('i'));
    for character in "This is the first line of text".chars() {
        send(&mut session, KeyCodeKind::Char(character));
        session.render_source(vp(12, true));
    }
    for count in 1..=3 {
        send(&mut session, KeyCodeKind::Enter);
        session.render_source(vp(12, true));
        assert_eq!(
            session.document(),
            format!("This is the first line of text{}", "\n".repeat(count))
        );
        assert_eq!(session.cursor(), (count, 0));
    }
}
