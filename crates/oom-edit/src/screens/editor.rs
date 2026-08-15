//! Source Insert screen rendering.
//!
//! T11 shipped a minimal body-only layout (status row is a single line).
//! T13 adds gutter, hint bar, proper status bar, selections, and match rendering.

use oom_edit_core::EditorSession;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::command::registry::Contexts;
use crate::theme::{Theme, Tier};
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

    let vp = oom_edit_core::Viewport {
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
    let search_style = theme.style(tier, oom_edit_core::SemanticStyle::Match);
    for (row, styled_line) in frame_data.lines.iter().enumerate() {
        let mut row_spans = spans::build_spans(&styled_line.text, &styled_line.spans, theme, tier);
        for decoration in frame_data
            .decorations
            .iter()
            .filter(|decoration| decoration.row == row)
        {
            row_spans = spans::apply_interval_style(
                row_spans,
                decoration.columns.clone(),
                theme.decoration_style(tier, decoration.kind),
                Some(search_style),
            )
            .spans;
        }
        lines.push(Line::from(row_spans));
    }

    // Fill remaining lines with empty ones.
    while lines.len() < height {
        lines.push(Line::from(""));
    }

    let paragraph = Paragraph::new(lines).block(Block::default().borders(Borders::NONE));
    frame.render_widget(paragraph, text_area);

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
}

/// Render the line-number gutter.
pub(crate) fn render_gutter(
    frame: &mut Frame<'_>,
    mode: oom_edit_core::Mode,
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
        .path()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| "(new)".to_string());
    let dirty = session.is_dirty();
    let cursor = session.cursor();

    // Check if command line is active.
    let command_line = session
        .command_line()
        .map(|text| format!(":{text}"))
        .or_else(|| session.rendered_search_prompt());

    let status = status_bar::StatusBar {
        mode,
        path,
        dirty,
        is_new: session.is_new(),
        cursor_line: cursor.0 + 1, // 1-based for display
        cursor_col: cursor.1 + 1,
        line_count: session.line_count(),
        spell_enabled: session.spell_enabled(),
        command_line: command_line.clone(),
    };

    let status_text = status.build(transient, theme, tier);

    let has_transient = transient.is_some() && command_line.is_none();
    let cmdline_active = command_line.is_some();
    let middle = if cmdline_active || has_transient {
        String::new()
    } else {
        let mut indicators = Vec::new();
        if dirty {
            indicators.push("[+]");
        }
        if !session.spell_enabled() {
            indicators.push("[spell off]");
        }
        let indicators = indicators.join(" ");
        let base = if !overlay_hints.is_empty() {
            overlay_hints.to_string()
        } else {
            let mut flexible_width = area
                .width
                .saturating_sub(status_bar::STATUS_CONTENT_OFFSET)
                .saturating_sub(status_bar::RULER_COLS);
            if !indicators.is_empty() {
                flexible_width = flexible_width
                    .saturating_sub(indicators.len() as u16)
                    .saturating_sub(2);
            }
            let cells = hint_bar::build_hints(ctx);
            hint_bar::format_hints(&cells, flexible_width)
        };
        match (indicators.is_empty(), base.is_empty()) {
            (true, _) => base,
            (false, true) => indicators,
            (false, false) => format!("{indicators}  {base}"),
        }
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
fn mode_to_context(mode: oom_edit_core::Mode) -> Contexts {
    match mode {
        oom_edit_core::Mode::Normal => Contexts::NORMAL,
        oom_edit_core::Mode::Insert => Contexts::INSERT,
        oom_edit_core::Mode::Select => Contexts::SELECT,
        oom_edit_core::Mode::Command => Contexts::COMMAND,
    }
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::{get_theme, Tier, DEFAULT_DARK};
    use oom_edit_core::{KeyCode, KeyCodeKind, KeyInput, Modifiers};
    use oom_spell::{BuildProgress, SpellEngineBuilder};
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

    fn move_right(session: &mut EditorSession, count: usize) {
        for _ in 0..count {
            session.handle_key(KeyInput {
                code: KeyCode {
                    kind: KeyCodeKind::Right,
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

    fn spell_engine() -> oom_spell::SpellEngine {
        let mut builder = SpellEngineBuilder::new(vec!["known\n".to_string()]);
        for _ in 0..100 {
            if builder.step(4096) == BuildProgress::Complete {
                return builder.finish().unwrap();
            }
        }
        panic!("test spell engine failed to finish within 100 steps");
    }

    fn drain_spell(session: &mut EditorSession, engine: &oom_spell::SpellEngine) {
        for _ in 0..10_000 {
            if !session.diagnostics_pending() {
                return;
            }
            assert!(session.spell_tick(engine, 8));
        }
        panic!("test spell scan failed to finish within 10,000 ticks");
    }

    #[test]
    fn source_spell_decorations_reach_insert_across_every_theme_and_tier() {
        for name in crate::theme::built_in_themes() {
            for tier in [Tier::TrueColor, Tier::Color16, Tier::Monochrome] {
                let engine = spell_engine();
                let mut session = EditorSession::from_text("# misspelledd\n");
                drain_spell(&mut session, &engine);
                feed(&mut session, "i");
                let theme = get_theme(name);
                let mut terminal = Terminal::new(TestBackend::new(40, 2)).unwrap();
                terminal
                    .draw(|frame| {
                        render_editor(
                            frame,
                            &mut session,
                            EditorViewport::new(0, true, 0, 0),
                            false,
                            frame.area(),
                            theme,
                            tier,
                        );
                    })
                    .unwrap();

                let gutter = (status_bar::gutter_width(session.line_count()) as u16).max(4);
                let cell = terminal.backend().buffer().cell((gutter + 2, 0)).unwrap();
                assert!(cell.modifier.contains(Modifier::UNDERLINED));
                assert!(cell.modifier.contains(Modifier::ITALIC));
                assert!(cell.modifier.contains(Modifier::BOLD));
                let decoration = theme.decoration_style(
                    tier,
                    oom_edit_core::DecorationKind::Diagnostic {
                        provider: oom_edit_core::DiagnosticProvider::Spell,
                        severity: oom_edit_core::DiagnosticSeverity::Warning,
                    },
                );
                if let Some(foreground) = decoration.fg {
                    assert_eq!(cell.fg, foreground);
                } else {
                    assert_eq!(cell.fg, ratatui::buffer::Cell::default().fg);
                }
                let neighbor = terminal.backend().buffer().cell((gutter + 14, 0)).unwrap();
                assert!(!neighbor.modifier.contains(Modifier::ITALIC));
            }
        }
    }

    #[test]
    fn source_search_wins_foreground_while_retaining_spell_carriers() {
        let engine = spell_engine();
        let mut session = EditorSession::from_text("# misspelledd\n");
        drain_spell(&mut session, &engine);
        session.rendered_layout_mut(34);
        feed(&mut session, "/misspelledd");
        session.handle_key(KeyInput {
            code: KeyCode {
                kind: KeyCodeKind::Enter,
            },
            mods: Modifiers::default(),
        });
        feed(&mut session, "i");
        assert_eq!(
            session
                .rendered_search()
                .map(|search| search.pattern.as_str()),
            Some("misspelledd")
        );
        let source_frame = session.render_source(oom_edit_core::Viewport {
            top_line: 0,
            height: 2,
            width: 34,
            wrap: true,
            left_col: 0,
            skip_rows: 0,
        });
        assert!(source_frame.lines[0].spans.iter().any(|span| {
            span.start_col <= 2
                && span.end_col > 2
                && span.style == oom_edit_core::SemanticStyle::Match
        }));
        assert!(source_frame
            .decorations
            .iter()
            .any(|decoration| decoration.row == 0 && decoration.columns.contains(&2)));
        let mut terminal = Terminal::new(TestBackend::new(40, 2)).unwrap();
        terminal
            .draw(|frame| {
                render_editor(
                    frame,
                    &mut session,
                    EditorViewport::new(0, true, 0, 0),
                    false,
                    frame.area(),
                    &DEFAULT_DARK,
                    Tier::TrueColor,
                );
            })
            .unwrap();

        let gutter = (status_bar::gutter_width(session.line_count()) as u16).max(4);
        let cell = terminal.backend().buffer().cell((gutter + 2, 0)).unwrap();
        assert_eq!(
            Some(cell.fg),
            DEFAULT_DARK
                .style(Tier::TrueColor, oom_edit_core::SemanticStyle::Match)
                .fg
        );
        assert!(cell.modifier.contains(Modifier::UNDERLINED));
        assert!(cell.modifier.contains(Modifier::ITALIC));
    }

    #[test]
    fn prompt_cursor_is_offset_after_badge_gap() {
        let row_style = DEFAULT_DARK.ui_style(Tier::TrueColor, crate::theme::UiSlot::StatusBar);

        let mut command = EditorSession::from_text("hello");
        feed(&mut command, ":w");
        let command_terminal = render_session_status(&command, 80);
        assert_eq!(
            command_terminal.backend().cursor_position(),
            ratatui::layout::Position::new(status_bar::STATUS_CONTENT_OFFSET + 2, 0)
        );
        assert!(buffer_row(&command_terminal, 0, 80).starts_with(" :CMD    :w"));
        let command_gap = command_terminal
            .backend()
            .buffer()
            .cell((status_bar::MODE_BADGE_COLS, 0))
            .unwrap();
        assert_eq!(command_gap.symbol(), " ");
        assert_eq!(command_gap.fg, row_style.fg.unwrap());
        assert_eq!(command_gap.bg, row_style.bg.unwrap());

        let mut rendered_search = EditorSession::from_text("# heading");
        rendered_search.render_layout(76);
        feed(&mut rendered_search, "/head");
        let view_terminal = render_session_status(&rendered_search, 80);
        assert_eq!(
            view_terminal.backend().cursor_position(),
            ratatui::layout::Position::new(status_bar::STATUS_CONTENT_OFFSET + 5, 0)
        );
        assert!(buffer_row(&view_terminal, 0, 80).starts_with(" NORMAL  /head"));
        let search_gap = view_terminal
            .backend()
            .buffer()
            .cell((status_bar::MODE_BADGE_COLS, 0))
            .unwrap();
        assert_eq!(search_gap.symbol(), " ");
        assert_eq!(search_gap.fg, row_style.fg.unwrap());
        assert_eq!(search_gap.bg, row_style.bg.unwrap());
    }

    #[test]
    fn hint_fitting_reserves_badge_gap() {
        let session = EditorSession::from_text("hello");
        let terminal = render_session_status(&session, 70);
        let row = buffer_row(&terminal, 0, 70);
        let middle_start = status_bar::STATUS_CONTENT_OFFSET as usize;
        let middle_end = 70 - status_bar::RULER_COLS as usize;
        let middle = &row[middle_start..middle_end];

        assert_eq!(middle.trim_end(), "v=character-wise selection");
        assert!(!middle.contains("Space w"));
    }

    #[test]
    fn spell_marker_reserves_width_before_hint_cells_are_composed() {
        let mut session = EditorSession::from_text("hello");
        session.set_spell_enabled(false);
        let terminal = render_session_status(&session, 80);
        let row = buffer_row(&terminal, 0, 80);
        let middle_start = status_bar::STATUS_CONTENT_OFFSET as usize;
        let middle_end = 80 - status_bar::RULER_COLS as usize;
        let middle = &row[middle_start..middle_end];

        assert_eq!(middle.trim_end(), "[spell off]  v=character-wise selection");
        assert!(!middle.contains("Space w"));

        feed(&mut session, "ix");
        session.handle_key(KeyInput {
            code: KeyCode {
                kind: KeyCodeKind::Esc,
            },
            mods: Modifiers::default(),
        });
        let terminal = render_session_status(&session, 80);
        let row = buffer_row(&terminal, 0, 80);
        let middle = &row[middle_start..middle_end];
        assert_eq!(
            middle.trim_end(),
            "[+] [spell off]  v=character-wise selection"
        );
        assert!(!middle.contains("Space w"));
    }

    #[test]
    fn entering_and_leaving_select_changes_fixed_badge_label_and_background() {
        let mut session = EditorSession::from_text("# heading");
        let normal = render_session_status(&session, 80);
        let normal_cell = normal.backend().buffer().cell((1, 0)).unwrap();
        assert!(buffer_row(&normal, 0, 80).starts_with(" NORMAL "));

        session.render_layout(76);
        feed(&mut session, "v");
        let select = render_session_status(&session, 80);
        let select_cell = select.backend().buffer().cell((1, 0)).unwrap();
        assert!(buffer_row(&select, 0, 80).starts_with(" SELECT "));
        assert_ne!(normal_cell.bg, select_cell.bg);

        session.handle_key(KeyInput {
            code: KeyCode {
                kind: KeyCodeKind::Esc,
            },
            mods: Modifiers::default(),
        });
        let normal_again = render_session_status(&session, 80);
        assert!(buffer_row(&normal_again, 0, 80).starts_with(" NORMAL "));
        assert_eq!(
            normal_again.backend().buffer().cell((1, 0)).unwrap().bg,
            normal_cell.bg
        );
    }

    #[test]
    fn rendered_motion_updates_source_ruler() {
        let mut session = EditorSession::from_text("# one\n\n# two\n\n# three\n");
        session.render_layout(74);
        for _ in 0..10 {
            if session.cursor().0 >= 4 {
                break;
            }
            feed(&mut session, "j");
        }
        assert_eq!(session.cursor(), (4, 2));

        let terminal = render_session_status(&session, 80);
        assert!(buffer_row(&terminal, 0, 80).ends_with("5:3  83%"));
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
        feed(&mut session, "i");
        move_right(&mut session, 15);
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
        feed(&mut session, "i");
        move_right(&mut session, 15);
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
        feed(&mut session, "i");
        move_right(&mut session, 2);
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
        feed(&mut session, "i");
        move_right(&mut session, 2);
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
        feed(&mut session, "i");
        move_right(&mut session, 6);
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
    fn mode_to_context_normal() {
        assert_eq!(
            mode_to_context(oom_edit_core::Mode::Normal),
            Contexts::NORMAL
        );
    }

    #[test]
    fn mode_to_context_insert() {
        assert_eq!(
            mode_to_context(oom_edit_core::Mode::Insert),
            Contexts::INSERT
        );
    }

    #[test]
    fn mode_to_context_select() {
        assert_eq!(
            mode_to_context(oom_edit_core::Mode::Select),
            Contexts::SELECT
        );
    }

    #[test]
    fn mode_to_context_command() {
        assert_eq!(
            mode_to_context(oom_edit_core::Mode::Command),
            Contexts::COMMAND
        );
    }
}
