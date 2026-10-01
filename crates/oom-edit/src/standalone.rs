//! Full-terminal host adapter. All editor behavior stays behind EditorPane.

use std::sync::Arc;
use std::time::{Duration, Instant};

use crossterm::event::Event;
use ratatui::buffer::{Buffer, CellDiffOption};
use ratatui::layout::Rect;
use ratatui::Frame;

use crate::pane::{EditorPane, PaneEvent, PaneInput, PaneTick};
use crate::pane_frame::{PaneCell, PaneFrame};
use crate::terminal_input::{crossterm_key_to_core, crossterm_mouse_to_pane};

pub(crate) struct StandaloneHost {
    pub(crate) pane: EditorPane,
    pub(crate) should_quit: bool,
    last_input: Instant,
    cursor_shapes: bool,
    frame_adapter: FrameAdapter,
}

impl StandaloneHost {
    pub(crate) fn new(pane: EditorPane, cursor_shapes: bool, now: Instant) -> Self {
        let mut host = Self {
            pane,
            should_quit: false,
            last_input: now,
            cursor_shapes,
            frame_adapter: FrameAdapter::default(),
        };
        host.consume_events();
        host
    }

    pub(crate) fn mode(&self) -> oom_edit_core::Mode {
        self.pane
            .tabs()
            .into_iter()
            .find(|tab| tab.active)
            .map_or(oom_edit_core::Mode::Normal, |tab| tab.mode)
    }

    pub(crate) fn cursor_shapes(&self) -> bool {
        self.cursor_shapes
    }

    pub(crate) fn record_input(&mut self, now: Instant) {
        self.last_input = now;
    }

    pub(crate) fn input_idle_for(&self, now: Instant, delay: Duration) -> bool {
        now.saturating_duration_since(self.last_input) >= delay
    }

    pub(crate) fn tick(&mut self, now: Instant) -> PaneTick {
        let result = self.pane.tick(now);
        self.consume_events();
        result
    }

    pub(crate) fn on_idle_unit(&mut self, max_bytes: usize) -> bool {
        let result = self.pane.idle_unit(max_bytes);
        self.consume_events();
        result.worked
    }

    pub(crate) fn handle_event_at(&mut self, event: &Event, now: Instant) {
        match event {
            Event::Key(key) if crate::event::accepts_key_event(key.kind) => {
                self.pane
                    .handle_input(PaneInput::Key(crossterm_key_to_core(key)), now);
            }
            Event::Paste(text) => {
                self.pane.handle_input(PaneInput::Paste(text.clone()), now);
            }
            Event::Mouse(mouse) => {
                self.pane
                    .handle_input(PaneInput::Mouse(crossterm_mouse_to_pane(*mouse)), now);
            }
            Event::Resize(width, height) => self.pane.resize(*width, *height, now),
            _ => {}
        }
        self.consume_events();
    }

    pub(crate) fn render_owned(&mut self, width: u16, height: u16, now: Instant) -> PaneFrame {
        let owned = self.pane.render(width, height, now);
        self.consume_events();
        owned
    }

    #[cfg(test)]
    pub(crate) fn render(&mut self, frame: &mut Frame<'_>, now: Instant) {
        let area = frame.area();
        let owned = self.render_owned(area.width, area.height, now);
        self.copy_frame(frame, area, &owned);
    }

    pub(crate) fn copy_frame(&mut self, frame: &mut Frame<'_>, area: Rect, pane: &PaneFrame) {
        self.frame_adapter.copy(frame, area, pane);
    }

    fn consume_events(&mut self) {
        let events = self.pane.drain_events();
        if events
            .iter()
            .any(|event| matches!(event, PaneEvent::QuitAllRequested { .. }))
            || (events
                .iter()
                .any(|event| matches!(event, PaneEvent::Closed { .. }))
                && self.pane.tabs().is_empty())
        {
            self.should_quit = true;
        }
    }

    #[cfg(test)]
    pub(crate) fn handle_event(&mut self, event: &Event) {
        self.handle_event_at(event, Instant::now());
    }

    #[cfg(test)]
    pub(crate) fn from_app_for_test(app: crate::app::App, now: Instant) -> Self {
        let cursor_shapes = app.cursor_shapes();
        Self::new(EditorPane::from_app_for_test(app), cursor_shapes, now)
    }
}

/// Cached renderer projection of an immutable owned grid, not editor state.
#[derive(Default)]
struct FrameAdapter {
    cells: Option<Arc<Vec<PaneCell>>>,
    buffer: Buffer,
}

impl FrameAdapter {
    fn copy(&mut self, frame: &mut Frame<'_>, area: Rect, pane: &PaneFrame) {
        let same_cells = self.buffer.area == Rect::new(0, 0, pane.width, pane.height)
            && self
                .cells
                .as_ref()
                .is_some_and(|cells| Arc::ptr_eq(cells, &pane.cells));
        if !same_cells {
            self.buffer.resize(Rect::new(0, 0, pane.width, pane.height));
            for (source, target) in pane.cells.iter().zip(self.buffer.content.iter_mut()) {
                target.reset();
                target
                    .set_symbol(&source.symbol)
                    .set_style(source.style.to_ratatui());
                target.set_diff_option(if source.continuation {
                    CellDiffOption::Skip
                } else {
                    CellDiffOption::None
                });
            }
            self.cells = Some(Arc::clone(&pane.cells));
        }
        let width = pane.width.min(area.width);
        let height = pane.height.min(area.height);
        let target = frame.buffer_mut();
        for row in 0..height {
            let source_start = usize::from(row) * usize::from(pane.width);
            let target_start = target.index_of(area.x, area.y + row);
            target.content[target_start..target_start + usize::from(width)].clone_from_slice(
                &self.buffer.content[source_start..source_start + usize::from(width)],
            );
        }
        if let Some(cursor) = pane
            .cursor
            .filter(|cursor| cursor.column < width && cursor.row < height)
        {
            frame.set_cursor_position((area.x + cursor.column, area.y + cursor.row));
        }
    }
}

// Existing unit tests inspect state only after driving the real host boundary.
// This access is never compiled into the binary or exposed to consumers.
#[cfg(test)]
impl std::ops::Deref for StandaloneHost {
    type Target = crate::app::App;
    fn deref(&self) -> &Self::Target {
        self.pane.app_for_test()
    }
}

#[cfg(test)]
impl std::ops::DerefMut for StandaloneHost {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.pane.app_mut_for_test()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::App;
    use crate::pane_frame::{PaneCursor, PaneCursorShape};
    use crate::theme::{ResolvedTheme, Tier};
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
    use oom_edit_core::{EditorSession, RecordingClipboardSink};
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    mod incremental_host_vector {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/support/incremental_host_vector.rs"
        ));
    }

    const MARKDOWN: &str = incremental_host_vector::MARKDOWN;

    fn reference(now: Instant) -> App {
        App::new(
            EditorSession::from_text(MARKDOWN),
            ResolvedTheme::injected("default-dark", false, Tier::TrueColor),
            true,
            false,
            Box::new(RecordingClipboardSink::default()),
            Box::new(crate::config::DisabledConfigStore),
            now,
        )
    }

    fn key(code: KeyCode, modifiers: KeyModifiers) -> Event {
        Event::Key(KeyEvent::new(code, modifiers))
    }

    fn keys(text: &str) -> impl Iterator<Item = Event> + '_ {
        text.chars()
            .map(|character| key(KeyCode::Char(character), KeyModifiers::NONE))
    }

    #[test]
    fn shift_v_standalone_terminal_events_select_and_cancel() {
        for cursor_shapes in [false, true] {
            for modifiers in [KeyModifiers::NONE, KeyModifiers::SHIFT] {
                let now = Instant::now();
                let mut host = StandaloneHost::from_app_for_test(reference(now), now);
                host.cursor_shapes = cursor_shapes;
                host.render_owned(80, 24, now);
                let line = key(KeyCode::Char('V'), modifiers);
                host.handle_event_at(&line, now);
                assert_eq!(host.mode(), oom_edit_core::Mode::Select);
                host.handle_event_at(&line, now);
                assert_eq!(host.mode(), oom_edit_core::Mode::Normal);
                host.handle_event_at(&key(KeyCode::Char('v'), KeyModifiers::NONE), now);
                host.handle_event_at(&line, now);
                host.handle_event_at(&line, now);
                assert_eq!(host.mode(), oom_edit_core::Mode::Normal);
            }
        }
    }

    #[test]
    fn fr_110_baseline_trace_replay() {
        let initial = Instant::now();
        let mut baseline = reference(initial);
        let mut host = StandaloneHost::from_app_for_test(reference(initial), initial);
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        let mut reference_terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        let mut inputs = vec![Event::Resize(80, 24)];
        inputs.extend(keys("gg2jvi"));
        inputs.push(key(KeyCode::Esc, KeyModifiers::NONE));
        inputs.extend(keys("ggiworld "));
        inputs.push(Event::Paste("λ paste e\u{301}".into()));
        inputs.push(key(KeyCode::Backspace, KeyModifiers::NONE));
        inputs.push(key(KeyCode::Esc, KeyModifiers::NONE));
        inputs.extend(keys("u"));
        inputs.push(key(KeyCode::Char('r'), KeyModifiers::CONTROL));
        inputs.extend(keys("ggVj\"ayy"));
        inputs.push(key(KeyCode::Esc, KeyModifiers::NONE));
        inputs.extend(keys("\"ap/Heading"));
        inputs.push(key(KeyCode::Enter, KeyModifiers::NONE));
        inputs.extend(keys("nN:set nowrap"));
        inputs.push(key(KeyCode::Enter, KeyModifiers::NONE));
        inputs.extend(keys(" ?"));
        inputs.push(key(KeyCode::Esc, KeyModifiers::NONE));
        inputs.extend(keys(" :"));
        inputs.push(key(KeyCode::Esc, KeyModifiers::NONE));
        inputs.extend(keys(":tabnew"));
        inputs.push(key(KeyCode::Enter, KeyModifiers::NONE));
        inputs.extend(keys(":tabprev"));
        inputs.push(key(KeyCode::Enter, KeyModifiers::NONE));
        inputs.push(key(KeyCode::Char('v'), KeyModifiers::CONTROL));
        inputs.extend(keys("jl"));
        inputs.push(key(KeyCode::Esc, KeyModifiers::NONE));
        inputs.push(Event::Resize(40, 10));
        inputs.push(Event::Resize(100, 30));
        let mut modes = std::collections::HashSet::new();

        for (index, event) in inputs.into_iter().enumerate() {
            let now = initial + Duration::from_millis(index as u64 * 10);
            baseline.handle_event_at(&event, now);
            host.handle_event_at(&event, now);
            baseline.tick(now);
            host.tick(now);
            if let Event::Resize(width, height) = event {
                terminal.backend_mut().resize(width, height);
                reference_terminal.backend_mut().resize(width, height);
            }
            reference_terminal
                .draw(|frame| baseline.render(frame))
                .unwrap();
            let backend = reference_terminal.backend();
            let cursor = backend.cursor_visible().then(|| {
                let position = backend.cursor_position();
                PaneCursor {
                    column: position.x,
                    row: position.y,
                    shape: crate::event::cursor_shape(baseline.mode(), true),
                }
            });
            let expected = PaneFrame::from_buffer(backend.buffer(), cursor);
            let completed = terminal.draw(|frame| host.render(frame, now)).unwrap();
            let mut actual = PaneFrame::from_buffer(completed.buffer, expected.cursor);
            actual.cursor = if terminal.backend().cursor_visible() {
                let position = terminal.backend().cursor_position();
                Some(PaneCursor {
                    column: position.x,
                    row: position.y,
                    shape: crate::event::cursor_shape(host.mode(), true),
                })
            } else {
                None
            };
            assert!(
                actual.visually_equals(&expected),
                "frame mismatch after event {index}"
            );
            let expected_session = baseline.active_mut().unwrap().session_mut();
            let tab = host.pane.active_tab().unwrap();
            assert_eq!(
                host.pane.text(&tab).unwrap(),
                expected_session.document(),
                "text after event {index}"
            );
            assert_eq!(
                host.pane.source_cursor(&tab).unwrap(),
                expected_session.cursor(),
                "cursor after event {index}"
            );
            assert_eq!(
                host.mode(),
                expected_session.mode(),
                "mode after event {index}"
            );
            assert_eq!(host.pane.tabs().len(), baseline.tab_count());
            modes.insert(format!("{:?}", host.mode()));
        }
        assert_eq!(
            modes,
            ["Normal", "Insert", "Select", "Command"]
                .map(str::to_string)
                .into_iter()
                .collect()
        );
    }

    #[test]
    fn shared_incremental_vector_preserves_standalone_cells_modes_and_cursor() {
        let initial = Instant::now();
        let mut baseline = reference(initial);
        let mut host = StandaloneHost::from_app_for_test(reference(initial), initial);
        let mut terminal = Terminal::new(TestBackend::new(96, 24)).unwrap();
        let mut baseline_terminal = Terminal::new(TestBackend::new(96, 24)).unwrap();
        for (index, step) in incremental_host_vector::steps().into_iter().enumerate() {
            let now = initial + Duration::from_millis(index as u64 * 10);
            match step {
                incremental_host_vector::Step::Event(event) => {
                    baseline.handle_event_at(&event, now);
                    host.handle_event_at(&event, now);
                }
                incremental_host_vector::Step::PaneSize(width, height) => {
                    let event = Event::Resize(width, height);
                    baseline.handle_event_at(&event, now);
                    host.handle_event_at(&event, now);
                    terminal.backend_mut().resize(width, height);
                    baseline_terminal.backend_mut().resize(width, height);
                }
            }
            baseline.tick(now);
            host.tick(now);
            baseline_terminal
                .draw(|frame| baseline.render(frame))
                .unwrap();
            terminal.draw(|frame| host.render(frame, now)).unwrap();
            assert_eq!(
                terminal.backend().buffer().content,
                baseline_terminal.backend().buffer().content,
                "standalone cells after shared step {index}"
            );
            let expected_session = baseline.active_mut().unwrap().session_mut();
            let tab = host.pane.active_tab().unwrap();
            assert_eq!(host.pane.text(&tab).unwrap(), expected_session.document());
            assert_eq!(
                host.pane.source_cursor(&tab).unwrap(),
                expected_session.cursor()
            );
            assert_eq!(host.mode(), expected_session.mode());
        }
    }

    #[test]
    fn fr_110_standalone_exit_is_an_event_policy_not_pane_lifetime() {
        let now = Instant::now();
        for command in [":q!", ":qa!"] {
            let mut host = StandaloneHost::from_app_for_test(reference(now), now);
            for event in keys(command).chain([key(KeyCode::Enter, KeyModifiers::NONE)]) {
                host.handle_event_at(&event, now);
            }
            assert!(host.should_quit, "{command} must request standalone exit");
        }
        let mut host = StandaloneHost::from_app_for_test(reference(now), now);
        for event in keys("iX")
            .chain([key(KeyCode::Esc, KeyModifiers::NONE)])
            .chain(keys(":qa"))
            .chain([key(KeyCode::Enter, KeyModifiers::NONE)])
        {
            host.handle_event_at(&event, now);
        }
        assert!(
            !host.should_quit,
            "dirty quit-all retains the existing refusal"
        );
        assert_eq!(host.pane.tabs().len(), 1);
    }

    #[test]
    fn terminal_kind_filter_does_not_replay_edits() {
        let now = Instant::now();
        let mut host = StandaloneHost::from_app_for_test(reference(now), now);
        host.handle_event_at(&key(KeyCode::Char('i'), KeyModifiers::NONE), now);
        let tab = host.pane.active_tab().unwrap();
        let before = host.pane.text(&tab).unwrap();
        for kind in [KeyEventKind::Repeat, KeyEventKind::Release] {
            host.handle_event_at(
                &Event::Key(KeyEvent::new_with_kind(
                    KeyCode::Char('x'),
                    KeyModifiers::NONE,
                    kind,
                )),
                now,
            );
        }
        assert_eq!(host.pane.text(&tab).unwrap(), before);
        host.handle_event_at(&key(KeyCode::Char('x'), KeyModifiers::NONE), now);
        assert!(host.pane.text(&tab).unwrap().starts_with('x'));
    }

    #[test]
    fn frame_adapter_translates_origin_once_and_roundtrips_owned_styles() {
        use crate::owned_style::{ColorValue, OwnedStyle, StyleModifiers};
        let mut pane = PaneFrame::blank(3, 2);
        std::sync::Arc::make_mut(&mut pane.cells)[0].symbol = "界".into();
        std::sync::Arc::make_mut(&mut pane.cells)[1].continuation = true;
        std::sync::Arc::make_mut(&mut pane.cells)[2].symbol = "e\u{301}".into();
        let style = OwnedStyle {
            foreground: Some(ColorValue::Indexed(12)),
            background: Some(ColorValue::Rgb {
                red: 2,
                green: 4,
                blue: 8,
            }),
            underline_color: Some(ColorValue::Default),
            modifiers: StyleModifiers {
                bold: true,
                italic: true,
                underlined: true,
                slow_blink: true,
                rapid_blink: true,
                reversed: true,
                hidden: true,
                crossed_out: true,
                ..StyleModifiers::default()
            },
            removed_modifiers: StyleModifiers {
                dim: true,
                ..StyleModifiers::default()
            },
        };
        std::sync::Arc::make_mut(&mut pane.cells)[2].style = style;
        pane.cursor = Some(PaneCursor {
            column: 2,
            row: 1,
            shape: PaneCursorShape::Bar,
        });
        let mut terminal = Terminal::new(TestBackend::new(12, 8)).unwrap();
        let mut adapter = FrameAdapter::default();
        let completed = terminal
            .draw(|frame| adapter.copy(frame, Rect::new(5, 3, 3, 2), &pane))
            .unwrap();
        assert_eq!(completed.buffer[(5, 3)].symbol(), "界");
        assert_eq!(completed.buffer[(7, 3)].symbol(), "e\u{301}");
        let target = &completed.buffer[(7, 3)];
        let converted = style.to_ratatui();
        assert_eq!(target.fg, converted.fg.unwrap());
        assert_eq!(target.bg, converted.bg.unwrap());
        assert_eq!(target.underline_color, converted.underline_color.unwrap());
        assert_eq!(target.modifier, style.to_ratatui().add_modifier);
        assert_eq!(completed.buffer[(4, 3)].symbol(), " ");
        assert_eq!(completed.buffer[(8, 3)].symbol(), " ");
        assert_eq!(
            terminal.backend().cursor_position(),
            ratatui::layout::Position::new(7, 4)
        );
    }
}
