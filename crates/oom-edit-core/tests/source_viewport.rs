use oom_edit_core::SemanticStyle;
use oom_edit_core::{
    EditorSession, Effect, KeyCode, KeyCodeKind, KeyInput, Modifiers, Severity, Viewport,
};

fn key(kind: KeyCodeKind) -> KeyInput {
    KeyInput {
        code: KeyCode { kind },
        mods: Modifiers::default(),
    }
}

fn feed(session: &mut EditorSession, input: &str) -> Vec<Effect> {
    let mut effects = Vec::new();
    for ch in input.chars() {
        effects.extend(session.handle_key(key(KeyCodeKind::Char(ch))));
    }
    effects
}

fn enter_insert(session: &mut EditorSession) {
    session.handle_key(key(KeyCodeKind::Char('i')));
}

fn move_right(session: &mut EditorSession, count: usize) {
    for _ in 0..count {
        session.handle_key(key(KeyCodeKind::Right));
    }
}

fn execute_ex(session: &mut EditorSession, command: &str) -> Vec<Effect> {
    session.handle_key(key(KeyCodeKind::Char(':')));
    feed(session, command);
    session.handle_key(key(KeyCodeKind::Enter))
}

fn viewport(width: u16, height: u16, wrap: bool, left_col: usize, skip_rows: usize) -> Viewport {
    Viewport {
        top_line: 0,
        height,
        width,
        wrap,
        left_col,
        skip_rows,
    }
}

fn long_line(len: usize) -> String {
    "x".repeat(len)
}

#[test]
fn render_source_wraps_long_line() {
    let mut session = EditorSession::from_text(&long_line(100));
    let frame = session.render_source(viewport(40, 3, true, 0, 0));
    assert_eq!(
        frame
            .lines
            .iter()
            .map(|line| line.text.len())
            .collect::<Vec<_>>(),
        [40, 40, 20]
    );
}

#[test]
fn render_source_wrap_cursor_maps_to_visual_row() {
    let mut session = EditorSession::from_text(&long_line(100));
    enter_insert(&mut session);
    move_right(&mut session, 45);
    let frame = session.render_source(viewport(40, 3, true, 0, 0));
    assert_eq!(frame.cursor, (1, 5));
}

#[test]
fn insert_column_zero_remains_stable_while_scrolling() {
    let mut session = EditorSession::from_text("abcdef\nx\n\nabcdef");
    enter_insert(&mut session);
    move_right(&mut session, 3);
    session.handle_key(key(KeyCodeKind::Down));
    session.handle_key(key(KeyCodeKind::Home));

    session.handle_key(key(KeyCodeKind::Down));
    assert_eq!(session.cursor(), (2, 0));
    session.handle_key(key(KeyCodeKind::Down));
    assert_eq!(session.cursor(), (3, 0));

    let frame = session.render_source(viewport(8, 4, true, 0, 0));
    assert_eq!(frame.cursor.1, 0);
}

#[test]
fn render_source_wrap_cursor_at_wrap_boundary() {
    let mut session = EditorSession::from_text(&long_line(100));
    enter_insert(&mut session);
    move_right(&mut session, 40);
    let frame = session.render_source(viewport(40, 3, true, 0, 0));
    assert_eq!(frame.cursor, (1, 0));
}

#[test]
fn render_source_wrap_fills_viewport_height() {
    let mut session = EditorSession::from_text(&long_line(100));
    let frame = session.render_source(viewport(10, 4, true, 0, 0));
    assert_eq!(frame.lines.len(), 4);
    assert!(frame.lines.iter().all(|line| line.text.len() == 10));
}

#[test]
fn render_source_wrap_multiple_long_lines() {
    let text = format!("{}\n{}", long_line(25), long_line(25));
    let mut session = EditorSession::from_text(&text);
    enter_insert(&mut session);
    session.handle_key(key(KeyCodeKind::Down));
    move_right(&mut session, 15);
    let frame = session.render_source(viewport(10, 6, true, 0, 0));
    assert_eq!(frame.cursor, (4, 5));
}

#[test]
fn render_source_wrap_preserves_spans_across_break() {
    let mut session = EditorSession::from_text(&format!("# {}", long_line(30)));
    let frame = session.render_source(viewport(10, 4, true, 0, 0));
    assert!(frame.lines[0]
        .spans
        .iter()
        .any(|span| span.style == SemanticStyle::Heading1));
    assert!(frame.lines[1]
        .spans
        .iter()
        .any(|span| span.style == SemanticStyle::Heading1));
}

#[test]
fn source_wrap_preserves_exact_wrapped_content() {
    let mut session = EditorSession::from_text(&format!("# {}", long_line(30)));
    let frame = session.render_source(viewport(10, 4, true, 0, 0));
    assert_eq!(
        frame
            .lines
            .iter()
            .map(|line| line.text.as_str())
            .collect::<Vec<_>>(),
        ["# xxxxxxxx", "xxxxxxxxxx", "xxxxxxxxxx", "xx"]
    );
    assert!(frame.lines.iter().all(|line| {
        line.spans
            .iter()
            .any(|span| span.style == SemanticStyle::Heading1)
    }));
}

#[test]
fn render_source_nowrap_horizontal_window() {
    let mut session = EditorSession::from_text("abcdefghijklmnopqrstuvwxyz");
    let frame = session.render_source(viewport(10, 1, false, 10, 0));
    assert_eq!(frame.lines[0].text, "«lmnopqrs»");
}

#[test]
fn render_source_nowrap_span_adjustment() {
    let mut session = EditorSession::from_text("# abcdefghijklmnop");
    let frame = session.render_source(viewport(8, 1, false, 2, 0));
    assert!(frame.lines[0].spans.iter().any(|span| {
        span.start_col == 1 && span.end_col == 7 && span.style == SemanticStyle::Heading1
    }));
}

#[test]
fn render_source_nowrap_span_partially_visible() {
    let mut session = EditorSession::from_text("# abcdefghijklmnop");
    let frame = session.render_source(viewport(8, 1, false, 4, 0));
    assert!(frame.lines[0]
        .spans
        .iter()
        .any(|span| span.style == SemanticStyle::Heading1));
}

#[test]
fn render_source_nowrap_cursor_offset() {
    let mut session = EditorSession::from_text("abcdefghijklmnopqrstuvwxyz");
    enter_insert(&mut session);
    move_right(&mut session, 15);
    let frame = session.render_source(viewport(10, 1, false, 10, 0));
    assert_eq!(frame.cursor, (0, 5));
}

#[test]
fn render_source_nowrap_short_line_no_padding() {
    let mut session = EditorSession::from_text("short");
    let frame = session.render_source(viewport(10, 1, false, 10, 0));
    assert!(frame.lines[0].text.is_empty());
}

#[test]
fn render_source_nowrap_slices_cjk_at_character_boundaries() {
    let mut session = EditorSession::from_text("甲乙丙丁戊己庚");
    let frame = session.render_source(viewport(3, 1, false, 2, 0));
    assert_eq!(frame.lines[0].text, "«丁»");
}

#[test]
fn nowrap_left_indicator_when_scrolled() {
    let mut session = EditorSession::from_text("abcdefghij");
    let frame = session.render_source(viewport(5, 1, false, 2, 0));
    assert!(frame.lines[0].text.starts_with('«'));
    assert!(frame.lines[0].spans.iter().any(|span| {
        span.start_col == 0 && span.end_col == 1 && span.style == SemanticStyle::Muted
    }));
}

#[test]
fn nowrap_right_indicator_when_content_extends() {
    let mut session = EditorSession::from_text("abcdefghij");
    let frame = session.render_source(viewport(5, 1, false, 0, 0));
    assert!(frame.lines[0].text.ends_with('»'));
}

#[test]
fn nowrap_both_indicators_simultaneously() {
    let mut session = EditorSession::from_text("abcdefghij");
    let frame = session.render_source(viewport(5, 1, false, 2, 0));
    assert_eq!(frame.lines[0].text, "«def»");
}

#[test]
fn nowrap_no_indicator_when_line_fits() {
    let mut session = EditorSession::from_text("abc");
    let frame = session.render_source(viewport(5, 1, false, 0, 0));
    assert_eq!(frame.lines[0].text, "abc");
}

#[test]
fn nowrap_indicator_does_not_appear_on_empty_lines() {
    let mut session = EditorSession::from_text("");
    let frame = session.render_source(viewport(5, 1, false, 2, 0));
    assert_eq!(frame.lines[0].text, "");
}

#[test]
fn render_source_skip_rows_skips_visual_rows() {
    let mut session =
        EditorSession::from_text("00000000001111111111222222222233333333334444444444");
    let frame = session.render_source(viewport(10, 3, true, 0, 2));
    assert_eq!(
        frame
            .lines
            .iter()
            .map(|line| line.text.as_str())
            .collect::<Vec<_>>(),
        ["2222222222", "3333333333", "4444444444"]
    );
    assert_eq!(frame.line_numbers, [None, None, None]);
}

#[test]
fn render_source_wrap_width_one_overwide_glyph_makes_progress() {
    let mut session = EditorSession::from_text("甲x");
    let frame = session.render_source(viewport(1, 2, true, 0, 0));
    assert_eq!(frame.lines[0].text, "甲");
    assert_eq!(frame.lines[1].text, "x");
}

#[test]
fn insert_cursor_at_exact_wrap_boundary_uses_blank_continuation() {
    let mut session = EditorSession::from_text(&long_line(40));
    feed(&mut session, "A");

    assert_eq!(session.cursor(), (0, 40));
    assert_eq!(session.visual_row_info(0, 40, 40, true), (1, 2));

    let frame = session.render_source(viewport(40, 2, true, 0, 0));
    assert_eq!(frame.lines[0].text, long_line(40));
    assert_eq!(frame.lines[1].text, "");
    assert_eq!(frame.line_numbers, [Some(1), None]);
    assert_eq!(frame.cursor, (1, 0));
}

#[test]
fn render_source_skip_rows_cursor_offset() {
    let mut session = EditorSession::from_text(&long_line(50));
    enter_insert(&mut session);
    move_right(&mut session, 35);
    let frame = session.render_source(viewport(10, 3, true, 0, 2));
    assert_eq!(frame.cursor, (1, 5));
}

#[test]
fn render_source_skip_rows_fills_viewport() {
    let text = format!("{}\nlast", long_line(30));
    let mut session = EditorSession::from_text(&text);
    let frame = session.render_source(viewport(10, 3, true, 0, 2));
    assert_eq!(frame.lines[1].text, "last");
    assert_eq!(frame.lines.len(), 3);
}

#[test]
fn skip_rows_zero_when_wrap_false() {
    let mut session = EditorSession::from_text("first\nsecond");
    let frame = session.render_source(viewport(10, 2, false, 0, 50));
    assert_eq!(frame.lines[0].text, "first");
    assert_eq!(frame.lines[1].text, "second");
}

#[test]
fn skip_rows_clamped_to_line_height() {
    let mut session = EditorSession::from_text(&long_line(25));
    let frame = session.render_source(viewport(10, 2, true, 0, 50));
    assert_eq!(frame.lines[0].text, "xxxxx");
}

#[test]
fn source_frame_line_numbers_content_rows() {
    let text = format!("{}\nlast", long_line(25));
    let mut session = EditorSession::from_text(&text);
    let frame = session.render_source(viewport(10, 4, true, 0, 0));
    assert_eq!(frame.line_numbers, [Some(1), None, None, Some(2)]);
}

#[test]
fn source_frame_line_numbers_after_skip() {
    let mut session = EditorSession::from_text(&long_line(30));
    let frame = session.render_source(viewport(10, 1, true, 0, 1));
    assert_eq!(frame.line_numbers, [None]);
}

#[test]
fn source_frame_line_numbers_nowrap() {
    let mut session = EditorSession::from_text("first\nsecond");
    let frame = session.render_source(viewport(10, 2, false, 0, 0));
    assert_eq!(frame.line_numbers, [Some(1), Some(2)]);
}

#[test]
fn visual_row_info_basic() {
    let session = EditorSession::from_text(&long_line(100));
    assert_eq!(session.visual_row_info(0, 0, 40, true), (0, 3));
}

#[test]
fn visual_row_info_cursor_position() {
    let session = EditorSession::from_text(&long_line(100));
    assert_eq!(session.visual_row_info(0, 45, 40, true), (1, 3));
}

#[test]
fn visual_row_info_nowrap() {
    let session = EditorSession::from_text(&long_line(100));
    assert_eq!(session.visual_row_info(0, 45, 40, false), (0, 1));
}

#[test]
fn visual_row_info_short_line() {
    let session = EditorSession::from_text("short");
    assert_eq!(session.visual_row_info(0, 3, 40, true), (0, 1));
}

#[test]
fn visual_row_info_empty_line() {
    let session = EditorSession::from_text("");
    assert_eq!(session.visual_row_info(0, 0, 40, true), (0, 1));
}

#[test]
fn set_wrap_ex_command_emits_effect() {
    let mut session = EditorSession::from_text("");
    assert!(execute_ex(&mut session, "set wrap").contains(&Effect::SetWrap(true)));
}

#[test]
fn set_nowrap_ex_command_emits_effect() {
    let mut session = EditorSession::from_text("");
    assert!(execute_ex(&mut session, "set nowrap").contains(&Effect::SetWrap(false)));
}

#[test]
fn set_unknown_option_shows_error() {
    let mut session = EditorSession::from_text("");
    assert!(execute_ex(&mut session, "set foobar").iter().any(|effect| matches!(
        effect,
        Effect::Message { text, severity: Severity::Warning } if text == "Unknown option: foobar"
    )));
}

#[test]
fn set_no_arg_shows_usage() {
    let mut session = EditorSession::from_text("");
    assert!(execute_ex(&mut session, "set")
        .iter()
        .any(|effect| matches!(
            effect,
            Effect::Message { text, severity: Severity::Warning } if text == "Usage: :set <option>"
        )));
}
