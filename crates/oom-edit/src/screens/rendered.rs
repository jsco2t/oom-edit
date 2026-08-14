//! Rendered Normal/Select screen.
//!
//! The core owns layout, source mapping, and Select metadata. This adapter
//! reserves and paints the source gutter, maps semantic spans through the
//! theme, and layers the subdued Normal cursor or distinct Select carrier.
//!
//! See plan §6.3, VN-1, VN-3.

use oom_edit_core::{EditorSession, RenderedLineRole};
use ratatui::buffer::CellWidth;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::theme::{Theme, Tier, UiSlot};
use crate::widgets::spans;
use crate::widgets::status_bar;

fn line_surface(theme: &Theme, tier: Tier, role: RenderedLineRole) -> Option<Style> {
    match role {
        RenderedLineRole::Document => None,
        RenderedLineRole::Metadata => Some(theme.ui_style(tier, UiSlot::MetadataPanel)),
        RenderedLineRole::CodeFence => Some(theme.ui_style(tier, UiSlot::CodeFence)),
    }
}

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
    let search = session.rendered_search().cloned();
    let max_lines = session
        .rendered_layout()
        .expect("rendered layout was built for this frame")
        .lines
        .len();
    if max_lines == 0 {
        return;
    }

    // Compute visible line range.
    let rendered_top = rendered_top.min(max_lines.saturating_sub(1));
    let rendered_bottom = (rendered_top + height).min(max_lines);
    let decorations = session.diagnostic_decoration_rows(rendered_top..rendered_bottom);
    let layout = session
        .rendered_layout()
        .expect("rendered layout was built for this frame");

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
        let base_surface = line_surface(theme, tier, rendered_line.role);
        if let Some(style) = base_surface {
            spans = build_highlighted_line(spans, text_width, style).spans;
        }
        for decoration in decorations.iter().filter(|decoration| decoration.row == i) {
            let style = theme.decoration_style(tier, decoration.kind);
            spans =
                spans::apply_interval_style(spans, decoration.columns.clone(), style, None).spans;
        }
        if let Some(search) = &search {
            let search_style = theme.style(tier, oom_edit_core::SemanticStyle::Match);
            for start in search.find_matches(&rendered_line.styled.text) {
                let end = start + search.pattern.len();
                let start_column = Line::from(&rendered_line.styled.text[..start]).width();
                let end_column = Line::from(&rendered_line.styled.text[..end]).width();
                spans = spans::apply_interval_style(
                    spans,
                    start_column..end_column,
                    search_style,
                    None,
                )
                .spans;
            }
        }

        let selected = selection
            .as_ref()
            .and_then(|selection| selection.rows.iter().find(|row| row.row == i))
            .map(|row| row.columns.clone())
            .filter(|columns| columns.start < columns.end);
        if let Some(columns) = selected {
            let mut style = theme.style(tier, oom_edit_core::SemanticStyle::Selection);
            if base_surface.is_some() {
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
    let text_width = spans
        .iter()
        .flat_map(|span| span.styled_graphemes(Style::default()))
        .map(|grapheme| usize::from(grapheme.symbol.cell_width()))
        .sum::<usize>();

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
    use crate::theme::{get_theme, Tier, DEFAULT_DARK, DEFAULT_LIGHT};
    use oom_edit_core::{KeyCode, KeyCodeKind, KeyInput, Modifiers, SemanticStyle};
    use oom_spell::{BuildProgress, SpellEngine, SpellEngineBuilder};
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

    fn spell_engine() -> SpellEngine {
        let mut builder = SpellEngineBuilder::new(vec!["known\n".to_string()]);
        for _ in 0..100 {
            if builder.step(4096) == BuildProgress::Complete {
                return builder.finish().unwrap();
            }
        }
        panic!("test spell engine failed to finish within 100 steps");
    }

    fn drain_spell(session: &mut EditorSession, engine: &SpellEngine) {
        for _ in 0..10_000 {
            if !session.diagnostics_pending() {
                return;
            }
            assert!(session.spell_tick(engine, 8));
        }
        panic!("test spell scan failed to finish within 10,000 ticks");
    }

    fn misspelled_session() -> EditorSession {
        let engine = spell_engine();
        let mut session = EditorSession::from_text("# misspelledd\n");
        drain_spell(&mut session, &engine);
        session
    }

    #[test]
    fn spell_decorations_reach_every_rendered_theme_tier_and_public_mode_path() {
        for name in crate::theme::built_in_themes() {
            for tier in [Tier::TrueColor, Tier::Color16, Tier::Monochrome] {
                let theme = get_theme(name);
                let mut session = misspelled_session();
                session.render_layout(34);
                let decoration = session.diagnostic_decoration_rows(0..usize::MAX)[0].clone();
                let mut terminal = Terminal::new(TestBackend::new(40, 2)).unwrap();
                terminal
                    .draw(|frame| {
                        render_rendered(frame, &mut session, 0, false, frame.area(), theme, tier);
                    })
                    .unwrap();
                let gutter = (status_bar::gutter_width(session.line_count()) as u16).max(4);
                let cell = terminal
                    .backend()
                    .buffer()
                    .cell((
                        gutter + decoration.columns.start as u16,
                        decoration.row as u16,
                    ))
                    .unwrap();
                assert!(cell.modifier.contains(Modifier::UNDERLINED));
                assert!(cell.modifier.contains(Modifier::ITALIC));
                assert!(cell.modifier.contains(Modifier::BOLD));
                let decoration_style = theme.decoration_style(tier, decoration.kind);
                if let Some(foreground) = decoration_style.fg {
                    assert_eq!(cell.fg, foreground);
                }
                let neighbor = terminal
                    .backend()
                    .buffer()
                    .cell((
                        gutter + decoration.columns.end as u16,
                        decoration.row as u16,
                    ))
                    .unwrap();
                assert!(!neighbor.modifier.contains(Modifier::ITALIC));
            }
        }

        for enter_mode in [Some('v'), Some(':'), None] {
            let mut session = misspelled_session();
            session.render_layout(34);
            session.handle_key(key('/'));
            for character in "misspelledd".chars() {
                session.handle_key(key(character));
            }
            session.handle_key(KeyInput {
                code: KeyCode {
                    kind: KeyCodeKind::Enter,
                },
                mods: Modifiers::default(),
            });
            if let Some(character) = enter_mode {
                session.handle_key(key(character));
            }
            let decoration = session.diagnostic_decoration_rows(0..usize::MAX)[0].clone();
            let mut terminal = Terminal::new(TestBackend::new(40, 2)).unwrap();
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
            let gutter = (status_bar::gutter_width(session.line_count()) as u16).max(4);
            let cell = terminal
                .backend()
                .buffer()
                .cell((
                    gutter + decoration.columns.start as u16,
                    decoration.row as u16,
                ))
                .unwrap();
            assert!(cell.modifier.contains(Modifier::UNDERLINED));
            assert!(cell.modifier.contains(Modifier::ITALIC));
            assert_eq!(
                Some(cell.fg),
                DEFAULT_DARK.style(Tier::TrueColor, SemanticStyle::Match).fg
            );
            if enter_mode == Some('v') {
                assert!(cell.modifier.contains(Modifier::REVERSED));
            } else {
                assert_eq!(
                    Some(cell.bg),
                    DEFAULT_DARK
                        .ui_style(Tier::TrueColor, UiSlot::CursorLine)
                        .bg
                );
            }
        }
    }

    #[test]
    fn rendered_compositor_uses_display_cells_for_wide_and_wrapped_diagnostics() {
        let engine = spell_engine();
        let mut wide = EditorSession::from_text("東京 wrng known\n");
        drain_spell(&mut wide, &engine);
        let mut wide_terminal = Terminal::new(TestBackend::new(20, 2)).unwrap();
        wide_terminal
            .draw(|frame| {
                render_rendered(
                    frame,
                    &mut wide,
                    0,
                    false,
                    frame.area(),
                    &DEFAULT_DARK,
                    Tier::TrueColor,
                );
            })
            .unwrap();
        let gutter = (status_bar::gutter_width(wide.line_count()) as u16).max(4);
        for column in 5..9 {
            let cell = wide_terminal
                .backend()
                .buffer()
                .cell((gutter + column, 0))
                .unwrap();
            assert!(cell.modifier.contains(Modifier::UNDERLINED));
        }
        for column in [4, 9] {
            let cell = wide_terminal
                .backend()
                .buffer()
                .cell((gutter + column, 0))
                .unwrap();
            assert!(!cell.modifier.contains(Modifier::UNDERLINED));
        }

        let mut wrapped = EditorSession::from_text("misspelledd known\n");
        drain_spell(&mut wrapped, &engine);
        let mut wrapped_terminal = Terminal::new(TestBackend::new(11, 4)).unwrap();
        wrapped_terminal
            .draw(|frame| {
                render_rendered(
                    frame,
                    &mut wrapped,
                    0,
                    false,
                    frame.area(),
                    &DEFAULT_DARK,
                    Tier::TrueColor,
                );
            })
            .unwrap();
        let gutter = (status_bar::gutter_width(wrapped.line_count()) as u16).max(4);
        assert_eq!(
            wrapped
                .diagnostic_decoration_rows(0..usize::MAX)
                .into_iter()
                .map(|row| (row.row, row.columns))
                .collect::<Vec<_>>(),
            [(0, 0..5), (1, 0..5), (2, 0..1)]
        );
        for (row, columns) in [(0, 0..5), (1, 0..5), (2, 0..1)] {
            for column in columns {
                let cell = wrapped_terminal
                    .backend()
                    .buffer()
                    .cell((gutter + column as u16, row as u16))
                    .unwrap();
                assert!(cell.modifier.contains(Modifier::UNDERLINED));
            }
        }
        let neighbor = wrapped_terminal
            .backend()
            .buffer()
            .cell((gutter + 1, 2))
            .unwrap();
        assert!(!neighbor.modifier.contains(Modifier::UNDERLINED));
    }

    #[test]
    fn diagnostic_composition_keeps_distinguishable_semantic_sibling_cells() {
        let decoration = DEFAULT_DARK.decoration_style(
            Tier::TrueColor,
            oom_edit_core::DecorationKind::Diagnostic {
                provider: oom_edit_core::DiagnosticProvider::Spell,
                severity: oom_edit_core::DiagnosticSeverity::Warning,
            },
        );
        let semantics = [
            SemanticStyle::Heading1,
            SemanticStyle::Emphasis,
            SemanticStyle::Link,
            SemanticStyle::CodeBlock,
        ];
        let lines = semantics
            .iter()
            .map(|&semantic| {
                let source = [oom_edit_core::Span {
                    start_col: 0,
                    end_col: 2,
                    style: semantic,
                }];
                let base = spans::build_spans("xx", &source, &DEFAULT_DARK, Tier::TrueColor)
                    .into_iter()
                    .map(|span| Span::styled(span.content.into_owned(), span.style))
                    .collect();
                spans::apply_interval_style(base, 0..1, decoration, None)
            })
            .collect::<Vec<_>>();
        let mut terminal = Terminal::new(TestBackend::new(2, semantics.len() as u16)).unwrap();
        terminal
            .draw(|frame| {
                frame.render_widget(ratatui::widgets::Paragraph::new(lines), frame.area());
            })
            .unwrap();

        for (row, semantic) in semantics.into_iter().enumerate() {
            let decorated = terminal.backend().buffer().cell((0, row as u16)).unwrap();
            assert!(decorated.modifier.contains(Modifier::UNDERLINED));
            assert!(decorated.modifier.contains(Modifier::ITALIC));

            let sibling = terminal.backend().buffer().cell((1, row as u16)).unwrap();
            let base_style = DEFAULT_DARK.style(Tier::TrueColor, semantic);
            let default_cell = ratatui::buffer::Cell::default();
            assert_eq!(sibling.fg, base_style.fg.unwrap_or(default_cell.fg));
            assert_eq!(sibling.bg, base_style.bg.unwrap_or(default_cell.bg));
            assert_eq!(sibling.modifier, base_style.add_modifier);
        }
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
    fn code_fence_surface_fills_width_and_preserves_syntax() {
        let text = "Before\n\n```rust\nlet message = \"hi\";\n\n\tnext();\n```\n\nAfter\n";

        for theme in [&DEFAULT_DARK, &DEFAULT_LIGHT] {
            let terminal_width = 36;
            let terminal_height = 10;
            let mut session = EditorSession::from_text(text);
            let gutter = status_bar::gutter_width(session.line_count()) as u16;
            let text_width = terminal_width - gutter;
            let (fence_rows, body_row, keyword_column, string_column, document_rows) = {
                let layout = session.render_layout(text_width);
                let fence_rows = layout
                    .lines
                    .iter()
                    .enumerate()
                    .filter(|(_, line)| line.role == RenderedLineRole::CodeFence)
                    .map(|(row, _)| row)
                    .collect::<Vec<_>>();
                let (body_row, body) = layout
                    .lines
                    .iter()
                    .enumerate()
                    .find(|(_, line)| line.styled.text.contains("let message"))
                    .expect("rendered Rust body");
                let keyword_column = body.styled.text.find("let").unwrap();
                let string_column = body.styled.text.find("\"hi\"").unwrap();
                let document_rows = layout
                    .lines
                    .iter()
                    .enumerate()
                    .filter(|(_, line)| line.role == RenderedLineRole::Document)
                    .map(|(row, _)| row)
                    .collect::<Vec<_>>();
                (
                    fence_rows,
                    body_row,
                    keyword_column,
                    string_column,
                    document_rows,
                )
            };

            assert_eq!(fence_rows.len(), 5, "{} fence boundary", theme.name);
            let mut terminal =
                Terminal::new(TestBackend::new(terminal_width, terminal_height)).unwrap();
            terminal
                .draw(|frame| {
                    render_rendered(
                        frame,
                        &mut session,
                        0,
                        false,
                        frame.area(),
                        theme,
                        Tier::TrueColor,
                    );
                })
                .unwrap();

            let buffer = terminal.backend().buffer();
            let fence_background = theme
                .ui_style(Tier::TrueColor, UiSlot::CodeFence)
                .bg
                .unwrap();
            for row in fence_rows {
                for column in 0..text_width {
                    assert_eq!(
                        buffer.cell((gutter + column, row as u16)).unwrap().bg,
                        fence_background,
                        "{} fence row {row}, column {column}",
                        theme.name
                    );
                }
            }

            assert_eq!(
                Some(
                    buffer
                        .cell((gutter + keyword_column as u16, body_row as u16))
                        .unwrap()
                        .fg
                ),
                theme.style(Tier::TrueColor, SemanticStyle::Keyword).fg
            );
            assert_eq!(
                Some(
                    buffer
                        .cell((gutter + string_column as u16, body_row as u16))
                        .unwrap()
                        .fg
                ),
                theme.style(Tier::TrueColor, SemanticStyle::StringLit).fg
            );
            for row in document_rows {
                assert_ne!(
                    buffer.cell((gutter, row as u16)).unwrap().bg,
                    fence_background,
                    "{} document row {row}",
                    theme.name
                );
                assert_ne!(
                    buffer
                        .cell((gutter + text_width - 1, row as u16))
                        .unwrap()
                        .bg,
                    fence_background,
                    "{} document padding row {row}",
                    theme.name
                );
            }
        }
    }

    #[test]
    fn code_fence_surface_composes_selection_and_cursor() {
        let text = "Before\n\n```rust\nlet value = \"ok\";\nnext();\n```\n\nAfter\n";
        let terminal_width = 40;
        let terminal_height = 10;
        let mut session = EditorSession::from_text(text);
        let gutter = status_bar::gutter_width(session.line_count()) as u16;
        let text_width = terminal_width - gutter;
        let (first_body_row, passive_fence_row, layout_len) = {
            let layout = session.render_layout(text_width);
            let first_body_row = layout
                .lines
                .iter()
                .position(|line| line.styled.text.contains("let value"))
                .unwrap();
            let passive_fence_row = layout
                .lines
                .iter()
                .position(|line| {
                    line.role == RenderedLineRole::CodeFence
                        && line.styled.text.starts_with("▏ rust")
                })
                .unwrap();
            (first_body_row, passive_fence_row, layout.lines.len())
        };

        for _ in 0..layout_len {
            if session.rendered_cursor_line() == first_body_row {
                break;
            }
            session.handle_key(key('j'));
        }
        assert_eq!(session.rendered_cursor_line(), first_body_row);
        session.handle_key(key('v'));
        session.handle_key(key('l'));
        session.handle_key(key('j'));
        let cursor_row = session.rendered_cursor_line();
        assert_ne!(cursor_row, first_body_row);
        let selected = session
            .rendered_selection()
            .unwrap()
            .rows
            .iter()
            .find(|row| row.row == first_body_row)
            .expect("first code row remains selected")
            .clone();

        let mut select_terminal =
            Terminal::new(TestBackend::new(terminal_width, terminal_height)).unwrap();
        select_terminal
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
        let selected_cell = select_terminal
            .backend()
            .buffer()
            .cell((
                gutter + selected.columns.start as u16,
                first_body_row as u16,
            ))
            .unwrap();
        assert_eq!(
            selected_cell.bg,
            DEFAULT_DARK
                .ui_style(Tier::TrueColor, UiSlot::CodeFence)
                .bg
                .unwrap()
        );
        assert!(selected_cell.modifier.contains(Modifier::REVERSED));

        session.handle_key(KeyInput {
            code: KeyCode {
                kind: KeyCodeKind::Esc,
            },
            mods: Modifiers::default(),
        });
        let active_row = session.rendered_cursor_line();
        let mut normal_terminal =
            Terminal::new(TestBackend::new(terminal_width, terminal_height)).unwrap();
        normal_terminal
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
        let buffer = normal_terminal.backend().buffer();
        assert_eq!(
            buffer.cell((gutter, active_row as u16)).unwrap().bg,
            DEFAULT_DARK
                .ui_style(Tier::TrueColor, UiSlot::CursorLine)
                .bg
                .unwrap()
        );
        assert_eq!(
            buffer.cell((gutter, passive_fence_row as u16)).unwrap().bg,
            DEFAULT_DARK
                .ui_style(Tier::TrueColor, UiSlot::CodeFence)
                .bg
                .unwrap()
        );
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
