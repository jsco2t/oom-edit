//! Editable rendered search through the terminal-neutral session API.

use oom_edit_core::{EditorSession, KeyCode, KeyCodeKind, KeyInput, Mode, Modifiers};

fn special(kind: KeyCodeKind) -> KeyInput {
    KeyInput {
        code: KeyCode { kind },
        mods: Modifiers::default(),
    }
}
fn key(c: char) -> KeyInput {
    special(KeyCodeKind::Char(c))
}
fn type_text(session: &mut EditorSession, text: &str) {
    for c in text.chars() {
        session.handle_key(key(c));
    }
}
fn prompt(session: &EditorSession) -> String {
    session.rendered_search_prompt().unwrap()
}
fn session() -> EditorSession {
    let mut session =
        EditorSession::from_text("origin\n\nalpha\n\nbeta\n\nalpha\n\ncafé 界.*hjkl\n");
    session.render_layout(80);
    session
}

#[test]
fn backspace_edits_the_draft_and_recomputes_the_match_from_fixed_origin() {
    let mut session = session();
    type_text(&mut session, "/alphax");
    session.handle_key(special(KeyCodeKind::Backspace));
    assert_eq!(prompt(&session), "/alpha");
    assert_eq!(session.cursor(), (2, 0));
    session.handle_key(special(KeyCodeKind::Enter));
    assert_eq!(session.rendered_search().unwrap().pattern, "alpha");
    assert_eq!(session.rendered_search_prompt(), None);
    session.handle_key(key('n'));
    assert_eq!(session.cursor(), (6, 0));
}

#[test]
fn arrows_and_delete_replace_text_in_the_middle_without_changing_the_document() {
    let mut session = session();
    let original = session.document();
    type_text(&mut session, "/alXha");
    for _ in 0..3 {
        session.handle_key(special(KeyCodeKind::Left));
    }
    session.handle_key(special(KeyCodeKind::Delete));
    session.handle_key(key('p'));
    assert_eq!(prompt(&session), "/alpha");
    session.handle_key(special(KeyCodeKind::Right));
    session.handle_key(special(KeyCodeKind::Backspace));
    session.handle_key(key('h'));
    assert_eq!(prompt(&session), "/alpha");
    assert_eq!(session.document(), original);
    assert_eq!(session.mode(), Mode::Normal);
}

#[test]
fn home_end_and_repeated_boundary_operations_are_safe() {
    let mut session = session();
    type_text(&mut session, "/eta");
    session.handle_key(special(KeyCodeKind::Home));
    session.handle_key(special(KeyCodeKind::Left));
    session.handle_key(special(KeyCodeKind::Backspace));
    session.handle_key(key('b'));
    assert_eq!(prompt(&session), "/beta");
    session.handle_key(special(KeyCodeKind::End));
    session.handle_key(special(KeyCodeKind::Right));
    session.handle_key(special(KeyCodeKind::Delete));
    assert_eq!(prompt(&session), "/beta");
    for _ in 0..8 {
        session.handle_key(special(KeyCodeKind::Backspace));
    }
    assert_eq!(prompt(&session), "/");
    for kind in [
        KeyCodeKind::Delete,
        KeyCodeKind::Left,
        KeyCodeKind::Right,
        KeyCodeKind::Home,
        KeyCodeKind::End,
    ] {
        session.handle_key(special(kind));
    }
    assert_eq!(prompt(&session), "/");
    session.handle_key(key('x'));
    assert_eq!(prompt(&session), "/x");
}

#[test]
fn unicode_punctuation_and_vim_letters_are_literal_and_edit_on_char_boundaries() {
    let mut session = session();
    type_text(&mut session, "/café 界.*hjkl");
    assert_eq!(prompt(&session), "/café 界.*hjkl");
    session.handle_key(special(KeyCodeKind::Home));
    for _ in 0..3 {
        session.handle_key(special(KeyCodeKind::Right));
    }
    session.handle_key(special(KeyCodeKind::Delete));
    session.handle_key(key('é'));
    session.handle_key(special(KeyCodeKind::Right));
    session.handle_key(special(KeyCodeKind::Delete));
    session.handle_key(key('界'));
    assert_eq!(prompt(&session), "/café 界.*hjkl");
    session.handle_key(KeyInput {
        code: KeyCode {
            kind: KeyCodeKind::Char('A'),
        },
        mods: Modifiers {
            shift: true,
            ..Modifiers::default()
        },
    });
    assert_eq!(prompt(&session), "/café 界A.*hjkl");
}

#[test]
fn cursor_only_keys_do_not_search_or_move_the_document_cursor() {
    let mut session = session();
    type_text(&mut session, "/alpha");
    let matched = session.cursor();
    let rendered = session.rendered_cursor();
    for kind in [
        KeyCodeKind::Left,
        KeyCodeKind::Home,
        KeyCodeKind::Right,
        KeyCodeKind::End,
        KeyCodeKind::Up,
        KeyCodeKind::Down,
    ] {
        session.handle_key(special(kind));
        assert_eq!(session.cursor(), matched);
        assert_eq!(session.rendered_cursor(), rendered);
        assert_eq!(prompt(&session), "/alpha");
    }
}

#[test]
fn cancellation_preserves_the_previous_submitted_search_after_edits() {
    let mut session = session();
    type_text(&mut session, "/alpha");
    session.handle_key(special(KeyCodeKind::Enter));
    type_text(&mut session, "/betax");
    session.handle_key(special(KeyCodeKind::Backspace));
    assert_eq!(prompt(&session), "/beta");
    session.handle_key(special(KeyCodeKind::Esc));
    assert_eq!(session.rendered_search_prompt(), None);
    assert_eq!(session.rendered_search().unwrap().pattern, "alpha");
}

#[test]
fn search_editing_also_works_with_retained_viewport_rows() {
    let mut session = EditorSession::from_text("origin\n\nalpha\n\nbeta\n");
    session.rendered_viewport(80, 0, 12);
    type_text(&mut session, "/betax");
    session.handle_key(special(KeyCodeKind::Backspace));
    assert_eq!(prompt(&session), "/beta");
    assert_eq!(session.cursor(), (4, 0));
    session.handle_key(special(KeyCodeKind::Home));
    session.handle_key(special(KeyCodeKind::Delete));
    session.handle_key(key('b'));
    assert_eq!(prompt(&session), "/beta");
    assert_eq!(session.cursor(), (4, 0));
}
