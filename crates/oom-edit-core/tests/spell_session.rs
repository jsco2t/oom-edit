use oom_edit_core::{
    DecorationKind, DiagnosticProvider, DiagnosticSeverity, EditorSession, Effect, KeyCode,
    KeyCodeKind, KeyInput, Modifiers, PositionError, SemanticStyle, TextPosition, Viewport,
};
use oom_spell::{BuildProgress, SpellEngine, SpellEngineBuilder};
use proptest::prelude::*;

fn key(character: char) -> KeyInput {
    KeyInput {
        code: KeyCode {
            kind: KeyCodeKind::Char(character),
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

fn ctrl(character: char) -> KeyInput {
    KeyInput {
        code: KeyCode {
            kind: KeyCodeKind::Char(character),
        },
        mods: Modifiers {
            ctrl: true,
            ..Modifiers::default()
        },
    }
}

fn engine(words: &str) -> SpellEngine {
    let mut builder = SpellEngineBuilder::new(vec![words.to_string()]);
    while builder.step(17) != BuildProgress::Complete {}
    builder.finish().expect("test dictionary must finish")
}

fn drain(session: &mut EditorSession, engine: &SpellEngine, budget: usize) {
    let mut ticks = 0;
    while session.diagnostics_pending() {
        assert!(session.spell_tick(engine, budget));
        ticks += 1;
        assert!(ticks < 10_000, "spell scan failed to make progress");
    }
}

#[test]
fn initial_scan_is_budgeted_and_builds_exact_warning_diagnostics() {
    let engine = engine("the\nquick\nbrown\nfox\nvisible\n");
    let text = concat!(
        "---\ntitle: fmwrng\n---\n\n",
        "teh quick `hidn` [visible](https://example.invalid/mispeld) brown fox\n\n",
        "```text\nfencewrng across several tiny ticks\n```\n\n",
        "<div>\nhtmlwrng\n</div>\n",
    );
    let mut session = EditorSession::from_text(text);

    assert!(session.spell_enabled());
    assert!(session.diagnostics_pending());
    assert!(!session.spell_tick(&engine, 0));
    assert!(session.diagnostics_pending());
    assert!(session.diagnostics().is_empty());
    assert!(session.spell_tick(&engine, 1));
    assert!(
        session.diagnostics_pending(),
        "one byte cannot drain the scan"
    );
    drain(&mut session, &engine, 3);

    let diagnostics = session.diagnostics();
    assert_eq!(diagnostics.len(), 1, "{diagnostics:#?}");
    assert_eq!(diagnostics[0].provider, DiagnosticProvider::Spell);
    assert_eq!(diagnostics[0].severity, DiagnosticSeverity::Warning);
    assert_eq!(&text[diagnostics[0].range.clone()], "teh");
    assert_eq!(diagnostics[0].source_text, "teh");
    assert!(diagnostics[0].message.contains("teh"));
}

#[test]
fn edits_inside_multiline_exclusions_and_reference_definitions_rescan_safely() {
    let engine = engine("visible\noutside\n");
    let mut code = EditorSession::from_text("`hidden\nwrng`\noutside");
    drain(&mut code, &engine, 2);
    assert!(code.diagnostics().is_empty());
    let code_word = code.document().find("wrng").unwrap();
    code.jump_to_offset(code_word).unwrap();
    code.handle_key(key('i'));
    code.handle_key(special(KeyCodeKind::Delete));
    code.handle_key(key('x'));
    code.handle_key(special(KeyCodeKind::Esc));
    drain(&mut code, &engine, 2);
    assert!(code.diagnostics().is_empty());

    let mut constructed_while_pending = EditorSession::from_text("hidden\nwrng\noutside");
    constructed_while_pending.jump_to_offset(0).unwrap();
    constructed_while_pending.handle_key(key('i'));
    constructed_while_pending.handle_key(key('`'));
    constructed_while_pending.handle_key(special(KeyCodeKind::Esc));
    let closing = constructed_while_pending
        .document()
        .find("\noutside")
        .unwrap();
    constructed_while_pending.jump_to_offset(closing).unwrap();
    constructed_while_pending.handle_key(key('i'));
    constructed_while_pending.handle_key(key('`'));
    constructed_while_pending.handle_key(special(KeyCodeKind::Esc));
    drain(&mut constructed_while_pending, &engine, 2);
    assert!(constructed_while_pending.diagnostics().is_empty());
    let pending_code_word = constructed_while_pending.document().find("wrng").unwrap();
    constructed_while_pending
        .jump_to_offset(pending_code_word)
        .unwrap();
    constructed_while_pending.handle_key(key('i'));
    constructed_while_pending.handle_key(special(KeyCodeKind::Delete));
    constructed_while_pending.handle_key(key('x'));
    constructed_while_pending.handle_key(special(KeyCodeKind::Esc));
    drain(&mut constructed_while_pending, &engine, 2);
    assert!(constructed_while_pending.diagnostics().is_empty());

    let mut reference = EditorSession::from_text(
        "[visible][wrng]\n\n[wrng]: https://example.invalid/destination\n",
    );
    drain(&mut reference, &engine, 3);
    assert!(
        reference.diagnostics().is_empty(),
        "{:#?}",
        reference.diagnostics()
    );
    let definition = reference.document().rfind("wrng").unwrap();
    reference.jump_to_offset(definition).unwrap();
    reference.handle_key(key('i'));
    reference.handle_key(special(KeyCodeKind::Delete));
    reference.handle_key(key('b'));
    reference.handle_key(special(KeyCodeKind::Esc));
    drain(&mut reference, &engine, 3);
    assert_eq!(
        reference
            .diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.source_text.as_str())
            .collect::<Vec<_>>(),
        ["wrng"]
    );
}

#[test]
fn structural_indentation_and_front_matter_edits_rescan_every_affected_block() {
    let engine = engine("outside\n");
    let mut indented = EditorSession::from_text("wrnga\n    wrngb\n");
    drain(&mut indented, &engine, 3);
    assert_eq!(
        indented
            .diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.source_text.as_str())
            .collect::<Vec<_>>(),
        ["wrnga", "wrngb"]
    );
    indented.jump_to_offset(0).unwrap();
    indented.handle_key(key('i'));
    indented.insert_paste("    ");
    indented.handle_key(special(KeyCodeKind::Esc));
    assert!(indented.diagnostics().is_empty());
    drain(&mut indented, &engine, 3);
    assert!(indented.diagnostics().is_empty());
    indented.handle_key(key('u'));
    drain(&mut indented, &engine, 3);
    assert_eq!(indented.diagnostics().len(), 2);

    let mut front_matter = EditorSession::from_text("--\nwrnga: wrngb\n---\noutside");
    drain(&mut front_matter, &engine, 3);
    assert_eq!(front_matter.diagnostics().len(), 2);
    front_matter.jump_to_offset(0).unwrap();
    front_matter.handle_key(key('i'));
    front_matter.handle_key(key('-'));
    front_matter.handle_key(special(KeyCodeKind::Esc));
    assert!(front_matter.diagnostics().is_empty());
    drain(&mut front_matter, &engine, 3);
    assert!(front_matter.diagnostics().is_empty());
    front_matter.handle_key(key('u'));
    drain(&mut front_matter, &engine, 3);
    assert_eq!(front_matter.diagnostics().len(), 2);
}

#[test]
fn ordinary_edit_shifts_unaffected_diagnostics_when_markdown_markers_exist_elsewhere() {
    let engine = engine("hello\nworld\n");
    let mut session = EditorSession::from_text("helo\n[world](https://example.invalid/path)\nwrld");
    drain(&mut session, &engine, 7);
    let first = session.diagnostics()[0].clone();
    let unaffected = session.diagnostics()[1].clone();

    session.apply_spell_replacement(&first, "hello");

    assert!(session.diagnostics_pending());
    assert_eq!(session.diagnostics().len(), 1);
    assert_eq!(session.diagnostics()[0].source_text, "wrld");
    assert_eq!(
        session.diagnostics()[0].range,
        unaffected.range.start + 1..unaffected.range.end + 1
    );
}

#[test]
fn local_edit_shifts_retained_multiline_exclusion_before_interior_edit() {
    let engine = engine("good\noutside\n");
    let mut session = EditorSession::from_text("good\n`hidden\nwrng`\noutside");
    drain(&mut session, &engine, 3);
    assert!(session.diagnostics().is_empty());

    session.jump_to_offset(0).unwrap();
    session.handle_key(key('i'));
    session.insert_paste("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
    session.handle_key(special(KeyCodeKind::Esc));
    drain(&mut session, &engine, 3);

    let code_word = session.document().find("wrng").unwrap();
    session.jump_to_offset(code_word).unwrap();
    session.handle_key(key('i'));
    session.handle_key(special(KeyCodeKind::Delete));
    session.handle_key(key('x'));
    session.handle_key(special(KeyCodeKind::Esc));
    drain(&mut session, &engine, 3);

    assert!(session
        .diagnostics()
        .iter()
        .all(|diagnostic| diagnostic.source_text != "xrng"));
    let mut fresh = EditorSession::from_text(&session.document());
    drain(&mut fresh, &engine, 3);
    assert_eq!(session.diagnostics(), fresh.diagnostics());
}

#[test]
fn diagnostics_remain_sorted_during_partial_local_rescan() {
    let engine = engine("good\n");
    let first_line = std::iter::repeat_n("good", 40)
        .collect::<Vec<_>>()
        .join(" ");
    let mut session = EditorSession::from_text(&format!("{first_line}\nwrng"));
    drain(&mut session, &engine, 7);
    assert_eq!(session.diagnostics()[0].source_text, "wrng");

    session.jump_to_offset(0).unwrap();
    session.handle_key(key('i'));
    session.handle_key(special(KeyCodeKind::Delete));
    session.handle_key(key('b'));
    session.handle_key(special(KeyCodeKind::Esc));
    let mut ticks = 0;
    while session.diagnostics_pending() {
        assert!(session.spell_tick(&engine, 5));
        ticks += 1;
        assert!(ticks < 10_000, "partial rescan failed to make progress");
        assert!(session
            .diagnostics()
            .windows(2)
            .all(|pair| pair[0].range.start <= pair[1].range.start));
    }
    assert_eq!(
        session
            .diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.source_text.as_str())
            .collect::<Vec<_>>(),
        ["bood", "wrng"]
    );
}

#[test]
fn replacement_is_tight_revalidated_and_one_undo_step() {
    let engine = engine("hello\nworld\n");
    let mut session = EditorSession::from_text("é helo\nhelo\nwrld");
    drain(&mut session, &engine, 2);
    let first = session.diagnostics()[0].clone();
    let third = session.diagnostics()[2].clone();

    let effects = session.apply_spell_replacement(&first, "hello");
    assert!(effects
        .iter()
        .any(|effect| matches!(effect, Effect::Edited)));
    assert_eq!(session.document(), "é hello\nhelo\nwrld");
    assert!(session.diagnostics_pending());
    assert_eq!(session.diagnostics().len(), 2);
    assert_eq!(
        session.diagnostics()[1].range,
        third.range.start + 1..third.range.end + 1
    );

    session.handle_key(key('u'));
    assert_eq!(session.document(), "é helo\nhelo\nwrld");
}

#[test]
fn stale_or_same_range_different_text_replacement_never_splices() {
    let engine = engine("hello\n");
    let mut session = EditorSession::from_text("helo");
    drain(&mut session, &engine, 8);
    let stale = session.diagnostics()[0].clone();

    session.handle_key(key('i'));
    session.handle_key(special(KeyCodeKind::Delete));
    session.handle_key(key('x'));
    session.handle_key(special(KeyCodeKind::Esc));
    assert_eq!(session.document(), "xelo");

    let effects = session.apply_spell_replacement(&stale, "hello");
    assert_eq!(session.document(), "xelo");
    assert!(effects.iter().any(|effect| matches!(
        effect,
        Effect::Message { severity, .. } if *severity == oom_edit_core::Severity::Warning
    )));
}

#[test]
fn disabled_state_hides_queries_but_resumes_after_edits() {
    let engine = engine("hello\n");
    let mut session = EditorSession::from_text("helo");
    drain(&mut session, &engine, 8);
    assert_eq!(session.diagnostics().len(), 1);

    session.set_spell_enabled(false);
    assert!(!session.spell_enabled());
    assert!(session.diagnostics().is_empty());
    assert!(!session.diagnostics_pending());
    assert!(!session.spell_tick(&engine, usize::MAX));

    session.handle_key(key('i'));
    session.handle_key(key('x'));
    session.handle_key(special(KeyCodeKind::Esc));
    assert!(session.diagnostics().is_empty());

    session.set_spell_enabled(true);
    assert!(session.diagnostics_pending());
    drain(&mut session, &engine, 1);
    assert_eq!(session.diagnostics().len(), 1);
    assert_eq!(session.diagnostics()[0].source_text, "xhelo");
}

#[test]
fn engine_generation_self_heals_across_sessions() {
    let mut engine = engine("hello\n");
    let mut first = EditorSession::from_text("helo");
    let mut second = EditorSession::from_text("helo");
    drain(&mut first, &engine, 8);
    drain(&mut second, &engine, 8);
    assert_eq!(second.diagnostics().len(), 1);

    engine.add_word("helo").unwrap();
    assert!(first.spell_tick(&engine, 1));
    assert!(second.spell_tick(&engine, 1));
    drain(&mut first, &engine, 1);
    drain(&mut second, &engine, 1);
    assert!(first.diagnostics().is_empty());
    assert!(second.diagnostics().is_empty());
}

#[test]
fn exact_range_and_unicode_scalar_position_contracts() {
    let mut session = EditorSession::from_text("aé\nxy");
    assert_eq!(session.text_for_range(0..3).as_deref(), Some("aé"));
    assert_eq!(session.text_for_range(6..6).as_deref(), Some(""));
    let reversed_start = 3;
    let reversed_end = 2;
    assert_eq!(session.text_for_range(reversed_start..reversed_end), None);
    assert_eq!(session.text_for_range(0..7), None);
    assert_eq!(session.text_for_range(1..2), None);

    assert_eq!(
        session.position_for_offset(3),
        Some(TextPosition { line: 0, column: 2 })
    );
    assert_eq!(
        session.position_for_offset(6),
        Some(TextPosition { line: 1, column: 2 })
    );
    assert_eq!(session.position_for_offset(2), None);
    assert_eq!(session.position_for_offset(7), None);

    session.render_layout(20);
    let effects = session.jump_to_offset(4).unwrap();
    assert_eq!(session.cursor(), (1, 0));
    assert!(effects
        .iter()
        .any(|effect| matches!(effect, Effect::CursorMoved)));
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
        Err(PositionError::NotCharBoundary)
    );
    assert_eq!(
        (session.cursor(), session.rendered_cursor()),
        before_invalid
    );
    assert_eq!(session.jump_to_offset(7), Err(PositionError::OutOfBounds));
    assert_eq!(
        (session.cursor(), session.rendered_cursor()),
        before_invalid
    );
    session.jump_to_offset(6).unwrap();
    assert_eq!(session.cursor(), (1, 2));
}

#[test]
fn jump_to_multibyte_offset_keeps_canonical_and_wrapped_rendered_cursors_atomic() {
    let text = format!("é {}wrng tail", "known ".repeat(12));
    let target = text.find("wrng").unwrap();
    let expected_column = text[..target].chars().count();
    assert_ne!(
        target, expected_column,
        "fixture must distinguish bytes from scalars"
    );
    let mut session = EditorSession::from_text(&text);
    session.render_layout(10);

    let effects = session.jump_to_offset(target).unwrap();
    let expected = session.position_for_offset(target).unwrap();
    assert_eq!(
        expected,
        TextPosition {
            line: 0,
            column: expected_column
        }
    );
    assert_eq!(session.cursor(), (expected.line, expected.column));
    assert_eq!(effects, vec![Effect::CursorMoved]);

    let rendered = session.rendered_cursor();
    assert!(
        rendered.row > 0,
        "the target must wrap below its multibyte source line's first visual row"
    );
    let layout = session.rendered_layout().unwrap();
    assert!(layout.lines[rendered.row].atoms.iter().any(|atom| {
        atom.columns.contains(&rendered.column)
            && atom
                .source
                .as_ref()
                .is_some_and(|source| source.contains(&target))
    }));
}

#[test]
fn diagnostic_cursor_lookup_and_wrapping_navigation_are_half_open() {
    let engine = engine("good\n");
    let mut session = EditorSession::from_text("bad good wrng nope");
    drain(&mut session, &engine, 5);
    let diagnostics = session.diagnostics().to_vec();
    assert_eq!(diagnostics.len(), 3);

    session.jump_to_offset(diagnostics[0].range.start).unwrap();
    assert_eq!(session.diagnostic_at_cursor(), Some(&diagnostics[0]));
    session.jump_to_offset(diagnostics[0].range.end).unwrap();
    assert_ne!(session.diagnostic_at_cursor(), Some(&diagnostics[0]));

    session
        .jump_to_offset(diagnostics[0].range.start + 1)
        .unwrap();
    session.handle_key(key(']'));
    session.handle_key(key('s'));
    assert_eq!(
        session.cursor(),
        session
            .position_for_offset(diagnostics[1].range.start)
            .map(|p| (p.line, p.column))
            .unwrap()
    );
    session.handle_key(key('['));
    session.handle_key(key('s'));
    assert_eq!(
        session.cursor(),
        session
            .position_for_offset(diagnostics[0].range.start)
            .map(|p| (p.line, p.column))
            .unwrap()
    );

    session.jump_to_offset(diagnostics[2].range.start).unwrap();
    session.handle_key(key(']'));
    session.handle_key(key('s'));
    assert_eq!(
        session.cursor(),
        session
            .position_for_offset(diagnostics[0].range.start)
            .map(|p| (p.line, p.column))
            .unwrap()
    );
}

#[test]
fn set_spell_commands_are_session_local() {
    let mut first = EditorSession::from_text("helo");
    let second = EditorSession::from_text("helo");

    for character in ":set nospell".chars() {
        first.handle_key(key(character));
    }
    let effects = first.handle_key(special(KeyCodeKind::Enter));
    assert!(!first.spell_enabled());
    assert!(second.spell_enabled());
    assert!(effects
        .iter()
        .any(|effect| matches!(effect, Effect::Message { .. })));

    for character in ":set spell".chars() {
        first.handle_key(key(character));
    }
    first.handle_key(special(KeyCodeKind::Enter));
    assert!(first.spell_enabled());
}

#[test]
fn suggestions_require_a_current_diagnostic() {
    let engine = engine("hello\nhelp\nworld\n");
    let mut session = EditorSession::from_text("helo");
    drain(&mut session, &engine, 2);
    let diagnostic = session.diagnostics()[0].clone();
    let suggestions = session.spell_suggestions(&engine, &diagnostic, 9);
    assert_eq!(suggestions.first().map(String::as_str), Some("hello"));

    session.apply_spell_replacement(&diagnostic, "hello");
    assert!(session
        .spell_suggestions(&engine, &diagnostic, 9)
        .is_empty());
}

#[test]
fn rendered_decoration_projection_handles_wrapping_wide_cells_tables_and_synthetic_glyphs() {
    let engine = engine("known\n");
    let kind = DecorationKind::Diagnostic {
        provider: DiagnosticProvider::Spell,
        severity: DiagnosticSeverity::Warning,
    };

    let mut wide = EditorSession::from_text("東京 wrng known\n");
    drain(&mut wide, &engine, 5);
    wide.render_layout(40);
    assert_eq!(
        wide.diagnostic_decoration_rows(0..usize::MAX),
        [oom_edit_core::DiagnosticDecorationRow {
            row: 0,
            columns: 5..9,
            kind,
        }]
    );

    let mut wrapped = EditorSession::from_text("misspelledd known\n");
    drain(&mut wrapped, &engine, 5);
    let wrapped_layout = wrapped.render_layout(5).clone();
    let wrapped_rows = wrapped.diagnostic_decoration_rows(0..usize::MAX);
    assert_eq!(
        wrapped_rows
            .iter()
            .map(|row| (row.row, row.columns.clone()))
            .collect::<Vec<_>>(),
        [(0, 0..5), (1, 0..5), (2, 0..1)]
    );
    assert_eq!(
        wrapped.diagnostic_decoration_rows(1..2),
        [oom_edit_core::DiagnosticDecorationRow {
            row: 1,
            columns: 0..5,
            kind,
        }]
    );
    assert!(wrapped_layout.lines[2].styled.text.starts_with('d'));

    for (text, expected_text, expected_row, expected_columns) in [
        ("- wrng known\n", "• wrng known", 0, 2..6),
        (
            "| wrng | known |\n| --- | --- |\n",
            "│ wrng │ known │",
            1,
            2..6,
        ),
    ] {
        let mut session = EditorSession::from_text(text);
        drain(&mut session, &engine, 5);
        let layout = session.render_layout(40).clone();
        assert_eq!(layout.lines[expected_row].styled.text, expected_text);
        let rows = session.diagnostic_decoration_rows(0..usize::MAX);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].row, expected_row);
        assert_eq!(rows[0].columns, expected_columns);
        assert_eq!(rows[0].kind, kind);
        for column in rows[0].columns.clone() {
            assert!(layout.lines[expected_row].atoms.iter().any(|atom| {
                atom.source.is_some() && atom.columns.start <= column && column < atom.columns.end
            }));
        }
    }

    assert!(wrapped
        .diagnostic_decoration_rows(usize::MAX..usize::MAX)
        .is_empty());
    let descending = std::ops::Range { start: 5, end: 2 };
    assert!(wrapped.diagnostic_decoration_rows(descending).is_empty());

    wrapped.set_spell_enabled(false);
    assert!(wrapped.diagnostic_decoration_rows(0..usize::MAX).is_empty());
}

#[test]
fn rendered_decoration_query_preserves_every_required_semantic_span_role() {
    let engine = engine("known\n");
    let mut session = EditorSession::from_text(
        "# wrng\n\n*wrng*\n\n[wrng](https://example.com)\n\n```\nknown\n```\n",
    );
    drain(&mut session, &engine, 5);
    let layout = session.render_layout(80).clone();
    let before = layout
        .lines
        .iter()
        .map(|line| line.styled.spans.clone())
        .collect::<Vec<_>>();
    for required in [
        SemanticStyle::Heading1,
        SemanticStyle::Emphasis,
        SemanticStyle::Link,
        SemanticStyle::CodeBlock,
    ] {
        assert!(
            before.iter().flatten().any(|span| span.style == required),
            "fixture must exercise {required:?}"
        );
    }

    assert_eq!(session.diagnostic_decoration_rows(0..usize::MAX).len(), 3);
    let after = session
        .rendered_layout()
        .unwrap()
        .lines
        .iter()
        .map(|line| line.styled.spans.clone())
        .collect::<Vec<_>>();
    assert_eq!(
        after, before,
        "decoration projection must not rewrite spans"
    );
}

#[test]
fn source_frame_decorations_use_display_cells_and_preserve_semantic_spans() {
    let engine = engine("known\n");
    let mut session = EditorSession::from_text("# 東京 wrng known\n");
    let before = session
        .render_source(Viewport {
            top_line: 0,
            height: 3,
            width: 8,
            wrap: true,
            left_col: 0,
            skip_rows: 0,
        })
        .lines
        .into_iter()
        .map(|line| line.spans)
        .collect::<Vec<_>>();
    drain(&mut session, &engine, 5);
    let frame = session.render_source(Viewport {
        top_line: 0,
        height: 3,
        width: 8,
        wrap: true,
        left_col: 0,
        skip_rows: 0,
    });

    assert_eq!(
        frame
            .lines
            .iter()
            .map(|line| line.spans.clone())
            .collect::<Vec<_>>(),
        before,
        "spell decoration must not rewrite semantic spans"
    );
    assert!(frame
        .lines
        .iter()
        .flat_map(|line| &line.spans)
        .any(|span| { matches!(span.style, SemanticStyle::Heading1 | SemanticStyle::Punct) }));
    assert_eq!(frame.decorations.len(), 2, "wrng crosses the width-8 wrap");
    assert_eq!(frame.decorations[0].row, 0);
    assert_eq!(frame.decorations[0].columns, 7..8);
    assert_eq!(frame.decorations[1].row, 1);
    assert_eq!(frame.decorations[1].columns, 0..3);

    let mut clipped = EditorSession::from_text("東京 abcdefgh wrng tail\n");
    drain(&mut clipped, &engine, 5);
    let clipped_frame = clipped.render_source(Viewport {
        top_line: 0,
        height: 1,
        width: 8,
        wrap: false,
        left_col: 3,
        skip_rows: 0,
    });
    assert_eq!(clipped_frame.lines[0].text, "«bcdefg»");
    assert_eq!(clipped_frame.decorations.len(), 1);
    assert_eq!(clipped_frame.decorations[0].row, 0);
    assert_eq!(clipped_frame.decorations[0].columns, 1..7);

    session.set_spell_enabled(false);
    let disabled = session.render_source(Viewport {
        top_line: 0,
        height: 3,
        width: 8,
        wrap: true,
        left_col: 0,
        skip_rows: 0,
    });
    assert!(disabled.decorations.is_empty());
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 48,
        rng_seed: proptest::test_runner::RngSeed::Fixed(0x5E11_C0DE),
        ..ProptestConfig::default()
    })]

    #[test]
    fn incremental_spell_state_matches_fresh_after_edit_history_and_generation_changes(
        operations in prop::collection::vec(0_u8..8, 1..40),
        budgets in prop::collection::vec(1_usize..12, 1..40),
    ) {
        let mut engine = engine("hello\nworld\ngood\ntext\n");
        let mut session = EditorSession::from_text("helo wrld text");
        for (index, operation) in operations.into_iter().enumerate() {
            match operation {
                0 => {
                    session.handle_key(key('i'));
                    session.handle_key(key('x'));
                    session.handle_key(special(KeyCodeKind::Esc));
                }
                1 => {
                    session.handle_key(key('i'));
                    session.handle_key(special(KeyCodeKind::Backspace));
                    session.handle_key(special(KeyCodeKind::Esc));
                }
                2 => { session.handle_key(key('u')); }
                3 => { session.handle_key(ctrl('r')); }
                4 => {
                    let budget = budgets[index % budgets.len()];
                    session.spell_tick(&engine, budget);
                }
                5 => {
                    if let Some(diagnostic) = session.diagnostics().first().cloned() {
                        session.apply_spell_replacement(&diagnostic, "hello");
                    }
                }
                6 => {
                    if let Some(diagnostic) = session.diagnostics().first() {
                        let _ = engine.add_word(&diagnostic.source_text);
                    }
                }
                7 => {
                    let text = session.document();
                    let boundaries: Vec<_> = (0..=text.len())
                        .filter(|offset| text.is_char_boundary(*offset))
                        .collect();
                    let offset = boundaries[index % boundaries.len()];
                    session.jump_to_offset(offset).unwrap();
                }
                _ => unreachable!(),
            }
        }

        session.spell_tick(&engine, 3);
        drain(&mut session, &engine, 3);
        let mut fresh = EditorSession::from_text(&session.document());
        drain(&mut fresh, &engine, usize::MAX);
        prop_assert_eq!(session.diagnostics(), fresh.diagnostics());
    }
}
