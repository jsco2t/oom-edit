//! Rendered Normal/Select screen.
//!
//! The core owns layout, source mapping, and Select metadata. This adapter
//! reserves and paints the source gutter, maps semantic spans through the
//! theme, and layers the subdued Normal cursor or distinct Select carrier.
//!
//! See plan §6.3, VN-1, VN-3.

use oom_edit_core::{EditorSession, RenderedLine, RenderedLineRole};
use ratatui::buffer::CellWidth;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

#[cfg(test)]
use crate::gutter::GutterTroubleSnapshot;
use crate::screens::editor::{DocumentPresentation, GutterRows};
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

fn gutter_continuation_sources(
    lines: &[RenderedLine],
    line_numbers: &[Option<usize>],
) -> Vec<Option<usize>> {
    let mut source_line = None;
    lines
        .iter()
        .enumerate()
        .map(
            |(row, line)| match line_numbers.get(row).copied().flatten() {
                Some(number) => {
                    source_line = Some(number.saturating_sub(1));
                    None
                }
                None if row > 0
                    && line_numbers.get(row).is_some_and(Option::is_none)
                    && line.kind == oom_edit_core::LineKind::Content
                    && line.source == lines[row - 1].source =>
                {
                    source_line
                }
                None => None,
            },
        )
        .collect()
}

/// App-owned viewport coordinates for a rendered surface.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct RenderedViewport {
    /// First visible rendered row.
    pub(crate) top: usize,
    /// First visible rendered display column.
    pub(crate) left: usize,
}

impl RenderedViewport {
    pub(crate) const fn new(top: usize, left: usize) -> Self {
        Self { top, left }
    }
}

/// App-owned presentation settings for the rendered surface.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct RenderedSettings {
    relative_line_numbers: bool,
    cursor_visible: bool,
    wrap_width: u16,
}

impl RenderedSettings {
    pub(crate) const fn new(relative_line_numbers: bool) -> Self {
        Self {
            relative_line_numbers,
            cursor_visible: true,
            wrap_width: u16::MAX,
        }
    }

    pub(crate) const fn with_wrap_width(mut self, wrap_width: u16) -> Self {
        self.wrap_width = wrap_width;
        self
    }

    /// Control whether this screen owns the frame cursor.
    pub(crate) const fn with_cursor_visible(mut self, cursor_visible: bool) -> Self {
        self.cursor_visible = cursor_visible;
        self
    }
}

/// Render Normal, Select, or Command into the body area.
#[cfg(test)]
pub fn render_rendered(
    frame: &mut Frame<'_>,
    session: &mut EditorSession,
    viewport: RenderedViewport,
    relative_line_numbers: bool,
    area: Rect,
    theme: &Theme,
    tier: Tier,
) {
    let gutter_trouble = GutterTroubleSnapshot::default();
    render_rendered_with_settings(
        frame,
        session,
        viewport,
        RenderedSettings::new(relative_line_numbers),
        area,
        DocumentPresentation::new(theme, tier, &gutter_trouble),
    );
}

/// Render with explicit rendered-surface presentation settings.
pub(crate) fn render_rendered_with_settings(
    frame: &mut Frame<'_>,
    session: &mut EditorSession,
    viewport: RenderedViewport,
    settings: RenderedSettings,
    area: Rect,
    presentation: DocumentPresentation<'_>,
) {
    let theme = presentation.theme;
    let tier = presentation.tier;
    let height = area.height.max(1) as usize;
    frame.render_widget(
        Block::default().style(theme.ui_style(tier, UiSlot::DocumentBody)),
        area,
    );
    let mode = session.mode();
    let source_cursor_line = session.cursor().0;
    let gutter_width =
        (status_bar::gutter_width(session.line_count(), settings.relative_line_numbers) as u16)
            .min(area.width);
    let text_width = area.width.saturating_sub(gutter_width);
    let surface_width = viewport
        .left
        .saturating_add(usize::from(text_width))
        .min(usize::from(u16::MAX)) as u16;
    session.render_layout(text_width.min(settings.wrap_width));
    let cursor = session.rendered_cursor();
    let cursor_line = cursor.row;
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
    let rendered_top = viewport.top.min(max_lines.saturating_sub(1));
    let rendered_bottom = (rendered_top + height).min(max_lines);
    let decorations = session.diagnostic_decoration_rows(rendered_top..rendered_bottom);
    let layout = session
        .rendered_layout()
        .expect("rendered layout was built for this frame");
    let gutter_continuations = gutter_continuation_sources(&layout.lines, &layout.line_numbers);

    if gutter_width > 0 {
        let gutter_area = Rect::new(area.x, area.y, gutter_width, area.height);
        super::editor::render_gutter(
            frame,
            mode,
            source_cursor_line,
            GutterRows::new(
                &layout.line_numbers[rendered_top..rendered_bottom],
                &gutter_continuations[rendered_top..rendered_bottom],
            ),
            settings.relative_line_numbers,
            gutter_area,
            presentation,
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
            spans = build_highlighted_line(spans, surface_width, style).spans;
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
            .filter(|columns| columns.iter().any(|columns| !columns.is_empty()));
        let mut line = if let Some(columns) = selected {
            let mut style = theme.style(tier, oom_edit_core::SemanticStyle::Selection);
            if base_surface.is_some() {
                style.bg = None;
            }
            let mut selected_line = Line::from(spans);
            for columns in columns {
                selected_line =
                    build_interval_highlighted_line(selected_line.spans, columns, style);
            }
            selected_line
        } else if i == cursor_line {
            let mut style = theme.ui_style(tier, UiSlot::CursorLine);
            if rendered_line.role == oom_edit_core::RenderedLineRole::Metadata {
                style = style.add_modifier(ratatui::style::Modifier::UNDERLINED);
            }
            build_highlighted_line(spans, surface_width, style)
        } else {
            Line::from(spans)
        };

        if mode == oom_edit_core::Mode::Normal && i == cursor.row {
            line = spans::apply_interval_style(
                line.spans,
                cursor.column..cursor.column.saturating_add(1),
                theme.ui_style(tier, UiSlot::NormalCursor),
                None,
            );
        }
        lines.push(crop_line(line, viewport.left, usize::from(text_width)));
    }

    // Fill remaining lines with blanks.
    while lines.len() < height {
        lines.push(Line::from(""));
    }

    // Truncate to exactly viewport height.
    lines.truncate(height);

    let paragraph = Paragraph::new(lines).block(Block::default().borders(Borders::NONE));
    frame.render_widget(paragraph, text_area);

    let cursor_row_visible = cursor.row >= rendered_top && cursor.row < rendered_bottom;
    let cursor_column_visible = cursor.column >= viewport.left
        && cursor.column < viewport.left.saturating_add(usize::from(text_width));
    if settings.cursor_visible
        && text_area.width > 0
        && text_area.height > 0
        && cursor_row_visible
        && cursor_column_visible
    {
        let row = text_area.y + (cursor.row - rendered_top) as u16;
        let column = text_area.x + (cursor.column - viewport.left) as u16;
        frame.set_cursor_position(ratatui::layout::Position::new(column, row));
    }
}

/// Crop one styled line in display cells without rendering partial glyphs.
fn crop_line<'a>(line: Line<'a>, left: usize, width: usize) -> Line<'a> {
    if width == 0 {
        return Line::default();
    }

    let right = left.saturating_add(width);
    let mut column = 0usize;
    let mut visible = Vec::new();
    for span in line.spans {
        for grapheme in span.styled_graphemes(Style::default()) {
            let grapheme_width = usize::from(grapheme.symbol.cell_width());
            let columns = column..column.saturating_add(grapheme_width);
            column = columns.end;

            if grapheme_width == 0 || columns.end <= left || columns.start >= right {
                continue;
            }

            if columns.start >= left && columns.end <= right {
                visible.push(Span::styled(grapheme.symbol.to_string(), grapheme.style));
            } else {
                let clipped_width = columns.end.min(right) - columns.start.max(left);
                visible.push(Span::styled(" ".repeat(clipped_width), grapheme.style));
            }
        }
    }
    Line::from(visible)
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
    use oom_edit_core::{
        DecorationKind, DiagnosticProvider, DiagnosticSeverity, KeyCode, KeyCodeKind, KeyInput,
        Modifiers, SemanticStyle,
    };
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
    fn rendered_markers_follow_numbered_rows_across_synthetic_rows_and_hscroll() {
        let theme = get_theme("catppuccin-mocha");
        let snapshot = GutterTroubleSnapshot::testing(&[
            (0, DiagnosticSeverity::Warning),
            (1, DiagnosticSeverity::Warning),
            (2, DiagnosticSeverity::Warning),
        ]);
        let source =
            "| a very wide heading | another wide heading |\n| --- | --- |\n| value | value |\n";
        let mut session = EditorSession::from_text(source);

        for left in [0, 5] {
            let mut terminal = Terminal::new(TestBackend::new(18, 12)).unwrap();
            terminal
                .draw(|frame| {
                    render_rendered_with_settings(
                        frame,
                        &mut session,
                        RenderedViewport::new(0, left),
                        RenderedSettings::new(false),
                        frame.area(),
                        DocumentPresentation::new(theme, Tier::TrueColor, &snapshot),
                    );
                })
                .unwrap();
            let line_numbers = session
                .rendered_layout()
                .unwrap()
                .line_numbers
                .iter()
                .take(12)
                .copied()
                .collect::<Vec<_>>();
            let layout_height = session.rendered_layout().unwrap().lines.len().min(12);
            assert!(line_numbers.iter().any(Option::is_none));
            let buffer = terminal.backend().buffer();
            let gutter_background = theme
                .ui_style(Tier::TrueColor, UiSlot::GutterBackground)
                .bg
                .unwrap();
            let body_background = theme
                .ui_style(Tier::TrueColor, UiSlot::DocumentBody)
                .bg
                .unwrap();
            let mut markers = 0;
            let gutter_width = status_bar::gutter_width(session.line_count(), false) as u16;
            for row in 0..12 {
                let expected = if line_numbers.get(row).copied().flatten().is_some() {
                    markers += 1;
                    "•"
                } else {
                    " "
                };
                assert_eq!(buffer.cell((0, row as u16)).unwrap().symbol(), expected);
                for column in 0..gutter_width {
                    assert_eq!(
                        buffer.cell((column, row as u16)).unwrap().bg,
                        gutter_background
                    );
                }
            }
            for row in layout_height..12 {
                assert_eq!(buffer.cell((17, row as u16)).unwrap().bg, body_background);
            }
            assert_eq!(markers, 2);
        }
    }

    #[test]
    fn display_cell_crop_preserves_styles_and_never_draws_half_a_wide_glyph() {
        let wide_style = Style::default().add_modifier(Modifier::BOLD);
        let combining_style = Style::default().add_modifier(Modifier::ITALIC);
        let line = Line::from(vec![
            Span::raw("ab"),
            Span::styled("東京", wide_style),
            Span::styled("e\u{301}z", combining_style),
        ]);

        let cropped = crop_line(line, 3, 4);
        let text = cropped
            .spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect::<String>();
        assert_eq!(text, " 京e\u{301}");
        assert_eq!(Line::from(cropped.spans.clone()).width(), 4);
        assert!(!text.contains('東'));
        assert!(cropped.spans[1].style.add_modifier.contains(Modifier::BOLD));
        assert!(cropped.spans[2]
            .style
            .add_modifier
            .contains(Modifier::ITALIC));

        let right_clipped = crop_line(Line::from("a東"), 0, 2);
        assert_eq!(
            right_clipped
                .spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect::<String>(),
            "a "
        );
    }

    #[test]
    fn crop_preserves_every_composited_rendered_style_layer() {
        let theme = &DEFAULT_DARK;
        let tier = Tier::TrueColor;
        let expected = vec![
            theme.style(tier, SemanticStyle::Link),
            theme.decoration_style(
                tier,
                DecorationKind::Diagnostic {
                    provider: DiagnosticProvider::Spell,
                    severity: DiagnosticSeverity::Warning,
                },
            ),
            theme.style(tier, SemanticStyle::Match),
            theme.style(tier, SemanticStyle::Selection),
            theme.ui_style(tier, UiSlot::CursorLine),
            theme.ui_style(tier, UiSlot::NormalCursor),
        ];
        let mut spans = vec![Span::raw("prefix")];
        spans.extend(
            expected
                .iter()
                .copied()
                .map(|style| Span::styled("x", style)),
        );

        let cropped = crop_line(Line::from(spans), 6, expected.len());
        assert_eq!(
            cropped
                .spans
                .iter()
                .map(|span| span.style)
                .collect::<Vec<_>>(),
            expected
        );
    }

    #[test]
    fn rendered_horizontal_crop_keeps_the_gutter_fixed() {
        let text = concat!(
            "| first naturally wide column | second naturally wide column | third naturally wide column |\n",
            "|---|---|---|\n",
            "| repeated content repeated content | 東京東京東京東京 | final repeated content repeated content |",
        );
        let render = |left| {
            let mut session = EditorSession::from_text(text);
            let mut terminal = Terminal::new(TestBackend::new(36, 8)).unwrap();
            terminal
                .draw(|frame| {
                    render_rendered(
                        frame,
                        &mut session,
                        RenderedViewport::new(0, left),
                        false,
                        frame.area(),
                        &DEFAULT_DARK,
                        Tier::TrueColor,
                    );
                })
                .unwrap();
            terminal.backend().buffer().clone()
        };

        let unscrolled = render(0);
        let scrolled = render(12);
        let gutter_width = status_bar::gutter_width(1, false) as u16;
        for y in 0..8 {
            for x in 0..gutter_width {
                assert_eq!(unscrolled[(x, y)], scrolled[(x, y)]);
            }
        }
        let content = |buffer: &ratatui::buffer::Buffer| {
            (0..8)
                .flat_map(|y| (gutter_width..36).map(move |x| buffer[(x, y)].symbol().to_string()))
                .collect::<Vec<_>>()
        };
        assert_ne!(content(&unscrolled), content(&scrolled));
    }

    #[test]
    fn rendered_normal_and_select_place_real_cursor_and_keep_painted_carriers() {
        for select in [false, true] {
            let mut session = EditorSession::from_text("abcdefghij");
            session.render_layout(16);
            for _ in 0..6 {
                session.handle_key(key('l'));
            }
            if select {
                session.handle_key(key('v'));
            }
            let mut terminal = Terminal::new(TestBackend::new(20, 2)).unwrap();
            terminal
                .draw(|frame| {
                    render_rendered(
                        frame,
                        &mut session,
                        RenderedViewport::new(0, 4),
                        false,
                        frame.area(),
                        &DEFAULT_DARK,
                        Tier::TrueColor,
                    );
                })
                .unwrap();

            let active = session.rendered_cursor();
            assert_eq!(active.row, 0);
            assert!(active.column >= 4);
            let gutter = status_bar::gutter_width(session.line_count(), false) as u16;
            let position = ratatui::layout::Position::new(
                gutter + (active.column - 4) as u16,
                active.row as u16,
            );
            assert_eq!(
                terminal.backend().cursor_position(),
                position,
                "select={select}, active={active:?}, selection={:?}",
                session.rendered_selection()
            );
            let cell = terminal.backend().buffer().cell(position).unwrap();
            if select {
                assert!(cell.modifier.contains(Modifier::REVERSED));
            } else {
                assert_eq!(
                    Some(cell.bg),
                    DEFAULT_DARK
                        .ui_style(Tier::TrueColor, UiSlot::NormalCursor)
                        .bg
                );
            }
        }
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
                        render_rendered(
                            frame,
                            &mut session,
                            RenderedViewport::new(0, 0),
                            false,
                            frame.area(),
                            theme,
                            tier,
                        );
                    })
                    .unwrap();
                let gutter = status_bar::gutter_width(session.line_count(), false) as u16;
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
                        RenderedViewport::new(0, 0),
                        false,
                        frame.area(),
                        &DEFAULT_DARK,
                        Tier::TrueColor,
                    );
                })
                .unwrap();
            let gutter = status_bar::gutter_width(session.line_count(), false) as u16;
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
            } else if enter_mode.is_none() {
                assert_eq!(
                    Some(cell.bg),
                    DEFAULT_DARK
                        .ui_style(Tier::TrueColor, UiSlot::NormalCursor)
                        .bg
                );
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
                    RenderedViewport::new(0, 0),
                    false,
                    frame.area(),
                    &DEFAULT_DARK,
                    Tier::TrueColor,
                );
            })
            .unwrap();
        let gutter = status_bar::gutter_width(wide.line_count(), false) as u16;
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
                    RenderedViewport::new(0, 0),
                    false,
                    frame.area(),
                    &DEFAULT_DARK,
                    Tier::TrueColor,
                );
            })
            .unwrap();
        let gutter = status_bar::gutter_width(wrapped.line_count(), false) as u16;
        assert_eq!(
            wrapped
                .diagnostic_decoration_rows(0..usize::MAX)
                .into_iter()
                .map(|row| (row.row, row.columns))
                .collect::<Vec<_>>(),
            [(0, 0..8), (1, 0..3)]
        );
        for (row, columns) in [(0, 0..8), (1, 0..3)] {
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
            .cell((gutter + 5, 1))
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
    fn table_body_boundary_dashes_use_muted_style_in_every_theme_tier() {
        let text = "| Header | Value |\n| --- | --- |\n| first | row |\n| second | row |\n";
        for name in crate::theme::built_in_themes() {
            for tier in [Tier::TrueColor, Tier::Color16, Tier::Monochrome] {
                let theme = get_theme(name);
                let mut session = EditorSession::from_text(text);
                let mut terminal = Terminal::new(TestBackend::new(40, 10)).unwrap();
                terminal
                    .draw(|frame| {
                        render_rendered(
                            frame,
                            &mut session,
                            RenderedViewport::new(0, 0),
                            false,
                            frame.area(),
                            theme,
                            tier,
                        );
                    })
                    .unwrap();
                let boundary_row = session
                    .rendered_layout()
                    .unwrap()
                    .lines
                    .iter()
                    .position(|line| line.styled.text.starts_with("│-"))
                    .unwrap();
                let gutter = status_bar::gutter_width(session.line_count(), false) as u16;
                let dash = terminal
                    .backend()
                    .buffer()
                    .cell((gutter + 1, boundary_row as u16))
                    .unwrap();
                assert_eq!(dash.symbol(), "-");
                assert!(
                    dash.modifier.contains(Modifier::DIM),
                    "theme={name}, tier={tier:?}"
                );
            }
        }
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
                    RenderedViewport::new(0, 0),
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
                    RenderedViewport::new(0, 0),
                    false,
                    frame.area(),
                    &DEFAULT_DARK,
                    Tier::TrueColor,
                );
            })
            .unwrap();

        let gutter_width = status_bar::gutter_width(normal.line_count(), false) as u16;
        let normal_line_cell = normal_terminal
            .backend()
            .buffer()
            .cell((gutter_width, 0))
            .unwrap();
        let normal_cursor = normal.rendered_cursor();
        let normal_cursor_cell = normal_terminal
            .backend()
            .buffer()
            .cell((
                gutter_width + normal_cursor.column as u16,
                normal_cursor.row as u16,
            ))
            .unwrap();
        let select_cell = select_terminal
            .backend()
            .buffer()
            .cell((gutter_width + 2, 0))
            .unwrap();
        assert_eq!(
            normal_line_cell.bg,
            DEFAULT_DARK
                .ui_style(Tier::TrueColor, UiSlot::CursorLine)
                .bg
                .unwrap()
        );
        assert_eq!(
            normal_cursor_cell.bg,
            DEFAULT_DARK
                .ui_style(Tier::TrueColor, UiSlot::NormalCursor)
                .bg
                .unwrap()
        );
        assert_ne!(normal_cursor_cell.bg, normal_line_cell.bg);
        assert!(!normal_line_cell.modifier.contains(Modifier::REVERSED));
        assert!(select_cell.modifier.contains(Modifier::REVERSED));
        assert_ne!(normal_line_cell.bg, select_cell.bg);
    }

    #[test]
    fn normal_cursor_cell_is_subtle_compositional_and_accessible() {
        for name in crate::theme::built_in_themes() {
            for tier in [Tier::TrueColor, Tier::Color16, Tier::Monochrome] {
                let theme = crate::theme::get_theme(name);
                let mut session = EditorSession::from_text("plain text\n");
                session.render_layout(36);
                session.handle_key(key('l'));
                let cursor = session.rendered_cursor();
                let mut terminal = Terminal::new(TestBackend::new(40, 3)).unwrap();
                terminal
                    .draw(|frame| {
                        render_rendered(
                            frame,
                            &mut session,
                            RenderedViewport::new(0, 0),
                            false,
                            frame.area(),
                            theme,
                            tier,
                        );
                    })
                    .unwrap();

                let gutter = status_bar::gutter_width(session.line_count(), false) as u16;
                let cursor_cell = terminal
                    .backend()
                    .buffer()
                    .cell((gutter + cursor.column as u16, cursor.row as u16))
                    .unwrap();
                let neighbor = terminal
                    .backend()
                    .buffer()
                    .cell((gutter + cursor.column as u16 + 1, cursor.row as u16))
                    .unwrap();
                let cursor_style = theme.ui_style(tier, UiSlot::NormalCursor);
                let line_style = theme.ui_style(tier, UiSlot::CursorLine);

                assert!(cursor_cell.modifier.contains(Modifier::BOLD));
                assert_eq!(
                    cursor_cell.fg,
                    theme
                        .style(tier, oom_edit_core::SemanticStyle::Text)
                        .fg
                        .unwrap()
                );
                if let Some(background) = cursor_style.bg {
                    assert_eq!(cursor_cell.bg, background);
                    assert_eq!(neighbor.bg, line_style.bg.unwrap());
                    assert_ne!(cursor_cell.bg, neighbor.bg);
                } else {
                    assert!(cursor_cell.modifier.contains(Modifier::UNDERLINED));
                    assert!(!neighbor.modifier.contains(Modifier::BOLD));
                }
            }
        }
    }

    #[test]
    fn character_selection_highlights_only_selected_atoms() {
        let mut session = EditorSession::from_text("# Heading\n");
        session.render_layout(36);
        session.handle_key(key('v'));
        session.handle_key(key('l'));
        let interval = session.rendered_selection().unwrap().rows[0]
            .columns
            .first()
            .cloned()
            .unwrap();
        let mut terminal = Terminal::new(TestBackend::new(40, 3)).unwrap();
        terminal
            .draw(|frame| {
                render_rendered(
                    frame,
                    &mut session,
                    RenderedViewport::new(0, 0),
                    false,
                    frame.area(),
                    &DEFAULT_DARK,
                    Tier::TrueColor,
                );
            })
            .unwrap();
        let gutter = status_bar::gutter_width(session.line_count(), false) as u16;
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
    fn table_selection_paints_each_interval_without_painting_separators() {
        let mut session =
            EditorSession::from_text("| Left | Right |\n| --- | --- |\n| AB | CD |\n");
        let body_row = session
            .render_layout(80)
            .lines
            .iter()
            .position(|line| line.styled.text.contains(" AB "))
            .unwrap();
        while session.rendered_cursor_line() < body_row {
            session.handle_key(key('j'));
        }
        session.handle_key(key('v'));
        session.handle_key(key('l'));
        session.handle_key(key('l'));
        let selected = session.rendered_selection().unwrap().rows[0].clone();
        assert_eq!(selected.row, body_row);
        assert_eq!(selected.columns.len(), 2);

        let mut terminal = Terminal::new(TestBackend::new(90, 6)).unwrap();
        terminal
            .draw(|frame| {
                render_rendered(
                    frame,
                    &mut session,
                    RenderedViewport::new(0, 0),
                    false,
                    frame.area(),
                    &DEFAULT_DARK,
                    Tier::TrueColor,
                );
            })
            .unwrap();
        let gutter = status_bar::gutter_width(session.line_count(), false) as u16;
        for interval in &selected.columns {
            for column in interval.clone() {
                assert!(terminal
                    .backend()
                    .buffer()
                    .cell((gutter + column as u16, body_row as u16))
                    .unwrap()
                    .modifier
                    .contains(Modifier::REVERSED));
            }
        }
        let separator = selected.columns[0].end;
        assert!(separator < selected.columns[1].start);
        assert!(!terminal
            .backend()
            .buffer()
            .cell((gutter + separator as u16, body_row as u16))
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
            .all(|row| row.columns.len() == 1 && row.columns[0] == (0..2)));

        let mut terminal = Terminal::new(TestBackend::new(40, 5)).unwrap();
        terminal
            .draw(|frame| {
                render_rendered(
                    frame,
                    &mut session,
                    RenderedViewport::new(0, 0),
                    false,
                    frame.area(),
                    &DEFAULT_DARK,
                    Tier::TrueColor,
                );
            })
            .unwrap();
        let gutter = status_bar::gutter_width(session.line_count(), false) as u16;
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
                    RenderedViewport::new(0, 0),
                    false,
                    frame.area(),
                    &DEFAULT_DARK,
                    Tier::TrueColor,
                );
            })
            .unwrap();
        let selection = session.rendered_selection().unwrap();
        let gutter = status_bar::gutter_width(session.line_count(), false) as u16;
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
                    RenderedViewport::new(0, 0),
                    false,
                    frame.area(),
                    &DEFAULT_DARK,
                    Tier::TrueColor,
                );
            })
            .unwrap();
        let gutter = status_bar::gutter_width(session.line_count(), false) as u16;
        let cell = terminal
            .backend()
            .buffer()
            .cell((
                gutter + selected.columns[0].start as u16,
                selected.row as u16,
            ))
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
                    RenderedViewport::new(0, 0),
                    false,
                    frame.area(),
                    &DEFAULT_DARK,
                    Tier::TrueColor,
                );
            })
            .unwrap();

        let gutter = status_bar::gutter_width(session.line_count(), false) as u16;
        let buffer = terminal.backend().buffer();
        let panel = DEFAULT_DARK.ui_style(Tier::TrueColor, UiSlot::MetadataPanel);
        let cursor_line = DEFAULT_DARK.ui_style(Tier::TrueColor, UiSlot::CursorLine);
        let normal_cursor = DEFAULT_DARK.ui_style(Tier::TrueColor, UiSlot::NormalCursor);
        let delimiter = DEFAULT_DARK.style(Tier::TrueColor, SemanticStyle::FmDelimiter);
        let key_style = DEFAULT_DARK.style(Tier::TrueColor, SemanticStyle::FmKey);
        let value_style = DEFAULT_DARK.style(Tier::TrueColor, SemanticStyle::FmValue);
        let text_style = DEFAULT_DARK.style(Tier::TrueColor, SemanticStyle::Text);

        let opening = buffer.cell((gutter, 0)).unwrap();
        assert_eq!(opening.symbol(), "┌");
        assert_eq!(Some(opening.fg), delimiter.fg);
        assert_eq!(Some(opening.bg), normal_cursor.bg);
        assert!(opening.modifier.contains(cursor_line.add_modifier));
        assert!(opening.modifier.contains(normal_cursor.add_modifier));
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
                    RenderedViewport::new(0, 0),
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
            let gutter = status_bar::gutter_width(session.line_count(), false) as u16;
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
                        RenderedViewport::new(0, 0),
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
        let gutter = status_bar::gutter_width(session.line_count(), false) as u16;
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
                    RenderedViewport::new(0, 0),
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
                gutter + selected.columns[0].start as u16,
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
                    RenderedViewport::new(0, 0),
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
    fn rendered_gutter_continuation_marks_only_width_wrapped_rows() {
        for relative in [false, true] {
            let mut session = EditorSession::from_text(
                "# Head\nA paragraph with enough words to wrap across several narrow rows.\n\n# Next\n",
            );
            let mut terminal = Terminal::new(TestBackend::new(24, 10)).unwrap();
            terminal
                .draw(|frame| {
                    render_rendered(
                        frame,
                        &mut session,
                        RenderedViewport::new(0, 4),
                        relative,
                        frame.area(),
                        &DEFAULT_DARK,
                        Tier::TrueColor,
                    );
                })
                .unwrap();
            let layout = session.rendered_layout().unwrap();
            let buffer = terminal.backend().buffer();
            let marker_column = status_bar::gutter_width(session.line_count(), relative) - 2;
            let continuations = gutter_continuation_sources(&layout.lines, &layout.line_numbers);
            assert!(layout
                .lines
                .iter()
                .any(|line| line.kind == oom_edit_core::LineKind::Synthetic));
            assert!(layout
                .lines
                .iter()
                .zip(&layout.line_numbers)
                .any(
                    |(line, number)| line.kind == oom_edit_core::LineKind::Content
                        && line.styled.text.is_empty()
                        && number.is_some()
                ));
            for (row, (_line, number)) in layout.lines.iter().zip(&layout.line_numbers).enumerate()
            {
                let expected = if continuations[row].is_some() {
                    "↳"
                } else if number.is_some() {
                    continue;
                } else {
                    " "
                };
                assert_eq!(
                    buffer
                        .cell((marker_column as u16, row as u16))
                        .unwrap()
                        .symbol(),
                    expected
                );
            }
            for row in layout.lines.len()..10 {
                assert_eq!(
                    buffer
                        .cell((marker_column as u16, row as u16))
                        .unwrap()
                        .symbol(),
                    " "
                );
            }
        }
    }

    #[test]
    fn gutter_continuation_classification_covers_rendered_surfaces() {
        let long = "alpha beta gamma delta epsilon zeta eta theta iota kappa lambda";
        let cases = [
            long.to_string(),
            format!("- {long}\n"),
            format!("> {long}\n"),
            format!("---\ntitle: {long}\n---\n"),
            format!("| value |\n| --- |\n| {long} {long} |\n"),
        ];

        for source in cases {
            let mut session = EditorSession::from_text(&source);
            let layout = session.render_layout(18);
            let continuations = gutter_continuation_sources(&layout.lines, &layout.line_numbers);
            assert!(
                continuations.iter().any(Option::is_some),
                "expected a width continuation for {source:?}"
            );
            for (row, continuation) in continuations.iter().copied().enumerate() {
                if continuation.is_some() {
                    assert_eq!(layout.lines[row].kind, oom_edit_core::LineKind::Content);
                    assert_eq!(layout.lines[row].source, layout.lines[row - 1].source);
                    assert_eq!(layout.line_numbers[row], None);
                }
            }
        }
    }

    #[test]
    fn rendered_blank_line_gutter_and_status_report_its_physical_position() {
        let text = "alpha beta gamma delta epsilon zeta eta theta 東京\n\n# next\n";
        let mut session = EditorSession::from_text(text);
        let layout = session.render_layout(12).clone();
        let blank_row = layout
            .lines
            .iter()
            .position(|line| line.styled.text.is_empty())
            .expect("physical blank line must have a rendered row");
        for _ in 0..blank_row {
            session.handle_key(key('j'));
        }

        for relative in [false, true] {
            let mut terminal = Terminal::new(TestBackend::new(40, 8)).unwrap();
            terminal
                .draw(|frame| {
                    render_rendered(
                        frame,
                        &mut session,
                        RenderedViewport::new(0, 0),
                        relative,
                        Rect::new(0, 0, 40, 7),
                        &DEFAULT_DARK,
                        Tier::TrueColor,
                    );
                    crate::screens::editor::render_status_row(
                        frame,
                        &session,
                        None,
                        "",
                        Rect::new(0, 7, 40, 1),
                        &DEFAULT_DARK,
                        Tier::TrueColor,
                    );
                })
                .unwrap();

            let buffer = terminal.backend().buffer();
            let blank_row = session.rendered_cursor_line();
            assert_eq!(
                session.rendered_layout().unwrap().line_numbers[blank_row],
                Some(2)
            );
            let gutter = (0..status_bar::gutter_width(session.line_count(), relative) as u16)
                .map(|column| buffer.cell((column, blank_row as u16)).unwrap().symbol())
                .collect::<String>();
            assert!(gutter.contains('2'), "blank-line gutter was {gutter:?}");

            let status = (0..40)
                .map(|column| buffer.cell((column, 7)).unwrap().symbol())
                .collect::<String>();
            assert!(status.contains("2:1"), "blank-line status was {status:?}");
        }
    }

    #[test]
    fn rendered_blank_after_list_gutter_and_status_report_its_physical_position() {
        let mut session = EditorSession::from_text("- result\n\n## Pass\n");
        let layout = session.render_layout(40).clone();
        let blank_row = layout
            .line_numbers
            .iter()
            .position(|line_number| *line_number == Some(2))
            .expect("the physical blank after a list must retain line two");
        for _ in 0..blank_row {
            session.handle_key(key('j'));
        }

        for relative in [false, true] {
            let mut terminal = Terminal::new(TestBackend::new(40, 8)).unwrap();
            terminal
                .draw(|frame| {
                    render_rendered(
                        frame,
                        &mut session,
                        RenderedViewport::new(0, 0),
                        relative,
                        Rect::new(0, 0, 40, 7),
                        &DEFAULT_DARK,
                        Tier::TrueColor,
                    );
                    crate::screens::editor::render_status_row(
                        frame,
                        &session,
                        None,
                        "",
                        Rect::new(0, 7, 40, 1),
                        &DEFAULT_DARK,
                        Tier::TrueColor,
                    );
                })
                .unwrap();

            let buffer = terminal.backend().buffer();
            let gutter = (0..status_bar::gutter_width(session.line_count(), relative) as u16)
                .map(|column| buffer.cell((column, blank_row as u16)).unwrap().symbol())
                .collect::<String>();
            assert!(gutter.contains('2'), "blank-line gutter was {gutter:?}");
            let status = (0..40)
                .map(|column| buffer.cell((column, 7)).unwrap().symbol())
                .collect::<String>();
            assert!(status.contains("2:1"), "blank-line status was {status:?}");
        }
    }

    #[test]
    fn rendered_body_starts_after_the_compact_gutter() {
        let marked = GutterTroubleSnapshot::testing(&[(0, DiagnosticSeverity::Warning)]);
        for (snapshot, expected_marker) in
            [(&marked, "•"), (&GutterTroubleSnapshot::default(), " ")]
        {
            let mut session = EditorSession::from_text("plain text\n");
            let mut terminal = Terminal::new(TestBackend::new(20, 2)).unwrap();
            terminal
                .draw(|frame| {
                    render_rendered_with_settings(
                        frame,
                        &mut session,
                        RenderedViewport::new(0, 0),
                        RenderedSettings::new(false),
                        frame.area(),
                        DocumentPresentation::new(&DEFAULT_DARK, Tier::TrueColor, snapshot),
                    );
                })
                .unwrap();

            assert_eq!(status_bar::gutter_width(session.line_count(), false), 3);
            let buffer = terminal.backend().buffer();
            assert_eq!(buffer.cell((0, 0)).unwrap().symbol(), expected_marker);
            assert_eq!(buffer.cell((2, 0)).unwrap().symbol(), " ");
            assert_eq!(buffer.cell((3, 0)).unwrap().symbol(), "p");
        }
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
                    RenderedViewport::new(0, 0),
                    false,
                    frame.area(),
                    &DEFAULT_DARK,
                    Tier::TrueColor,
                );
            })
            .unwrap();

        let gutter_width = status_bar::gutter_width(session.line_count(), false);
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
