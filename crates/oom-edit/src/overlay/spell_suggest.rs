//! Spelling-suggestion overlay state and rendering.

use oom_edit_core::{Diagnostic, KeyCodeKind, KeyInput};
use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::Line,
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

use crate::theme::{Theme, Tier};

pub(crate) const MAX_SUGGESTIONS: usize = 9;
const OVERLAY_WIDTH: u16 = 52;
const OVERLAY_HEIGHT: u16 = 14;

/// Semantic result of one suggestion-overlay key transition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum SpellSuggestAction {
    /// The overlay consumed the key without requesting App work.
    StayOpen,
    /// Close the overlay without changing the document.
    Close,
    /// Revalidate and apply this replacement through the session facade.
    Apply(String),
    /// Revalidate and add the diagnostic word through the personal store.
    AddWord,
}

/// Complete state owned by an open spelling-suggestion overlay.
#[derive(Debug)]
pub struct SpellSuggestState {
    diagnostic: Diagnostic,
    suggestions: Vec<String>,
    selected: Option<usize>,
}

impl SpellSuggestState {
    /// Construct a modal state with at most nine suggestion rows.
    pub fn new(diagnostic: Diagnostic, mut suggestions: Vec<String>) -> Self {
        suggestions.truncate(MAX_SUGGESTIONS);
        let selected = (!suggestions.is_empty()).then_some(0);
        Self {
            diagnostic,
            suggestions,
            selected,
        }
    }

    /// Diagnostic identity captured when the overlay opened.
    pub(crate) fn diagnostic(&self) -> &Diagnostic {
        &self.diagnostic
    }

    /// Apply one exclusive modal key transition.
    pub(crate) fn handle_key(&mut self, key: &KeyInput) -> SpellSuggestAction {
        match key.code.kind {
            KeyCodeKind::Esc => SpellSuggestAction::Close,
            KeyCodeKind::Char('c') if key.mods.ctrl => SpellSuggestAction::Close,
            KeyCodeKind::Char('j') | KeyCodeKind::Down if key.mods == Default::default() => {
                self.select_next();
                SpellSuggestAction::StayOpen
            }
            KeyCodeKind::Char('k') | KeyCodeKind::Up if key.mods == Default::default() => {
                self.select_previous();
                SpellSuggestAction::StayOpen
            }
            KeyCodeKind::Char('a') if key.mods == Default::default() => SpellSuggestAction::AddWord,
            KeyCodeKind::Char(digit @ '1'..='9') if key.mods == Default::default() => {
                let index = digit as usize - '1' as usize;
                self.suggestions
                    .get(index)
                    .cloned()
                    .map_or(SpellSuggestAction::StayOpen, SpellSuggestAction::Apply)
            }
            KeyCodeKind::Enter if key.mods == Default::default() => self
                .selected
                .and_then(|index| self.suggestions.get(index))
                .cloned()
                .map_or(SpellSuggestAction::StayOpen, SpellSuggestAction::Apply),
            _ => SpellSuggestAction::StayOpen,
        }
    }

    fn select_next(&mut self) {
        let Some(selected) = self.selected else {
            return;
        };
        self.selected = Some((selected + 1) % self.suggestions.len());
    }

    fn select_previous(&mut self) {
        let Some(selected) = self.selected else {
            return;
        };
        self.selected = Some((selected + self.suggestions.len() - 1) % self.suggestions.len());
    }

    /// Render the modal over the current frame.
    pub(crate) fn render(&self, frame: &mut Frame<'_>, _theme: &Theme, _tier: Tier) {
        let area = centered(frame.area(), OVERLAY_WIDTH, OVERLAY_HEIGHT);
        frame.render_widget(Clear, area);

        let title = format!(" Spelling: {} ", self.diagnostic.source_text);
        let block = Block::default().borders(Borders::ALL).title(title);
        let inner = block.inner(area);
        frame.render_widget(block, area);

        let lines = if self.suggestions.is_empty() {
            vec![Line::from("No suggestions")]
        } else {
            self.suggestions
                .iter()
                .enumerate()
                .map(|(index, suggestion)| {
                    let prefix = if self.selected == Some(index) {
                        "▸"
                    } else {
                        " "
                    };
                    let style = if self.selected == Some(index) {
                        Style::default().add_modifier(Modifier::REVERSED)
                    } else {
                        Style::default()
                    };
                    Line::styled(format!("{prefix} {}. {suggestion}", index + 1), style)
                })
                .collect()
        };
        frame.render_widget(Paragraph::new(lines), inner);
    }

    /// Preferred centered geometry (width, height).
    pub(crate) const fn geometry(&self) -> (u16, u16) {
        (OVERLAY_WIDTH, OVERLAY_HEIGHT)
    }

    /// Modal hints displayed in the bottom bar.
    pub(crate) const fn hints(&self) -> &'static str {
        "j/k navigate · 1-9/Enter apply · a add · Esc close"
    }

    #[cfg(test)]
    pub(crate) fn selected(&self) -> Option<usize> {
        self.selected
    }

    #[cfg(test)]
    pub(crate) fn suggestions(&self) -> &[String] {
        &self.suggestions
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

#[cfg(test)]
mod tests {
    use oom_edit_core::{DiagnosticProvider, DiagnosticSeverity, KeyCode, Modifiers};

    use super::*;

    fn diagnostic() -> Diagnostic {
        Diagnostic {
            provider: DiagnosticProvider::Spell,
            severity: DiagnosticSeverity::Warning,
            range: 0..3,
            source_text: "teh".to_string(),
            message: "Unknown word: teh".to_string(),
        }
    }

    fn key(kind: KeyCodeKind) -> KeyInput {
        KeyInput {
            code: KeyCode { kind },
            mods: Modifiers::default(),
        }
    }

    fn ctrl(c: char) -> KeyInput {
        KeyInput {
            code: KeyCode {
                kind: KeyCodeKind::Char(c),
            },
            mods: Modifiers {
                ctrl: true,
                ..Modifiers::default()
            },
        }
    }

    #[test]
    fn initial_state_selects_first_suggestion_and_caps_at_nine() {
        let suggestions = (1..=12).map(|n| format!("word{n}")).collect();
        let state = SpellSuggestState::new(diagnostic(), suggestions);
        assert_eq!(state.selected(), Some(0));
        assert_eq!(state.suggestions().len(), 9);
    }

    #[test]
    fn j_and_k_transitions_wrap_with_named_before_and_after_states() {
        let mut state = SpellSuggestState::new(
            diagnostic(),
            vec!["the".to_string(), "ten".to_string(), "tea".to_string()],
        );

        assert_eq!(state.selected(), Some(0), "before first j");
        state.handle_key(&key(KeyCodeKind::Char('j')));
        assert_eq!(state.selected(), Some(1), "j moves to the next row");
        state.handle_key(&key(KeyCodeKind::Char('j')));
        assert_eq!(state.selected(), Some(2), "second j moves forward again");
        state.handle_key(&key(KeyCodeKind::Char('j')));
        assert_eq!(state.selected(), Some(0), "j wraps last to first");

        assert_eq!(state.selected(), Some(0), "before k");
        assert_eq!(
            state.handle_key(&key(KeyCodeKind::Char('k'))),
            SpellSuggestAction::StayOpen,
            "k action"
        );
        assert_eq!(state.selected(), Some(2), "after k wraps to last");
        state.handle_key(&key(KeyCodeKind::Char('k')));
        assert_eq!(state.selected(), Some(1), "k moves backward normally");
    }

    #[test]
    fn digits_one_through_nine_apply_the_corresponding_existing_row() {
        let suggestions = (1..=9).map(|n| format!("word{n}")).collect();
        let mut state = SpellSuggestState::new(diagnostic(), suggestions);

        for digit in '1'..='9' {
            let expected = format!("word{}", digit.to_digit(10).unwrap());
            assert_eq!(
                state.handle_key(&key(KeyCodeKind::Char(digit))),
                SpellSuggestAction::Apply(expected),
                "digit {digit} must apply its one-based row"
            );
        }
    }

    #[test]
    fn out_of_range_digit_and_enter_without_selection_are_no_ops() {
        let mut short = SpellSuggestState::new(diagnostic(), vec!["the".to_string()]);
        for digit in '2'..='9' {
            assert_eq!(
                short.handle_key(&key(KeyCodeKind::Char(digit))),
                SpellSuggestAction::StayOpen,
                "out-of-range digit {digit} must be a no-op"
            );
        }
        assert_eq!(
            short.handle_key(&key(KeyCodeKind::Char('0'))),
            SpellSuggestAction::StayOpen,
            "zero is not a suggestion shortcut"
        );

        let mut empty = SpellSuggestState::new(diagnostic(), Vec::new());
        assert_eq!(empty.selected(), None, "empty state has no selection");
        assert_eq!(
            empty.handle_key(&key(KeyCodeKind::Enter)),
            SpellSuggestAction::StayOpen,
            "Enter cannot apply an absent row"
        );
        assert_eq!(
            empty.handle_key(&key(KeyCodeKind::Char('a'))),
            SpellSuggestAction::AddWord,
            "add remains available with no suggestions"
        );
    }

    #[test]
    fn enter_applies_selection_and_escape_or_ctrl_c_closes() {
        let mut state =
            SpellSuggestState::new(diagnostic(), vec!["the".to_string(), "ten".to_string()]);
        state.handle_key(&key(KeyCodeKind::Char('j')));
        assert_eq!(
            state.handle_key(&key(KeyCodeKind::Enter)),
            SpellSuggestAction::Apply("ten".to_string())
        );
        assert_eq!(
            state.handle_key(&key(KeyCodeKind::Esc)),
            SpellSuggestAction::Close
        );
        assert_eq!(state.handle_key(&ctrl('c')), SpellSuggestAction::Close);
    }
}
