//! Search state and rendered matching invariants through the core facade.
use oom_edit_core::{EditorSession, KeyCode, KeyCodeKind, KeyInput, LineKind, Modifiers};

fn key(kind: KeyCodeKind) -> KeyInput {
    KeyInput {
        code: KeyCode { kind },
        mods: Modifiers::default(),
    }
}
fn type_keys(session: &mut EditorSession, text: &str) {
    for c in text.chars() {
        session.handle_key(key(KeyCodeKind::Char(c)));
    }
}
fn submit(session: &mut EditorSession, text: &str) {
    type_keys(session, text);
    session.handle_key(key(KeyCodeKind::Enter));
}

#[test]
fn clearing_search_removes_the_submitted_pattern_without_editing_text() {
    let mut session = EditorSession::from_text("how how\n\nhow\n");
    session.render_layout(80);
    submit(&mut session, "/how");
    let before = session.document();
    submit(&mut session, ":noh");
    assert!(session.rendered_search().is_none());
    assert_eq!(session.document(), before);
    assert!(!session.is_dirty());
    session.handle_key(key(KeyCodeKind::Char('n')));
    assert!(session.rendered_search().is_none());
}

#[test]
fn rendered_match_oracle_excludes_hidden_urls_and_synthetic_footer_rows() {
    let mut session = EditorSession::from_text("# how\n\n[how](https://how.invalid)\n");
    session.render_layout(80);
    submit(&mut session, "/how");
    let search = session.rendered_search().unwrap().clone();
    let matches: usize = session
        .render_layout(80)
        .lines
        .iter()
        .filter(|line| line.kind == LineKind::Content)
        .map(|line| search.find_matches(&line.styled.text).len())
        .sum();
    assert_eq!(matches, 2);
    let retained = session.rendered_viewport(80, 0, 100);
    assert_eq!(
        retained
            .lines
            .iter()
            .filter(|line| line.kind == LineKind::Content)
            .map(|line| search.find_matches(&line.styled.text).len())
            .sum::<usize>(),
        2
    );
}

#[test]
fn empty_cancelled_and_submitted_drafts_keep_the_existing_search_history() {
    let mut session = EditorSession::from_text("how\n\nnow\n");
    session.rendered_viewport(12, 0, 10);
    submit(&mut session, "/how");
    type_keys(&mut session, "/now");
    session.handle_key(key(KeyCodeKind::Esc));
    assert_eq!(session.rendered_search().unwrap().pattern, "how");
    type_keys(&mut session, "/");
    assert!(session.rendered_search().unwrap().pattern.is_empty());
    session.handle_key(key(KeyCodeKind::Esc));
    assert_eq!(session.rendered_search().unwrap().pattern, "how");
}
