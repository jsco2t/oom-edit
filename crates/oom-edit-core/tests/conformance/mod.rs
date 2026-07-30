//! Conformance manifest for the rendered-first four-mode public contract.

use oom_edit_core::{
    EditorSession, Effect, KeyCode, KeyCodeKind, KeyInput, Mode, Modifiers, SelectionShape,
};
use proptest::prelude::*;

const REQUIRED_PUBLIC_BEHAVIORS: &[&str] = &[
    "VN-1:j",
    "VN-1:k",
    "VN-1:Down",
    "VN-1:Up",
    "VN-2:gg",
    "VN-2:G",
    "VN-2:count-gg",
    "VN-2:count-G",
    "VN-2:Ctrl-d",
    "VN-2:Ctrl-u",
    "VN-2:Ctrl-f",
    "VN-2:Ctrl-b",
    "VN-2:{",
    "VN-2:}",
    "VN-3:Tab",
    "VN-3:Shift-Tab",
    "VN-3:Enter",
    "VN-4:[[",
    "VN-4:]]",
    "VN-5:/",
    "VN-5:?",
    "VN-5:n",
    "VN-5:N",
    "VN-6:gt",
    "VN-6:gT",
    "VN-6:unbound",
    "VP-1:ranges",
    "VP-2:mapping",
    "VP-3:canonical",
    "VP-4:property",
    "SEL-1:v",
    "SEL-1:V",
    "SEL-1:Ctrl-V",
    "SEL-1:motion",
    "SEL-1:swap",
    "SEL-1:cancel",
    "SEL-2:character",
    "SEL-2:empty",
    "SEL-3:line",
    "SEL-4:block",
    "SEL-5:character-y",
    "SEL-5:character-d",
    "SEL-5:character-x",
    "SEL-5:character-c",
    "SEL-5:line-y",
    "SEL-5:line-d",
    "SEL-5:line-x",
    "SEL-5:line-c",
    "SEL-5:block-y",
    "SEL-5:block-d",
    "SEL-5:block-x",
    "SEL-5:block-c",
    "SEL-6:named",
    "SEL-6:black-hole",
    "SEL-6:numbered-small-delete",
    "SEL-6:system-clipboard",
    "SEL-7:block-undo",
    "SEL-7:block-redo",
    "SEL-7:put-shape",
    "SEL-8:indent",
    "SEL-8:outdent",
    "SEL-9:metadata",
    "SEL-10:intervals",
    "SEL-11:manifest",
    "CMI-1:start",
    "CMI-2:insert",
    "CMI-2:select",
    "CMI-2:command",
    "CMI-2:resize",
    "CMI-3:gutter-ruler",
    "CMI-4:styles",
    "V-X1:write",
    "V-X2:quit",
    "V-X3:write-quit",
    "V-X4:edit",
    "V-X5:saveas",
    "V-X6:line",
    "V-X7:substitute",
    "V-X8:noh-help",
    "SP-1:set-toggle",
    "SP-1:session-isolation",
    "SP-2:next-previous-wrap",
    "SP-2:empty",
    "SP-2:mid-diagnostic",
    "SP-2:count",
    "SP-3:cursor-half-open",
    "SP-4:stale-replacement",
    "SP-4:same-range-different-text",
    "SP-5:disabled-queries",
    "SP-5:disabled-edit-resume",
    "SP-6:generation-self-heal",
    "SP-7:utf8-eof-position",
    "SP-8:atomic-jump",
];

type ConformanceCase = (&'static str, &'static [&'static str], fn());

const COVERAGE_CASES: &[ConformanceCase] = &[
    (
        "rendered vertical j/k/arrows",
        &["VN-1:j", "VN-1:k", "VN-1:Down", "VN-1:Up"],
        rendered_vertical_motions,
    ),
    (
        "rendered document/page/block/count motions",
        &[
            "VN-2:gg",
            "VN-2:G",
            "VN-2:count-gg",
            "VN-2:count-G",
            "VN-2:Ctrl-d",
            "VN-2:Ctrl-u",
            "VN-2:Ctrl-f",
            "VN-2:Ctrl-b",
            "VN-2:{",
            "VN-2:}",
        ],
        rendered_document_page_block_and_count_motions,
    ),
    (
        "rendered jump targets",
        &["VN-3:Tab", "VN-3:Shift-Tab", "VN-3:Enter"],
        rendered_jump_targets_both_directions_and_enter,
    ),
    (
        "rendered heading motions",
        &["VN-4:[[", "VN-4:]]"],
        rendered_heading_motions,
    ),
    (
        "rendered search directions/repeats",
        &["VN-5:/", "VN-5:?", "VN-5:n", "VN-5:N"],
        rendered_search_directions_and_repeats,
    ),
    (
        "rendered reserved/unbound no-op",
        &["VN-6:gt", "VN-6:gT", "VN-6:unbound"],
        rendered_reserved_and_unbound_keys_are_noops,
    ),
    (
        "source spans and mapping",
        &["VP-1:ranges", "VP-2:mapping"],
        exact_markdown_structure_ranges,
    ),
    (
        "canonical rendered movement",
        &["VP-3:canonical", "CMI-3:gutter-ruler"],
        rendered_navigation_and_search_update_canonical_source,
    ),
    (
        "cross-mode property",
        &["VP-4:property"],
        random_mode_width_and_motion_mapping_stays_in_bounds,
    ),
    (
        "Select endpoint mechanics",
        &["SEL-1:v", "SEL-1:V", "SEL-1:Ctrl-V", "SEL-1:swap"],
        select_forward_reverse_swap_and_escape,
    ),
    (
        "Select required motion surface",
        &["SEL-1:motion"],
        select_required_motion_surface,
    ),
    (
        "Select exact shape projections",
        &[
            "SEL-2:character",
            "SEL-2:empty",
            "SEL-3:line",
            "SEL-4:block",
        ],
        select_shape_projections_are_exact,
    ),
    (
        "Select metadata intervals and semantic styles",
        &["SEL-9:metadata", "SEL-10:intervals", "CMI-4:styles"],
        select_metadata_intervals_and_styles,
    ),
    (
        "Select register/put/history",
        &[
            "SEL-6:named",
            "SEL-6:black-hole",
            "SEL-6:numbered-small-delete",
            "SEL-6:system-clipboard",
            "SEL-7:put-shape",
        ],
        select_operators_registers_put_and_history_conform,
    ),
    (
        "Select shape/operator matrix",
        &[
            "SEL-5:character-y",
            "SEL-5:character-d",
            "SEL-5:character-x",
            "SEL-5:character-c",
            "SEL-5:line-y",
            "SEL-5:line-d",
            "SEL-5:line-x",
            "SEL-5:line-c",
            "SEL-5:block-y",
            "SEL-5:block-d",
            "SEL-5:block-x",
            "SEL-5:block-c",
            "SEL-7:block-undo",
            "SEL-7:block-redo",
        ],
        select_shape_operator_matrix_conforms,
    ),
    (
        "Select line operators and cancel",
        &["SEL-1:cancel", "SEL-8:indent", "SEL-8:outdent"],
        select_delete_x_change_indent_and_outdent_conform,
    ),
    (
        "Select manifest",
        &["SEL-11:manifest"],
        select_manifest_meta_test_covers_every_declared_requirement,
    ),
    (
        "initial mode",
        &["CMI-1:start"],
        session_starts_in_rendered_normal,
    ),
    (
        "mode transitions",
        &["CMI-2:insert", "CMI-2:select", "CMI-2:command"],
        all_mode_roundtrips_preserve_source_anchor,
    ),
    (
        "exact Select resize provenance",
        &["CMI-2:resize"],
        select_resize_preserves_exact_endpoint_provenance,
    ),
    ("Ex write", &["V-X1:write"], v_x1_write),
    ("Ex quit", &["V-X2:quit"], v_x2_quit),
    ("Ex write quit", &["V-X3:write-quit"], v_x3_write_quit),
    ("Ex edit", &["V-X4:edit"], v_x4_edit),
    ("Ex saveas", &["V-X5:saveas"], v_x5_saveas),
    ("Ex line jump", &["V-X6:line"], v_x6_line_jump),
    ("Ex substitute", &["V-X7:substitute"], v_x7_substitute),
    ("Ex noh/help", &["V-X8:noh-help"], v_x8_noh_and_help),
    (
        "spell toggles and session isolation",
        &["SP-1:set-toggle", "SP-1:session-isolation"],
        spell_toggles_and_session_isolation,
    ),
    (
        "spell navigation",
        &[
            "SP-2:next-previous-wrap",
            "SP-2:empty",
            "SP-2:mid-diagnostic",
            "SP-2:count",
        ],
        spell_navigation_empty_wrap_and_mid_diagnostic,
    ),
    (
        "spell diagnostic cursor boundaries",
        &["SP-3:cursor-half-open"],
        spell_diagnostic_cursor_is_half_open,
    ),
    (
        "spell replacement revalidation",
        &["SP-4:stale-replacement", "SP-4:same-range-different-text"],
        spell_replacement_revalidates_identity_and_text,
    ),
    (
        "spell disabled semantics",
        &["SP-5:disabled-queries", "SP-5:disabled-edit-resume"],
        spell_disabled_queries_and_edit_resume,
    ),
    (
        "spell generation self healing",
        &["SP-6:generation-self-heal"],
        spell_generation_self_heals,
    ),
    (
        "spell positions and atomic jump",
        &["SP-7:utf8-eof-position", "SP-8:atomic-jump"],
        spell_positions_and_atomic_jump,
    ),
];

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

fn move_to_text(session: &mut EditorSession, needle: &str, width: u16) {
    let target = session
        .render_layout(width)
        .lines
        .iter()
        .position(|line| line.styled.text.contains(needle))
        .unwrap();
    while session.rendered_cursor_line() < target {
        session.handle_key(key('j'));
    }
}

fn spell_engine(words: &str) -> oom_spell::SpellEngine {
    let mut builder = oom_spell::SpellEngineBuilder::new(vec![words.to_string()]);
    while builder.step(31) != oom_spell::BuildProgress::Complete {}
    builder.finish().unwrap()
}

fn drain_spell(session: &mut EditorSession, engine: &oom_spell::SpellEngine) {
    while session.diagnostics_pending() {
        assert!(session.spell_tick(engine, 7));
    }
}

fn enter_ex(session: &mut EditorSession, command: &str) -> Vec<Effect> {
    session.handle_key(key(':'));
    for character in command.chars() {
        session.handle_key(key(character));
    }
    session.handle_key(special(KeyCodeKind::Enter))
}

#[test]
fn select_manifest_meta_test_covers_every_declared_requirement() {
    let mut covered: Vec<_> = COVERAGE_CASES
        .iter()
        .flat_map(|(_, ids, _)| ids.iter().copied())
        .collect();
    covered.sort_unstable();
    let mut required = REQUIRED_PUBLIC_BEHAVIORS.to_vec();
    required.sort_unstable();
    assert_eq!(covered, required, "public behavior manifest drift");
    let unique: std::collections::HashSet<_> = covered.iter().copied().collect();
    assert_eq!(unique.len(), covered.len(), "duplicate behavior mappings");
    assert!(COVERAGE_CASES.iter().all(|(name, ids, _)| {
        !name.trim().is_empty() && !ids.is_empty() && ids.iter().all(|id| id.contains('-'))
    }));
}

#[test]
fn session_starts_in_rendered_normal() {
    let session = EditorSession::from_text("");
    assert_eq!((session.mode(), session.cursor()), (Mode::Normal, (0, 0)));
}

#[test]
fn rendered_vertical_motions() {
    for down in [key('j'), special(KeyCodeKind::Down)] {
        let mut session = EditorSession::from_text("one\n\ntwo\n\nthree\n");
        session.render_layout(40);
        let start = session.rendered_cursor_line();
        session.handle_key(down);
        assert!(session.rendered_cursor_line() > start);
        let moved = session.rendered_cursor_line();
        session.handle_key(if matches!(down.code.kind, KeyCodeKind::Down) {
            special(KeyCodeKind::Up)
        } else {
            key('k')
        });
        assert!(session.rendered_cursor_line() < moved);
    }
}

#[test]
fn rendered_document_page_block_and_count_motions() {
    let text = (1..=20)
        .map(|number| format!("paragraph {number}\n\n"))
        .collect::<String>();
    let mut session = EditorSession::from_text(&text);
    let last = session.render_layout(40).lines.len() - 1;

    session.handle_key(ctrl('d'));
    assert!(session.rendered_cursor_line() > 0);
    session.handle_key(ctrl('u'));
    assert_eq!(session.rendered_cursor_line(), 0);
    session.handle_key(ctrl('f'));
    assert_eq!(session.rendered_cursor_line(), last);
    session.handle_key(ctrl('b'));
    assert_eq!(session.rendered_cursor_line(), 0);

    session.handle_key(key('}'));
    assert!(session.rendered_cursor_line() > 0);
    session.handle_key(key('{'));
    assert!(session.rendered_cursor_line() < last);

    for ch in "3gg".chars() {
        session.handle_key(key(ch));
    }
    assert_eq!(session.rendered_cursor_line(), 2);
    for ch in "5G".chars() {
        session.handle_key(key(ch));
    }
    assert_eq!(session.rendered_cursor_line(), 4);
    session.handle_key(key('G'));
    assert_eq!(session.rendered_cursor_line(), last);
    session.handle_key(key('g'));
    session.handle_key(key('g'));
    assert_eq!(session.rendered_cursor_line(), 0);
}

#[test]
fn rendered_jump_targets_both_directions_and_enter() {
    let mut session =
        EditorSession::from_text("[first](https://first.test)\n\n[second](https://second.test)\n");
    session.render_layout(50);
    session.handle_key(special(KeyCodeKind::Tab));
    let second = session.rendered_cursor_line();
    assert!(second > 0);
    let effects = session.handle_key(special(KeyCodeKind::Enter));
    assert!(effects.iter().any(|effect| matches!(
        effect,
        Effect::Message { text, .. } if text.contains("https://second.test")
    )));
    session.handle_key(special(KeyCodeKind::BackTab));
    assert!(session.rendered_cursor_line() < second);
}

#[test]
fn rendered_heading_motions() {
    let mut session = EditorSession::from_text("# One\n\nbody\n\n# Two\n\nbody\n\n# Three\n");
    session.render_layout(50);
    session.handle_key(key(']'));
    session.handle_key(key(']'));
    assert_eq!(session.cursor().0, 4);
    session.handle_key(key('['));
    session.handle_key(key('['));
    assert_eq!(session.cursor().0, 0);
}

#[test]
fn rendered_search_directions_and_repeats() {
    let mut forward = EditorSession::from_text("alpha\n\nbeta\n\nalpha\n");
    forward.render_layout(40);
    forward.handle_key(key('/'));
    for ch in "alpha".chars() {
        forward.handle_key(key(ch));
    }
    forward.handle_key(special(KeyCodeKind::Enter));
    assert_eq!(forward.cursor().0, 4);
    forward.handle_key(key('n'));
    assert_eq!(forward.cursor().0, 0);
    forward.handle_key(key('N'));
    assert_eq!(forward.cursor().0, 4);

    let mut backward = EditorSession::from_text("alpha\n\nbeta\n\nalpha\n");
    backward.render_layout(40);
    backward.handle_key(key('G'));
    backward.handle_key(key('?'));
    for ch in "beta".chars() {
        backward.handle_key(key(ch));
    }
    backward.handle_key(special(KeyCodeKind::Enter));
    assert_eq!(backward.cursor().0, 2);
}

#[test]
fn rendered_reserved_and_unbound_keys_are_noops() {
    for keys in ["gt", "gT", "x", "Q"] {
        let mut session = EditorSession::from_text("one\n\ntwo\n");
        session.render_layout(40);
        let before = (
            session.document(),
            session.cursor(),
            session.rendered_cursor_line(),
        );
        for ch in keys.chars() {
            session.handle_key(key(ch));
        }
        assert_eq!(
            (
                session.document(),
                session.cursor(),
                session.rendered_cursor_line()
            ),
            before,
            "unbound sequence {keys:?}"
        );
    }
}

#[test]
fn rendered_navigation_and_search_update_canonical_source() {
    let mut session =
        EditorSession::from_text("# One\n\nTwo\n\n# Links\n\n[link](https://example.test)\n");
    session.render_layout(32);
    session.handle_key(key('j'));
    session.handle_key(key('j'));
    assert_eq!(session.cursor(), (2, 0));
    session.handle_key(special(KeyCodeKind::Tab));
    assert!(session.rendered_cursor_line() > 0);
    let before_gg = session.rendered_cursor_line();
    session.handle_key(key('g'));
    assert_eq!(session.rendered_cursor_line(), before_gg);
    session.handle_key(key('g'));
    assert_eq!(session.cursor(), (0, 2));
    session.handle_key(key('G'));
    assert!(session.cursor().0 >= 4);
    session.handle_key(key('/'));
    for ch in "Two".chars() {
        session.handle_key(key(ch));
    }
    session.handle_key(special(KeyCodeKind::Enter));
    assert_eq!(session.cursor(), (2, 0));
}

#[test]
fn select_forward_reverse_swap_and_escape() {
    let text = "one\ntwo\nthree\n";
    let mut session = EditorSession::from_text(text);
    session.render_layout(40);
    session.handle_key(key('V'));
    session.handle_key(key('j'));
    let before_swap = session.rendered_selection().unwrap();
    session.handle_key(key('o'));
    let after_swap = session.rendered_selection().unwrap();
    assert_eq!(before_swap.anchor, after_swap.active);
    assert_eq!(before_swap.active, after_swap.anchor);
    assert_eq!(before_swap.source_ranges, after_swap.source_ranges);
    session.handle_key(special(KeyCodeKind::Esc));
    assert_eq!(
        (session.mode(), session.document()),
        (Mode::Normal, text.to_string())
    );

    let mut shapes = EditorSession::from_text("alpha\n\nbeta\n");
    shapes.render_layout(40);
    shapes.handle_key(key('v'));
    assert_eq!(
        shapes.rendered_selection().unwrap().shape,
        SelectionShape::Character
    );
    shapes.handle_key(key('l'));
    assert!(shapes.rendered_selection().unwrap().source_ranges[0].len() > 1);
    shapes.handle_key(key('V'));
    assert_eq!(
        shapes.rendered_selection().unwrap().shape,
        SelectionShape::Line
    );
    shapes.handle_key(ctrl('v'));
    assert_eq!(
        shapes.rendered_selection().unwrap().shape,
        SelectionShape::Block
    );
    shapes.handle_key(ctrl('v'));
    assert_eq!(shapes.mode(), Mode::Normal);

    shapes.handle_key(key('v'));
    shapes.handle_key(ctrl('c'));
    assert_eq!(shapes.mode(), Mode::Normal);
}

#[test]
fn select_shape_projections_are_exact() {
    let mut character = EditorSession::from_text("abcd\n");
    character.render_layout(40);
    character.handle_key(key('v'));
    character.handle_key(key('l'));
    let selection = character.rendered_selection().unwrap();
    assert_eq!(selection.shape, SelectionShape::Character);
    assert_eq!(selection.source_ranges, vec![0..2]);
    assert_eq!(selection.rows[0].columns, 0..2);

    let wrapped_text = "alpha beta gamma delta\n";
    let mut line = EditorSession::from_text(wrapped_text);
    let rendered_rows = line.render_layout(8).lines.len();
    assert!(rendered_rows > 1);
    line.handle_key(key('V'));
    let selection = line.rendered_selection().unwrap();
    assert_eq!(selection.shape, SelectionShape::Line);
    assert_eq!(selection.source_ranges, vec![0..wrapped_text.len()]);
    assert_eq!(selection.rows.len(), rendered_rows);
    assert!(selection
        .rows
        .iter()
        .all(|row| row.columns.start == 0 && row.columns.start < row.columns.end));

    let mut block = EditorSession::from_text("abcd\n\nwxyz\n");
    block.render_layout(40);
    block.handle_key(ctrl('v'));
    block.handle_key(key('l'));
    block.handle_key(key('j'));
    block.handle_key(key('j'));
    let selection = block.rendered_selection().unwrap();
    assert_eq!(selection.shape, SelectionShape::Block);
    assert_eq!(selection.source_ranges, vec![0..2, 6..8]);
    assert_eq!(selection.block_width, Some(2));
    assert_eq!(
        selection
            .rows
            .iter()
            .map(|row| row.columns.clone())
            .collect::<Vec<_>>(),
        vec![0..2, 0..2, 0..2]
    );

    let mut empty = EditorSession::from_text("one\n\ntwo\n");
    empty.render_layout(40);
    empty.handle_key(key('j'));
    empty.handle_key(key('v'));
    assert!(empty.rendered_selection().unwrap().source_ranges.is_empty());
}

#[test]
fn select_required_motion_surface() {
    for (motion, column, source_column, ranges) in [
        (key('l'), 1, 1, 0..2),
        (special(KeyCodeKind::Right), 1, 1, 0..2),
        (key('w'), 6, 6, 0..7),
        (key('W'), 6, 6, 0..7),
        (key('e'), 4, 4, 0..5),
        (key('E'), 4, 4, 0..5),
        (key('$'), 21, 21, 0..22),
    ] {
        let mut session = EditorSession::from_text("alpha beta-gamma delta\n");
        session.render_layout(40);
        session.handle_key(key('v'));
        session.handle_key(motion);
        let selection = session.rendered_selection().unwrap();
        assert_eq!(
            selection.active,
            oom_edit_core::RenderedPoint { row: 0, column },
            "{motion:?}"
        );
        assert_eq!(session.cursor(), (0, source_column), "{motion:?}");
        assert_eq!(
            selection.source_ranges.as_slice(),
            std::slice::from_ref(&ranges),
            "{motion:?}"
        );
    }

    for (motion, column, source_column, ranges) in [
        (key('h'), 20, 20, 20..22),
        (special(KeyCodeKind::Left), 20, 20, 20..22),
        (key('b'), 17, 17, 17..22),
        (key('B'), 17, 17, 17..22),
        (key('0'), 0, 0, 0..22),
        (key('^'), 0, 0, 0..22),
    ] {
        let mut session = EditorSession::from_text("alpha beta-gamma delta\n");
        session.render_layout(40);
        session.handle_key(key('$'));
        session.handle_key(key('v'));
        session.handle_key(motion);
        let selection = session.rendered_selection().unwrap();
        assert_eq!(
            selection.active,
            oom_edit_core::RenderedPoint { row: 0, column },
            "{motion:?}"
        );
        assert_eq!(session.cursor(), (0, source_column), "{motion:?}");
        assert_eq!(
            selection.source_ranges.as_slice(),
            std::slice::from_ref(&ranges),
            "{motion:?}"
        );
    }

    let document = (1..=24)
        .map(|number| format!("paragraph {number}\n\n"))
        .collect::<String>();
    for (motion, row, source, range_count, last_end) in [
        (key('j'), 1, (0, 0), 1, 11),
        (special(KeyCodeKind::Down), 1, (0, 0), 1, 11),
        (ctrl('d'), 23, (22, 0), 12, 157),
        (ctrl('f'), 46, (46, 0), 24, 314),
        (key('G'), 46, (46, 0), 24, 314),
        (key('}'), 1, (0, 0), 1, 11),
    ] {
        let mut session = EditorSession::from_text(&document);
        session.render_layout(20);
        session.handle_key(key('v'));
        session.handle_key(motion);
        let selection = session.rendered_selection().unwrap();
        assert_eq!(
            selection.active,
            oom_edit_core::RenderedPoint { row, column: 0 },
            "{motion:?}"
        );
        assert_eq!(session.cursor(), source, "{motion:?}");
        assert_eq!(selection.source_ranges.len(), range_count, "{motion:?}");
        assert_eq!(
            selection.source_ranges.first().unwrap().start,
            0,
            "{motion:?}"
        );
        assert_eq!(
            selection.source_ranges.last().unwrap().end,
            last_end,
            "{motion:?}"
        );
    }

    for (motion, row, source, range_count, first_start) in [
        (key('k'), 45, (44, 0), 1, 313),
        (special(KeyCodeKind::Up), 45, (44, 0), 1, 313),
        (ctrl('u'), 23, (22, 0), 12, 159),
        (ctrl('b'), 0, (0, 0), 24, 0),
        (key('{'), 45, (44, 0), 1, 313),
    ] {
        let mut session = EditorSession::from_text(&document);
        session.render_layout(20);
        session.handle_key(key('G'));
        session.handle_key(key('v'));
        session.handle_key(motion);
        let selection = session.rendered_selection().unwrap();
        assert_eq!(
            selection.active,
            oom_edit_core::RenderedPoint { row, column: 0 },
            "{motion:?}"
        );
        assert_eq!(session.cursor(), source, "{motion:?}");
        assert_eq!(selection.source_ranges.len(), range_count, "{motion:?}");
        assert_eq!(
            selection.source_ranges.first().unwrap().start,
            first_start,
            "{motion:?}"
        );
        assert_eq!(
            selection.source_ranges.last().unwrap().end,
            314,
            "{motion:?}"
        );
    }

    let mut gg = EditorSession::from_text(&document);
    gg.render_layout(20);
    gg.handle_key(key('G'));
    gg.handle_key(key('v'));
    gg.handle_key(key('g'));
    gg.handle_key(key('g'));
    let selection = gg.rendered_selection().unwrap();
    assert_eq!(
        selection.active,
        oom_edit_core::RenderedPoint { row: 0, column: 0 }
    );
    assert_eq!(gg.cursor(), (0, 0));
    assert_eq!(selection.source_ranges.len(), 24);
    assert_eq!(selection.source_ranges.first().unwrap().start, 0);
    assert_eq!(selection.source_ranges.last().unwrap().end, 314);

    let mut search = EditorSession::from_text("one\n\ntarget\n");
    search.render_layout(20);
    search.handle_key(key('v'));
    search.handle_key(key('/'));
    for character in "target".chars() {
        search.handle_key(key(character));
    }
    search.handle_key(special(KeyCodeKind::Enter));
    let selection = search.rendered_selection().unwrap();
    assert_eq!(
        selection.active,
        oom_edit_core::RenderedPoint { row: 2, column: 0 }
    );
    assert_eq!(search.cursor(), (2, 0));
    assert_eq!(selection.source_ranges, vec![0..3, 5..6]);

    let mut jump = EditorSession::from_text("before\n\n[link](https://example.test)\n");
    jump.render_layout(40);
    jump.handle_key(key('v'));
    jump.handle_key(special(KeyCodeKind::Tab));
    let selection = jump.rendered_selection().unwrap();
    assert_eq!(
        selection.active,
        oom_edit_core::RenderedPoint { row: 2, column: 0 }
    );
    assert_eq!(jump.cursor(), (2, 1));
    assert_eq!(selection.source_ranges, vec![0..6, 9..10]);

    let mut desired = EditorSession::from_text("abcdefghij\n\nxy\n\nabcdefghij\n");
    desired.render_layout(40);
    desired.handle_key(key('$'));
    desired.handle_key(key('v'));
    for _ in 0..4 {
        desired.handle_key(key('j'));
    }
    assert_eq!(desired.rendered_selection().unwrap().active.column, 9);
}

#[test]
fn select_metadata_intervals_and_styles() {
    let text = "---\ntitle: Example\n---\n\nbody\n";
    let mut session = EditorSession::from_text(text);
    let before = session.render_layout(40).clone();
    session.handle_key(key('j'));
    session.handle_key(key('v'));
    session.handle_key(key('l'));
    let selection = session.rendered_selection().unwrap();
    assert_eq!(selection.shape, SelectionShape::Character);
    assert_eq!(selection.rows.len(), 1);
    assert_eq!(
        selection.rows[0].columns.end - selection.rows[0].columns.start,
        2
    );
    assert!(selection.source_ranges.iter().all(|range| {
        range.start < range.end && range.end <= text.len() && !text[range.clone()].contains('│')
    }));
    let selected_row = selection.rows[0].row;
    let after = session.rendered_layout().unwrap();
    assert_eq!(
        after.lines[selected_row].role,
        oom_edit_core::RenderedLineRole::Metadata
    );
    assert_eq!(
        after.lines[selected_row].styled,
        before.lines[selected_row].styled
    );
    assert!(!after.lines[selected_row].styled.spans.is_empty());
}

#[test]
fn select_operators_registers_put_and_history_conform() {
    let mut session = EditorSession::from_text("# one\n# two\n# three\n");
    session.render_layout(40);
    session.handle_key(key('V'));
    session.handle_key(key('"'));
    session.handle_key(key('+'));
    let effects = session.handle_key(key('y'));
    assert!(effects
        .iter()
        .any(|effect| matches!(effect, Effect::ClipboardWrite(text) if text == "# one\n")));

    session.handle_key(key('V'));
    session.handle_key(key('d'));
    assert_eq!(session.document(), "# two\n# three\n");
    session.render_layout(40);
    session.handle_key(key('P'));
    assert_eq!(session.document(), "# one\n# two\n# three\n");
    session.handle_key(key('u'));
    assert_eq!(session.document(), "# two\n# three\n");
    session.handle_key(ctrl('r'));
    assert_eq!(session.document(), "# one\n# two\n# three\n");

    let mut named = EditorSession::from_text("abcd\n");
    named.render_layout(40);
    named.handle_key(key('v'));
    named.handle_key(key('l'));
    named.handle_key(key('"'));
    named.handle_key(key('a'));
    named.handle_key(key('y'));
    named.handle_key(key('"'));
    named.handle_key(key('a'));
    named.handle_key(key('p'));
    assert_eq!(named.document().len(), "abcd\n".len() + 2);

    for register in ['d', 'v', 'o', 'y', 'c', 'x'] {
        let mut named = EditorSession::from_text("abcd\n");
        named.render_layout(40);
        named.handle_key(key('v'));
        named.handle_key(key('l'));
        named.handle_key(key('"'));
        named.handle_key(key(register));
        named.handle_key(key('y'));
        assert_eq!(named.document(), "abcd\n", "register {register}");
        named.render_layout(40);
        named.handle_key(key('g'));
        named.handle_key(key('g'));
        named.handle_key(key('0'));
        named.handle_key(key('"'));
        named.handle_key(key(register));
        named.handle_key(key('P'));
        assert_eq!(named.document(), "ababcd\n", "register {register}");
    }

    let mut numbered = EditorSession::from_text("# one\n# two\n");
    numbered.render_layout(40);
    numbered.handle_key(key('V'));
    numbered.handle_key(key('d'));
    numbered.render_layout(40);
    numbered.handle_key(key('"'));
    numbered.handle_key(key('1'));
    numbered.handle_key(key('P'));
    assert_eq!(numbered.document(), "# one\n# two\n");

    let mut small_delete = EditorSession::from_text("abcd\n");
    small_delete.render_layout(40);
    small_delete.handle_key(key('v'));
    small_delete.handle_key(key('l'));
    small_delete.handle_key(key('d'));
    small_delete.render_layout(40);
    small_delete.handle_key(key('"'));
    small_delete.handle_key(key('-'));
    small_delete.handle_key(key('P'));
    assert_eq!(small_delete.document(), "abcd\n");

    let mut black_hole = EditorSession::from_text("# one\n# two\n");
    black_hole.render_layout(40);
    black_hole.handle_key(key('V'));
    black_hole.handle_key(key('y'));
    black_hole.render_layout(40);
    black_hole.handle_key(key('j'));
    black_hole.handle_key(key('j'));
    black_hole.handle_key(key('V'));
    black_hole.handle_key(key('"'));
    black_hole.handle_key(key('_'));
    black_hole.handle_key(key('d'));
    assert_eq!(black_hole.document(), "# one\n");
    black_hole.render_layout(40);
    black_hole.handle_key(key('p'));
    assert!(!black_hole.document().contains("# two"));
    assert_eq!(black_hole.document().matches("# one").count(), 2);

    let mut empty = EditorSession::from_text("one\n\ntwo\n");
    empty.render_layout(40);
    empty.handle_key(key('j'));
    empty.handle_key(key('v'));
    assert!(empty.rendered_selection().unwrap().source_ranges.is_empty());
    empty.handle_key(key('c'));
    assert_eq!(
        (empty.mode(), empty.document()),
        (Mode::Select, "one\n\ntwo\n".into())
    );
    assert!(!empty.is_dirty());
}

#[test]
fn select_delete_x_change_indent_and_outdent_conform() {
    let mut delete = EditorSession::from_text("# one\n# two\n");
    delete.render_layout(40);
    delete.handle_key(key('V'));
    delete.handle_key(key('x'));
    assert_eq!(delete.document(), "# two\n");

    let mut change = EditorSession::from_text("# one\n# two\n");
    change.render_layout(40);
    change.handle_key(key('V'));
    change.handle_key(key('c'));
    assert_eq!(change.mode(), Mode::Insert);

    let mut indent = EditorSession::from_text("# one\n# two\n");
    indent.render_layout(40);
    indent.handle_key(key('V'));
    indent.handle_key(key('j'));
    indent.handle_key(key('j'));
    indent.handle_key(key('>'));
    assert_eq!(indent.document(), "    # one\n    # two\n");
    indent.render_layout(40);
    indent.handle_key(key('V'));
    indent.handle_key(key('j'));
    indent.handle_key(key('j'));
    indent.handle_key(key('<'));
    assert_eq!(indent.document(), "# one\n# two\n");
}

#[test]
fn select_shape_operator_matrix_conforms() {
    for shape in [
        SelectionShape::Character,
        SelectionShape::Line,
        SelectionShape::Block,
    ] {
        for operator in ['y', 'd', 'x', 'c'] {
            let (text, payload, deleted) = match shape {
                SelectionShape::Character => ("abcd\n", "ab", "cd\n"),
                SelectionShape::Line => ("# one\n# two\n", "# one\n", "# two\n"),
                SelectionShape::Block => ("abcd\n\nwxyz\n", "ab\n\nwx", "cd\n\nyz\n"),
            };
            let mut session = EditorSession::from_text(text);
            session.render_layout(40);
            match shape {
                SelectionShape::Character => {
                    session.handle_key(key('v'));
                    session.handle_key(key('l'));
                }
                SelectionShape::Line => {
                    session.handle_key(key('V'));
                }
                SelectionShape::Block => {
                    session.handle_key(ctrl('v'));
                    session.handle_key(key('l'));
                    session.handle_key(key('j'));
                    session.handle_key(key('j'));
                }
            }
            session.handle_key(key('"'));
            session.handle_key(key('+'));
            let effects = session.handle_key(key(operator));
            assert!(effects.iter().any(
                |effect| matches!(effect, Effect::ClipboardWrite(actual) if actual == payload)
            ));
            if operator == 'y' {
                assert_eq!(session.document(), text, "{shape:?}/{operator}");
                assert_eq!(session.mode(), Mode::Normal, "{shape:?}/{operator}");
            } else {
                assert_eq!(session.document(), deleted, "{shape:?}/{operator}");
                assert_eq!(session.cursor(), (0, 0), "{shape:?}/{operator}");
                assert_eq!(
                    session.mode(),
                    if operator == 'c' {
                        Mode::Insert
                    } else {
                        Mode::Normal
                    },
                    "{shape:?}/{operator}"
                );
            }
        }
    }

    for (shape, text, expected) in [
        (SelectionShape::Character, "abcd\n", "ababcd\n"),
        (
            SelectionShape::Line,
            "# one\n# two\n",
            "# one\n# one\n# two\n",
        ),
        (
            SelectionShape::Block,
            "abcd\n\nwxyz\n",
            "ababcd\n\nwxwxyz\n",
        ),
    ] {
        let mut session = EditorSession::from_text(text);
        session.render_layout(40);
        match shape {
            SelectionShape::Character => {
                session.handle_key(key('v'));
                session.handle_key(key('l'));
            }
            SelectionShape::Line => {
                session.handle_key(key('V'));
            }
            SelectionShape::Block => {
                session.handle_key(ctrl('v'));
                session.handle_key(key('l'));
                session.handle_key(key('j'));
                session.handle_key(key('j'));
            }
        }
        session.handle_key(key('y'));
        session.render_layout(40);
        session.handle_key(key('g'));
        session.handle_key(key('g'));
        session.handle_key(key('0'));
        session.handle_key(key('P'));
        assert_eq!(session.document(), expected, "{shape:?} put shape");
        session.handle_key(key('u'));
        assert_eq!(session.document(), text, "{shape:?} put undo");
    }

    let mut block = EditorSession::from_text("abcd\n\nwxyz\n");
    block.render_layout(40);
    block.handle_key(ctrl('v'));
    block.handle_key(key('l'));
    block.handle_key(key('j'));
    block.handle_key(key('j'));
    block.handle_key(key('d'));
    assert_eq!(block.document(), "cd\n\nyz\n");
    block.handle_key(key('u'));
    assert_eq!(block.document(), "abcd\n\nwxyz\n");
    block.handle_key(ctrl('r'));
    assert_eq!(block.document(), "cd\n\nyz\n");
}

#[test]
fn rendered_character_operators_preserve_unselected_markdown_syntax() {
    let cases = [
        ("\\*escaped\\*\n", 8, "\\*escaped\\*", "\n"),
        ("*emphasis*\n", 7, "emphasis", "**\n"),
        ("`code`\n", 3, "code", "``\n"),
        (
            "[label](https://example.test)\n",
            4,
            "label",
            "[](https://example.test)\n",
        ),
        ("![alt](image.png)\n", 2, "alt", "![](image.png)\n"),
        ("**[nested](target)**\n", 5, "nested", "**[](target)**\n"),
    ];

    for (source, right_moves, payload, deleted) in cases {
        for operator in ['y', 'd', 'c'] {
            let mut session = EditorSession::from_text(source);
            session.render_layout(80);
            session.handle_key(key('v'));
            for _ in 0..right_moves {
                session.handle_key(key('l'));
            }
            session.handle_key(key('"'));
            session.handle_key(key('+'));
            let effects = session.handle_key(key(operator));
            assert!(effects.iter().any(
                |effect| matches!(effect, Effect::ClipboardWrite(actual) if actual == payload)
            ));
            assert_eq!(
                session.document(),
                if operator == 'y' { source } else { deleted },
                "{source:?}/{operator}"
            );
            assert_eq!(
                session.mode(),
                if operator == 'c' {
                    Mode::Insert
                } else {
                    Mode::Normal
                },
                "{source:?}/{operator}"
            );

            let document = session.document();
            for range in session
                .render_layout(80)
                .lines
                .iter()
                .flat_map(|line| &line.atoms)
                .filter_map(|atom| atom.source.as_ref())
            {
                assert!(document.is_char_boundary(range.start));
                assert!(document.is_char_boundary(range.end));
            }
        }
    }
}

#[test]
fn wide_block_registers_put_without_character_count_padding() {
    let source = "- 東x\n- 大y\n";
    let mut session = EditorSession::from_text(source);
    session.render_layout(40);
    session.handle_key(ctrl('v'));
    session.handle_key(key('j'));
    session.handle_key(key('y'));
    session.render_layout(40);
    session.handle_key(key('g'));
    session.handle_key(key('g'));
    session.handle_key(key('p'));
    assert_eq!(session.document(), "- 東東x\n- 大大y\n");
    session.handle_key(key('u'));
    assert_eq!(session.document(), source);

    session.render_layout(40);
    session.handle_key(key('g'));
    session.handle_key(key('g'));
    session.handle_key(ctrl('v'));
    session.handle_key(key('j'));
    session.handle_key(key('d'));
    assert_eq!(session.document(), "- x\n- y\n");
    session.render_layout(40);
    session.handle_key(key('P'));
    assert_eq!(session.document(), source);

    let mixed = "- 東\n- a\n\n- xx\n- yy\n";
    let mut ragged = EditorSession::from_text(mixed);
    ragged.render_layout(40);
    ragged.handle_key(ctrl('v'));
    ragged.handle_key(key('j'));
    let selection = ragged.rendered_selection().unwrap();
    assert_eq!(selection.block_width, Some(2));
    assert_eq!(selection.source_ranges, vec![2..5, 8..9]);
    ragged.handle_key(key('y'));
    ragged.render_layout(40);
    ragged.handle_key(key('/'));
    ragged.handle_key(key('x'));
    ragged.handle_key(key('x'));
    ragged.handle_key(special(KeyCodeKind::Enter));
    ragged.handle_key(key('P'));
    assert_eq!(ragged.document(), "- 東\n- a\n\n- 東xx\n- a yy\n");

    let mixed_prefixes = "- z\n- q\n\n- 東X\n- abY\n";
    let mut aligned = EditorSession::from_text(mixed_prefixes);
    aligned.render_layout(40);
    aligned.handle_key(ctrl('v'));
    aligned.handle_key(key('j'));
    aligned.handle_key(key('y'));
    aligned.render_layout(40);
    aligned.handle_key(key('/'));
    aligned.handle_key(key('X'));
    aligned.handle_key(special(KeyCodeKind::Enter));
    aligned.handle_key(key('l'));
    aligned.handle_key(key('P'));
    assert_eq!(aligned.document(), "- z\n- q\n\n- 東zX\n- abqY\n");

    let combining_prefixes = "- z\n- q\n\n- a\u{301}X\n- b\u{301}Y\n";
    let mut atomic = EditorSession::from_text(combining_prefixes);
    atomic.render_layout(40);
    atomic.handle_key(ctrl('v'));
    atomic.handle_key(key('j'));
    atomic.handle_key(key('y'));
    atomic.render_layout(40);
    atomic.handle_key(key('/'));
    atomic.handle_key(key('X'));
    atomic.handle_key(special(KeyCodeKind::Enter));
    atomic.handle_key(key('l'));
    atomic.handle_key(key('P'));
    assert_eq!(
        atomic.document(),
        "- z\n- q\n\n- a\u{301}zX\n- b\u{301}qY\n"
    );
}

#[test]
fn block_change_typing_is_one_undo_redo_transaction() {
    let source = "- ax\n- by\n";
    let mut session = EditorSession::from_text(source);
    session.render_layout(40);
    session.handle_key(ctrl('v'));
    session.handle_key(key('j'));
    session.handle_key(key('c'));
    assert_eq!(session.mode(), Mode::Insert);
    session.handle_key(key('Z'));
    session.handle_key(special(KeyCodeKind::Esc));
    let changed = session.document();
    assert_eq!(changed, "- Zx\n- y\n");
    session.handle_key(key('u'));
    assert_eq!(session.document(), source);
    session.handle_key(ctrl('r'));
    assert_eq!(session.document(), changed);
}

#[test]
fn all_mode_roundtrips_preserve_source_anchor() {
    let mut session = EditorSession::from_text("# One\n\nTwo\n");
    move_to_text(&mut session, "Two", 40);
    let source = session.cursor();
    session.handle_key(key('v'));
    session.handle_key(special(KeyCodeKind::Esc));
    session.handle_key(key(':'));
    session.handle_key(special(KeyCodeKind::Esc));
    session.handle_key(key('i'));
    session.handle_key(special(KeyCodeKind::Esc));
    assert_eq!((session.mode(), session.cursor()), (Mode::Normal, source));
}

#[test]
fn select_resize_preserves_exact_endpoint_provenance() {
    let text = "alpha beta gamma delta epsilon\n";
    let mut session = EditorSession::from_text(text);
    session.render_layout(11);
    session.handle_key(key('v'));
    session.handle_key(key('w'));
    assert_eq!(
        session.rendered_selection().unwrap().source_ranges,
        vec![0..7]
    );

    for width in [40, 7, 11] {
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
        assert_eq!(point_source(selection.anchor), Some(0..1), "width {width}");
        assert_eq!(point_source(selection.active), Some(6..7), "width {width}");
        assert_eq!(selection.source_ranges, vec![0..7], "width {width}");

        let mut painted_sources = selection
            .rows
            .iter()
            .flat_map(|row| row.source_ranges.iter().cloned())
            .collect::<Vec<_>>();
        painted_sources.sort_by_key(|range| (range.start, range.end));
        let mut normalized: Vec<std::ops::Range<usize>> = Vec::new();
        for range in painted_sources {
            if let Some(previous) = normalized.last_mut() {
                if range.start <= previous.end {
                    previous.end = previous.end.max(range.end);
                    continue;
                }
            }
            normalized.push(range);
        }
        let expected_painted = if width == 7 {
            vec![0..5, 6..7]
        } else {
            std::iter::once(0..7).collect()
        };
        assert_eq!(normalized, expected_painted, "width {width}");
    }
}

#[test]
fn collapsing_selected_metadata_cancels_the_hidden_projection() {
    let text = "---\ntitle: Example\n---\n\nbody\n";
    let mut session = EditorSession::from_text(text);
    session.render_layout(40);
    session.handle_key(key('j'));
    session.handle_key(key('v'));
    session.handle_key(key('l'));
    assert!(!session
        .rendered_selection()
        .unwrap()
        .source_ranges
        .is_empty());

    session.handle_key(key('z'));
    session.render_layout(40);
    assert_eq!(session.mode(), Mode::Normal);
    assert!(session.rendered_selection().is_none());
    session.handle_key(key('d'));
    assert_eq!(session.document(), text);
}

#[test]
fn inherited_wrapped_rows_map_to_one_source_operation() {
    let text =
        "A paragraph with enough words to wrap across several narrow rendered rows.\n\nnext\n";
    let mut session = EditorSession::from_text(text);
    session.render_layout(12);
    session.handle_key(key('V'));
    session.handle_key(key('j'));
    session.handle_key(key('j'));
    assert_eq!(
        &text[session.rendered_selection().unwrap().source_ranges[0].clone()],
        "A paragraph with enough words to wrap across several narrow rendered rows.\n"
    );
    session.handle_key(key('d'));
    assert_eq!(session.document(), "\nnext\n");
}

#[test]
fn resized_block_uses_one_coherent_projection_for_payload_and_delete() {
    let text = "abcdef ghijkl\n\nuvwxyz 123456\n";
    let selected = || {
        let mut session = EditorSession::from_text(text);
        session.render_layout(40);
        session.handle_key(ctrl('v'));
        session.handle_key(key('l'));
        session.handle_key(key('l'));
        session.handle_key(key('j'));
        session.handle_key(key('j'));
        session.render_layout(8);
        session
    };

    let mut yank = selected();
    let selection = yank.rendered_selection().unwrap();
    assert_eq!(
        selection.anchor,
        oom_edit_core::RenderedPoint { row: 0, column: 0 }
    );
    assert_eq!(
        selection.active,
        oom_edit_core::RenderedPoint { row: 3, column: 2 }
    );
    assert_eq!(selection.source_ranges, vec![0..3, 7..10, 15..18]);
    assert_eq!(selection.block_width, Some(3));
    assert_eq!(
        selection
            .rows
            .iter()
            .map(|row| (row.row, row.columns.clone(), row.source_ranges.clone()))
            .collect::<Vec<_>>(),
        vec![
            (0, 0..3, std::iter::once(0..3).collect()),
            (1, 0..3, std::iter::once(7..10).collect()),
            (2, 0..3, vec![]),
            (3, 0..3, std::iter::once(15..18).collect()),
        ]
    );
    yank.handle_key(key('"'));
    yank.handle_key(key('+'));
    let effects = yank.handle_key(key('y'));
    assert!(effects.iter().any(
        |effect| matches!(effect, Effect::ClipboardWrite(actual) if actual == "abc\nghi\n\nuvw")
    ));
    assert_eq!(yank.document(), text);

    let mut delete = selected();
    delete.handle_key(key('"'));
    delete.handle_key(key('+'));
    let effects = delete.handle_key(key('d'));
    assert!(effects.iter().any(
        |effect| matches!(effect, Effect::ClipboardWrite(actual) if actual == "abc\nghi\n\nuvw")
    ));
    assert_eq!(delete.document(), "def jkl\n\nxyz 123456\n");
}

#[test]
fn select_empty_and_footer_rows_are_safe() {
    let mut empty = EditorSession::from_text("");
    empty.render_layout(20);
    empty.handle_key(key('v'));
    assert!(empty.rendered_selection().unwrap().source_ranges.is_empty());

    let text = "[link](https://example.test)\n";
    let mut linked = EditorSession::from_text(text);
    linked.render_layout(20);
    linked.handle_key(key('G'));
    let source_before_footer_move = linked.cursor();
    linked.handle_key(key('j'));
    assert_eq!(linked.cursor(), source_before_footer_move);
    linked.handle_key(key('v'));
    let selection = linked.rendered_selection().unwrap();
    assert!(selection.source_ranges.is_empty());
    assert_eq!(linked.cursor(), source_before_footer_move);
}

#[test]
fn exact_markdown_structure_ranges() {
    fn selected_text(text: &str, needle: &str) -> String {
        let mut session = EditorSession::from_text(text);
        let row = session
            .render_layout(32)
            .lines
            .iter()
            .position(|line| line.styled.text.contains(needle))
            .unwrap_or_else(|| panic!("rendered layout did not contain {needle:?}"));
        while session.rendered_cursor_line() < row {
            let previous = session.rendered_cursor_line();
            session.handle_key(key('j'));
            assert_ne!(
                session.rendered_cursor_line(),
                previous,
                "cannot navigate to {needle:?}"
            );
        }
        session.handle_key(key('0'));
        session.handle_key(key('V'));
        session
            .rendered_selection()
            .unwrap()
            .source_ranges
            .iter()
            .map(|range| &text[range.clone()])
            .collect()
    }

    let front_matter = "---\ntitle: café\ntags:\n  - rust\n---\n\n# Body\n";
    assert_eq!(selected_text(front_matter, "metadata"), "---\n");
    assert_eq!(selected_text("- alpha\n- beta\n", "alpha"), "- alpha\n");
    assert_eq!(
        selected_text(
            "| key | value |\n|---|---|\n| unique | cellvalue |\n",
            "cellvalue"
        ),
        "| unique | cellvalue |\n"
    );
    assert_eq!(
        selected_text("```rust\nfn unique() {}\n```\n", "fn unique"),
        "fn unique() {}\n"
    );
    assert_eq!(selected_text("first\n\n東京 café", "東京"), "東京 café");

    let linked = "[unique link](https://example.test)\n";
    let mut session = EditorSession::from_text(linked);
    session.render_layout(24);
    session.handle_key(key('G'));
    let canonical = session.cursor();
    session.handle_key(key('j'));
    session.handle_key(key('V'));
    assert_eq!(session.cursor(), canonical);
    assert_eq!(
        session.rendered_selection().unwrap().source_ranges,
        vec![0..linked.len()]
    );
}

fn ex(session: &mut EditorSession, command: &str) -> Vec<Effect> {
    session.handle_key(key(':'));
    for ch in command.chars() {
        session.handle_key(key(ch));
    }
    session.handle_key(special(KeyCodeKind::Enter))
}

#[test]
fn v_x1_write() {
    let mut session = EditorSession::from_text("text\n");
    let bare = ex(&mut session, "w");
    assert!(bare.iter().any(|effect| matches!(
        effect,
        Effect::SaveRequested {
            path: None,
            retarget: false,
            then_quit: false,
            ..
        }
    )));
    let copy = ex(&mut session, "w copy.md");
    assert!(copy.iter().any(|effect| matches!(
        effect,
        Effect::SaveRequested { path: Some(path), retarget: false, then_quit: false, .. }
            if path.ends_with("copy.md")
    )));
}

#[test]
fn v_x2_quit() {
    let mut session = EditorSession::from_text("text\n");
    assert!(ex(&mut session, "q")
        .iter()
        .any(|effect| matches!(effect, Effect::QuitRequested { force: false })));
    assert!(ex(&mut session, "q!")
        .iter()
        .any(|effect| matches!(effect, Effect::QuitRequested { force: true })));
}

#[test]
fn v_x3_write_quit() {
    for command in ["wq", "x"] {
        let mut session = EditorSession::from_text("text\n");
        assert!(ex(&mut session, command).iter().any(|effect| matches!(
            effect,
            Effect::SaveRequested {
                then_quit: true,
                ..
            }
        )));
    }
}

#[test]
fn v_x4_edit() {
    let mut session = EditorSession::from_text("text\n");
    assert!(ex(&mut session, "e next.md").iter().any(|effect| matches!(
        effect,
        Effect::OpenRequested { path, force: false } if path.ends_with("next.md")
    )));
    assert!(ex(&mut session, "e! next.md").iter().any(|effect| matches!(
        effect,
        Effect::OpenRequested { path, force: true } if path.ends_with("next.md")
    )));
}

#[test]
fn v_x5_saveas() {
    let mut session = EditorSession::from_text("text\n");
    assert!(ex(&mut session, "saveas next.md")
        .iter()
        .any(|effect| matches!(
            effect,
            Effect::SaveRequested { path: Some(path), retarget: true, .. }
                if path.ends_with("next.md")
        )));
}

#[test]
fn v_x6_line_jump() {
    let mut session = EditorSession::from_text("one\ntwo\nthree\n");
    session.render_layout(40);
    ex(&mut session, "3");
    assert_eq!(session.cursor(), (2, 0));
}

#[test]
fn v_x7_substitute() {
    let mut session = EditorSession::from_text("one one\ntwo one\n");
    session.render_layout(40);
    ex(&mut session, "%s/one/ONE/g");
    assert_eq!(session.document(), "ONE ONE\ntwo ONE\n");
    session.handle_key(key('u'));
    assert_eq!(session.document(), "one one\ntwo one\n");
}

#[test]
fn v_x8_noh_and_help() {
    let mut session = EditorSession::from_text("one\ntwo one\n");
    session.render_layout(40);
    session.handle_key(key('/'));
    for ch in "one".chars() {
        session.handle_key(key(ch));
    }
    session.handle_key(special(KeyCodeKind::Enter));
    assert!(session.rendered_search().is_some());
    ex(&mut session, "noh");
    assert!(session.rendered_search().is_none());
    assert!(ex(&mut session, "help")
        .iter()
        .any(|effect| matches!(effect, Effect::HelpRequested)));
}

#[test]
fn spell_toggles_and_session_isolation() {
    let mut first = EditorSession::from_text("helo");
    let second = EditorSession::from_text("helo");
    let effects = enter_ex(&mut first, "set nospell");
    assert!(!first.spell_enabled());
    assert!(second.spell_enabled());
    assert!(effects
        .iter()
        .any(|effect| matches!(effect, Effect::Message { .. })));
    enter_ex(&mut first, "set spell");
    assert!(first.spell_enabled());
}

#[test]
fn spell_navigation_empty_wrap_and_mid_diagnostic() {
    let engine = spell_engine("good\n");
    let mut empty = EditorSession::from_text("good\n\ngood\n\ngood");
    drain_spell(&mut empty, &engine);
    empty.render_layout(20);
    empty.handle_key(key('2'));
    empty.handle_key(key(']'));
    assert!(empty.handle_key(key('s')).is_empty());
    assert_eq!(empty.cursor(), (0, 0));
    empty.handle_key(key('G'));
    assert_eq!(empty.cursor().0, 4, "empty ]s must consume its count");

    let mut session = EditorSession::from_text("bad good wrng nope");
    drain_spell(&mut session, &engine);
    session.render_layout(30);
    let diagnostics = session.diagnostics().to_vec();
    session
        .jump_to_offset(diagnostics[0].range.start + 1)
        .unwrap();
    session.handle_key(key(']'));
    session.handle_key(key('s'));
    assert_eq!(
        session.cursor(),
        session
            .position_for_offset(diagnostics[1].range.start)
            .map(|position| (position.line, position.column))
            .unwrap()
    );
    session.handle_key(key('['));
    session.handle_key(key('s'));
    assert_eq!(
        session.cursor(),
        session
            .position_for_offset(diagnostics[0].range.start)
            .map(|position| (position.line, position.column))
            .unwrap()
    );
    session.jump_to_offset(diagnostics[2].range.start).unwrap();
    session.handle_key(key(']'));
    session.handle_key(key('s'));
    assert_eq!(session.diagnostic_at_cursor(), Some(&diagnostics[0]));
    session.handle_key(key('2'));
    session.handle_key(key(']'));
    session.handle_key(key('s'));
    assert_eq!(session.diagnostic_at_cursor(), Some(&diagnostics[2]));
    session.handle_key(key(']'));
    session.handle_key(key('s'));
    assert_eq!(session.diagnostic_at_cursor(), Some(&diagnostics[0]));
}

#[test]
fn spell_diagnostic_cursor_is_half_open() {
    let engine = spell_engine("good\n");
    let mut session = EditorSession::from_text("wrng good");
    drain_spell(&mut session, &engine);
    let diagnostic = session.diagnostics()[0].clone();
    for offset in diagnostic.range.clone() {
        session.jump_to_offset(offset).unwrap();
        assert_eq!(session.diagnostic_at_cursor(), Some(&diagnostic));
    }
    session.jump_to_offset(diagnostic.range.end).unwrap();
    assert_eq!(session.diagnostic_at_cursor(), None);
}

#[test]
fn spell_replacement_revalidates_identity_and_text() {
    let engine = spell_engine("hello\n");
    let mut session = EditorSession::from_text("helo");
    drain_spell(&mut session, &engine);
    let stale = session.diagnostics()[0].clone();
    session.handle_key(key('i'));
    session.handle_key(special(KeyCodeKind::Delete));
    session.handle_key(key('x'));
    session.handle_key(special(KeyCodeKind::Esc));
    let before = session.document();
    let effects = session.apply_spell_replacement(&stale, "hello");
    assert_eq!(session.document(), before);
    assert!(effects.iter().any(|effect| matches!(
        effect,
        Effect::Message {
            severity: oom_edit_core::Severity::Warning,
            ..
        }
    )));
}

#[test]
fn spell_disabled_queries_and_edit_resume() {
    let engine = spell_engine("hello\n");
    let mut session = EditorSession::from_text("helo");
    drain_spell(&mut session, &engine);
    session.set_spell_enabled(false);
    assert!(session.diagnostics().is_empty());
    assert!(!session.diagnostics_pending());
    assert!(!session.spell_tick(&engine, usize::MAX));
    session.handle_key(key('i'));
    session.handle_key(key('x'));
    session.handle_key(special(KeyCodeKind::Esc));
    session.set_spell_enabled(true);
    drain_spell(&mut session, &engine);
    assert_eq!(session.diagnostics()[0].source_text, "xhelo");
}

#[test]
fn spell_generation_self_heals() {
    let mut engine = spell_engine("hello\n");
    let mut first = EditorSession::from_text("helo");
    let mut second = EditorSession::from_text("helo");
    drain_spell(&mut first, &engine);
    drain_spell(&mut second, &engine);
    engine.add_word("helo").unwrap();
    assert!(first.spell_tick(&engine, 1));
    assert!(second.spell_tick(&engine, 1));
    drain_spell(&mut first, &engine);
    drain_spell(&mut second, &engine);
    assert!(first.diagnostics().is_empty());
    assert!(second.diagnostics().is_empty());
}

#[test]
fn spell_positions_and_atomic_jump() {
    let mut session = EditorSession::from_text("aé\nxy");
    assert_eq!(session.text_for_range(6..6).as_deref(), Some(""));
    assert_eq!(session.text_for_range(1..2), None);
    assert_eq!(
        session.position_for_offset(6),
        Some(oom_edit_core::TextPosition { line: 1, column: 2 })
    );
    assert_eq!(session.position_for_offset(2), None);
    session.render_layout(20);
    let effects = session.jump_to_offset(4).unwrap();
    assert_eq!(session.cursor(), (1, 0));
    assert!(effects.contains(&Effect::CursorMoved));
    let rendered = session.rendered_cursor();
    assert!(session.rendered_layout().unwrap().lines[rendered.row]
        .atoms
        .iter()
        .any(|atom| atom.columns.contains(&rendered.column)
            && atom
                .source
                .as_ref()
                .is_some_and(|source| source.contains(&4))));
    let before_invalid = (session.cursor(), session.rendered_cursor());
    assert_eq!(
        session.jump_to_offset(2),
        Err(oom_edit_core::PositionError::NotCharBoundary)
    );
    assert_eq!(
        (session.cursor(), session.rendered_cursor()),
        before_invalid
    );
    assert_eq!(
        session.jump_to_offset(7),
        Err(oom_edit_core::PositionError::OutOfBounds)
    );
    assert_eq!(
        (session.cursor(), session.rendered_cursor()),
        before_invalid
    );
    session.jump_to_offset(6).unwrap();
    assert_eq!(session.cursor(), (1, 2));
}

proptest! {
    #[test]
    fn random_mode_width_and_motion_mapping_stays_in_bounds(
        payload in prop::collection::vec(any::<char>(), 0..30),
        widths in prop::collection::vec(1u16..100, 1..20),
        actions in prop::collection::vec(0u8..9, 1..80),
    ) {
        let payload: String = payload.into_iter().collect();
        let text = format!("# Héading\n\n{payload}\n\nUnicode café 東京 wrapping words.\n\n- item\n");
        let mut session = EditorSession::from_text(&text);
        for (index, action) in actions.iter().copied().enumerate() {
            let width = widths[index % widths.len()];
            session.render_layout(width);
            let anchor_before_resize = session.cursor();
            let selection_before = session.rendered_selection().map(|selection| selection.source_ranges);
            session.render_layout(width.saturating_add(1));
            prop_assert_eq!(session.cursor(), anchor_before_resize);
            prop_assert_eq!(session.rendered_selection().map(|selection| selection.source_ranges), selection_before);

            match action {
                0 => { session.handle_key(key('j')); }
                1 => { session.handle_key(key('k')); }
                2 if session.mode() == Mode::Normal => { session.handle_key(key('v')); }
                2 if session.mode() == Mode::Select => { session.handle_key(special(KeyCodeKind::Esc)); }
                3 if session.mode() == Mode::Normal => {
                    session.handle_key(key(':'));
                    session.handle_key(special(KeyCodeKind::Esc));
                }
                4 if session.mode() == Mode::Normal => {
                    session.handle_key(key('i'));
                    session.handle_key(key('Ω'));
                    session.handle_key(special(KeyCodeKind::Esc));
                    session.render_layout(width);
                }
                5 if session.mode() == Mode::Normal => {
                    session.handle_key(key('/'));
                    for ch in "東京".chars() { session.handle_key(key(ch)); }
                    session.handle_key(special(KeyCodeKind::Enter));
                }
                6 if session.mode() == Mode::Normal => { session.handle_key(key('u')); }
                7 if session.mode() == Mode::Normal => { session.handle_key(ctrl('r')); }
                _ => {}
            }
            let (line, col) = session.cursor();
            prop_assert!(line < session.line_count());
            prop_assert!(col <= session.line(line).unwrap_or_default().chars().count());
            if session.mode() != Mode::Insert {
                session.render_layout(width);
                prop_assert!(session.rendered_cursor_line() < session.rendered_layout().unwrap().lines.len().max(1));
            }
        }
    }

    #[test]
    fn every_selection_shape_is_utf8_safe_ordered_and_resize_deterministic(
        width in 4u16..60,
        horizontal in 0usize..12,
        vertical in 0usize..8,
        shape_index in 0u8..3,
    ) {
        let text = "---\ntitle: café 東京\n---\n\nalpha beta gamma delta\n\n- item one\n- item two\n";
        let mut session = EditorSession::from_text(text);
        session.render_layout(width);
        match shape_index {
            0 => { session.handle_key(key('v')); }
            1 => { session.handle_key(key('V')); }
            _ => { session.handle_key(ctrl('v')); }
        }
        for _ in 0..horizontal { session.handle_key(key('l')); }
        for _ in 0..vertical { session.handle_key(key('j')); }

        let assert_projection = |selection: &oom_edit_core::RenderedSelection| {
            prop_assert_eq!(selection.shape, match shape_index {
                0 => SelectionShape::Character,
                1 => SelectionShape::Line,
                _ => SelectionShape::Block,
            });
            for pair in selection.source_ranges.windows(2) {
                prop_assert!(pair[0].end < pair[1].start);
            }
            for range in &selection.source_ranges {
                prop_assert!(range.start < range.end);
                prop_assert!(range.end <= text.len());
                prop_assert!(text.is_char_boundary(range.start));
                prop_assert!(text.is_char_boundary(range.end));
                if shape_index == 1 {
                    prop_assert!(range.start == 0 || text.as_bytes().get(range.start - 1) == Some(&b'\n'));
                    prop_assert!(range.end == text.len() || text.as_bytes().get(range.end - 1) == Some(&b'\n'));
                }
            }
            for pair in selection.rows.windows(2) {
                prop_assert!(pair[0].row < pair[1].row);
            }
            Ok(())
        };

        let original = session.rendered_selection().unwrap();
        assert_projection(&original)?;
        session.render_layout(width.saturating_add(7));
        let resized = session.rendered_selection().unwrap();
        assert_projection(&resized)?;
        session.render_layout(width);
        let restored = session.rendered_selection().unwrap();
        assert_projection(&restored)?;
        prop_assert_eq!(restored.shape, original.shape);
        prop_assert_eq!(restored.source_ranges, original.source_ranges);
    }
}
