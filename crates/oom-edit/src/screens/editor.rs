//! Editor screen — the source-view rendering.
//!
//! T11 shipped a minimal body-only layout (status row is a single line).
//! T13 adds gutter, hint bar, proper status bar, selections, and match rendering.

use oom_edit_core::session::EditorSession;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::command::registry::Contexts;
use crate::command::Keymap;
use crate::theme::{Theme, Tier, UiSlot};
use crate::widgets::hint_bar;
use crate::widgets::spans;
use crate::widgets::status_bar;

/// Render the editor screen into the given frame area.
///
/// The `area` is the editor body rect; the application owns surrounding UI.
#[derive(Debug, Clone, Copy)]
pub struct EditorViewport {
    pub top_line: usize,
    pub wrap: bool,
    pub left_col: usize,
    pub skip_rows: usize,
}

impl EditorViewport {
    pub const fn new(top_line: usize, wrap: bool, left_col: usize, skip_rows: usize) -> Self {
        Self {
            top_line,
            wrap,
            left_col,
            skip_rows,
        }
    }
}

pub fn render_editor(
    frame: &mut Frame<'_>,
    session: &mut EditorSession,
    viewport: EditorViewport,
    relative_line_numbers: bool,
    area: Rect,
    theme: &Theme,
    tier: Tier,
) {
    render_body(
        frame,
        session,
        viewport,
        relative_line_numbers,
        area,
        theme,
        tier,
    );
}

/// Return the source text width after reserving the line-number gutter.
pub(crate) fn source_text_width(area_width: u16, line_count: usize) -> u16 {
    let gutter_w = (status_bar::gutter_width(line_count) as u16).max(4);
    area_width.saturating_sub(gutter_w.min(area_width))
}

/// Render the editor body (gutter + source lines + cursor + selections).
fn render_body(
    frame: &mut Frame<'_>,
    session: &mut EditorSession,
    viewport: EditorViewport,
    relative_line_numbers: bool,
    area: Rect,
    theme: &Theme,
    tier: Tier,
) {
    let height = area.height.max(1) as usize;

    let mode = session.mode();
    let cursor = session.cursor();
    let line_count = session.line_count();

    // Compute gutter width.
    let gutter_w = status_bar::gutter_width(line_count) as u16;
    let gutter_w = gutter_w.max(4);
    let gutter_area_width = gutter_w.min(area.width);

    let vp = oom_edit_core::session::Viewport {
        top_line: viewport.top_line,
        height: area.height,
        width: area.width.saturating_sub(gutter_area_width),
        wrap: viewport.wrap,
        left_col: viewport.left_col,
        skip_rows: viewport.skip_rows,
    };

    let frame_data = session.render_source(vp);

    // Render gutter.
    if gutter_area_width > 0 && gutter_area_width < area.width {
        let gutter_area = Rect {
            x: area.x,
            y: area.y,
            width: gutter_area_width,
            height: area.height,
        };
        render_gutter(
            frame,
            mode,
            cursor.0,
            &frame_data.line_numbers,
            relative_line_numbers,
            gutter_area,
        );
    }

    // Text area starts after gutter.
    let text_area = Rect {
        x: area.x + gutter_area_width,
        y: area.y,
        width: area.width.saturating_sub(gutter_area_width),
        height: area.height,
    };

    // Build lines for ratatui from the source frame.
    let mut lines = Vec::with_capacity(height);
    for styled_line in &frame_data.lines {
        let spans = spans::build_spans(&styled_line.text, &styled_line.spans, theme, tier);
        lines.push(Line::from(spans));
    }

    // Fill remaining lines with empty ones.
    while lines.len() < height {
        lines.push(Line::from(""));
    }

    let paragraph = Paragraph::new(lines).block(Block::default().borders(Borders::NONE));
    frame.render_widget(paragraph, text_area);

    // Draw cursor line highlight in Normal/Visual modes (not Insert).
    let cursor_row = frame_data.cursor.0 as usize;
    if matches!(
        mode,
        oom_edit_core::session::Mode::Normal
            | oom_edit_core::session::Mode::Visual
            | oom_edit_core::session::Mode::VisualLine
            | oom_edit_core::session::Mode::VisualBlock
    ) && cursor_row < height
    {
        render_cursor_line_highlight(frame, text_area, cursor_row, theme, tier);
    }

    // Draw cursor.
    let (cursor_row, cursor_col) = frame_data.cursor;
    let row = (text_area.y + cursor_row).min(text_area.y + text_area.height.saturating_sub(1));
    let display_col = frame_data
        .lines
        .get(cursor_row as usize)
        .map(|line| {
            let prefix: String = line.text.chars().take(cursor_col as usize).collect();
            Line::from(prefix).width() as u16
        })
        .unwrap_or(0);
    let col = text_area.x + display_col;
    frame.set_cursor_position(ratatui::layout::Position::new(col, row));

    // Render visual selections.
    for sel in &frame_data.selections {
        render_selection(frame, text_area, sel);
    }
}

/// Render the line-number gutter.
fn render_gutter(
    frame: &mut Frame<'_>,
    mode: oom_edit_core::session::Mode,
    cursor_line: usize,
    line_numbers: &[Option<usize>],
    relative_line_numbers: bool,
    area: Rect,
) {
    let gutter_lines: Vec<String> = line_numbers
        .iter()
        .map(|line_number| match line_number {
            Some(line_number) => status_bar::build_gutter(
                mode,
                line_number - 1,
                cursor_line,
                1,
                *line_number,
                relative_line_numbers,
                area.width as usize,
            )
            .remove(0),
            None => " ".repeat(area.width as usize),
        })
        .collect();

    let mut text_lines = Vec::new();
    for gutter_line in &gutter_lines {
        text_lines.push(Line::styled(
            gutter_line.clone(),
            Style::default().add_modifier(Modifier::DIM),
        ));
    }

    // Pad if needed.
    while text_lines.len() < area.height as usize {
        text_lines.push(Line::from(""));
    }

    let paragraph = Paragraph::new(text_lines).block(Block::default().borders(Borders::NONE));
    frame.render_widget(paragraph, area);
}

/// Render cursor line highlight in Normal/Visual modes.
fn render_cursor_line_highlight(
    frame: &mut Frame<'_>,
    area: Rect,
    cursor_row: usize,
    theme: &Theme,
    tier: Tier,
) {
    let row = area.y + cursor_row as u16;
    if row < area.y + area.height {
        let highlight_area = Rect {
            x: area.x,
            y: row,
            width: area.width,
            height: 1,
        };
        let style = theme.ui_style(tier, UiSlot::CursorLine);
        // Render a blank rectangle with the cursor line background.
        let paragraph = Paragraph::new("").style(style);
        frame.render_widget(paragraph, highlight_area);
    }
}

/// Render a visual selection as reversed text.
///
/// Full implementation would map byte ranges to viewport coordinates
/// and render a highlighted overlay. Placeholder for now.
fn render_selection(_frame: &mut Frame<'_>, _area: Rect, _sel: &std::ops::Range<usize>) {
    // Selection highlighting would map byte ranges to viewport coordinates
    // and render a highlighted overlay here.
}

/// Render the full status row using one fixed-badge geometry.
pub fn render_status_row(
    frame: &mut Frame<'_>,
    session: &EditorSession,
    transient: Option<&status_bar::Transient>,
    overlay_hints: &str,
    area: Rect,
    theme: &Theme,
    tier: Tier,
) {
    let mode = session.mode();
    let ctx = mode_to_context(mode);

    // Build status bar state.
    let path = session
        .document_ref()
        .path()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| "(new)".to_string());
    let dirty = session.is_dirty();
    let cursor = session.cursor();

    // Check if command line is active.
    let command_line = session
        .command_line()
        .map(|text| format!(":{text}"))
        .or_else(|| session.view_search_prompt());

    let status = status_bar::StatusBar {
        mode,
        path,
        dirty,
        is_new: session.document_ref().is_new(),
        cursor_line: cursor.0 + 1, // 1-based for display
        line_count: session.line_count(),
        command_line: command_line.clone(),
    };

    let status_text = status.build(transient, theme, tier);

    let has_transient = transient.is_some() && command_line.is_none();
    let cmdline_active = command_line.is_some();
    let middle = if cmdline_active || has_transient {
        String::new()
    } else if !overlay_hints.is_empty() {
        overlay_hints.to_string()
    } else {
        let flexible_width = area
            .width
            .saturating_sub(status_bar::MODE_BADGE_COLS)
            .saturating_sub(status_bar::RULER_COLS);
        let km = Keymap::default();
        let cells = hint_bar::build_hints(ctx, &km);
        hint_bar::format_hints(&cells, flexible_width)
    };

    status_bar::render(
        frame,
        area,
        &status_text,
        &middle,
        overlay_hints.is_empty(),
        theme,
        tier,
    );
}

/// Convert editor mode to UI context.
fn mode_to_context(mode: oom_edit_core::session::Mode) -> Contexts {
    match mode {
        oom_edit_core::session::Mode::Normal => Contexts::NORMAL,
        oom_edit_core::session::Mode::Insert => Contexts::INSERT,
        oom_edit_core::session::Mode::Visual
        | oom_edit_core::session::Mode::VisualLine
        | oom_edit_core::session::Mode::VisualBlock => Contexts::VISUAL,
        oom_edit_core::session::Mode::Command => Contexts::COMMAND,
        oom_edit_core::session::Mode::View => Contexts::VIEW,
    }
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::DEFAULT_DARK;
    use oom_edit_core::session::{KeyCode, KeyCodeKind, KeyInput, Modifiers};
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    fn feed(session: &mut EditorSession, input: &str) {
        for ch in input.chars() {
            session.handle_key(KeyInput {
                code: KeyCode {
                    kind: KeyCodeKind::Char(ch),
                },
                mods: Modifiers::default(),
            });
        }
    }

    fn buffer_row(terminal: &Terminal<TestBackend>, row: usize, width: usize) -> String {
        terminal.backend().buffer().content()[row * width..(row + 1) * width]
            .iter()
            .map(|cell| cell.symbol())
            .collect()
    }

    fn render_session_status(session: &EditorSession, width: u16) -> Terminal<TestBackend> {
        let mut terminal = Terminal::new(TestBackend::new(width, 1)).unwrap();
        terminal
            .draw(|frame| {
                render_status_row(
                    frame,
                    session,
                    None,
                    "",
                    frame.area(),
                    &DEFAULT_DARK,
                    Tier::TrueColor,
                );
            })
            .unwrap();
        terminal
    }

    #[test]
    fn prompt_cursor_is_offset_after_badge() {
        let mut command = EditorSession::from_text("hello");
        feed(&mut command, ":w");
        let command_terminal = render_session_status(&command, 80);
        assert_eq!(
            command_terminal.backend().cursor_position(),
            ratatui::layout::Position::new(status_bar::MODE_BADGE_COLS + 2, 0)
        );
        assert!(buffer_row(&command_terminal, 0, 80).starts_with(" :CMD    :w"));

        let mut view_search = EditorSession::from_text("# heading");
        view_search.toggle_view();
        feed(&mut view_search, "/head");
        let view_terminal = render_session_status(&view_search, 80);
        assert_eq!(
            view_terminal.backend().cursor_position(),
            ratatui::layout::Position::new(status_bar::MODE_BADGE_COLS + 5, 0)
        );
        assert!(buffer_row(&view_terminal, 0, 80).starts_with(" VIEW    /head"));
    }

    #[test]
    fn entering_and_leaving_view_changes_fixed_badge_label_and_background() {
        let mut session = EditorSession::from_text("# heading");
        let normal = render_session_status(&session, 80);
        let normal_cell = normal.backend().buffer().cell((1, 0)).unwrap();
        assert!(buffer_row(&normal, 0, 80).starts_with(" NORMAL "));

        session.toggle_view();
        let view = render_session_status(&session, 80);
        let view_cell = view.backend().buffer().cell((1, 0)).unwrap();
        assert!(buffer_row(&view, 0, 80).starts_with(" VIEW "));
        assert_ne!(normal_cell.bg, view_cell.bg);

        session.toggle_view();
        let normal_again = render_session_status(&session, 80);
        assert!(buffer_row(&normal_again, 0, 80).starts_with(" NORMAL "));
        assert_eq!(
            normal_again.backend().buffer().cell((1, 0)).unwrap().bg,
            normal_cell.bg
        );
    }

    #[test]
    fn render_editor_uses_full_area_height() {
        let text = (1..=20)
            .map(|line| format!("line {line:02}"))
            .collect::<Vec<_>>()
            .join("\n");
        let mut session = EditorSession::from_text(&text);
        let backend = TestBackend::new(40, 20);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| {
                let area = frame.area();
                render_editor(
                    frame,
                    &mut session,
                    EditorViewport::new(0, true, 0, 0),
                    false,
                    area,
                    &DEFAULT_DARK,
                    Tier::TrueColor,
                );
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        let last_row: String = buffer.content()[19 * 40..20 * 40]
            .iter()
            .map(|cell| cell.symbol())
            .collect();

        assert!(
            last_row.contains("line 20"),
            "expected the final editor-area row to be rendered, got {last_row:?}"
        );
    }

    #[test]
    fn render_editor_places_content_after_gutter_gap() {
        let mut session = EditorSession::from_text("x");
        let mut terminal = Terminal::new(TestBackend::new(12, 1)).unwrap();

        terminal
            .draw(|frame| {
                render_editor(
                    frame,
                    &mut session,
                    EditorViewport::new(0, false, 0, 0),
                    false,
                    frame.area(),
                    &DEFAULT_DARK,
                    Tier::TrueColor,
                );
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        assert_eq!(buffer.cell((3, 0)).unwrap().symbol(), "1");
        assert_eq!(buffer.cell((4, 0)).unwrap().symbol(), " ");
        assert_eq!(buffer.cell((5, 0)).unwrap().symbol(), " ");
        assert_eq!(buffer.cell((6, 0)).unwrap().symbol(), "x");
    }

    #[test]
    fn cursor_line_preserves_semantic_foreground() {
        let semantic_style =
            DEFAULT_DARK.style(Tier::TrueColor, oom_edit_core::SemanticStyle::Heading1);
        let cursor_style = DEFAULT_DARK.ui_style(Tier::TrueColor, UiSlot::CursorLine);
        let mut terminal = Terminal::new(TestBackend::new(3, 1)).unwrap();

        terminal
            .draw(|frame| {
                let area = frame.area();
                frame.render_widget(Paragraph::new(Line::styled("abc", semantic_style)), area);
                render_cursor_line_highlight(frame, area, 0, &DEFAULT_DARK, Tier::TrueColor);
            })
            .unwrap();

        let cell = terminal
            .backend()
            .buffer()
            .cell((0, 0))
            .expect("rendered cursor-line cell");
        assert_eq!(cell.fg, semantic_style.fg.expect("semantic foreground"));
        assert_eq!(cell.bg, cursor_style.bg.expect("cursor-line background"));
        assert!(cell.modifier.contains(cursor_style.add_modifier));
    }

    #[test]
    fn gutter_blank_for_continuation_rows() {
        let mut session = EditorSession::from_text(&"x".repeat(25));
        let mut terminal = Terminal::new(TestBackend::new(14, 3)).unwrap();
        terminal
            .draw(|frame| {
                let area = frame.area();
                render_editor(
                    frame,
                    &mut session,
                    EditorViewport::new(0, true, 0, 0),
                    false,
                    area,
                    &DEFAULT_DARK,
                    Tier::TrueColor,
                );
            })
            .unwrap();
        assert_eq!(&buffer_row(&terminal, 1, 14)[..6], "      ");
        assert_eq!(&buffer_row(&terminal, 2, 14)[..6], "      ");
    }

    #[test]
    fn gutter_absolute_for_content_rows_under_wrap() {
        let text = format!("{}\nlast", "x".repeat(25));
        let mut session = EditorSession::from_text(&text);
        let mut terminal = Terminal::new(TestBackend::new(16, 4)).unwrap();
        terminal
            .draw(|frame| {
                let area = frame.area();
                render_editor(
                    frame,
                    &mut session,
                    EditorViewport::new(0, true, 0, 0),
                    false,
                    area,
                    &DEFAULT_DARK,
                    Tier::TrueColor,
                );
            })
            .unwrap();
        assert!(buffer_row(&terminal, 0, 16)[..6].contains('1'));
        assert!(buffer_row(&terminal, 3, 16)[..6].contains('2'));
    }

    #[test]
    fn gutter_all_numbers_under_nowrap() {
        let mut session = EditorSession::from_text("first\nsecond");
        let mut terminal = Terminal::new(TestBackend::new(14, 2)).unwrap();
        terminal
            .draw(|frame| {
                let area = frame.area();
                render_editor(
                    frame,
                    &mut session,
                    EditorViewport::new(0, false, 0, 0),
                    false,
                    area,
                    &DEFAULT_DARK,
                    Tier::TrueColor,
                );
            })
            .unwrap();
        assert!(!buffer_row(&terminal, 0, 14)[..6].trim().is_empty());
        assert!(!buffer_row(&terminal, 1, 14)[..6].trim().is_empty());
    }

    #[test]
    fn editor_cursor_not_clamped_with_wrap() {
        let mut session = EditorSession::from_text(&"x".repeat(25));
        feed(&mut session, "15l");
        let mut terminal = Terminal::new(TestBackend::new(14, 3)).unwrap();
        terminal
            .draw(|frame| {
                let area = frame.area();
                render_editor(
                    frame,
                    &mut session,
                    EditorViewport::new(0, true, 0, 0),
                    false,
                    area,
                    &DEFAULT_DARK,
                    Tier::TrueColor,
                );
            })
            .unwrap();
        assert_eq!(
            terminal.backend().cursor_position(),
            ratatui::layout::Position::new(13, 1)
        );
    }

    #[test]
    fn editor_cursor_not_clamped_with_hscroll() {
        let mut session = EditorSession::from_text(&"x".repeat(25));
        feed(&mut session, "15l");
        let mut terminal = Terminal::new(TestBackend::new(14, 1)).unwrap();
        terminal
            .draw(|frame| {
                let area = frame.area();
                render_editor(
                    frame,
                    &mut session,
                    EditorViewport::new(0, false, 10, 0),
                    false,
                    area,
                    &DEFAULT_DARK,
                    Tier::TrueColor,
                );
            })
            .unwrap();
        assert_eq!(
            terminal.backend().cursor_position(),
            ratatui::layout::Position::new(11, 0)
        );
    }

    #[test]
    fn editor_cursor_wraps_to_blank_row_at_insert_boundary() {
        let mut session = EditorSession::from_text(&"x".repeat(40));
        feed(&mut session, "A");
        let mut terminal = Terminal::new(TestBackend::new(44, 2)).unwrap();
        terminal
            .draw(|frame| {
                let area = frame.area();
                render_editor(
                    frame,
                    &mut session,
                    EditorViewport::new(0, true, 0, 0),
                    false,
                    area,
                    &DEFAULT_DARK,
                    Tier::TrueColor,
                );
            })
            .unwrap();
        assert_eq!(
            terminal.backend().cursor_position(),
            ratatui::layout::Position::new(8, 1)
        );
    }

    #[test]
    fn editor_cursor_display_column_handles_cjk_wrap() {
        let mut session = EditorSession::from_text("甲乙丙丁");
        feed(&mut session, "2l");
        let mut terminal = Terminal::new(TestBackend::new(8, 2)).unwrap();
        terminal
            .draw(|frame| {
                let area = frame.area();
                render_editor(
                    frame,
                    &mut session,
                    EditorViewport::new(0, true, 0, 0),
                    false,
                    area,
                    &DEFAULT_DARK,
                    Tier::TrueColor,
                );
            })
            .unwrap();
        assert_eq!(
            terminal.backend().cursor_position(),
            ratatui::layout::Position::new(6, 1)
        );
    }

    #[test]
    fn editor_cursor_display_column_handles_combining_mark() {
        let mut session = EditorSession::from_text("a\u{301}bcdef");
        feed(&mut session, "2l");
        let mut terminal = Terminal::new(TestBackend::new(10, 1)).unwrap();
        terminal
            .draw(|frame| {
                let area = frame.area();
                render_editor(
                    frame,
                    &mut session,
                    EditorViewport::new(0, false, 0, 0),
                    false,
                    area,
                    &DEFAULT_DARK,
                    Tier::TrueColor,
                );
            })
            .unwrap();
        assert_eq!(
            terminal.backend().cursor_position(),
            ratatui::layout::Position::new(7, 0)
        );
    }

    #[test]
    fn editor_cursor_display_column_handles_cjk_hscroll() {
        let mut session = EditorSession::from_text("甲乙丙丁戊己庚辛");
        feed(&mut session, "6l");
        let mut terminal = Terminal::new(TestBackend::new(10, 1)).unwrap();
        terminal
            .draw(|frame| {
                let area = frame.area();
                render_editor(
                    frame,
                    &mut session,
                    EditorViewport::new(0, false, 4, 0),
                    false,
                    area,
                    &DEFAULT_DARK,
                    Tier::TrueColor,
                );
            })
            .unwrap();
        assert_eq!(
            terminal.backend().cursor_position(),
            ratatui::layout::Position::new(9, 0)
        );
    }

    #[test]
    fn editor_cursor_line_highlight_on_visual_row() {
        let mut session = EditorSession::from_text(&"x".repeat(25));
        feed(&mut session, "15l");
        let mut terminal = Terminal::new(TestBackend::new(14, 3)).unwrap();
        terminal
            .draw(|frame| {
                let area = frame.area();
                render_editor(
                    frame,
                    &mut session,
                    EditorViewport::new(0, true, 0, 0),
                    false,
                    area,
                    &DEFAULT_DARK,
                    Tier::TrueColor,
                );
            })
            .unwrap();
        let cursor_bg = DEFAULT_DARK
            .ui_style(Tier::TrueColor, UiSlot::CursorLine)
            .bg
            .expect("cursor-line background");
        assert_ne!(
            terminal.backend().buffer().cell((7, 0)).unwrap().bg,
            cursor_bg
        );
        assert_eq!(
            terminal.backend().buffer().cell((7, 1)).unwrap().bg,
            cursor_bg
        );
    }

    #[test]
    fn mode_to_context_normal() {
        assert_eq!(
            mode_to_context(oom_edit_core::session::Mode::Normal),
            Contexts::NORMAL
        );
    }

    #[test]
    fn mode_to_context_insert() {
        assert_eq!(
            mode_to_context(oom_edit_core::session::Mode::Insert),
            Contexts::INSERT
        );
    }

    #[test]
    fn mode_to_context_visual() {
        assert_eq!(
            mode_to_context(oom_edit_core::session::Mode::Visual),
            Contexts::VISUAL
        );
    }

    #[test]
    fn mode_to_context_visual_line() {
        assert_eq!(
            mode_to_context(oom_edit_core::session::Mode::VisualLine),
            Contexts::VISUAL
        );
    }

    #[test]
    fn mode_to_context_visual_block() {
        assert_eq!(
            mode_to_context(oom_edit_core::session::Mode::VisualBlock),
            Contexts::VISUAL
        );
    }

    #[test]
    fn mode_to_context_command() {
        assert_eq!(
            mode_to_context(oom_edit_core::session::Mode::Command),
            Contexts::COMMAND
        );
    }

    #[test]
    fn mode_to_context_view() {
        assert_eq!(
            mode_to_context(oom_edit_core::session::Mode::View),
            Contexts::VIEW
        );
    }
}
