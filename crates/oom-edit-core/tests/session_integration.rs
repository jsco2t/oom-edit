//! Integration coverage for the four-mode rendered-first session contract.

use oom_edit_core::{
    EditorSession, Effect, KeyCode, KeyCodeKind, KeyInput, Mode, Modifiers, RenderedLineRole,
    SelectionShape, SemanticStyle, Viewport,
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

#[test]
fn session_starts_in_rendered_normal() {
    let mut session = EditorSession::from_text("# Hello\n\nWorld\n");
    assert_eq!(session.mode(), Mode::Normal);
    assert_eq!(session.cursor(), (0, 0));
    assert!(!session.render_layout(37).lines.is_empty());
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
    assert_eq!(selection.rows[0].columns, 0..4);
    session.handle_key(key('"'));
    session.handle_key(key('+'));
    let effects = session.handle_key(key('y'));
    assert!(effects.iter().any(
        |effect| matches!(effect, Effect::ClipboardWrite(payload) if payload == "e\u{301} 東")
    ));

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
        vec![0..4, 0..4, 0..4]
    );
}

#[test]
fn entity_selection_yanks_and_deletes_the_complete_markdown_source() {
    for entity in ["&amp;", "&#38;", "&#x26;", "&fjlig;"] {
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
        yank.handle_key(key('"'));
        yank.handle_key(key('+'));
        let effects = yank.handle_key(key('y'));
        assert!(effects
            .iter()
            .any(|effect| matches!(effect, Effect::ClipboardWrite(payload) if payload == entity)));

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
    session.handle_key(key('y'));
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
fn select_system_yank_emits_exact_payload() {
    let mut session = EditorSession::from_text("# one\n# two\n");
    session.render_layout(40);
    session.handle_key(key('V'));
    session.handle_key(key('"'));
    session.handle_key(key('+'));
    let effects = session.handle_key(key('y'));
    assert!(effects
        .iter()
        .any(|effect| matches!(effect, Effect::ClipboardWrite(text) if text == "# one\n")));
    assert_eq!(session.document(), "# one\n# two\n");
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
