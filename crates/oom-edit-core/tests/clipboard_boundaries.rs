//! Regression coverage for source syntax at rendered copy boundaries.

use oom_edit_core::{
    ClipboardContent, EditorSession, Effect, KeyCode, KeyCodeKind, KeyInput, Mode, Modifiers,
    RenderedPoint,
};

fn key(c: char) -> KeyInput {
    special(KeyCodeKind::Char(c))
}

fn special(kind: KeyCodeKind) -> KeyInput {
    KeyInput {
        code: KeyCode { kind },
        mods: Modifiers::default(),
    }
}

fn copied(effects: &[Effect]) -> &ClipboardContent {
    let contents: Vec<_> = effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::ClipboardWrite(content) => Some(content),
            _ => None,
        })
        .collect();
    assert_eq!(contents.len(), 1);
    contents[0]
}

fn select_all(session: &mut EditorSession, width: u16, shape: char, reverse: bool) {
    let layout = session.render_layout(width);
    let first = layout
        .lines
        .iter()
        .position(|line| line.kind == oom_edit_core::LineKind::Content)
        .unwrap();
    let last = layout
        .lines
        .iter()
        .rposition(|line| line.kind == oom_edit_core::LineKind::Content)
        .unwrap();
    let last_column = layout.lines[last]
        .atoms
        .last()
        .map_or(0, |atom| atom.columns.start);
    if reverse {
        session.move_to_rendered_point(RenderedPoint {
            row: last,
            column: last_column,
        });
        session.handle_key(key(shape));
        session.handle_key(special(KeyCodeKind::Home));
        for _ in 0..last_column {
            session.handle_key(key('h'));
        }
    } else {
        session.move_to_rendered_point(RenderedPoint {
            row: first,
            column: 0,
        });
        session.handle_key(key(shape));
        session.handle_key(special(KeyCodeKind::End));
        for _ in 0..last_column {
            session.handle_key(key('l'));
        }
    }
    assert_eq!(session.mode(), Mode::Select);
}

#[test]
fn whole_document_yank_preserves_heading_and_final_fence() {
    for source in [
        "# first\n\nsome text\n\n# second\n\n```txt\n//hello\n```",
        "# first\n\nsome text\n\n# second\n\n```txt\n//hello\n```\n",
        "## café 界\n\nsome repeated text text\n\n~~~txt\n//héllo 界\n~~~",
        "## café 界\n\nsome repeated text text\n\n~~~txt\n//héllo 界\n~~~\n",
        "- first\n\nsome text\n\nlast",
        "> first\n\nsome text\n\nlast",
        "***\n\n# first\n\nsome text\n\nlast\n\n***",
    ] {
        for width in [12, 120] {
            for shape in ['v', 'V'] {
                for reverse in [false, true] {
                    let mut session = EditorSession::from_text(source);
                    select_all(&mut session, width, shape, reverse);
                    let effects = session.handle_key(key('y'));
                    let content = copied(&effects);
                    assert_eq!(
                        content.markdown(),
                        source,
                        "width={width} shape={shape} reverse={reverse}"
                    );
                    assert!(!content.plain_text().contains('#'));
                    assert!(!content.plain_text().contains("```"));
                    assert!(!content.plain_text().contains("~~~"));
                    assert!(content.plain_text().contains("some"));
                    assert_eq!(session.document(), source);
                    assert_eq!(session.mode(), Mode::Normal);
                }
            }
        }
    }
}

#[test]
fn character_yank_does_not_include_an_unselected_final_paragraph_newline() {
    let source = "# first\n\nsome text\n\nlast\n";
    for shape in ['v', 'V'] {
        let mut session = EditorSession::from_text(source);
        select_all(&mut session, 40, shape, false);
        let effects = session.handle_key(key('y'));
        let expected = if shape == 'v' {
            source.trim_end_matches('\n')
        } else {
            source
        };
        assert_eq!(copied(&effects).markdown(), expected);
    }
}

#[test]
fn whole_document_yank_register_pastes_complete_source_into_empty_buffer() {
    for source in [
        "# first\n\nsome text\n\n# second\n\n```txt\n//hello\n```",
        "# first\n\nsome text\n\n# second\n\n```txt\n//hello\n```\n",
    ] {
        for shape in ['v', 'V'] {
            let mut session = EditorSession::from_text(source);
            select_all(&mut session, 40, shape, false);
            session.handle_key(key('"'));
            session.handle_key(key('a'));
            session.handle_key(key('y'));
            session.handle_key(key('i'));
            session.jump_to_offset(source.len()).unwrap();
            for _ in source.chars() {
                session.handle_key(special(KeyCodeKind::Backspace));
            }
            session.handle_key(special(KeyCodeKind::Esc));
            assert_eq!(session.document(), "");
            for c in "\"ap".chars() {
                session.handle_key(key(c));
            }
            assert_eq!(session.document(), source, "shape={shape}");
        }
    }
}

#[test]
fn partial_heading_and_fence_yanks_do_not_capture_unselected_syntax() {
    let source = "# first heading\n\n```txt\n//hello world\n```\n";
    for (needle, expected) in [("irst", "irst"), ("hello", "hello")] {
        let start = source.find(needle).unwrap();
        let mut session = EditorSession::from_text(source);
        session
            .select_source_offsets(start, start + needle.len() - 1, 12)
            .unwrap();
        let effects = session.handle_key(key('y'));
        assert_eq!(copied(&effects).markdown(), expected);
        assert_eq!(session.document(), source);
    }
}

#[test]
fn block_yank_at_heading_start_stays_rectangular() {
    let mut session = EditorSession::from_text("# first\n\nsecond");
    session.render_layout(40);
    session.handle_key(KeyInput {
        code: KeyCode {
            kind: KeyCodeKind::Char('v'),
        },
        mods: Modifiers {
            ctrl: true,
            ..Modifiers::default()
        },
    });
    session.handle_key(key('l'));
    let effects = session.handle_key(key('y'));
    assert_eq!(copied(&effects).markdown(), "fi");
}

#[test]
fn partial_heading_delete_and_change_keep_unselected_source() {
    let source = "# first heading\n\nlast";
    for operator in ['d', 'c'] {
        let mut session = EditorSession::from_text(source);
        session.select_source_offsets(3, 6, 40).unwrap();
        session.handle_key(key(operator));
        assert_eq!(session.document(), "# f heading\n\nlast");
    }
}
