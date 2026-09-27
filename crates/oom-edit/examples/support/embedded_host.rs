//! Consumer-owned host: only curated oom-edit APIs are imported.
use crossterm::event::{
    Event, KeyCode as TerminalKey, KeyEvent, KeyEventKind, KeyModifiers, MouseButton,
    MouseEventKind,
};
use oom_edit::{
    AllowAllFileAccess, ClipboardError, ClipboardSink, ColorValue, Config, ConfigPersistError,
    DisplayMode, EditorPane, KeyCode, KeyCodeKind, KeyInput, Modifiers, OwnedStyle,
    PaneCursorShape, PaneEvent, PaneInit, PaneInput, PaneMouse, PaneMouseKind, PaneOptions,
    PaneServices, PaneTick, RequestId, StyleModifiers, ThemeCatalog, ThemePersistenceSink,
    ThemeSelection, ThemeSlot, Tier,
};
use ratatui::{
    buffer::CellDiffOption,
    layout::Rect,
    style::{Color, Modifier, Style},
    widgets::Paragraph,
    Frame,
};
use std::path::{Path, PathBuf};
use std::time::Instant;

/// This demonstration deliberately disables clipboard and theme persistence.
pub struct SilentServices;
impl ClipboardSink for SilentServices {
    fn copy(&mut self, _: &str) -> Result<(), ClipboardError> {
        Ok(())
    }
}
impl ThemePersistenceSink for SilentServices {
    fn persist_theme(&mut self, _: ThemeSlot, _: &str) -> Result<(), ConfigPersistError> {
        Ok(())
    }
}

pub struct EmbeddedHost {
    pub pane: EditorPane,
    pub global_count: usize,
    pub quit: bool,
    pub events: Vec<PaneEvent>,
    pub cursor_shape: Option<PaneCursorShape>,
    message: String,
    prompt: HostPrompt,
}
enum HostPrompt {
    Editor,
    SavePath { request: RequestId, text: String },
}

pub fn pane_area(area: Rect) -> Rect {
    let panel_width = (area.width / 5).min(24);
    Rect::new(
        area.x + panel_width,
        area.y,
        area.width - panel_width,
        area.height.saturating_sub(1),
    )
}

impl EmbeddedHost {
    pub fn new(base: &Path, paths: Vec<PathBuf>, now: Instant) -> Self {
        Self::with_options(base, paths, PaneOptions::default(), now)
    }

    pub fn with_options(
        base: &Path,
        paths: Vec<PathBuf>,
        options: PaneOptions,
        now: Instant,
    ) -> Self {
        let construction = EditorPane::construct(PaneInit {
            config: Config::default(),
            theme_catalog: ThemeCatalog::builtins(),
            theme_selection: ThemeSelection::new(None, DisplayMode::Dark, Tier::Monochrome),
            services: PaneServices {
                clipboard_sink: Box::new(SilentServices),
                theme_sink: Box::new(SilentServices),
                file_access_policy: Box::new(AllowAllFileAccess),
                config_base_directory: base.into(),
                personal_dictionary_path: base.join("personal.txt"),
                working_directory: base.into(),
            },
            options,
            initial_paths: paths,
            now,
        });
        let message = construction
            .report
            .paths
            .iter()
            .filter_map(|result| result.as_ref().err())
            .map(ToString::to_string)
            .chain(
                construction
                    .report
                    .warnings
                    .iter()
                    .map(|warning| format!("{warning:?}")),
            )
            .collect::<Vec<_>>()
            .join("; ");
        let mut host = Self {
            pane: construction.pane,
            global_count: 0,
            quit: false,
            events: Vec::new(),
            cursor_shape: None,
            message,
            prompt: HostPrompt::Editor,
        };
        host.consume_events();
        host
    }

    pub fn handle_event(&mut self, event: Event, area: Rect, now: Instant) {
        match event {
            Event::Key(event) => {
                if let Some(key) = translate_key(&event) {
                    if key.code.kind == KeyCodeKind::F(1) && key.mods == Modifiers::default() {
                        self.global_count += 1;
                        self.message = format!("Host help opened {} time(s)", self.global_count);
                    } else if key.code.kind == KeyCodeKind::Char('q')
                        && key.mods
                            == (Modifiers {
                                alt: true,
                                ..Modifiers::default()
                            })
                    {
                        if let Err(error) = self.pane.close_all() {
                            self.message = error.to_string();
                        }
                    } else if let HostPrompt::SavePath { request, text } = &mut self.prompt {
                        match key.code.kind {
                            KeyCodeKind::Esc => {
                                let request = request.clone();
                                self.prompt = HostPrompt::Editor;
                                self.pane.set_focused(true);
                                if let Err(error) = self.pane.cancel_request(&request) {
                                    self.message = error.to_string();
                                }
                            }
                            KeyCodeKind::Enter => {
                                let request = request.clone();
                                let path = PathBuf::from(text.as_str());
                                // The pane authorizes writes and publishes the terminal outcome.
                                match self.pane.provide_save_path(&request, Some(&path)) {
                                    Ok(()) => {
                                        self.prompt = HostPrompt::Editor;
                                        self.pane.set_focused(true);
                                    }
                                    Err(error) => self.message = error.to_string(),
                                }
                            }
                            KeyCodeKind::Backspace => {
                                text.pop();
                            }
                            KeyCodeKind::Char(character) if !key.mods.ctrl && !key.mods.alt => {
                                text.push(character)
                            }
                            _ => {}
                        }
                    } else {
                        self.pane.handle_input(PaneInput::Key(key), now);
                    }
                }
            }
            Event::Paste(text) => match &mut self.prompt {
                HostPrompt::SavePath { text: path, .. } => {
                    path.push_str(&text.replace(['\r', '\n'], ""))
                }
                HostPrompt::Editor => {
                    self.pane.handle_input(PaneInput::Paste(text), now);
                }
            },
            Event::Mouse(mouse) if matches!(self.prompt, HostPrompt::Editor) => {
                let inside = area.contains((mouse.column, mouse.row).into());
                if mouse.kind == MouseEventKind::Down(MouseButton::Left) {
                    self.pane.set_focused(inside);
                }
                if inside {
                    let kind = match mouse.kind {
                        MouseEventKind::Down(MouseButton::Left) => PaneMouseKind::LeftDown,
                        MouseEventKind::Drag(MouseButton::Left) => PaneMouseKind::LeftDrag,
                        MouseEventKind::Up(MouseButton::Left) => PaneMouseKind::LeftUp,
                        MouseEventKind::ScrollUp => PaneMouseKind::ScrollUp,
                        MouseEventKind::ScrollDown => PaneMouseKind::ScrollDown,
                        MouseEventKind::Moved => PaneMouseKind::Moved,
                        _ => PaneMouseKind::Other,
                    };
                    self.pane.handle_input(
                        PaneInput::Mouse(PaneMouse {
                            kind,
                            column: mouse.column - area.x,
                            row: mouse.row - area.y,
                            modifiers: key_modifiers(mouse.modifiers),
                        }),
                        now,
                    );
                }
            }
            Event::FocusLost => self.pane.set_focused(false),
            Event::FocusGained if matches!(self.prompt, HostPrompt::Editor) => {
                self.pane.set_focused(true)
            }
            Event::Resize(width, height) => {
                let area = pane_area(Rect::new(0, 0, width, height));
                self.pane.resize(area.width, area.height, now);
            }
            _ => {}
        }
        self.consume_events();
    }

    pub fn draw(&mut self, frame: &mut Frame<'_>, now: Instant) {
        let area = frame.area();
        let editor = pane_area(area);
        frame.render_widget(
            Paragraph::new("Host panel\n\nF1: host help\nAlt-q: close all\n\nClick to focus"),
            Rect::new(area.x, area.y, editor.x - area.x, editor.height),
        );
        let owned = self.pane.render(editor.width, editor.height, now);
        for row in 0..owned.height {
            for column in 0..owned.width {
                let source =
                    &owned.cells[usize::from(row) * usize::from(owned.width) + usize::from(column)];
                let target = &mut frame.buffer_mut()[(editor.x + column, editor.y + row)];
                target.reset();
                target
                    .set_symbol(&source.symbol)
                    .set_style(style(source.style));
                target.set_diff_option(if source.continuation {
                    CellDiffOption::Skip
                } else {
                    CellDiffOption::None
                });
            }
        }
        self.cursor_shape = owned.cursor.map(|cursor| cursor.shape);
        if let Some(cursor) = owned.cursor {
            frame.set_cursor_position((editor.x + cursor.column, editor.y + cursor.row));
        }
        if area.height > 0 {
            frame.render_widget(
                Paragraph::new(self.status_text()),
                Rect::new(area.x, area.bottom() - 1, area.width, 1),
            );
        }
        self.consume_events();
    }

    pub fn status_text(&self) -> String {
        if let HostPrompt::SavePath { text, .. } = &self.prompt {
            return format!(
                "Save path: {text} [Enter save / Esc cancel] {}",
                self.message
            );
        }
        let mode = self.pane.status().map_or_else(
            || "Empty".into(),
            |status| format!("{:?} {}", status.mode, status.ruler.text),
        );
        let hints = if let Some(which_key) = self.pane.which_key() {
            format!(
                "{}: {}",
                which_key.prefix,
                which_key
                    .entries
                    .iter()
                    .map(|entry| format!(
                        "{}={}",
                        match entry.key.code.kind {
                            KeyCodeKind::Char(key) => key.to_string(),
                            _ => format!("{:?}", entry.key.code.kind),
                        },
                        entry.label
                    ))
                    .collect::<Vec<_>>()
                    .join("  ")
            )
        } else {
            self.pane
                .hints()
                .iter()
                .map(|hint| {
                    if hint.disabled {
                        format!("{} (disabled)", hint.text)
                    } else {
                        hint.text.clone()
                    }
                })
                .collect::<Vec<_>>()
                .join("  ")
        };
        format!("{mode} | {hints} | {}", self.message)
    }

    pub fn tick(&mut self, now: Instant) -> PaneTick {
        let tick = self.pane.tick(now);
        self.consume_events();
        tick
    }

    fn consume_events(&mut self) {
        loop {
            let events = self.pane.drain_events();
            if events.is_empty() {
                break;
            }
            for event in &events {
                match event {
                    PaneEvent::AllClosed { .. } => self.quit = true,
                    PaneEvent::QuitAllRequested { .. } => {
                        if let Err(error) = self.pane.close_all() {
                            self.message = error.to_string();
                        }
                    }
                    PaneEvent::SavePathRequested { request, .. } => {
                        self.prompt = HostPrompt::SavePath {
                            request: request.clone(),
                            text: String::new(),
                        };
                        self.pane.set_focused(false);
                    }
                    PaneEvent::Failed { request, error, .. } => {
                        self.message = error.to_string();
                        if matches!(&self.prompt, HostPrompt::SavePath { request: waiting, .. } if waiting == request)
                        {
                            self.prompt = HostPrompt::Editor;
                            self.pane.set_focused(true);
                        }
                    }
                    PaneEvent::Cancelled { request } if matches!(&self.prompt, HostPrompt::SavePath { request: waiting, .. } if waiting == request) =>
                    {
                        self.prompt = HostPrompt::Editor;
                        self.pane.set_focused(true);
                    }
                    _ => {}
                }
            }
            self.events.extend(events);
        }
    }
}

pub fn translate_key(event: &KeyEvent) -> Option<KeyInput> {
    if event.kind != KeyEventKind::Press {
        return None;
    }
    let mut mods = key_modifiers(event.modifiers);
    let mut kind = match event.code {
        TerminalKey::Char(character) => KeyCodeKind::Char(character),
        TerminalKey::Backspace => KeyCodeKind::Backspace,
        TerminalKey::Enter => KeyCodeKind::Enter,
        TerminalKey::Left => KeyCodeKind::Left,
        TerminalKey::Right => KeyCodeKind::Right,
        TerminalKey::Up => KeyCodeKind::Up,
        TerminalKey::Down => KeyCodeKind::Down,
        TerminalKey::Tab => KeyCodeKind::Tab,
        TerminalKey::BackTab => KeyCodeKind::BackTab,
        TerminalKey::Home => KeyCodeKind::Home,
        TerminalKey::End => KeyCodeKind::End,
        TerminalKey::PageUp => KeyCodeKind::PageUp,
        TerminalKey::PageDown => KeyCodeKind::PageDown,
        TerminalKey::Delete => KeyCodeKind::Delete,
        TerminalKey::Esc => KeyCodeKind::Esc,
        TerminalKey::F(number) => KeyCodeKind::F(number),
        _ => KeyCodeKind::Noop,
    };
    if mods.ctrl && !mods.alt && !mods.shift {
        let alias = match kind {
            KeyCodeKind::Char('[') => Some(KeyCodeKind::Esc),
            KeyCodeKind::Char('i') => Some(KeyCodeKind::Tab),
            KeyCodeKind::Char('m') => Some(KeyCodeKind::Enter),
            KeyCodeKind::Char('h') => Some(KeyCodeKind::Backspace),
            _ => None,
        };
        if let Some(alias) = alias {
            kind = alias;
            mods = Modifiers::default();
        }
    }
    if kind == KeyCodeKind::Tab && mods.shift && !mods.ctrl && !mods.alt {
        kind = KeyCodeKind::BackTab;
        mods = Modifiers::default();
    }
    Some(KeyInput {
        code: KeyCode { kind },
        mods,
    })
}
fn key_modifiers(modifiers: KeyModifiers) -> Modifiers {
    Modifiers {
        ctrl: modifiers.contains(KeyModifiers::CONTROL),
        alt: modifiers.contains(KeyModifiers::ALT),
        shift: modifiers.contains(KeyModifiers::SHIFT),
    }
}
fn color(value: ColorValue) -> Color {
    match value {
        ColorValue::Default => Color::Reset,
        ColorValue::Indexed(index) => Color::Indexed(index),
        ColorValue::Rgb { red, green, blue } => Color::Rgb(red, green, blue),
    }
}
fn modifiers(value: StyleModifiers) -> Modifier {
    let mut modifiers = Modifier::empty();
    for (enabled, modifier) in [
        (value.bold, Modifier::BOLD),
        (value.dim, Modifier::DIM),
        (value.italic, Modifier::ITALIC),
        (value.underlined, Modifier::UNDERLINED),
        (value.slow_blink, Modifier::SLOW_BLINK),
        (value.rapid_blink, Modifier::RAPID_BLINK),
        (value.reversed, Modifier::REVERSED),
        (value.hidden, Modifier::HIDDEN),
        (value.crossed_out, Modifier::CROSSED_OUT),
    ] {
        if enabled {
            modifiers.insert(modifier);
        }
    }
    modifiers
}
fn style(value: OwnedStyle) -> Style {
    Style {
        fg: value.foreground.map(color),
        bg: value.background.map(color),
        underline_color: value.underline_color.map(color),
        add_modifier: modifiers(value.modifiers),
        sub_modifier: modifiers(value.removed_modifiers),
    }
}
