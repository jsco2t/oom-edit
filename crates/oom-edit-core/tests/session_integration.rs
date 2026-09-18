//! Integration coverage for the four-mode rendered-first session contract.

use oom_edit_core::{
    ClipboardContent, EditorSession, Effect, KeyCode, KeyCodeKind, KeyInput, Mode, Modifiers,
    RenderedLineRole, RenderedPoint, SelectionShape, SemanticStyle, Viewport,
};
use unicode_width::UnicodeWidthStr;

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

#[test]
fn pointer_move_and_drag_follow_rendered_source_atoms() {
    let source = "one **two** three\n\n| A | B |\n|---|---|\n| same | same |\n";
    let mut session = EditorSession::from_text(source);
    let layout = session.render_layout(18).clone();
    let two = source.find("two").unwrap();
    let start = layout
        .lines
        .iter()
        .enumerate()
        .flat_map(|(row, line)| {
            line.atoms.iter().filter_map(move |atom| {
                (atom.source.as_ref().is_some_and(|range| range.start == two)).then_some(
                    RenderedPoint {
                        row,
                        column: atom.columns.start,
                    },
                )
            })
        })
        .next()
        .unwrap();
    let end = layout
        .lines
        .iter()
        .enumerate()
        .flat_map(|(row, line)| {
            line.atoms.iter().filter_map(move |atom| {
                (atom
                    .source
                    .as_ref()
                    .is_some_and(|range| range.start == two + 2))
                .then_some(RenderedPoint {
                    row,
                    column: atom.columns.start,
                })
            })
        })
        .next()
        .unwrap();
    session.move_to_rendered_point(start);
    assert_eq!(session.cursor(), (0, 6));
    session.select_rendered_points(start, end);
    assert_eq!(session.mode(), Mode::Select);
    assert_eq!(
        session.rendered_selection().unwrap().source_ranges,
        vec![two..two + 3]
    );
    assert_eq!(session.rendered_cursor(), end);

    let repeated = source.rfind("same").unwrap();
    let repeated_point = layout
        .lines
        .iter()
        .enumerate()
        .flat_map(|(row, line)| {
            line.atoms.iter().filter_map(move |atom| {
                (atom
                    .source
                    .as_ref()
                    .is_some_and(|range| range.start == repeated))
                .then_some(RenderedPoint {
                    row,
                    column: atom.columns.start,
                })
            })
        })
        .next()
        .unwrap();
    session.move_to_rendered_point(repeated_point);
    assert_eq!(
        session.position_for_offset(repeated).unwrap().line,
        session.cursor().0
    );
    assert_eq!(session.rendered_cursor(), repeated_point);
}

#[test]
fn pointer_mapping_handles_synthetic_cells_wide_unicode_and_source_windows() {
    let mut session = EditorSession::from_text("ab界cd\nnext\n");
    let layout = session.render_layout(4).clone();
    let wide = layout
        .lines
        .iter()
        .enumerate()
        .flat_map(|(row, line)| {
            line.atoms.iter().filter_map(move |atom| {
                (atom.source.as_ref().is_some_and(|range| range.start == 2)).then_some(
                    RenderedPoint {
                        row,
                        column: atom.columns.start + 1,
                    },
                )
            })
        })
        .next()
        .unwrap();
    session.move_to_rendered_point(wide);
    assert_eq!(session.cursor(), (0, 2));
    let far = RenderedPoint {
        row: usize::MAX,
        column: usize::MAX,
    };
    session.move_to_rendered_point(far);
    assert!(session.cursor().0 <= 1);

    let viewport = Viewport {
        top_line: 0,
        height: 3,
        width: 3,
        wrap: true,
        left_col: 0,
        skip_rows: 1,
    };
    assert_eq!(
        session.source_offset_at_viewport_cell(viewport, 0, 0),
        Some(2)
    );
    assert_eq!(
        session.source_offset_at_viewport_cell(viewport, 0, 1),
        Some(2)
    );
    assert_eq!(
        session.source_offset_at_viewport_cell(viewport, 2, 0),
        Some(8)
    );
    assert_eq!(session.source_offset_at_viewport_cell(viewport, 3, 0), None);
    let nowrap = Viewport {
        wrap: false,
        left_col: 2,
        skip_rows: 0,
        ..viewport
    };
    assert_eq!(
        session.source_offset_at_viewport_cell(nowrap, 0, 0),
        Some(5)
    );
    assert_eq!(
        session.source_offset_at_viewport_cell(nowrap, 0, 1),
        Some(5)
    );
    assert_eq!(
        session.source_offset_at_viewport_cell(nowrap, 0, 2),
        Some(6)
    );
}

#[test]
fn source_pointer_drag_enters_select_with_utf8_safe_ranges() {
    let mut session = EditorSession::from_text("a界b\n");
    session.handle_key(key('i'));
    assert_eq!(session.mode(), Mode::Insert);
    let error = session.select_source_offsets(2, 4, 20).unwrap_err();
    assert_eq!(error, oom_edit_core::PositionError::NotCharBoundary);
    assert_eq!(session.mode(), Mode::Insert);
    session.select_source_offsets(1, 4, 20).unwrap();
    assert_eq!(session.mode(), Mode::Select);
    assert_eq!(
        session.rendered_selection().unwrap().source_ranges,
        vec![1..5]
    );
}

#[test]
fn insert_escape_accepts_multibyte_unicode_line_ending() {
    let mut session = EditorSession::from_text("a\u{85}b");
    session.handle_key(key('i'));
    session.handle_key(key('Ω'));
    session.handle_key(special(KeyCodeKind::Esc));
    assert_eq!(session.mode(), Mode::Normal);
    assert!(session.document().contains('Ω'));
}

fn ctrl(ch: char) -> KeyInput {
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

fn shifted(ch: char) -> KeyInput {
    KeyInput {
        code: KeyCode {
            kind: KeyCodeKind::Char(ch),
        },
        mods: Modifiers {
            shift: true,
            ..Modifiers::default()
        },
    }
}

fn clipboard_writes(effects: &[Effect]) -> Vec<&str> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::ClipboardWrite(content) => Some(content.markdown()),
            _ => None,
        })
        .collect()
}

fn clipboard_contents(effects: &[Effect]) -> Vec<&ClipboardContent> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::ClipboardWrite(content) => Some(content),
            _ => None,
        })
        .collect()
}

fn render_and_move_to(session: &mut EditorSession, needle: &str, width: u16) -> usize {
    let target = session
        .render_layout(width)
        .lines
        .iter()
        .position(|line| line.styled.text.contains(needle))
        .unwrap_or_else(|| panic!("rendered layout should contain {needle:?}"));
    while session.rendered_cursor_line() < target {
        session.handle_key(key('j'));
    }
    while session.rendered_cursor_line() > target {
        session.handle_key(key('k'));
    }
    target
}

fn first_code_fence_rows(session: &mut EditorSession, width: u16) -> (usize, usize) {
    let layout = session.render_layout(width);
    let first = layout
        .lines
        .iter()
        .position(|line| line.role == RenderedLineRole::CodeFence)
        .expect("rendered layout should contain a fenced-code surface");
    let end = layout.lines[first..]
        .iter()
        .position(|line| line.role != RenderedLineRole::CodeFence)
        .map_or(layout.lines.len(), |offset| first + offset);
    (first, end - 1)
}

fn move_to_rendered_row(session: &mut EditorSession, row: usize) {
    while session.rendered_cursor_line() < row {
        session.handle_key(key('j'));
    }
    while session.rendered_cursor_line() > row {
        session.handle_key(key('k'));
    }
}

fn yank_first_code_fence(
    source: &str,
    shape_key: char,
    reverse: bool,
) -> (String, String, EditorSession) {
    let mut session = EditorSession::from_text(source);
    let (first, last) = first_code_fence_rows(&mut session, 120);
    let (anchor, active, motion) = if reverse {
        (last, first, 'k')
    } else {
        (first, last, 'j')
    };
    move_to_rendered_row(&mut session, anchor);
    session.handle_key(key(shape_key));
    for _ in 0..anchor.abs_diff(active) {
        session.handle_key(key(motion));
    }
    let effects = session.handle_key(key('y'));
    let content = clipboard_contents(&effects);
    assert_eq!(content.len(), 1);
    (
        content[0].markdown().to_string(),
        content[0].plain_text().to_string(),
        session,
    )
}

#[test]
fn session_starts_in_rendered_normal() {
    let mut session = EditorSession::from_text("# Hello\n\nWorld\n");
    assert_eq!(session.mode(), Mode::Normal);
    assert_eq!(session.cursor(), (0, 0));
    assert!(!session.render_layout(37).lines.is_empty());
}

#[test]
fn rendered_copy_preserves_markdown_and_prepares_sanitized_plain_text() {
    let source = "`App` consumes `Effect::ClipboardWrite` through an injected `ClipboardSink`.\n";
    let expected_markdown =
        "`App` consumes `Effect::ClipboardWrite` through an injected `ClipboardSink`.";
    let expected_plain = "App consumes Effect::ClipboardWrite through an injected ClipboardSink.";
    let mut session = EditorSession::from_text(source);
    let atom_count = session.render_layout(120).lines[0]
        .atoms
        .iter()
        .filter(|atom| atom.source.is_some())
        .count();

    session.handle_key(key('v'));
    for _ in 1..atom_count {
        session.handle_key(key('l'));
    }
    let effects = session.handle_key(key('y'));
    let contents = clipboard_contents(&effects);

    assert_eq!(contents.len(), 1);
    assert_eq!(contents[0].markdown(), expected_markdown);
    assert_eq!(contents[0].plain_text(), expected_plain);
    assert_eq!(session.document(), source);
    assert_eq!(session.mode(), Mode::Normal);
}

#[test]
fn rendered_plain_text_yank_is_one_shot_and_keeps_markdown_register() {
    let source = "`alpha`\n\n**beta**\n\nomega";
    for yank_key in [key('Y'), shifted('Y')] {
        let mut session = EditorSession::from_text(source);
        let layout = session.render_layout(40);
        let last_atoms = layout
            .lines
            .iter()
            .rev()
            .find(|line| line.atoms.iter().any(|atom| atom.source.is_some()))
            .unwrap()
            .atoms
            .iter()
            .filter(|atom| atom.source.is_some())
            .count();

        session.handle_key(key('v'));
        session.handle_key(special(KeyCodeKind::End));
        for _ in 1..last_atoms {
            session.handle_key(key('l'));
        }
        let effects = session.handle_key(yank_key);
        let contents = clipboard_contents(&effects);

        assert_eq!(contents.len(), 1);
        assert_eq!(contents[0].markdown(), "alpha\nbeta\nomega");
        assert_eq!(contents[0].plain_text(), "alpha\nbeta\nomega");
        assert_eq!(session.mode(), Mode::Normal);
        assert_eq!(session.document(), source);

        session.handle_key(key('p'));
        assert_eq!(session.document(), format!("{source}{source}"));
    }
}

#[test]
fn rendered_plain_text_yank_rejects_ctrl_and_alt_variants() {
    for mods in [
        Modifiers {
            ctrl: true,
            ..Modifiers::default()
        },
        Modifiers {
            alt: true,
            ..Modifiers::default()
        },
    ] {
        let mut session = EditorSession::from_text("`code`");
        session.render_layout(40);
        session.handle_key(key('v'));
        let effects = session.handle_key(KeyInput {
            code: KeyCode {
                kind: KeyCodeKind::Char('Y'),
            },
            mods,
        });

        assert!(clipboard_contents(&effects).is_empty());
        assert_eq!(session.mode(), Mode::Select);
        assert_eq!(session.document(), "`code`");
    }
}

#[test]
fn rendered_character_yank_preserves_multiline_markdown_and_whitespace() {
    let source = concat!(
        "alpha trailing  \n",
        "\n",
        "  **beta** and [link](https://example.test)\n",
        "\n",
        "```rust\n",
        "let café = `one`;  \n",
        "```\n",
        "\n",
        "| Name | Value |\n",
        "| --- | --- |\n",
        "| x | &amp; |\n",
        "\n",
        "omega",
    );
    for reverse in [false, true] {
        let mut session = EditorSession::from_text(source);
        let layout = session.render_layout(120);
        let last_row = layout
            .lines
            .iter()
            .rposition(|line| line.atoms.iter().any(|atom| atom.source.is_some()))
            .unwrap();
        let first_atoms = layout.lines[0]
            .atoms
            .iter()
            .filter(|atom| atom.source.is_some())
            .count();
        let last_atoms = layout.lines[last_row]
            .atoms
            .iter()
            .filter(|atom| atom.source.is_some())
            .count();

        if reverse {
            session.handle_key(special(KeyCodeKind::End));
            for _ in 1..last_atoms {
                session.handle_key(key('l'));
            }
            session.handle_key(key('v'));
            session.handle_key(special(KeyCodeKind::Home));
            for _ in 1..first_atoms {
                session.handle_key(key('h'));
            }
        } else {
            session.handle_key(key('v'));
            session.handle_key(special(KeyCodeKind::End));
            for _ in 1..last_atoms {
                session.handle_key(key('l'));
            }
        }

        let effects = session.handle_key(key('y'));
        let contents = clipboard_contents(&effects);
        assert_eq!(contents.len(), 1);
        assert_eq!(contents[0].markdown(), source);
        assert_eq!(session.document(), source);
    }
}

#[test]
fn rendered_character_yank_preserves_soft_wrap_spaces_without_newlines() {
    let source = "alpha beta gamma delta epsilon zeta eta theta";
    let mut session = EditorSession::from_text(source);
    let layout = session.render_layout(9);
    assert!(layout.lines.len() > 2);
    let last_atoms = layout
        .lines
        .last()
        .unwrap()
        .atoms
        .iter()
        .filter(|atom| atom.source.is_some())
        .count();

    session.handle_key(key('v'));
    session.handle_key(special(KeyCodeKind::End));
    for _ in 1..last_atoms {
        session.handle_key(key('l'));
    }
    let effects = session.handle_key(key('y'));

    assert_eq!(clipboard_writes(&effects), [source]);
}

#[test]
fn rendered_complete_code_fence_yank_preserves_exact_source() {
    let cases = [
        (
            concat!(
                "before\n\n",
                "````rust extra\n",
                "fn main() {  \n",
                "    println!(\"hello\");\n",
                "}\n",
                "````\n",
                "\nafter\n",
            ),
            concat!(
                "````rust extra\n",
                "fn main() {  \n",
                "    println!(\"hello\");\n",
                "}\n",
                "````\n",
            ),
        ),
        (
            "~~~~text\nThis is some text\n~~~~\n",
            "~~~~text\nThis is some text\n~~~~\n",
        ),
        (
            "> ```text\n> nested code\n> ```\n",
            "> ```text\n> nested code\n> ```\n",
        ),
        (
            "- ~~~unknown-language\n  nested code\n  ~~~",
            "- ~~~unknown-language\n  nested code\n  ~~~",
        ),
    ];

    for (source, expected) in cases {
        for shape_key in ['v', 'V'] {
            for reverse in [false, true] {
                let (markdown, _, session) = yank_first_code_fence(source, shape_key, reverse);
                assert_eq!(markdown, expected, "shape={shape_key}, reverse={reverse}");
                assert_eq!(session.document(), source);
            }
        }
    }
}

#[test]
fn rendered_complete_empty_fence_yanks_without_source_backed_body_atoms() {
    let source = "```text\n```\n";

    for shape_key in ['v', 'V'] {
        for reverse in [false, true] {
            let (markdown, plain_text, session) = yank_first_code_fence(source, shape_key, reverse);
            assert_eq!(markdown, source);
            assert_eq!(plain_text, "");
            assert_eq!(session.document(), source);
        }
    }
}

#[test]
fn rendered_selection_spanning_multiple_complete_fences_yanks_exact_source() {
    let source = concat!(
        "```rust\n",
        "first\n",
        "```\n",
        "\n",
        "between\n",
        "\n",
        "~~~unknown\n",
        "second\n",
        "~~~\n",
    );

    for shape_key in ['v', 'V'] {
        for reverse in [false, true] {
            let mut session = EditorSession::from_text(source);
            let code_rows: Vec<_> = session
                .render_layout(120)
                .lines
                .iter()
                .enumerate()
                .filter_map(|(row, line)| (line.role == RenderedLineRole::CodeFence).then_some(row))
                .collect();
            let first = *code_rows.first().unwrap();
            let last = *code_rows.last().unwrap();
            let (anchor, active, motion) = if reverse {
                (last, first, 'k')
            } else {
                (first, last, 'j')
            };
            move_to_rendered_row(&mut session, anchor);
            session.handle_key(key(shape_key));
            for _ in 0..anchor.abs_diff(active) {
                session.handle_key(key(motion));
            }

            assert_eq!(clipboard_writes(&session.handle_key(key('y'))), [source]);
            assert_eq!(session.document(), source);
        }
    }
}

#[test]
fn rendered_complete_fence_selection_survives_layout_rebuild() {
    let source = "```text\nThis is some text\n```\n";

    for shape_key in ['v', 'V'] {
        let mut session = EditorSession::from_text(source);
        let (first, last) = first_code_fence_rows(&mut session, 120);
        move_to_rendered_row(&mut session, first);
        session.handle_key(key(shape_key));
        for _ in first..last {
            session.handle_key(key('j'));
        }

        session.render_layout(24);
        assert_eq!(clipboard_writes(&session.handle_key(key('y'))), [source]);
    }
}

#[test]
fn rendered_code_fence_content_only_yanks_do_not_add_delimiters() {
    let source = "```rust\n  alpha  \n    beta\n```\n";

    let mut character = EditorSession::from_text(source);
    let first_body = render_and_move_to(&mut character, "alpha", 120);
    let last_body = character
        .render_layout(120)
        .lines
        .iter()
        .position(|line| line.styled.text.contains("beta"))
        .unwrap();
    let last_atoms = character.render_layout(120).lines[last_body]
        .atoms
        .iter()
        .filter(|atom| atom.source.is_some())
        .count();
    character.handle_key(key('v'));
    for _ in first_body..last_body {
        character.handle_key(key('j'));
    }
    for _ in 1..last_atoms {
        character.handle_key(key('l'));
    }
    assert_eq!(
        clipboard_writes(&character.handle_key(key('y'))),
        ["  alpha  \n    beta"]
    );

    let mut line = EditorSession::from_text(source);
    render_and_move_to(&mut line, "alpha", 120);
    line.handle_key(key('V'));
    line.handle_key(key('j'));
    let line_selection = line.rendered_selection().unwrap();
    assert_eq!(
        line_selection
            .source_ranges
            .iter()
            .map(|range| &source[range.clone()])
            .collect::<Vec<_>>(),
        ["```rust\n  alpha  \n    beta\n"]
    );
    assert_eq!(
        clipboard_writes(&line.handle_key(key('y'))),
        ["  alpha  \n    beta\n"]
    );
}

#[test]
fn rendered_complete_fence_plain_text_yank_keeps_exact_markdown_register() {
    let source = "```rust\nfn main() {}\n```\n";
    for (put, insertion) in [
        ('p', source.find('f').unwrap() + 1),
        ('P', source.find('f').unwrap()),
    ] {
        let mut session = EditorSession::from_text(source);
        let (first, last) = first_code_fence_rows(&mut session, 120);
        move_to_rendered_row(&mut session, first);
        session.handle_key(key('v'));
        for _ in first..last {
            session.handle_key(key('j'));
        }

        assert_eq!(
            clipboard_writes(&session.handle_key(key('Y'))),
            ["fn main() {}\n"]
        );
        session.handle_key(key(put));
        assert_eq!(
            session.document(),
            format!("{}{source}{}", &source[..insertion], &source[insertion..])
        );
    }
}

#[test]
fn complete_fence_yank_expansion_does_not_broaden_delete_geometry() {
    let source = "```rust\ncode\n```\n";
    let mut session = EditorSession::from_text(source);
    let (first, last) = first_code_fence_rows(&mut session, 120);
    move_to_rendered_row(&mut session, first);
    session.handle_key(key('v'));
    for _ in first..last {
        session.handle_key(key('j'));
    }

    let selection = session.rendered_selection().unwrap();
    assert!(selection
        .source_ranges
        .iter()
        .all(|range| range.start > 0 && range.end < source.len()));
    session.handle_key(key('x'));
    assert_eq!(session.document(), "```rust\n\n```\n");
}

#[test]
fn rendered_multiline_yank_register_round_trips_exact_source() {
    let source = "alpha  \n\n  **beta**\nomega";
    let mut session = EditorSession::from_text(source);
    let layout = session.render_layout(40);
    let last_atoms = layout
        .lines
        .iter()
        .rev()
        .find(|line| line.atoms.iter().any(|atom| atom.source.is_some()))
        .unwrap()
        .atoms
        .iter()
        .filter(|atom| atom.source.is_some())
        .count();

    session.handle_key(key('v'));
    session.handle_key(special(KeyCodeKind::End));
    for _ in 1..last_atoms {
        session.handle_key(key('l'));
    }
    assert_eq!(clipboard_writes(&session.handle_key(key('y'))), [source]);

    session.handle_key(key('p'));
    assert_eq!(session.document(), format!("{source}{source}"));
}

#[test]
fn rendered_copy_preserves_multi_backtick_code_span_boundaries() {
    let source = "backtick escaping: `` `backticks` inside code ``\n";
    let expected_markdown = "backtick escaping: `` `backticks` inside code ``";
    let expected_plain = "backtick escaping: `backticks` inside code";
    let mut session = EditorSession::from_text(source);
    let atom_count = session.render_layout(120).lines[0]
        .atoms
        .iter()
        .filter(|atom| atom.source.is_some())
        .count();

    session.handle_key(key('v'));
    for _ in 1..atom_count {
        session.handle_key(key('l'));
    }
    let effects = session.handle_key(key('y'));
    let contents = clipboard_contents(&effects);

    assert_eq!(contents.len(), 1);
    assert_eq!(contents[0].markdown(), expected_markdown);
    assert_eq!(contents[0].plain_text(), expected_plain);
    assert_eq!(session.document(), source);

    let mut partial = EditorSession::from_text(source);
    partial.render_layout(120);
    for _ in 0.."backtick escaping: ".len() {
        partial.handle_key(key('l'));
    }
    partial.handle_key(key('v'));
    for _ in 1.."`backticks`".len() {
        partial.handle_key(key('l'));
    }
    let partial_effects = partial.handle_key(key('y'));
    let partial_contents = clipboard_contents(&partial_effects);
    assert_eq!(partial_contents.len(), 1);
    assert_eq!(partial_contents[0].markdown(), "`backticks`");

    let mut line = EditorSession::from_text(source);
    line.render_layout(120);
    line.handle_key(key('V'));
    assert_eq!(clipboard_writes(&line.handle_key(key('y'))), [source]);

    let mut block = EditorSession::from_text(source);
    let block_atom_count = block.render_layout(120).lines[0]
        .atoms
        .iter()
        .filter(|atom| atom.source.is_some())
        .count();
    block.handle_key(ctrl('v'));
    for _ in 1..block_atom_count {
        block.handle_key(key('l'));
    }
    let block_effects = block.handle_key(key('y'));
    let block_contents = clipboard_contents(&block_effects);
    assert_eq!(block_contents.len(), 1);
    assert_eq!(block_contents[0].markdown(), expected_markdown);
    assert_eq!(block_contents[0].plain_text(), expected_plain);
}

#[test]
fn rendered_navigation_updates_canonical_source_cursor() {
    let mut session = EditorSession::from_text("# One\n\nTwo\n\nThree\n");
    render_and_move_to(&mut session, "Two", 40);
    assert_eq!(session.cursor(), (2, 0));
    render_and_move_to(&mut session, "Three", 40);
    assert_eq!(session.cursor(), (4, 0));
}

#[test]
fn first_actual_width_preserves_source_anchor() {
    let text = "# One\n\nA long paragraph whose words wrap differently at narrow widths.\n";
    let mut session = EditorSession::from_text(text);
    session.render_layout(72);
    render_and_move_to(&mut session, "long paragraph", 72);
    let source = session.cursor();
    session.render_layout(19);
    assert_eq!(session.cursor(), source);
    session.render_layout(51);
    assert_eq!(session.cursor(), source);
}

#[test]
fn insert_select_command_roundtrip_preserves_source_position() {
    let mut session = EditorSession::from_text("# One\n\nTwo\n");
    render_and_move_to(&mut session, "Two", 40);
    let source = session.cursor();

    session.handle_key(key('v'));
    assert_eq!(session.mode(), Mode::Select);
    session.handle_key(special(KeyCodeKind::Esc));
    assert_eq!((session.mode(), session.cursor()), (Mode::Normal, source));

    session.handle_key(key(':'));
    assert_eq!(session.mode(), Mode::Command);
    session.handle_key(special(KeyCodeKind::Esc));
    assert_eq!((session.mode(), session.cursor()), (Mode::Normal, source));

    session.handle_key(key('i'));
    assert_eq!(session.mode(), Mode::Insert);
    session.handle_key(special(KeyCodeKind::Esc));
    assert_eq!((session.mode(), session.cursor()), (Mode::Normal, source));
}

#[test]
fn rendered_move_then_insert_edits_the_moved_source_position() {
    let mut session = EditorSession::from_text("# One\n\nTwo\n\nThree\n");
    render_and_move_to(&mut session, "Two", 40);
    assert_eq!(session.cursor(), (2, 0));

    session.handle_key(key('i'));
    for ch in "NEW ".chars() {
        session.handle_key(key(ch));
    }
    session.handle_key(special(KeyCodeKind::Esc));
    session.render_layout(40);

    assert_eq!(session.mode(), Mode::Normal);
    assert_eq!(session.document(), "# One\n\nNEW Two\n\nThree\n");
    assert_eq!(session.cursor().0, 2);
}

#[test]
fn rendered_search_prompt_and_result_use_canonical_cursor() {
    let mut session = EditorSession::from_text("alpha\n\nbeta\n\nalpha\n");
    session.render_layout(40);
    session.handle_key(key('/'));
    assert_eq!(session.rendered_search_prompt().as_deref(), Some("/"));
    for ch in "beta".chars() {
        session.handle_key(key(ch));
    }
    session.handle_key(special(KeyCodeKind::Enter));
    assert_eq!(session.rendered_search_prompt(), None);
    assert_eq!(session.rendered_search().unwrap().pattern, "beta");
    assert_eq!(session.cursor(), (2, 0));
}

#[test]
fn render_source_is_highlighted_for_insert() {
    let mut session = EditorSession::from_text("# Hello\n\nWorld\n");
    session.render_layout(40);
    session.handle_key(key('i'));
    let frame = session.render_source(Viewport {
        top_line: 0,
        height: 5,
        width: 40,
        wrap: true,
        left_col: 0,
        skip_rows: 0,
    });
    assert_eq!(frame.lines.len(), 5);
    assert!(frame.lines[0]
        .spans
        .iter()
        .any(|span| span.style == SemanticStyle::Heading1));
}

#[test]
fn rendered_layout_line_numbers_follow_distinct_content_spans() {
    let mut session = EditorSession::from_text("# Heading\n\nA paragraph with words that wrap.\n");
    let layout = session.render_layout(12);
    assert_eq!(layout.lines.len(), layout.line_numbers.len());
    assert_eq!(layout.line_numbers.first(), Some(&Some(1)));
    for (line, number) in layout.lines.iter().zip(&layout.line_numbers) {
        if line.kind == oom_edit_core::LineKind::Synthetic {
            assert_eq!(*number, None);
        }
    }
    assert!(
        layout
            .line_numbers
            .iter()
            .filter(|number| number.is_some())
            .count()
            >= 2
    );
}

#[test]
fn select_forward_and_reverse_ranges_are_line_aligned() {
    let text = "# One\n\nTwo\n\nThree\n";
    let mut forward = EditorSession::from_text(text);
    forward.render_layout(40);
    forward.handle_key(key('V'));
    forward.handle_key(key('/'));
    for ch in "Two".chars() {
        forward.handle_key(key(ch));
    }
    forward.handle_key(special(KeyCodeKind::Enter));
    let forward_ranges = forward.rendered_selection().unwrap().source_ranges;
    assert_eq!(forward_ranges, vec![0..11]);
    assert_eq!(&text[forward_ranges[0].clone()], "# One\n\nTwo\n");

    let mut reverse = EditorSession::from_text(text);
    for ch in ":3".chars() {
        reverse.handle_key(key(ch));
    }
    reverse.handle_key(special(KeyCodeKind::Enter));
    reverse.render_layout(40);
    reverse.handle_key(key('V'));
    reverse.handle_key(key('?'));
    for ch in "One".chars() {
        reverse.handle_key(key(ch));
    }
    reverse.handle_key(special(KeyCodeKind::Enter));
    assert_eq!(
        reverse.rendered_selection().unwrap().source_ranges,
        forward_ranges
    );
}

#[test]
fn v_enters_character_selection() {
    let mut session = EditorSession::from_text("# alpha\n");
    session.render_layout(40);
    session.handle_key(key('v'));
    assert_eq!(
        session.rendered_selection().unwrap().shape,
        SelectionShape::Character
    );
}

#[test]
fn capital_v_enters_line_selection() {
    let mut session = EditorSession::from_text("# alpha\n");
    session.render_layout(40);
    session.handle_key(key('V'));
    assert_eq!(
        session.rendered_selection().unwrap().shape,
        SelectionShape::Line
    );
}

#[test]
fn ctrl_v_enters_block_selection() {
    let mut session = EditorSession::from_text("# alpha\n");
    session.render_layout(40);
    session.handle_key(ctrl('v'));
    assert_eq!(
        session.rendered_selection().unwrap().shape,
        SelectionShape::Block
    );
}

#[test]
fn selection_o_swaps_endpoints() {
    let mut session = EditorSession::from_text("alpha\n");
    session.render_layout(40);
    session.handle_key(key('v'));
    session.handle_key(key('l'));
    let before = session.rendered_selection().unwrap();
    session.handle_key(key('o'));
    let after = session.rendered_selection().unwrap();
    assert_eq!((after.anchor, after.active), (before.active, before.anchor));
    assert_eq!(after.source_ranges, before.source_ranges);
}

#[test]
fn rendered_character_selection_maps_inline_source() {
    let text = "# alpha\n";
    let mut session = EditorSession::from_text(text);
    session.render_layout(40);
    session.handle_key(key('v'));
    session.handle_key(key('l'));
    let selection = session.rendered_selection().unwrap();
    assert_eq!(selection.source_ranges, vec![2..4]);
    assert_eq!(&text[selection.source_ranges[0].clone()], "al");
}

#[test]
fn wrapped_table_character_selection_is_source_driven_and_operator_exact() {
    let text = "| Description | Neighbor |\n| --- | --- |\n| alpha beta gamma delta epsilon zeta eta theta iota kappa lambda | NEIGHBOR CONTENT THAT FILLS ITS COLUMN |\n| OTHER | ROW |\n";
    let cell_start = text.find("alpha").unwrap();
    let cell_end = text.find(" | NEIGHBOR").unwrap();
    let neighbor_start = text.find("NEIGHBOR").unwrap();
    let neighbor_end = neighbor_start + "NEIGHBOR CONTENT THAT FILLS ITS COLUMN".len();

    let select_cell = || {
        let mut session = EditorSession::from_text(text);
        let first_row = render_and_move_to(&mut session, "alpha beta", 80);
        session.handle_key(key('v'));
        session.handle_key(key('j'));
        assert_eq!(session.rendered_cursor_line(), first_row + 1);
        session
    };

    let mut forward = select_cell();
    let before_resize = forward.rendered_selection().unwrap();
    let boundary_rows = forward
        .rendered_layout()
        .unwrap()
        .lines
        .iter()
        .enumerate()
        .filter_map(|(row, line)| line.styled.text.starts_with("│-").then_some(row))
        .collect::<Vec<_>>();
    assert_eq!(boundary_rows.len(), 1);
    assert!(before_resize
        .rows
        .iter()
        .all(|row| !boundary_rows.contains(&row.row)));
    assert!(before_resize.rows.len() >= 2);
    assert!(before_resize.source_ranges.iter().all(|range| {
        cell_start <= range.start
            && range.end <= cell_end
            && (range.end <= neighbor_start || neighbor_end <= range.start)
    }));
    assert!(before_resize
        .rows
        .iter()
        .all(|row| row.columns.iter().all(|columns| !columns.is_empty())));

    forward.handle_key(key('o'));
    let reversed = forward.rendered_selection().unwrap();
    assert_eq!(reversed.source_ranges, before_resize.source_ranges);
    forward.render_layout(120);
    assert_eq!(
        forward.rendered_selection().unwrap().source_ranges,
        before_resize.source_ranges
    );

    let selected_text = &text[before_resize.source_ranges.first().unwrap().start
        ..before_resize.source_ranges.last().unwrap().end];
    let mut yank = select_cell();
    let effects = yank.handle_key(key('y'));
    assert_eq!(clipboard_writes(&effects), [selected_text]);

    let mut expected_after_removal = text.to_string();
    for range in before_resize.source_ranges.iter().rev() {
        expected_after_removal.replace_range(range.clone(), "");
    }
    let mut delete = select_cell();
    delete.handle_key(key('d'));
    assert_eq!(delete.document(), expected_after_removal);

    let mut change = select_cell();
    change.handle_key(key('c'));
    assert_eq!(change.mode(), Mode::Insert);
    assert_eq!(change.document(), expected_after_removal);
}

#[test]
fn rendered_navigation_crosses_synthetic_table_body_boundaries_deterministically() {
    let text = "| Header | Value |\n| --- | --- |\n| first | row |\n| second | row |\n";
    let mut session = EditorSession::from_text(text);
    let first = render_and_move_to(&mut session, "first", 80);
    let boundary = session
        .rendered_layout()
        .unwrap()
        .lines
        .iter()
        .position(|line| line.styled.text.starts_with("│-"))
        .unwrap();
    assert_eq!(boundary, first + 1);

    session.handle_key(key('j'));
    assert_eq!(session.rendered_cursor_line(), boundary);
    assert_eq!(session.cursor().0, 2);

    session.handle_key(key('j'));
    assert!(session.rendered_cursor_line() > boundary);
    assert_eq!(session.cursor().0, 3);
}

#[test]
fn table_body_boundaries_never_enter_selection_shapes_or_operator_payloads() {
    let text = "| Header | Value |\n| --- | --- |\n| first | row |\n| second | row |\n";

    for selection_key in [key('v'), key('V'), ctrl('v')] {
        let mut session = EditorSession::from_text(text);
        let first = render_and_move_to(&mut session, "first", 80);
        let boundary = session
            .rendered_layout()
            .unwrap()
            .lines
            .iter()
            .position(|line| line.styled.text.starts_with("│-"))
            .unwrap();
        assert_eq!(boundary, first + 1);

        session.handle_key(selection_key);
        session.handle_key(key('j'));
        session.handle_key(key('j'));
        let selection = session.rendered_selection().unwrap();
        assert!(selection
            .source_ranges
            .iter()
            .all(|range| !text[range.clone()].contains("---")));

        let effects = session.handle_key(key('y'));
        let writes = clipboard_writes(&effects);
        assert_eq!(writes.len(), 1);
        assert!(!writes[0].contains("---"));
    }
}

#[test]
fn public_rendered_cursor_is_only_a_rendered_point() {
    let mut session = EditorSession::from_text("alpha\n");
    session.render_layout(40);
    let point: oom_edit_core::RenderedPoint = session.rendered_cursor();
    assert_eq!(point, oom_edit_core::RenderedPoint { row: 0, column: 0 });
}

#[test]
fn rendered_word_motions_move_the_active_display_point() {
    let mut session = EditorSession::from_text("alpha beta-gamma\n");
    session.render_layout(40);
    session.handle_key(key('w'));
    assert_eq!(session.cursor(), (0, 6));
    session.handle_key(key('b'));
    assert_eq!(session.cursor(), (0, 0));
    session.handle_key(key('e'));
    assert_eq!(session.cursor(), (0, 4));
    session.handle_key(key('W'));
    assert_eq!(session.cursor(), (0, 6));
}

#[test]
fn rendered_line_selection_expands_physical_lines() {
    let text = "# alpha\n# beta";
    let mut session = EditorSession::from_text(text);
    session.render_layout(40);
    session.handle_key(key('V'));
    assert_eq!(
        session.rendered_selection().unwrap().source_ranges,
        vec![0..8]
    );
}

#[test]
fn rendered_block_selection_projects_rectangle() {
    let text = "abcd\n\nwxyz\n";
    let mut session = EditorSession::from_text(text);
    session.render_layout(40);
    session.handle_key(ctrl('v'));
    session.handle_key(key('l'));
    session.handle_key(key('j'));
    session.handle_key(key('j'));
    let selection = session.rendered_selection().unwrap();
    assert_eq!(selection.shape, SelectionShape::Block);
    assert_eq!(selection.source_ranges, vec![0..2, 6..8]);
    assert_eq!(selection.block_width, Some(2));
}

#[test]
fn synthetic_cells_never_enter_source_ranges() {
    let mut session = EditorSession::from_text("one\n\ntwo\n");
    session.render_layout(40);
    session.handle_key(key('j'));
    session.handle_key(key('v'));
    assert!(session
        .rendered_selection()
        .unwrap()
        .source_ranges
        .is_empty());
}

#[test]
fn link_index_normal_y_and_enter_copy_only_the_focused_destination() {
    let destination = "https://example.com/東京?q=é";
    let mut session = EditorSession::from_text(&format!(
        "[first](https://example.com/first) and [second]({destination})\n"
    ));
    render_and_move_to(&mut session, &format!("[1] {destination}"), 80);

    for input in [key('y'), special(KeyCodeKind::Enter)] {
        let effects = session.handle_key(input);
        assert_eq!(
            effects,
            vec![Effect::ClipboardWrite(ClipboardContent::invariant(
                destination.to_string()
            ))]
        );
        assert_eq!(session.mode(), Mode::Normal);
    }
}

#[test]
fn link_index_select_y_exits_and_enter_stays_without_source_ranges() {
    let destination = "https://example.com/repeated";
    let mut session = EditorSession::from_text(&format!(
        "[first]({destination}) and [second]({destination})\n"
    ));
    render_and_move_to(&mut session, &format!("[1] {destination}"), 80);

    session.handle_key(key('v'));
    assert_eq!(session.mode(), Mode::Select);
    assert!(session
        .rendered_selection()
        .expect("selection should be visible on the synthetic row")
        .source_ranges
        .is_empty());
    assert_eq!(
        session.handle_key(key('y')),
        vec![
            Effect::ClipboardWrite(ClipboardContent::invariant(destination.to_string())),
            Effect::ModeChanged(Mode::Normal),
        ]
    );
    assert_eq!(session.mode(), Mode::Normal);

    session.handle_key(key('v'));
    assert_eq!(
        session.handle_key(key('Y')),
        vec![
            Effect::ClipboardWrite(ClipboardContent::invariant(destination.to_string())),
            Effect::ModeChanged(Mode::Normal),
        ]
    );
    assert_eq!(session.mode(), Mode::Normal);

    session.handle_key(key('v'));
    assert_eq!(
        session.handle_key(special(KeyCodeKind::Enter)),
        vec![Effect::ClipboardWrite(ClipboardContent::invariant(
            destination.to_string()
        ))]
    );
    assert_eq!(session.mode(), Mode::Select);
}

#[test]
fn link_index_yank_keys_keep_named_and_black_hole_isolation() {
    let destination = "https://example.com/register-safe";
    for yank_key in ['y', 'Y'] {
        for register in ['a', '_'] {
            let mut session = EditorSession::from_text(&format!("[label]({destination})\n"));
            render_and_move_to(&mut session, &format!("[0] {destination}"), 80);
            for input in [key('v'), key('"'), key(register)] {
                session.handle_key(input);
            }

            let effects = session.handle_key(key(yank_key));
            assert!(clipboard_contents(&effects).is_empty());
            assert_eq!(session.mode(), Mode::Normal);
        }
    }
}

#[test]
fn link_index_select_y_preserves_source_backed_yank_semantics() {
    let destination = "https://example.com/source-backed";
    let source = format!("[label]({destination})\n");
    let mut session = EditorSession::from_text(&source);
    session.render_layout(80);

    session.handle_key(key('V'));
    session.handle_key(key('G'));
    assert_eq!(
        session
            .rendered_selection()
            .expect("selection should span the source and synthetic link row")
            .source_ranges,
        vec![0..source.len()]
    );

    let effects = session.handle_key(key('y'));
    assert_eq!(clipboard_writes(&effects), [source.as_str()]);
    assert_eq!(session.mode(), Mode::Normal);

    session.handle_key(key('p'));
    assert_eq!(session.document(), format!("{source}{source}"));
}

#[test]
fn unicode_selection_atoms_stay_utf8_safe() {
    let text = "e\u{301} 東京\n大阪\n";
    let mut session = EditorSession::from_text(text);
    let layout = session.render_layout(40);
    for source in layout
        .lines
        .iter()
        .flat_map(|line| &line.atoms)
        .filter_map(|atom| atom.source.as_ref())
    {
        assert!(text.is_char_boundary(source.start));
        assert!(text.is_char_boundary(source.end));
    }
    let combining_atom = layout
        .lines
        .iter()
        .flat_map(|line| &line.atoms)
        .find(|atom| {
            atom.source
                .as_ref()
                .is_some_and(|source| &text[source.clone()] == "e\u{301}")
        })
        .expect("combining sequence is one rendered atom");
    assert_eq!(combining_atom.columns.end - combining_atom.columns.start, 1);

    session.handle_key(key('v'));
    assert_eq!(
        session.rendered_selection().unwrap().source_ranges,
        vec![0..3]
    );
    session.handle_key(key('l'));
    session.handle_key(key('l'));
    let selection = session.rendered_selection().unwrap();
    assert_eq!(selection.source_ranges, vec![0..7]);
    assert_eq!(selection.rows[0].columns, vec![0..4]);
    let effects = session.handle_key(key('y'));
    assert_eq!(clipboard_writes(&effects), ["e\u{301} 東"]);

    let mut block = EditorSession::from_text("東京\n\n大阪\n");
    block.render_layout(40);
    block.handle_key(ctrl('v'));
    block.handle_key(key('l'));
    block.handle_key(key('j'));
    block.handle_key(key('j'));
    let selection = block.rendered_selection().unwrap();
    assert_eq!(selection.shape, SelectionShape::Block);
    assert_eq!(selection.source_ranges, vec![0..6, 8..14]);
    assert_eq!(selection.block_width, Some(4));
    assert_eq!(
        selection
            .rows
            .iter()
            .map(|row| row.columns.clone())
            .collect::<Vec<_>>(),
        vec![vec![0..4], vec![0..4], vec![0..4]]
    );
}

#[test]
fn entity_selection_yanks_and_deletes_the_complete_markdown_source() {
    for (entity, plain_text) in [
        ("&amp;", "&"),
        ("&#38;", "&"),
        ("&#x26;", "&"),
        ("&fjlig;", "fj"),
    ] {
        let text = format!("A {entity} B\n");
        let entity_start = text.find(entity).unwrap();

        let mut yank = EditorSession::from_text(&text);
        yank.render_layout(40);
        yank.handle_key(key('l'));
        yank.handle_key(key('l'));
        yank.handle_key(key('v'));
        assert_eq!(
            yank.rendered_selection().unwrap().source_ranges,
            vec![entity_start..entity_start + entity.len()]
        );
        let effects = yank.handle_key(key('y'));
        let contents = clipboard_contents(&effects);
        assert_eq!(contents.len(), 1);
        assert_eq!(contents[0].markdown(), entity);
        assert_eq!(contents[0].plain_text(), plain_text);

        let mut delete = EditorSession::from_text(&text);
        delete.render_layout(40);
        delete.handle_key(key('l'));
        delete.handle_key(key('l'));
        delete.handle_key(key('v'));
        delete.handle_key(key('d'));
        assert_eq!(delete.document(), "A  B\n");
        assert!(!delete.render_layout(40).lines.is_empty());
    }
}

#[test]
fn partial_inline_selection_does_not_add_unselected_delimiters() {
    let mut session = EditorSession::from_text("*emphasis* and [label](target)\n");
    session.render_layout(80);
    session.handle_key(key('v'));
    for _ in 0..3 {
        session.handle_key(key('l'));
    }

    let effects = session.handle_key(key('y'));
    let contents = clipboard_contents(&effects);
    assert_eq!(contents.len(), 1);
    assert_eq!(contents[0].markdown(), "emph");
    assert_eq!(contents[0].plain_text(), "emph");
}

#[test]
fn empty_and_synthetic_only_layouts_ignore_word_motions() {
    for text in ["", "\n"] {
        for motion in ['w', 'e', 'b'] {
            let mut session = EditorSession::from_text(text);
            session.render_layout(40);
            session.handle_key(key(motion));
            assert_eq!(session.cursor(), (0, 0), "{text:?}/{motion}");
        }
    }
}

#[test]
fn leaving_insert_remaps_rendered_cursor_after_motion_and_edits() {
    let text = "# One\n\n# Two\n";
    let mut moved = EditorSession::from_text(text);
    moved.render_layout(40);
    moved.handle_key(key('i'));
    moved.handle_key(special(KeyCodeKind::Down));
    moved.handle_key(special(KeyCodeKind::Down));
    assert_eq!(moved.cursor().0, 2);
    moved.handle_key(special(KeyCodeKind::Esc));
    assert_eq!(moved.rendered_cursor_line(), 2);
    moved.handle_key(key('j'));
    assert_eq!(moved.cursor().0, 2);

    let mut edited = EditorSession::from_text(text);
    edited.render_layout(40);
    edited.handle_key(key('i'));
    edited.handle_key(special(KeyCodeKind::Down));
    edited.handle_key(special(KeyCodeKind::Down));
    edited.handle_key(key('X'));
    edited.handle_key(special(KeyCodeKind::Esc));
    assert_eq!(edited.cursor().0, 2);
    let cursor_row = edited.rendered_cursor_line();
    assert_eq!(cursor_row, 0, "dirty layout remains invalid until rebuild");
    edited.render_layout(40);
    let remapped = edited.rendered_cursor_line();
    assert!(edited.rendered_layout().unwrap().lines[remapped]
        .styled
        .text
        .contains('X'));
}

#[test]
fn select_yank_and_escape_are_non_destructive() {
    let text = "one\ntwo\n";
    let mut session = EditorSession::from_text(text);
    session.render_layout(40);
    session.handle_key(key('v'));
    let effects = session.handle_key(key('y'));
    assert_eq!(clipboard_writes(&effects), ["o"]);
    assert_eq!(
        (session.mode(), session.document()),
        (Mode::Normal, text.into())
    );

    session.handle_key(key('v'));
    session.handle_key(special(KeyCodeKind::Esc));
    assert_eq!(
        (session.mode(), session.document()),
        (Mode::Normal, text.into())
    );
}

#[test]
fn select_yank_populates_the_unnamed_linewise_register() {
    let mut session = EditorSession::from_text("# one\n# two\n# three\n");
    session.render_layout(40);
    session.handle_key(key('V'));
    session.handle_key(key('y'));
    assert_eq!(session.document(), "# one\n# two\n# three\n");

    session.handle_key(key('p'));
    assert_eq!(session.document(), "# one\n# one\n# two\n# three\n");
    assert_eq!(session.mode(), Mode::Normal);
}

#[test]
fn select_anchor_and_active_source_survive_resize() {
    let text = "alpha beta gamma delta epsilon\n";
    let mut session = EditorSession::from_text(text);
    session.render_layout(11);
    session.handle_key(key('v'));
    session.handle_key(key('w'));

    for width in [47, 7, 11] {
        session.render_layout(width);
        let selection = session.rendered_selection().unwrap();
        let layout = session.rendered_layout().unwrap();
        let point_source = |point: oom_edit_core::RenderedPoint| {
            layout.lines[point.row]
                .atoms
                .iter()
                .find(|atom| {
                    atom.columns.contains(&point.column) || atom.columns.start == point.column
                })
                .and_then(|atom| atom.source.clone())
        };
        assert_eq!(session.mode(), Mode::Select);
        assert_eq!(session.cursor(), (0, 6));
        assert_eq!(point_source(selection.anchor), Some(0..1));
        assert_eq!(point_source(selection.active), Some(6..7));
        assert_eq!(selection.source_ranges, vec![0..7]);

        let mut row_sources = selection
            .rows
            .iter()
            .flat_map(|row| row.source_ranges.iter().cloned())
            .collect::<Vec<_>>();
        row_sources.sort_by_key(|range| (range.start, range.end));
        let row_sources = row_sources.into_iter().fold(
            Vec::<std::ops::Range<usize>>::new(),
            |mut ranges, range| {
                if let Some(previous) = ranges.last_mut() {
                    if range.start <= previous.end {
                        previous.end = previous.end.max(range.end);
                        return ranges;
                    }
                }
                ranges.push(range);
                ranges
            },
        );
        let expected_painted = if width == 7 {
            vec![0..5, 6..7]
        } else {
            std::iter::once(0..7).collect()
        };
        assert_eq!(row_sources, expected_painted);
    }
}

#[test]
fn select_default_and_explicit_system_yanks_emit_exact_payload_once() {
    for register in [None, Some('+'), Some('*')] {
        let mut session = EditorSession::from_text("# one\n# two\n");
        session.render_layout(40);
        session.handle_key(key('V'));
        if let Some(register) = register {
            session.handle_key(key('"'));
            session.handle_key(key(register));
        }
        let effects = session.handle_key(key('y'));
        assert_eq!(clipboard_writes(&effects), ["# one\n"]);
        assert_eq!(session.document(), "# one\n# two\n");
        assert_eq!(session.mode(), Mode::Normal);
    }
}

#[test]
fn select_plain_text_yank_preserves_register_publication_rules() {
    for (register, publishes) in [
        (None, true),
        (Some('+'), true),
        (Some('*'), true),
        (Some('a'), false),
        (Some('_'), false),
    ] {
        let mut session = EditorSession::from_text("# one\n# two\n");
        session.render_layout(40);
        session.handle_key(key('V'));
        if let Some(register) = register {
            session.handle_key(key('"'));
            session.handle_key(key(register));
        }
        let effects = session.handle_key(key('Y'));
        let expected_writes = if publishes { vec!["one\n"] } else { Vec::new() };
        assert_eq!(clipboard_writes(&effects), expected_writes);
        assert_eq!(session.document(), "# one\n# two\n");
        assert_eq!(session.mode(), Mode::Normal);

        if register == Some('a') {
            for input in [key('"'), key('a'), key('p')] {
                session.handle_key(input);
            }
            assert_eq!(session.document(), "# one\n# one\n# two\n");
        }
    }
}

#[test]
fn select_delete_put_undo_and_redo_use_vim_history() {
    let mut session = EditorSession::from_text("# one\n# two\n# three\n");
    session.render_layout(40);
    session.handle_key(key('V'));
    session.handle_key(key('d'));
    assert_eq!(session.document(), "# two\n# three\n");
    session.render_layout(40);
    session.handle_key(key('p'));
    assert_eq!(session.document(), "# two\n# one\n# three\n");
    session.handle_key(key('u'));
    assert_eq!(session.document(), "# two\n# three\n");
    session.handle_key(ctrl('r'));
    assert_eq!(session.document(), "# two\n# one\n# three\n");
}

#[test]
fn select_x_matches_delete_and_change_enters_insert() {
    let mut x = EditorSession::from_text("# one\n# two\n");
    x.render_layout(40);
    x.handle_key(key('V'));
    x.handle_key(key('x'));
    assert_eq!((x.mode(), x.document()), (Mode::Normal, "# two\n".into()));

    let mut change = EditorSession::from_text("# one\n# two\n");
    change.render_layout(40);
    change.handle_key(key('V'));
    change.handle_key(key('c'));
    assert_eq!(change.mode(), Mode::Insert);
    change.handle_key(key('n'));
    change.handle_key(key('e'));
    change.handle_key(key('w'));
    change.handle_key(special(KeyCodeKind::Esc));
    assert_eq!(change.document(), "new# two\n");
}

#[test]
fn select_indent_and_outdent_apply_each_line_once() {
    let mut session = EditorSession::from_text("# one\n# two\n");
    session.render_layout(40);
    session.handle_key(key('V'));
    session.handle_key(key('j'));
    session.handle_key(key('j'));
    session.handle_key(key('>'));
    assert_eq!(session.mode(), Mode::Normal);
    assert_eq!(session.document(), "    # one\n    # two\n");

    session.render_layout(40);
    session.handle_key(key('V'));
    session.handle_key(key('j'));
    session.handle_key(key('j'));
    session.handle_key(key('<'));
    assert_eq!(session.document(), "# one\n# two\n");
}

#[test]
fn rendered_insert_entry_keys_have_distinct_vim_semantics() {
    let text = "first\n\nsecond\n\nthird";
    let outcomes = ['i', 'a', 'I', 'A', 'o', 'O'].map(|action| {
        let mut session = EditorSession::from_text(text);
        render_and_move_to(&mut session, "second", 40);
        session.handle_key(key(action));
        (action, session.mode(), session.cursor(), session.document())
    });
    assert!(outcomes.iter().all(|(_, mode, _, _)| *mode == Mode::Insert));
    assert_ne!(outcomes[0].2, outcomes[1].2);
    assert_ne!(outcomes[2].2, outcomes[3].2);
    assert_eq!(
        outcomes[4].3.matches('\n').count(),
        text.matches('\n').count() + 1
    );
    assert_eq!(
        outcomes[5].3.matches('\n').count(),
        text.matches('\n').count() + 1
    );
}

#[test]
fn every_rendered_insert_entry_roundtrips_after_typing() {
    let cases = [
        ('i', "first\n\nXsecond\n\nthird"),
        ('a', "first\n\nsXecond\n\nthird"),
        ('I', "first\n\nXsecond\n\nthird"),
        ('A', "first\n\nsecondX\n\nthird"),
        ('o', "first\n\nsecond\nX\n\nthird"),
        ('O', "first\n\nX\nsecond\n\nthird"),
    ];
    for (action, expected) in cases {
        let mut session = EditorSession::from_text("first\n\nsecond\n\nthird");
        render_and_move_to(&mut session, "second", 40);
        session.handle_key(key(action));
        session.handle_key(key('X'));
        session.handle_key(special(KeyCodeKind::Esc));
        session.render_layout(40);
        assert_eq!(session.mode(), Mode::Normal, "entry {action}");
        assert_eq!(session.document(), expected, "entry {action}");
        assert!(session.cursor().0 < session.line_count(), "entry {action}");
    }
}

#[test]
fn front_matter_panel_is_structured_and_collapsible() {
    let text = "---\ntitle: Example\ntags:\n  - rust\n---\n\n# Body\n";
    let mut session = EditorSession::from_text(text);
    let expanded: Vec<_> = session
        .render_layout(50)
        .lines
        .iter()
        .map(|line| line.styled.text.clone())
        .collect();
    assert!(expanded.iter().any(|line| line.contains("metadata")));
    assert!(expanded.iter().any(|line| line.contains("title")));
    assert!(!expanded.iter().any(|line| line == "---"));

    session.handle_key(key('z'));
    let collapsed: Vec<_> = session
        .render_layout(50)
        .lines
        .iter()
        .map(|line| line.styled.text.clone())
        .collect();
    assert!(collapsed.iter().any(|line| line.starts_with("▸ metadata")));
    assert!(!collapsed.iter().any(|line| line.contains("title:")));
    let collapsed_layout = session.rendered_layout().unwrap();
    assert_eq!(collapsed_layout.lines[0].source, 0..4);
    assert_eq!(collapsed_layout.line_numbers[0], Some(1));
    assert_eq!(session.cursor(), (0, 0));

    session.handle_key(key('j'));
    assert_eq!(session.rendered_cursor().row, 1);
    assert_eq!(session.cursor(), (0, 0));
    session.handle_key(key('j'));
    assert!(session.cursor().0 >= 6);
    session.handle_key(key('k'));
    assert_eq!(session.cursor().0, 0);
    session.handle_key(key('z'));
    let reexpanded = session.render_layout(50);
    assert!(reexpanded
        .lines
        .iter()
        .any(|line| line.styled.text.contains("title: Example")));
    assert_eq!(session.cursor().0, 0);
}

#[test]
fn front_matter_panel_preserves_source_order_and_nested_lines() {
    let text = "---\n# comment\nauthor:\n  name: \"Ada\"\ntags:\n  - rust\n  - tui\ntitle: Last\n---\n\n# Body\n";
    let mut session = EditorSession::from_text(text);
    let metadata: Vec<_> = session
        .render_layout(80)
        .lines
        .iter()
        .filter(|line| line.role == RenderedLineRole::Metadata)
        .collect();
    let rendered = metadata
        .iter()
        .map(|line| line.styled.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    for needle in [
        "# comment",
        "author:",
        "name:",
        "tags:",
        "- rust",
        "- tui",
        "title: Last",
    ] {
        assert!(rendered.contains(needle), "missing source line {needle:?}");
    }
    let positions: Vec<_> = [
        "# comment",
        "author:",
        "name:",
        "tags:",
        "- rust",
        "- tui",
        "title: Last",
    ]
    .into_iter()
    .map(|needle| rendered.find(needle).unwrap())
    .collect();
    assert!(positions.windows(2).all(|pair| pair[0] < pair[1]));
}

#[test]
fn front_matter_rows_keep_physical_source_numbers() {
    let text = "---\ntitle: one\n\n# note\nnested:\n  child: two\n---\nbody\n";
    let mut session = EditorSession::from_text(text);
    let layout = session.render_layout(80);
    let numbers: Vec<_> = layout
        .lines
        .iter()
        .zip(&layout.line_numbers)
        .filter(|(line, _)| line.role == RenderedLineRole::Metadata)
        .map(|(_, number)| *number)
        .collect();
    assert_eq!(numbers, (1..=7).map(Some).collect::<Vec<_>>());
    for line in layout
        .lines
        .iter()
        .filter(|line| line.role == RenderedLineRole::Metadata)
    {
        assert!(line.source.start <= line.source.end);
        assert!(line.source.end <= text.len());
        assert!(text.is_char_boundary(line.source.start));
        assert!(text.is_char_boundary(line.source.end));
    }
}

#[test]
fn front_matter_wraps_inside_panel_without_overflow() {
    let text = "---\ntitle: a very long café 東京 metadata title\n---";
    let mut session = EditorSession::from_text(text);
    let layout = session.render_layout(14);
    assert!(layout
        .lines
        .iter()
        .all(|line| UnicodeWidthStr::width(line.styled.text.as_str()) <= 14));
    let title_source = 4..text.rfind('\n').unwrap() + 1;
    let wrapped: Vec<_> = layout
        .lines
        .iter()
        .zip(&layout.line_numbers)
        .filter(|(line, _)| line.source == title_source)
        .collect();
    assert!(wrapped.len() > 1);
    assert_eq!(wrapped[0].1, &Some(2));
    assert!(wrapped.iter().skip(1).all(|(_, number)| number.is_none()));
}

#[test]
fn metadata_rows_expose_renderer_neutral_role() {
    let text = "+++\n# comment\n[author]\nname = \"Ada\"\n\ntags = [\"rust\", \"tui\"]\n+++\n";
    let mut session = EditorSession::from_text(text);
    let layout = session.render_layout(20);
    let metadata = layout
        .lines
        .iter()
        .filter(|line| line.role == RenderedLineRole::Metadata)
        .collect::<Vec<_>>();
    assert!(metadata.len() >= 7);
    assert!(metadata
        .iter()
        .all(|line| UnicodeWidthStr::width(line.styled.text.as_str()) <= 20));
    let rendered = metadata
        .iter()
        .map(|line| line.styled.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    let mut previous = 0;
    for needle in ["# comment", "[author]", "name = \"Ada\"", "tags ="] {
        let position = rendered.find(needle).unwrap();
        assert!(
            position >= previous,
            "TOML source order changed at {needle:?}"
        );
        previous = position;
    }
    assert!(metadata.iter().any(|line| line
        .styled
        .text
        .strip_prefix("│ ")
        .and_then(|body| body.strip_suffix(" │"))
        .is_some_and(|body| body.trim().is_empty())));
    for line in metadata {
        assert!(line.source.end <= text.len());
        assert!(text.is_char_boundary(line.source.start));
        assert!(text.is_char_boundary(line.source.end));
    }
}

#[test]
fn front_matter_edge_cases_are_width_safe() {
    for text in [
        "---\n---",
        "+++\n+++",
        "---\ntitle: café",
        "+++\ntitle = \"東京\"",
    ] {
        for width in 0..=3 {
            let mut session = EditorSession::from_text(text);
            let layout = session.render_layout(width);
            assert!(layout.lines.iter().all(|line| UnicodeWidthStr::width(
                line.styled.text.as_str()
            ) <= usize::from(width)));
        }
    }
}

#[test]
fn command_help_and_line_jump_return_to_rendered_normal() {
    let mut session = EditorSession::from_text("one\ntwo\nthree\n");
    session.render_layout(40);
    for ch in ":help".chars() {
        session.handle_key(key(ch));
    }
    let effects = session.handle_key(special(KeyCodeKind::Enter));
    assert!(effects
        .iter()
        .any(|effect| matches!(effect, Effect::HelpRequested)));
    assert_eq!(session.mode(), Mode::Normal);

    for ch in ":3".chars() {
        session.handle_key(key(ch));
    }
    session.handle_key(special(KeyCodeKind::Enter));
    assert_eq!((session.mode(), session.cursor()), (Mode::Normal, (2, 0)));
}

#[test]
fn ex_paths_are_literal_and_substitute_only_parses_at_the_command_prefix() {
    let mut session = EditorSession::from_text("alpha\nalpha\n");
    session.render_layout(60);
    for (command, expected) in [
        (
            "tabnew ./examples/kitchen-sink.md",
            "./examples/kitchen-sink.md",
        ),
        (
            "tabnew /tmp/notes/spaced file!.md",
            "/tmp/notes/spaced file!.md",
        ),
    ] {
        for character in format!(":{command}").chars() {
            session.handle_key(key(character));
        }
        let effects = session.handle_key(special(KeyCodeKind::Enter));
        assert!(effects.iter().any(|effect| matches!(
            effect,
            Effect::TabNewRequested { path } if path == std::path::Path::new(expected)
        )));
        assert!(!effects.iter().any(|effect| matches!(
            effect,
            Effect::Message { text, .. } if text.contains("substitute range")
        )));
    }

    for character in ":e ./examples/kitchen-sink.md".chars() {
        session.handle_key(key(character));
    }
    let effects = session.handle_key(special(KeyCodeKind::Enter));
    assert!(effects.iter().any(|effect| matches!(
        effect,
        Effect::OpenRequested { path, force: false }
            if path == std::path::Path::new("./examples/kitchen-sink.md")
    )));

    for character in ":w /tmp/notes/spaced file.md".chars() {
        session.handle_key(key(character));
    }
    let effects = session.handle_key(special(KeyCodeKind::Enter));
    assert!(effects.iter().any(|effect| matches!(
        effect,
        Effect::SaveRequested { path: Some(path), .. }
            if path == std::path::Path::new("/tmp/notes/spaced file.md")
    )));

    for character in ":%s/alpha/beta/g".chars() {
        session.handle_key(key(character));
    }
    session.handle_key(special(KeyCodeKind::Enter));
    assert_eq!(session.document(), "beta\nbeta\n");
}

#[test]
fn reload_commands_emit_explicit_effects_without_empty_open_paths() {
    let mut session = EditorSession::from_text("current\n");
    session.render_layout(60);
    for (command, expected) in [
        ("e", Effect::ReloadCurrentRequested { force: false }),
        ("e!", Effect::ReloadCurrentRequested { force: true }),
        ("reload", Effect::ReloadCurrentRequested { force: true }),
        ("reload-all", Effect::ReloadAllRequested),
    ] {
        for character in format!(":{command}").chars() {
            session.handle_key(key(character));
        }
        let effects = session.handle_key(special(KeyCodeKind::Enter));
        assert!(effects.contains(&expected), "{command}: {effects:?}");
    }
    for character in ":reload extra".chars() {
        session.handle_key(key(character));
    }
    let effects = session.handle_key(special(KeyCodeKind::Enter));
    assert!(effects.iter().any(|effect| matches!(
        effect,
        Effect::Message {
            severity: oom_edit_core::Severity::Warning,
            ..
        }
    )));
}

#[test]
fn ex_history_keeps_ten_submissions_and_restores_draft_after_navigation() {
    let mut session = EditorSession::from_text("text\n");
    session.render_layout(60);
    for number in 0..12 {
        for character in format!(":unknown{number}").chars() {
            session.handle_key(key(character));
        }
        session.handle_key(special(KeyCodeKind::Enter));
    }
    for character in ":draft".chars() {
        session.handle_key(key(character));
    }
    session.handle_key(special(KeyCodeKind::Up));
    assert_eq!(session.command_line().as_deref(), Some("unknown11"));
    for _ in 0..20 {
        session.handle_key(special(KeyCodeKind::Up));
    }
    assert_eq!(session.command_line().as_deref(), Some("unknown2"));
    for _ in 0..9 {
        session.handle_key(special(KeyCodeKind::Down));
    }
    assert_eq!(session.command_line().as_deref(), Some("unknown11"));
    session.handle_key(special(KeyCodeKind::Down));
    assert_eq!(session.command_line().as_deref(), Some("draft"));
    session.handle_key(special(KeyCodeKind::Down));
    assert_eq!(session.command_line().as_deref(), Some("draft"));
    session.handle_key(special(KeyCodeKind::Esc));
    assert_eq!(session.mode(), Mode::Normal);
    session.handle_key(key(':'));
    session.handle_key(special(KeyCodeKind::Up));
    assert_eq!(session.command_line().as_deref(), Some("unknown11"));
}

#[test]
fn ex_history_detaches_on_edits_and_does_not_persist_to_new_session() {
    let mut session = EditorSession::from_text("text\n");
    session.render_layout(60);
    for character in ":unknown".chars() {
        session.handle_key(key(character));
    }
    session.handle_key(special(KeyCodeKind::Enter));
    session.handle_key(key(':'));
    session.handle_key(special(KeyCodeKind::Up));
    session.handle_key(special(KeyCodeKind::Backspace));
    session.handle_key(key('x'));
    assert_eq!(session.command_line().as_deref(), Some("unknowx"));
    session.handle_key(special(KeyCodeKind::Up));
    assert_eq!(session.command_line().as_deref(), Some("unknown"));
    session.handle_key(special(KeyCodeKind::Down));
    assert_eq!(session.command_line().as_deref(), Some("unknowx"));
    session.handle_key(special(KeyCodeKind::Enter));
    session.handle_key(key(':'));
    session.handle_key(special(KeyCodeKind::Up));
    assert_eq!(session.command_line().as_deref(), Some("unknowx"));
    session.handle_key(special(KeyCodeKind::Esc));

    let mut fresh = EditorSession::from_text("text\n");
    fresh.render_layout(60);
    fresh.handle_key(key(':'));
    fresh.handle_key(special(KeyCodeKind::Up));
    assert_eq!(fresh.command_line().as_deref(), Some(""));
}

#[test]
fn ex_history_keeps_duplicates_and_paste_detaches_recalled_path() {
    let mut session = EditorSession::from_text("text\n");
    session.render_layout(60);
    for _ in 0..2 {
        for character in ":tabnew ./old".chars() {
            session.handle_key(key(character));
        }
        session.handle_key(special(KeyCodeKind::Enter));
    }
    session.handle_key(key(':'));
    session.handle_key(special(KeyCodeKind::Up));
    assert_eq!(session.command_line().as_deref(), Some("tabnew ./old"));
    session.insert_paste("-edited.md");
    assert_eq!(
        session.command_line().as_deref(),
        Some("tabnew ./old-edited.md")
    );
    session.handle_key(special(KeyCodeKind::Up));
    assert_eq!(session.command_line().as_deref(), Some("tabnew ./old"));
    session.handle_key(special(KeyCodeKind::Down));
    assert_eq!(
        session.command_line().as_deref(),
        Some("tabnew ./old-edited.md")
    );
    session.handle_key(special(KeyCodeKind::Esc));
    session.handle_key(key(':'));
    session.handle_key(special(KeyCodeKind::Up));
    session.handle_key(special(KeyCodeKind::Up));
    session.handle_key(special(KeyCodeKind::Down));
    assert_eq!(session.command_line().as_deref(), Some("tabnew ./old"));
    session.handle_key(special(KeyCodeKind::Down));
    assert_eq!(session.command_line().as_deref(), Some(""));
    session.handle_key(special(KeyCodeKind::Enter));
    session.handle_key(key(':'));
    session.handle_key(special(KeyCodeKind::Up));
    assert_eq!(session.command_line().as_deref(), Some("tabnew ./old"));
}

#[test]
fn prefilled_command_prompt_waits_for_submission_and_preserves_history_rules() {
    let mut session = EditorSession::from_text("unchanged\n");
    session.render_layout(60);
    let effects = session.open_command_prompt("wq");
    assert_eq!(effects, vec![Effect::ModeChanged(Mode::Command)]);
    assert_eq!(session.command_line().as_deref(), Some("wq"));
    assert_eq!(session.document(), "unchanged\n");
    let rejected = session.open_command_prompt("q");
    assert!(rejected
        .iter()
        .any(|effect| matches!(effect, Effect::Message { .. })));
    assert_eq!(session.command_line().as_deref(), Some("wq"));
    let effects = session.handle_key(special(KeyCodeKind::Enter));
    assert!(effects.iter().any(|effect| matches!(
        effect,
        Effect::SaveRequested {
            then_quit: true,
            ..
        }
    )));
    session.handle_key(key(':'));
    session.handle_key(special(KeyCodeKind::Up));
    assert_eq!(session.command_line().as_deref(), Some("wq"));
    session.handle_key(special(KeyCodeKind::Esc));

    session.open_command_prompt("q!");
    session.handle_key(special(KeyCodeKind::Esc));
    session.handle_key(key(':'));
    session.handle_key(special(KeyCodeKind::Up));
    assert_eq!(session.command_line().as_deref(), Some("wq"));

    for invalid in [":q", "q\nqa!", "q\0"] {
        let mut fresh = EditorSession::from_text("unchanged\n");
        let effects = fresh.open_command_prompt(invalid);
        assert!(effects
            .iter()
            .any(|effect| matches!(effect, Effect::Message { .. })));
        assert_eq!(fresh.mode(), Mode::Normal);
        assert_eq!(fresh.document(), "unchanged\n");
    }
}

#[test]
fn prefilled_command_prompt_accepts_rendered_selection() {
    for selection in [
        key('v'),
        key('V'),
        KeyInput {
            code: KeyCode {
                kind: KeyCodeKind::Char('v'),
            },
            mods: Modifiers {
                ctrl: true,
                ..Modifiers::default()
            },
        },
    ] {
        let mut session = EditorSession::from_text("one two\n");
        session.render_layout(60);
        session.handle_key(selection);
        assert_eq!(session.mode(), Mode::Select);
        assert_eq!(
            session.open_command_prompt("e "),
            vec![Effect::ModeChanged(Mode::Command)]
        );
        assert_eq!(session.command_line().as_deref(), Some("e "));
        session.handle_key(special(KeyCodeKind::Esc));
        assert_eq!(session.document(), "one two\n");
    }
}

#[test]
fn command_paste_accepts_one_ascii_path_and_rejects_control_input_atomically() {
    let mut session = EditorSession::from_text("keep\n");
    session.render_layout(60);
    for character in ":tabnew".chars() {
        session.handle_key(key(character));
    }
    assert!(session
        .insert_paste(" ./examples/kitchen-sink.md\r\n")
        .is_empty());
    assert_eq!(
        session.command_line().as_deref(),
        Some("tabnew ./examples/kitchen-sink.md")
    );
    for invalid in ["bad\n:qa!", "bad\0path", "café.md", "bad\tpath"] {
        let before = session.command_line();
        let effects = session.insert_paste(invalid);
        assert!(effects
            .iter()
            .any(|effect| matches!(effect, Effect::Message { .. })));
        assert_eq!(session.command_line(), before);
        assert_eq!(session.document(), "keep\n");
    }
    let effects = session.handle_key(special(KeyCodeKind::Enter));
    assert!(effects.iter().any(|effect| matches!(
        effect,
        Effect::TabNewRequested { path }
            if path == std::path::Path::new("./examples/kitchen-sink.md")
    )));
}

#[test]
fn save_and_undo_dirty_tracking_survive_rendered_routing() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("dirty.md");
    let mut session = EditorSession::from_text("one\ntwo\n");
    session.render_layout(40);
    session.handle_key(key('V'));
    session.handle_key(key('d'));
    assert!(session.is_dirty());
    session.save(Some(&path), false).unwrap();
    assert!(!session.is_dirty());
    session.handle_key(key('u'));
    assert!(session.is_dirty());
    session.handle_key(ctrl('r'));
    assert!(!session.is_dirty());
}
