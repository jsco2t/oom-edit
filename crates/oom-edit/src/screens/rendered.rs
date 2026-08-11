//! Rendered Normal/Select screen.
//!
//! The core owns layout, source mapping, and Select metadata. This adapter
//! reserves and paints the source gutter, maps semantic spans through the
//! theme, and layers the subdued Normal cursor or distinct Select carrier.
//!
//! See plan §6.3, VN-1, VN-3.

use oom_edit_core::EditorSession;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::theme::{Theme, Tier, UiSlot};
use crate::widgets::spans;
use crate::widgets::status_bar;

/// Render Normal, Select, or Command into the body area.
pub fn render_rendered(
    frame: &mut Frame<'_>,
    session: &mut EditorSession,
    rendered_top: usize,
    relative_line_numbers: bool,
    area: Rect,
    theme: &Theme,
    tier: Tier,
) {
    let height = area.height.max(1) as usize;
    let mode = session.mode();
    let source_cursor_line = session.cursor().0;
    let gutter_width = (status_bar::gutter_width(session.line_count()) as u16)
        .max(4)
        .min(area.width);
    let text_width = area.width.saturating_sub(gutter_width);
    session.render_layout(text_width);
    let cursor_line = session.rendered_cursor_line();
    let selection = session.rendered_selection();
    let layout = session
        .rendered_layout()
        .expect("rendered layout was built for this frame");

    if layout.lines.is_empty() {
        return;
    }

    // Compute visible line range.
    let max_lines = layout.lines.len();
    let rendered_top = rendered_top.min(max_lines.saturating_sub(1));
    let rendered_bottom = (rendered_top + height).min(max_lines);

    if gutter_width > 0 && gutter_width < area.width {
        let gutter_area = Rect::new(area.x, area.y, gutter_width, area.height);
        super::editor::render_gutter(
            frame,
            mode,
            source_cursor_line,
            &layout.line_numbers[rendered_top..rendered_bottom],
            relative_line_numbers,
            gutter_area,
        );
    }
    let text_area = Rect::new(
        area.x.saturating_add(gutter_width),
        area.y,
        text_width,
        area.height,
    );

    // Build ratatui lines from the rendered layout's styled lines.
    let mut lines: Vec<Line<'_>> = Vec::with_capacity(height);

    for i in rendered_top..rendered_bottom {
        let rendered_line = &layout.lines[i];
        let mut spans = spans::build_spans(
            &rendered_line.styled.text,
            &rendered_line.styled.spans,
            theme,
            tier,
        );
        if rendered_line.role == oom_edit_core::RenderedLineRole::Metadata {
            spans = build_highlighted_line(
                spans,
                text_width,
                theme.ui_style(tier, UiSlot::MetadataPanel),
            )
            .spans;
        }

        let selected = selection
            .as_ref()
            .and_then(|selection| selection.rows.iter().find(|row| row.row == i))
            .map(|row| row.columns.clone())
            .filter(|columns| columns.start < columns.end);
        if let Some(columns) = selected {
            let mut style = theme.style(tier, oom_edit_core::SemanticStyle::Selection);
            if rendered_line.role == oom_edit_core::RenderedLineRole::Metadata {
                style.bg = None;
            }
            lines.push(build_interval_highlighted_line(spans, columns, style));
        } else if i == cursor_line {
            let mut style = theme.ui_style(tier, UiSlot::CursorLine);
            if rendered_line.role == oom_edit_core::RenderedLineRole::Metadata {
                style = style.add_modifier(ratatui::style::Modifier::UNDERLINED);
            }
            lines.push(build_highlighted_line(spans, text_width, style));
        } else {
            lines.push(Line::from(spans));
        }
    }

    // Fill remaining lines with blanks.
    while lines.len() < height {
        lines.push(Line::from(""));
    }

    // Truncate to exactly viewport height.
    lines.truncate(height);

    let paragraph = Paragraph::new(lines).block(Block::default().borders(Borders::NONE));
    frame.render_widget(paragraph, text_area);
}

/// Apply a carrier only to display groups intersecting `columns`.
fn build_interval_highlighted_line<'a>(
    spans: Vec<Span<'a>>,
    columns: std::ops::Range<usize>,
    overlay: Style,
) -> Line<'a> {
    let mut result = Vec::new();
    let mut display_column = 0;
    for span in spans {
        let mut groups: Vec<String> = Vec::new();
        for character in span.content.chars() {
            if Span::raw(character.to_string()).width() == 0 {
                if let Some(previous) = groups.last_mut() {
                    previous.push(character);
                } else {
                    groups.push(character.to_string());
                }
            } else {
                groups.push(character.to_string());
            }
        }
        for group in groups {
            let width = group
                .chars()
                .map(|character| Span::raw(character.to_string()).width())
                .sum::<usize>();
            let group_columns = display_column..display_column + width;
            display_column += width;
            let selected = group_columns.end > columns.start && group_columns.start < columns.end;
            let mut style = span.style;
            if selected {
                style = style.add_modifier(overlay.add_modifier);
                if let Some(background) = overlay.bg {
                    style = style.bg(background);
                }
            }
            result.push(Span::styled(group, style));
        }
    }
    if display_column < columns.end {
        if display_column < columns.start {
            result.push(Span::raw(" ".repeat(columns.start - display_column)));
        }
        let padding_start = display_column.max(columns.start);
        if padding_start < columns.end {
            result.push(Span::styled(
                " ".repeat(columns.end - padding_start),
                overlay,
            ));
        }
    }
    Line::from(result)
}

/// Overlay a full-row carrier without replacing semantic foreground styles.
fn build_highlighted_line<'a>(mut spans: Vec<Span<'a>>, width: u16, style: Style) -> Line<'a> {
    let text_width = spans.iter().map(Span::width).sum::<usize>();

    for span in &mut spans {
        span.style = span.style.add_modifier(style.add_modifier);
        if let Some(background) = style.bg {
            span.style = span.style.bg(background);
        }
    }

    let padding = (width as usize).saturating_sub(text_width);
    if padding > 0 {
        spans.push(Span::styled(" ".repeat(padding), style));
    }

    Line::from(spans)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::{Tier, DEFAULT_DARK};
    use oom_edit_core::{KeyCode, KeyCodeKind, KeyInput, Modifiers, SemanticStyle};
    use ratatui::backend::TestBackend;
    use ratatui::style::Modifier;
    use ratatui::Terminal;

    fn key(ch: char) -> KeyInput {
        KeyInput {
            code: KeyCode {
                kind: KeyCodeKind::Char(ch),
            },
            mods: Modifiers::default(),
        }
    }

    fn ctrl(ch: char) -> KeyInput {
        let mut input = key(ch);
        input.mods.ctrl = true;
        input
    }

    #[test]
    fn highlighted_row_preserves_semantic_foregrounds() {
        let source = [oom_edit_core::Span {
            start_col: 0,
            end_col: 2,
            style: SemanticStyle::Emphasis,
        }];
        let semantic = spans::build_spans("em text", &source, &DEFAULT_DARK, Tier::TrueColor);
        let selection = DEFAULT_DARK.style(Tier::TrueColor, SemanticStyle::Selection);
        let line = build_highlighted_line(semantic, 12, selection);
        assert_eq!(line.width(), 12);
        assert_eq!(
            line.spans[0].style.fg,
            DEFAULT_DARK
                .style(Tier::TrueColor, SemanticStyle::Emphasis)
                .fg
        );
    }

    #[test]
    fn normal_cursor_and_select_rows_use_distinct_styles() {
        let mut normal = EditorSession::from_text("# Heading\n\nBody\n");
        let mut normal_terminal = Terminal::new(TestBackend::new(40, 4)).unwrap();
        normal_terminal
            .draw(|frame| {
                render_rendered(
                    frame,
                    &mut normal,
                    0,
                    false,
                    frame.area(),
                    &DEFAULT_DARK,
                    Tier::TrueColor,
                );
            })
            .unwrap();

        let mut select = EditorSession::from_text("# Heading\n\nBody\n");
        select.render_layout(36);
        select.handle_key(key('v'));
        let mut select_terminal = Terminal::new(TestBackend::new(40, 4)).unwrap();
        select_terminal
            .draw(|frame| {
                render_rendered(
                    frame,
                    &mut select,
                    0,
                    false,
                    frame.area(),
                    &DEFAULT_DARK,
                    Tier::TrueColor,
                );
            })
            .unwrap();

        let gutter_width = status_bar::gutter_width(normal.line_count()) as u16;
        let normal_cell = normal_terminal
            .backend()
            .buffer()
            .cell((gutter_width, 0))
            .unwrap();
        let select_cell = select_terminal
            .backend()
            .buffer()
            .cell((gutter_width + 2, 0))
            .unwrap();
        assert_eq!(
            normal_cell.bg,
            DEFAULT_DARK
                .ui_style(Tier::TrueColor, UiSlot::CursorLine)
                .bg
                .unwrap()
        );
        assert!(!normal_cell.modifier.contains(Modifier::REVERSED));
        assert!(select_cell.modifier.contains(Modifier::REVERSED));
        assert_ne!(normal_cell.bg, select_cell.bg);
    }

    #[test]
    fn character_selection_highlights_only_selected_atoms() {
        let mut session = EditorSession::from_text("# Heading\n");
        session.render_layout(36);
        session.handle_key(key('v'));
        session.handle_key(key('l'));
        let interval = session.rendered_selection().unwrap().rows[0]
            .columns
            .clone();
        let mut terminal = Terminal::new(TestBackend::new(40, 3)).unwrap();
        terminal
            .draw(|frame| {
                render_rendered(
                    frame,
                    &mut session,
                    0,
                    false,
                    frame.area(),
                    &DEFAULT_DARK,
                    Tier::TrueColor,
                );
            })
            .unwrap();
        let gutter = status_bar::gutter_width(session.line_count()) as u16;
        let buffer = terminal.backend().buffer();
        for column in interval.clone() {
            assert!(buffer
                .cell((gutter + column as u16, 0))
                .unwrap()
                .modifier
                .contains(Modifier::REVERSED));
        }
        assert!(!buffer
            .cell((gutter + interval.end as u16, 0))
            .unwrap()
            .modifier
            .contains(Modifier::REVERSED));
    }

    #[test]
    fn block_selection_paints_a_consistent_rectangle() {
        let mut session = EditorSession::from_text("abcd\n\nwxyz\n");
        session.render_layout(36);
        session.handle_key(ctrl('v'));
        session.handle_key(key('l'));
        session.handle_key(key('j'));
        session.handle_key(key('j'));
        let selection = session.rendered_selection().unwrap();
        assert_eq!(selection.block_width, Some(2));
        assert!(selection
            .rows
            .iter()
            .all(|row| row.columns.end - row.columns.start == 2));

        let mut terminal = Terminal::new(TestBackend::new(40, 5)).unwrap();
        terminal
            .draw(|frame| {
                render_rendered(
                    frame,
                    &mut session,
                    0,
                    false,
                    frame.area(),
                    &DEFAULT_DARK,
                    Tier::TrueColor,
                );
            })
            .unwrap();
        let gutter = status_bar::gutter_width(session.line_count()) as u16;
        let buffer = terminal.backend().buffer();
        for row in 0..3 {
            for column in 0..2 {
                assert!(
                    buffer
                        .cell((gutter + column, row))
                        .unwrap()
                        .modifier
                        .contains(Modifier::REVERSED),
                    "missing block overlay at row {row}, column {column}"
                );
            }
            assert!(!buffer
                .cell((gutter + 2, row))
                .unwrap()
                .modifier
                .contains(Modifier::REVERSED));
        }
    }

    #[test]
    fn wrapped_line_selection_paints_every_visual_row_it_will_delete() {
        let text = "alpha beta gamma delta epsilon zeta\n";
        let mut session = EditorSession::from_text(text);
        let selected_rows = session.render_layout(12).lines.len();
        assert!(selected_rows > 1);
        session.handle_key(key('V'));
        assert_eq!(
            session.rendered_selection().unwrap().rows.len(),
            selected_rows
        );

        let mut terminal = Terminal::new(TestBackend::new(16, 6)).unwrap();
        terminal
            .draw(|frame| {
                render_rendered(
                    frame,
                    &mut session,
                    0,
                    false,
                    frame.area(),
                    &DEFAULT_DARK,
                    Tier::TrueColor,
                );
            })
            .unwrap();
        let selection = session.rendered_selection().unwrap();
        let gutter = status_bar::gutter_width(session.line_count()) as u16;
        let buffer = terminal.backend().buffer();
        for row in &selection.rows {
            assert!(buffer
                .cell((gutter, row.row as u16))
                .unwrap()
                .modifier
                .contains(Modifier::REVERSED));
        }
    }

    #[test]
    fn wide_character_selection_never_splits_a_display_atom() {
        let semantic = spans::build_spans("東京", &[], &DEFAULT_DARK, Tier::TrueColor);
        let selection = DEFAULT_DARK.style(Tier::TrueColor, SemanticStyle::Selection);
        let line = build_interval_highlighted_line(semantic, 1..2, selection);
        assert_eq!(line.spans[0].content, "東");
        assert_eq!(line.spans[0].width(), 2);
        assert!(line.spans[0]
            .style
            .add_modifier
            .contains(Modifier::REVERSED));
        assert!(!line.spans[1]
            .style
            .add_modifier
            .contains(Modifier::REVERSED));
    }

    #[test]
    fn metadata_selection_preserves_panel_surface_and_semantic_foreground() {
        let mut session = EditorSession::from_text("---\ntitle: Example\n---\n");
        session.render_layout(36);
        session.handle_key(key('j'));
        session.handle_key(key('v'));
        session.handle_key(key('l'));
        let selected = session
            .rendered_selection()
            .unwrap()
            .rows
            .iter()
            .find(|row| !row.source_ranges.is_empty())
            .unwrap()
            .clone();
        let mut terminal = Terminal::new(TestBackend::new(40, 5)).unwrap();
        terminal
            .draw(|frame| {
                render_rendered(
                    frame,
                    &mut session,
                    0,
                    false,
                    frame.area(),
                    &DEFAULT_DARK,
                    Tier::TrueColor,
                );
            })
            .unwrap();
        let gutter = status_bar::gutter_width(session.line_count()) as u16;
        let cell = terminal
            .backend()
            .buffer()
            .cell((gutter + selected.columns.start as u16, selected.row as u16))
            .unwrap();
        assert_eq!(
            cell.bg,
            DEFAULT_DARK
                .ui_style(Tier::TrueColor, UiSlot::MetadataPanel)
                .bg
                .unwrap()
        );
        assert!(cell.modifier.contains(Modifier::REVERSED));
        assert_eq!(
            Some(cell.fg),
            DEFAULT_DARK.style(Tier::TrueColor, SemanticStyle::FmKey).fg
        );
    }

    #[test]
    fn metadata_cursor_delimiters_key_and_value_keep_composed_styles() {
        let mut session = EditorSession::from_text("---\ntitle: Example\n---\n");
        let mut terminal = Terminal::new(TestBackend::new(40, 5)).unwrap();
        terminal
            .draw(|frame| {
                render_rendered(
                    frame,
                    &mut session,
                    0,
                    false,
                    frame.area(),
                    &DEFAULT_DARK,
                    Tier::TrueColor,
                );
            })
            .unwrap();

        let gutter = status_bar::gutter_width(session.line_count()) as u16;
        let buffer = terminal.backend().buffer();
        let panel = DEFAULT_DARK.ui_style(Tier::TrueColor, UiSlot::MetadataPanel);
        let cursor_line = DEFAULT_DARK.ui_style(Tier::TrueColor, UiSlot::CursorLine);
        let delimiter = DEFAULT_DARK.style(Tier::TrueColor, SemanticStyle::FmDelimiter);
        let key_style = DEFAULT_DARK.style(Tier::TrueColor, SemanticStyle::FmKey);
        let value_style = DEFAULT_DARK.style(Tier::TrueColor, SemanticStyle::FmValue);
        let text_style = DEFAULT_DARK.style(Tier::TrueColor, SemanticStyle::Text);

        let opening = buffer.cell((gutter, 0)).unwrap();
        assert_eq!(opening.symbol(), "┌");
        assert_eq!(Some(opening.fg), delimiter.fg);
        assert_eq!(Some(opening.bg), cursor_line.bg);
        assert!(opening.modifier.contains(cursor_line.add_modifier));
        assert!(opening.modifier.contains(Modifier::UNDERLINED));

        let metadata_key = buffer.cell((gutter + 2, 1)).unwrap();
        let separator = buffer.cell((gutter + 7, 1)).unwrap();
        let value = buffer.cell((gutter + 9, 1)).unwrap();
        let closing = buffer.cell((gutter, 2)).unwrap();
        assert_eq!(metadata_key.symbol(), "t");
        assert_eq!(Some(metadata_key.fg), key_style.fg);
        assert_eq!(separator.symbol(), ":");
        assert_eq!(Some(separator.fg), text_style.fg);
        assert_eq!(value.symbol(), "E");
        assert_eq!(Some(value.fg), value_style.fg);
        assert_eq!(closing.symbol(), "└");
        assert_eq!(Some(closing.fg), delimiter.fg);
        for cell in [metadata_key, separator, value, closing] {
            assert_eq!(Some(cell.bg), panel.bg);
        }

        let mut non_cursor_session = EditorSession::from_text("---\ntitle: Example\n---\n");
        non_cursor_session.render_layout(36);
        non_cursor_session.handle_key(key('j'));
        let mut non_cursor_terminal = Terminal::new(TestBackend::new(40, 5)).unwrap();
        non_cursor_terminal
            .draw(|frame| {
                render_rendered(
                    frame,
                    &mut non_cursor_session,
                    0,
                    false,
                    frame.area(),
                    &DEFAULT_DARK,
                    Tier::TrueColor,
                );
            })
            .unwrap();

        let non_cursor_opening = non_cursor_terminal
            .backend()
            .buffer()
            .cell((gutter, 0))
            .unwrap();
        assert_eq!(non_cursor_opening.symbol(), opening.symbol());
        assert_eq!(non_cursor_opening.fg, opening.fg);
        assert_eq!(Some(non_cursor_opening.bg), panel.bg);
        assert!(!non_cursor_opening.modifier.contains(Modifier::UNDERLINED));
    }

    #[test]
    fn rendered_gutter_shows_source_number_and_blanks_wrapped_rows() {
        let mut session = EditorSession::from_text(
            "A paragraph with enough words to wrap across several narrow rows.\n",
        );
        let mut terminal = Terminal::new(TestBackend::new(24, 5)).unwrap();
        terminal
            .draw(|frame| {
                render_rendered(
                    frame,
                    &mut session,
                    0,
                    false,
                    frame.area(),
                    &DEFAULT_DARK,
                    Tier::TrueColor,
                );
            })
            .unwrap();
        let buffer = terminal.backend().buffer();
        assert_eq!(buffer.cell((0, 0)).unwrap().symbol(), " ");
        assert_eq!(buffer.cell((3, 0)).unwrap().symbol(), "1");
        assert_eq!(buffer.cell((3, 1)).unwrap().symbol(), " ");
    }

    #[test]
    fn rendered_gutter_shows_multi_digit_source_numbers() {
        let text = (1..=12)
            .map(|line| format!("# heading {line}\n"))
            .collect::<String>();
        let mut session = EditorSession::from_text(&text);
        let mut terminal = Terminal::new(TestBackend::new(40, 30)).unwrap();
        terminal
            .draw(|frame| {
                render_rendered(
                    frame,
                    &mut session,
                    0,
                    false,
                    frame.area(),
                    &DEFAULT_DARK,
                    Tier::TrueColor,
                );
            })
            .unwrap();

        let gutter_width = status_bar::gutter_width(session.line_count());
        let buffer = terminal.backend().buffer();
        assert!((0..30).any(|row| {
            (0..gutter_width)
                .filter_map(|col| buffer.cell((col as u16, row)))
                .map(|cell| cell.symbol())
                .collect::<String>()
                .contains("10")
        }));
    }
}

// ── Tests ───────────────────────────────────────────────────────────────────
