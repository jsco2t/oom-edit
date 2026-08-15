//! Provider-neutral document-diagnostics modal.

use oom_edit_core::{
    Diagnostic, DiagnosticProvider, DiagnosticSeverity, KeyCodeKind, KeyInput, TextPosition,
};
use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

use crate::theme::{Theme, Tier, UiSlot};

const OVERLAY_WIDTH: u16 = 76;
const OVERLAY_HEIGHT: u16 = 18;
const VISIBLE_DIAGNOSTICS: usize = (OVERLAY_HEIGHT as usize).saturating_sub(3);

/// Host/session progress captured by App without exposing either owner here.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum TroubleProgress {
    Complete,
    Pending,
    Unavailable(String),
}

/// One provider-neutral diagnostic plus its display-ready source position.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct TroubleEntry {
    diagnostic: Diagnostic,
    position: TextPosition,
}

impl TroubleEntry {
    pub(crate) const fn new(diagnostic: Diagnostic, position: TextPosition) -> Self {
        Self {
            diagnostic,
            position,
        }
    }
}

/// Semantic result of one Trouble key transition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum TroubleAction {
    StayOpen,
    Close,
    Jump(Diagnostic),
}

/// Complete presentation state for the open diagnostics modal.
#[derive(Debug)]
pub struct TroubleState {
    entries: Vec<TroubleEntry>,
    progress: TroubleProgress,
    selected: Option<usize>,
    scroll: usize,
    warning: Option<String>,
}

impl TroubleState {
    pub(crate) fn new(entries: Vec<TroubleEntry>, progress: TroubleProgress) -> Self {
        let selected = (!entries.is_empty()).then_some(0);
        Self {
            entries,
            progress,
            selected,
            scroll: 0,
            warning: None,
        }
    }

    /// Replace the App-owned snapshot while retaining diagnostic identity.
    pub(crate) fn refresh(&mut self, entries: Vec<TroubleEntry>, progress: TroubleProgress) {
        let previous_index = self.selected.unwrap_or(0);
        let selected_identity = self
            .selected
            .and_then(|index| self.entries.get(index))
            .map(|entry| entry.diagnostic.clone());

        self.entries = entries;
        self.progress = progress;
        self.warning = None;
        self.selected = selected_identity
            .as_ref()
            .and_then(|identity| {
                self.entries
                    .iter()
                    .position(|entry| &entry.diagnostic == identity)
            })
            .or_else(|| {
                (!self.entries.is_empty()).then(|| previous_index.min(self.entries.len() - 1))
            });
        self.ensure_selection_visible();
    }

    pub(crate) fn handle_key(&mut self, key: &KeyInput) -> TroubleAction {
        match key.code.kind {
            KeyCodeKind::Esc => TroubleAction::Close,
            KeyCodeKind::Char('c') if key.mods.ctrl => TroubleAction::Close,
            KeyCodeKind::Char('j') | KeyCodeKind::Down if key.mods == Default::default() => {
                self.select_next();
                TroubleAction::StayOpen
            }
            KeyCodeKind::Char('k') | KeyCodeKind::Up if key.mods == Default::default() => {
                self.select_previous();
                TroubleAction::StayOpen
            }
            KeyCodeKind::Enter if key.mods == Default::default() => self
                .selected
                .and_then(|index| self.entries.get(index))
                .map(|entry| TroubleAction::Jump(entry.diagnostic.clone()))
                .unwrap_or(TroubleAction::StayOpen),
            _ => TroubleAction::StayOpen,
        }
    }

    pub(crate) fn mark_stale(&mut self, message: impl Into<String>) {
        self.warning = Some(message.into());
    }

    fn select_next(&mut self) {
        let Some(selected) = self.selected else {
            return;
        };
        if selected + 1 < self.entries.len() {
            self.selected = Some(selected + 1);
            self.warning = None;
            self.ensure_selection_visible();
        }
    }

    fn select_previous(&mut self) {
        let Some(selected) = self.selected else {
            return;
        };
        if selected > 0 {
            self.selected = Some(selected - 1);
            self.warning = None;
            self.ensure_selection_visible();
        }
    }

    fn ensure_selection_visible(&mut self) {
        self.scroll = scroll_for_selection(
            self.scroll,
            self.selected,
            self.entries.len(),
            VISIBLE_DIAGNOSTICS,
        );
    }

    pub(crate) fn render(&self, frame: &mut Frame<'_>, theme: &Theme, tier: Tier) {
        let area = centered(frame.area(), OVERLAY_WIDTH, OVERLAY_HEIGHT);
        frame.render_widget(Clear, area);

        let title = format!(" Trouble ({}) ", self.entries.len());
        let block = Block::default().borders(Borders::ALL).title(title);
        let inner = block.inner(area);
        frame.render_widget(block, area);

        let footer_needed = self.warning.is_some()
            || self.entries.is_empty()
            || !matches!(&self.progress, TroubleProgress::Complete);
        let reserve_footer = usize::from(
            footer_needed && (self.entries.is_empty() || usize::from(inner.height) > 1),
        );
        let row_capacity = usize::from(inner.height).saturating_sub(reserve_footer);
        let render_scroll =
            scroll_for_selection(self.scroll, self.selected, self.entries.len(), row_capacity);

        let mut lines = self
            .entries
            .iter()
            .enumerate()
            .skip(render_scroll)
            .take(row_capacity)
            .map(|(index, entry)| self.entry_line(index, entry, theme, tier))
            .collect::<Vec<_>>();

        if let Some(warning) = &self.warning {
            lines.push(Line::styled(
                format!("⚠ {warning}"),
                theme.ui_style(tier, UiSlot::StatusWarning),
            ));
        } else {
            match &self.progress {
                TroubleProgress::Complete if self.entries.is_empty() => {
                    lines.push(Line::from("No diagnostics"));
                }
                TroubleProgress::Complete => {}
                TroubleProgress::Pending => lines.push(Line::styled(
                    "… checking diagnostics",
                    theme.ui_style(tier, UiSlot::StatusInfo),
                )),
                TroubleProgress::Unavailable(reason) => lines.push(Line::styled(
                    format!("✗ diagnostics unavailable: {reason}"),
                    theme.ui_style(tier, UiSlot::StatusError),
                )),
            }
        }

        frame.render_widget(Paragraph::new(lines), inner);
    }

    fn entry_line<'a>(
        &self,
        index: usize,
        entry: &'a TroubleEntry,
        theme: &Theme,
        tier: Tier,
    ) -> Line<'a> {
        let selected = self.selected == Some(index);
        let selection = selected.then_some(Modifier::REVERSED);
        let base = selection.map_or_else(Style::default, |modifier| {
            Style::default().add_modifier(modifier)
        });
        let severity = severity_label(entry.diagnostic.severity);
        let mut severity_style = theme.ui_style(tier, severity_slot(entry.diagnostic.severity));
        if let Some(modifier) = selection {
            severity_style = severity_style.add_modifier(modifier);
        }
        Line::from(vec![
            Span::styled(if selected { "▸ " } else { "  " }, base),
            Span::styled(
                format!("{}:{} ", entry.position.line + 1, entry.position.column + 1),
                base,
            ),
            Span::styled(format!("{severity:<7} "), severity_style),
            Span::styled(
                format!("[{}] ", provider_label(entry.diagnostic.provider)),
                base,
            ),
            Span::styled(entry.diagnostic.message.as_str(), base),
        ])
    }

    pub(crate) const fn geometry(&self) -> (u16, u16) {
        (OVERLAY_WIDTH, OVERLAY_HEIGHT)
    }

    pub(crate) const fn hints(&self) -> &'static str {
        "j/k navigate · Enter jump · Esc close"
    }

    #[cfg(test)]
    pub(crate) fn selected(&self) -> Option<usize> {
        self.selected
    }

    #[cfg(test)]
    pub(crate) fn scroll(&self) -> usize {
        self.scroll
    }

    #[cfg(test)]
    pub(crate) fn entries(&self) -> &[TroubleEntry] {
        &self.entries
    }

    #[cfg(test)]
    pub(crate) fn progress(&self) -> &TroubleProgress {
        &self.progress
    }

    #[cfg(test)]
    pub(crate) fn warning(&self) -> Option<&str> {
        self.warning.as_deref()
    }
}

const fn provider_label(provider: DiagnosticProvider) -> &'static str {
    match provider {
        DiagnosticProvider::Spell => "spell",
    }
}

const fn severity_label(severity: DiagnosticSeverity) -> &'static str {
    match severity {
        DiagnosticSeverity::Error => "error",
        DiagnosticSeverity::Warning => "warning",
        DiagnosticSeverity::Info => "info",
        DiagnosticSeverity::Hint => "hint",
    }
}

const fn severity_slot(severity: DiagnosticSeverity) -> UiSlot {
    match severity {
        DiagnosticSeverity::Error => UiSlot::StatusError,
        DiagnosticSeverity::Warning => UiSlot::StatusWarning,
        DiagnosticSeverity::Info => UiSlot::StatusInfo,
        DiagnosticSeverity::Hint => UiSlot::HintKey,
    }
}

fn centered(parent: Rect, preferred_width: u16, preferred_height: u16) -> Rect {
    let width = preferred_width.min(parent.width);
    let height = preferred_height.min(parent.height);
    Rect {
        x: parent.x + parent.width.saturating_sub(width) / 2,
        y: parent.y + parent.height.saturating_sub(height) / 2,
        width,
        height,
    }
}

fn scroll_for_selection(
    current: usize,
    selected: Option<usize>,
    entry_count: usize,
    capacity: usize,
) -> usize {
    let Some(selected) = selected else {
        return 0;
    };
    if capacity == 0 {
        return selected.min(entry_count.saturating_sub(1));
    }
    let mut scroll = current;
    if selected < scroll {
        scroll = selected;
    } else if selected >= scroll.saturating_add(capacity) {
        scroll = selected + 1 - capacity;
    }
    scroll.min(entry_count.saturating_sub(capacity))
}

#[cfg(test)]
mod tests {
    use oom_edit_core::{KeyCode, Modifiers};
    use ratatui::{backend::TestBackend, Terminal};

    use super::*;

    fn diagnostic(index: usize, severity: DiagnosticSeverity) -> Diagnostic {
        Diagnostic {
            provider: DiagnosticProvider::Spell,
            severity,
            range: index * 10..index * 10 + 3,
            source_text: format!("bad{index}"),
            message: format!("message {index}"),
        }
    }

    fn entry(index: usize, severity: DiagnosticSeverity) -> TroubleEntry {
        TroubleEntry::new(
            diagnostic(index, severity),
            TextPosition {
                line: index,
                column: index + 1,
            },
        )
    }

    fn key(kind: KeyCodeKind) -> KeyInput {
        KeyInput {
            code: KeyCode { kind },
            mods: Modifiers::default(),
        }
    }

    fn rendered_text(state: &TroubleState) -> String {
        let mut terminal = Terminal::new(TestBackend::new(80, 20)).unwrap();
        terminal
            .draw(|frame| {
                state.render(
                    frame,
                    crate::theme::get_theme("default-dark"),
                    Tier::TrueColor,
                );
            })
            .unwrap();
        (0..20)
            .map(|row| {
                (0..80)
                    .map(|column| {
                        terminal
                            .backend()
                            .buffer()
                            .cell((column, row))
                            .unwrap()
                            .symbol()
                    })
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn navigation_scrolls_and_enter_returns_the_selected_generic_diagnostic() {
        let entries = (0..20)
            .map(|index| entry(index, DiagnosticSeverity::Warning))
            .collect::<Vec<_>>();
        let expected = entries[VISIBLE_DIAGNOSTICS].diagnostic.clone();
        let mut state = TroubleState::new(entries, TroubleProgress::Complete);

        for _ in 0..VISIBLE_DIAGNOSTICS {
            assert_eq!(
                state.handle_key(&key(KeyCodeKind::Char('j'))),
                TroubleAction::StayOpen
            );
        }
        assert_eq!(state.selected(), Some(VISIBLE_DIAGNOSTICS));
        assert_eq!(state.scroll(), 1);
        assert_eq!(
            state.handle_key(&key(KeyCodeKind::Enter)),
            TroubleAction::Jump(expected)
        );

        assert_eq!(
            state.handle_key(&key(KeyCodeKind::Up)),
            TroubleAction::StayOpen
        );
        assert_eq!(state.selected(), Some(VISIBLE_DIAGNOSTICS - 1));
        assert_eq!(state.scroll(), 1);
        assert_eq!(
            state.handle_key(&key(KeyCodeKind::Down)),
            TroubleAction::StayOpen
        );
        assert_eq!(state.selected(), Some(VISIBLE_DIAGNOSTICS));
        state.handle_key(&key(KeyCodeKind::Up));
        for _ in 0..VISIBLE_DIAGNOSTICS - 1 {
            state.handle_key(&key(KeyCodeKind::Char('k')));
        }
        assert_eq!(state.selected(), Some(0));
        assert_eq!(state.scroll(), 0);
    }

    #[test]
    fn refresh_preserves_identity_then_clamps_when_the_selected_row_disappears() {
        let first = entry(0, DiagnosticSeverity::Warning);
        let selected = entry(1, DiagnosticSeverity::Info);
        let third = entry(2, DiagnosticSeverity::Error);
        let mut state = TroubleState::new(
            vec![first.clone(), selected.clone(), third.clone()],
            TroubleProgress::Pending,
        );
        state.handle_key(&key(KeyCodeKind::Char('j')));

        state.refresh(
            vec![third.clone(), first, selected.clone()],
            TroubleProgress::Complete,
        );
        assert_eq!(state.selected(), Some(2));
        assert_eq!(state.entries()[2], selected);
        assert_eq!(state.progress(), &TroubleProgress::Complete);

        state.refresh(vec![third], TroubleProgress::Complete);
        assert_eq!(state.selected(), Some(0));
    }

    #[test]
    fn short_terminal_keeps_selection_and_pending_footer_visible() {
        let entries = (0..20)
            .map(|index| entry(index, DiagnosticSeverity::Warning))
            .collect::<Vec<_>>();
        let mut state = TroubleState::new(entries, TroubleProgress::Pending);
        for _ in 0..10 {
            state.handle_key(&key(KeyCodeKind::Char('j')));
        }

        let mut terminal = Terminal::new(TestBackend::new(50, 8)).unwrap();
        terminal
            .draw(|frame| {
                state.render(
                    frame,
                    crate::theme::get_theme("default-dark"),
                    Tier::TrueColor,
                );
            })
            .unwrap();
        let rendered = (0..8)
            .map(|row| {
                (0..50)
                    .map(|column| {
                        terminal
                            .backend()
                            .buffer()
                            .cell((column, row))
                            .unwrap()
                            .symbol()
                    })
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n");

        assert!(rendered.contains("▸ 11:12 warning [spell] message 10"));
        assert!(rendered.contains("… checking diagnostics"));
        assert!(!rendered.contains("message 0"));
    }

    #[test]
    fn empty_pending_complete_unavailable_and_stale_states_are_explicit() {
        let mut state = TroubleState::new(Vec::new(), TroubleProgress::Pending);
        assert_eq!(state.selected(), None);
        assert_eq!(state.progress(), &TroubleProgress::Pending);
        assert!(rendered_text(&state).contains("… checking diagnostics"));

        state.refresh(Vec::new(), TroubleProgress::Complete);
        assert_eq!(state.progress(), &TroubleProgress::Complete);
        assert!(rendered_text(&state).contains("No diagnostics"));
        state.refresh(
            Vec::new(),
            TroubleProgress::Unavailable("provider failed".to_string()),
        );
        assert_eq!(
            state.progress(),
            &TroubleProgress::Unavailable("provider failed".to_string())
        );
        assert!(rendered_text(&state).contains("✗ diagnostics unavailable: provider failed"));
        state.mark_stale("selected diagnostic is stale");
        assert_eq!(state.warning(), Some("selected diagnostic is stale"));
        assert!(rendered_text(&state).contains("⚠ selected diagnostic is stale"));
        assert_eq!(
            state.handle_key(&key(KeyCodeKind::Enter)),
            TroubleAction::StayOpen
        );
    }

    #[test]
    fn escape_and_ctrl_c_close_without_forwarding() {
        let mut state = TroubleState::new(
            vec![entry(0, DiagnosticSeverity::Hint)],
            TroubleProgress::Complete,
        );
        assert_eq!(
            state.handle_key(&key(KeyCodeKind::Esc)),
            TroubleAction::Close
        );
        let ctrl_c = KeyInput {
            code: KeyCode {
                kind: KeyCodeKind::Char('c'),
            },
            mods: Modifiers {
                ctrl: true,
                ..Modifiers::default()
            },
        };
        assert_eq!(state.handle_key(&ctrl_c), TroubleAction::Close);
    }
}
