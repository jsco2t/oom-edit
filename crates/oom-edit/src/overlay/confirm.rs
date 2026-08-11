//! Confirm overlays — quit-dirty and overwrite-dirty confirmations.
//!
//! [`ConfirmQuit`] — triggered when `QuitRequested{force:false}` arrives
//! with a dirty document: `Save and quit / Quit without saving / Cancel`.
//!
//! [`ConfirmOverwrite`] — triggered when `SaveError::ExternallyModified`
//! is returned: `Overwrite (:w!) / Reload (:e!) / Cancel`.
//!
//! Keys:
//! - ConfirmQuit: `y` (save & quit), `n` (quit without save), `w` (save & quit),
//!   `Esc`/`Ctrl-C` (cancel)
//! - ConfirmOverwrite: `o` (overwrite), `r` (reload), `Esc`/`Ctrl-C` (cancel)
//!
//! Hints: "y/n to confirm" / "o/r to confirm"

use ratatui::{
    style::Style,
    text::Line,
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use oom_edit_core::KeyInput;
use std::path::PathBuf;

use crate::lifecycle::{CloseTabRequest, SaveRequest};

#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfirmResult {
    Confirm,
    Quit,
    Cancel,
    Reload,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DirtyCloseChoice {
    SaveAndClose,
    Discard,
    Cancel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExternalSaveChoice {
    Overwrite,
    Reload,
    Cancel,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfirmationResolution {
    DirtyClose {
        action: CloseTabRequest,
        choice: DirtyCloseChoice,
    },
    ExternalSave {
        request: SaveRequest,
        disk_path: PathBuf,
        choice: ExternalSaveChoice,
    },
}

/// Confirm quit overlay (dirty buffer).
///
/// Options: `Save and quit` (`y`/`w`) / `Quit without saving` (`n`) / `Cancel` (`Esc`).
#[derive(Debug)]
pub struct ConfirmQuit {
    action: CloseTabRequest,
    /// Which option is currently highlighted (0=save+quit, 1=quit, 2=cancel).
    selected: usize,
}

impl ConfirmQuit {
    /// Open a new confirm-quit overlay.
    pub fn for_action(action: CloseTabRequest) -> Self {
        Self {
            action,
            selected: 0,
        }
    }

    #[cfg(test)]
    pub fn new() -> Self {
        Self::for_action(CloseTabRequest {
            target: 0,
            force: false,
            dirty_policy: crate::lifecycle::DirtyClosePolicy::Confirm,
        })
    }

    /// Handle one exclusive modal key, resolving shortcuts immediately.
    pub fn resolve_key(&mut self, key: &KeyInput) -> Option<ConfirmationResolution> {
        use oom_edit_core::KeyCodeKind;

        let choice = match key.code.kind {
            KeyCodeKind::Char('y') if !key.mods.ctrl && !key.mods.alt && !key.mods.shift => {
                self.selected = 0;
                Some(DirtyCloseChoice::SaveAndClose)
            }
            KeyCodeKind::Char('w') if !key.mods.ctrl && !key.mods.alt && !key.mods.shift => {
                self.selected = 0;
                Some(DirtyCloseChoice::SaveAndClose)
            }
            KeyCodeKind::Char('n') if !key.mods.ctrl && !key.mods.alt && !key.mods.shift => {
                self.selected = 1;
                Some(DirtyCloseChoice::Discard)
            }
            KeyCodeKind::Esc => {
                self.selected = 2;
                Some(DirtyCloseChoice::Cancel)
            }
            KeyCodeKind::Char('c') if key.mods.ctrl && !key.mods.alt => {
                self.selected = 2;
                Some(DirtyCloseChoice::Cancel)
            }
            KeyCodeKind::Up => {
                self.selected = self.selected.saturating_sub(1);
                None
            }
            KeyCodeKind::Down => {
                self.selected = (self.selected + 1).min(2);
                None
            }
            KeyCodeKind::Enter => Some(self.selected_choice()),
            _ => None,
        };
        choice.map(|choice| ConfirmationResolution::DirtyClose {
            action: self.action,
            choice,
        })
    }

    #[cfg(test)]
    pub fn handle_key(&mut self, key: &KeyInput) -> bool {
        let kind = key.code.kind;
        self.resolve_key(key);
        matches!(
            kind,
            oom_edit_core::KeyCodeKind::Char('y' | 'w' | 'n')
                | oom_edit_core::KeyCodeKind::Up
                | oom_edit_core::KeyCodeKind::Down
        )
    }

    #[cfg(test)]
    pub fn result(&self) -> ConfirmResult {
        match self.selected_choice() {
            DirtyCloseChoice::SaveAndClose => ConfirmResult::Confirm,
            DirtyCloseChoice::Discard => ConfirmResult::Quit,
            DirtyCloseChoice::Cancel => ConfirmResult::Cancel,
        }
    }

    fn selected_choice(&self) -> DirtyCloseChoice {
        match self.selected {
            0 => DirtyCloseChoice::SaveAndClose,
            1 => DirtyCloseChoice::Discard,
            _ => DirtyCloseChoice::Cancel,
        }
    }

    /// Render the overlay.
    pub fn render(&self, frame: &mut Frame<'_>) {
        let area = centered_area(40, 7, frame.area());
        let block = Block::default().borders(Borders::ALL).title(" Quit? ");

        frame.render_widget(block.clone(), area);

        let lines = [
            Line::raw(""),
            Line::raw("  Save and quit?    [y/w]"),
            Line::raw("  Quit without save [n]"),
            Line::raw("  Cancel            [Esc]"),
            Line::raw(""),
        ];

        // Highlight the selected option (lines[1] = selected==0, lines[2] = selected==1, lines[3] = selected==2)
        let mut rendered_lines: Vec<Line<'_>> = Vec::new();
        for (i, line) in lines.iter().enumerate() {
            if i == self.selected + 1 {
                rendered_lines.push(Line::styled(
                    line.to_string(),
                    Style::default().add_modifier(ratatui::style::Modifier::REVERSED),
                ));
            } else {
                rendered_lines.push(line.clone());
            }
        }

        let paragraph = Paragraph::new(rendered_lines);
        let inner = block.inner(area);
        frame.render_widget(paragraph, inner);
    }

    /// Preferred centered geometry (width, height).
    #[allow(dead_code)]
    pub fn geometry(&self) -> (u16, u16) {
        (40, 7)
    }

    /// Hint string.
    #[allow(dead_code)]
    pub fn hints(&self) -> &'static str {
        "y/w save+quit · n quit · Esc cancel"
    }
}

/// Confirm overwrite overlay (externally modified file).
///
/// Options: `Overwrite (:w!)` (`o`) / `Reload (:e!)` (`r`) / `Cancel` (`Esc`).
#[derive(Debug)]
pub struct ConfirmOverwrite {
    request: SaveRequest,
    disk_path: PathBuf,
    /// Which option is currently highlighted (0=overwrite, 1=reload, 2=cancel).
    selected: usize,
}

impl ConfirmOverwrite {
    /// Open a new confirm-overwrite overlay.
    pub fn for_request(request: SaveRequest, disk_path: PathBuf) -> Self {
        Self {
            request,
            disk_path,
            selected: 0,
        }
    }

    #[cfg(test)]
    pub fn new() -> Self {
        Self::for_request(
            SaveRequest {
                target: 0,
                path: None,
                force: false,
                retarget: true,
                continuation: crate::lifecycle::SaveContinuation::StayOpen,
            },
            PathBuf::from("fixture.md"),
        )
    }

    /// Handle a key event. Returns true if consumed.
    pub fn resolve_key(&mut self, key: &KeyInput) -> Option<ConfirmationResolution> {
        use oom_edit_core::KeyCodeKind;

        let choice = match key.code.kind {
            KeyCodeKind::Char('o') if !key.mods.ctrl && !key.mods.alt && !key.mods.shift => {
                self.selected = 0;
                Some(ExternalSaveChoice::Overwrite)
            }
            KeyCodeKind::Char('r') if !key.mods.ctrl && !key.mods.alt && !key.mods.shift => {
                self.selected = 1;
                Some(ExternalSaveChoice::Reload)
            }
            KeyCodeKind::Esc => {
                self.selected = 2;
                Some(ExternalSaveChoice::Cancel)
            }
            KeyCodeKind::Char('c') if key.mods.ctrl && !key.mods.alt => {
                self.selected = 2;
                Some(ExternalSaveChoice::Cancel)
            }
            KeyCodeKind::Up => {
                self.selected = self.selected.saturating_sub(1);
                None
            }
            KeyCodeKind::Down => {
                self.selected = (self.selected + 1).min(2);
                None
            }
            KeyCodeKind::Enter => Some(self.selected_choice()),
            _ => None,
        };
        choice.map(|choice| ConfirmationResolution::ExternalSave {
            request: self.request.clone(),
            disk_path: self.disk_path.clone(),
            choice,
        })
    }

    #[cfg(test)]
    pub fn handle_key(&mut self, key: &KeyInput) -> bool {
        let kind = key.code.kind;
        self.resolve_key(key);
        matches!(
            kind,
            oom_edit_core::KeyCodeKind::Char('o' | 'r')
                | oom_edit_core::KeyCodeKind::Up
                | oom_edit_core::KeyCodeKind::Down
        )
    }

    #[cfg(test)]
    pub fn result(&self) -> ConfirmResult {
        match self.selected_choice() {
            ExternalSaveChoice::Overwrite => ConfirmResult::Confirm,
            ExternalSaveChoice::Reload => ConfirmResult::Reload,
            ExternalSaveChoice::Cancel => ConfirmResult::Cancel,
        }
    }

    fn selected_choice(&self) -> ExternalSaveChoice {
        match self.selected {
            0 => ExternalSaveChoice::Overwrite,
            1 => ExternalSaveChoice::Reload,
            _ => ExternalSaveChoice::Cancel,
        }
    }

    /// Render the overlay.
    pub fn render(&self, frame: &mut Frame<'_>) {
        let area = centered_area(40, 7, frame.area());
        let block = Block::default()
            .borders(Borders::ALL)
            .title(" File Modified ");

        frame.render_widget(block.clone(), area);

        let lines = [
            Line::raw(""),
            Line::raw("  Overwrite (:w!)   [o]"),
            Line::raw("  Reload (:e!)      [r]"),
            Line::raw("  Cancel            [Esc]"),
            Line::raw(""),
        ];

        // Highlight the selected option (lines[1] = selected==0, lines[2] = selected==1, lines[3] = selected==2)
        let mut rendered_lines: Vec<Line<'_>> = Vec::new();
        for (i, line) in lines.iter().enumerate() {
            if i == self.selected + 1 {
                rendered_lines.push(Line::styled(
                    line.to_string(),
                    Style::default().add_modifier(ratatui::style::Modifier::REVERSED),
                ));
            } else {
                rendered_lines.push(line.clone());
            }
        }

        let paragraph = Paragraph::new(rendered_lines);
        let inner = block.inner(area);
        frame.render_widget(paragraph, inner);
    }

    /// Preferred centered geometry (width, height).
    #[allow(dead_code)]
    pub fn geometry(&self) -> (u16, u16) {
        (40, 7)
    }

    /// Hint string.
    #[allow(dead_code)]
    pub fn hints(&self) -> &'static str {
        "o overwrite · r reload · Esc cancel"
    }
}

// ── Layout helpers ──────────────────────────────────────────────────────────

/// Compute a centered rectangle of the given size within the parent area.
pub fn centered_area(
    width: u16,
    height: u16,
    parent: ratatui::layout::Rect,
) -> ratatui::layout::Rect {
    let x = parent.width.saturating_sub(width).saturating_sub(1) / 2;
    let y = parent.height.saturating_sub(height).saturating_sub(1) / 2;
    ratatui::layout::Rect::new(
        x,
        y,
        width.min(parent.width.saturating_sub(x)),
        height.min(parent.height.saturating_sub(y)),
    )
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn char_key(c: char) -> KeyInput {
        KeyInput {
            code: oom_edit_core::KeyCode {
                kind: oom_edit_core::KeyCodeKind::Char(c),
            },
            mods: oom_edit_core::Modifiers::default(),
        }
    }

    fn esc_key() -> KeyInput {
        KeyInput {
            code: oom_edit_core::KeyCode {
                kind: oom_edit_core::KeyCodeKind::Esc,
            },
            mods: oom_edit_core::Modifiers::default(),
        }
    }

    fn ctrl_c_key() -> KeyInput {
        KeyInput {
            code: oom_edit_core::KeyCode {
                kind: oom_edit_core::KeyCodeKind::Char('c'),
            },
            mods: oom_edit_core::Modifiers {
                ctrl: true,
                ..Default::default()
            },
        }
    }

    fn up_key() -> KeyInput {
        KeyInput {
            code: oom_edit_core::KeyCode {
                kind: oom_edit_core::KeyCodeKind::Up,
            },
            mods: oom_edit_core::Modifiers::default(),
        }
    }

    fn down_key() -> KeyInput {
        KeyInput {
            code: oom_edit_core::KeyCode {
                kind: oom_edit_core::KeyCodeKind::Down,
            },
            mods: oom_edit_core::Modifiers::default(),
        }
    }

    fn enter_key() -> KeyInput {
        KeyInput {
            code: oom_edit_core::KeyCode {
                kind: oom_edit_core::KeyCodeKind::Enter,
            },
            mods: oom_edit_core::Modifiers::default(),
        }
    }

    // ── ConfirmQuit ─────────────────────────────────────────────────────

    #[test]
    fn confirm_quit_esc_returns_false() {
        let mut overlay = ConfirmQuit::new();
        assert!(!overlay.handle_key(&esc_key()));
        assert_eq!(overlay.result(), ConfirmResult::Cancel);
    }

    #[test]
    fn confirm_quit_ctrlc_returns_false() {
        let mut overlay = ConfirmQuit::new();
        assert!(!overlay.handle_key(&ctrl_c_key()));
        assert_eq!(overlay.result(), ConfirmResult::Cancel);
    }

    #[test]
    fn confirm_quit_y_returns_true() {
        let mut overlay = ConfirmQuit::new();
        assert!(overlay.handle_key(&char_key('y')));
        assert_eq!(overlay.result(), ConfirmResult::Confirm);
    }

    #[test]
    fn confirm_quit_w_returns_true() {
        let mut overlay = ConfirmQuit::new();
        assert!(overlay.handle_key(&char_key('w')));
        assert_eq!(overlay.result(), ConfirmResult::Confirm);
    }

    #[test]
    fn confirm_quit_n_returns_true() {
        let mut overlay = ConfirmQuit::new();
        assert!(overlay.handle_key(&char_key('n')));
        assert_eq!(overlay.result(), ConfirmResult::Quit);
    }

    #[test]
    fn confirm_quit_up_navigates() {
        let mut overlay = ConfirmQuit::new();
        assert!(overlay.handle_key(&char_key('n')));

        assert!(overlay.handle_key(&up_key()));
        assert_eq!(overlay.result(), ConfirmResult::Confirm);
    }

    #[test]
    fn confirm_quit_down_navigates() {
        let mut overlay = ConfirmQuit::new();

        assert!(overlay.handle_key(&down_key()));
        assert_eq!(overlay.result(), ConfirmResult::Quit);
    }

    #[test]
    fn confirm_quit_enter_returns_false() {
        let mut overlay = ConfirmQuit::new();
        assert!(!overlay.handle_key(&enter_key()));
        assert_eq!(overlay.result(), ConfirmResult::Confirm);
    }

    #[test]
    fn confirm_quit_unknown_returns_false() {
        let mut overlay = ConfirmQuit::new();
        assert!(!overlay.handle_key(&char_key('z')));
        assert_eq!(overlay.result(), ConfirmResult::Confirm);
    }

    // ── ConfirmOverwrite ────────────────────────────────────────────────

    #[test]
    fn confirm_overwrite_esc_returns_false() {
        let mut overlay = ConfirmOverwrite::new();
        assert!(!overlay.handle_key(&esc_key()));
        assert_eq!(overlay.result(), ConfirmResult::Cancel);
    }

    #[test]
    fn confirm_overwrite_ctrlc_returns_false() {
        let mut overlay = ConfirmOverwrite::new();
        assert!(!overlay.handle_key(&ctrl_c_key()));
        assert_eq!(overlay.result(), ConfirmResult::Cancel);
    }

    #[test]
    fn confirm_overwrite_o_returns_true() {
        let mut overlay = ConfirmOverwrite::new();
        assert!(overlay.handle_key(&char_key('o')));
        assert_eq!(overlay.result(), ConfirmResult::Confirm);
    }

    #[test]
    fn confirm_overwrite_r_returns_true() {
        let mut overlay = ConfirmOverwrite::new();
        assert!(overlay.handle_key(&char_key('r')));
        assert_eq!(overlay.result(), ConfirmResult::Reload);
    }

    #[test]
    fn confirm_overwrite_up_navigates() {
        let mut overlay = ConfirmOverwrite::new();
        assert!(!overlay.handle_key(&esc_key()));

        assert!(overlay.handle_key(&up_key()));
        assert_eq!(overlay.result(), ConfirmResult::Reload);
    }

    #[test]
    fn confirm_overwrite_down_navigates() {
        let mut overlay = ConfirmOverwrite::new();

        assert!(overlay.handle_key(&down_key()));
        assert_eq!(overlay.result(), ConfirmResult::Reload);
    }

    #[test]
    fn confirm_overwrite_enter_returns_false() {
        let mut overlay = ConfirmOverwrite::new();
        assert!(!overlay.handle_key(&enter_key()));
        assert_eq!(overlay.result(), ConfirmResult::Confirm);
    }

    #[test]
    fn confirm_overwrite_unknown_returns_false() {
        let mut overlay = ConfirmOverwrite::new();
        assert!(!overlay.handle_key(&char_key('z')));
        assert_eq!(overlay.result(), ConfirmResult::Confirm);
    }

    // ── Geometry ────────────────────────────────────────────────────────

    #[test]
    fn confirm_quit_geometry() {
        let overlay = ConfirmQuit::new();
        assert_eq!(overlay.geometry(), (40, 7));
    }

    #[test]
    fn confirm_overwrite_geometry() {
        let overlay = ConfirmOverwrite::new();
        assert_eq!(overlay.geometry(), (40, 7));
    }
}
