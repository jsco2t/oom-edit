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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DiskChangeChoice {
    KeepMine,
    Reload,
    Cancel,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfirmationResolution {
    DiskChange {
        target: usize,
        path: PathBuf,
        version: oom_edit_core::DiskVersion,
        choice: DiskChangeChoice,
    },
    DirtyClose {
        action: CloseTabRequest,
        choice: DirtyCloseChoice,
    },
    ExternalSave {
        request: SaveRequest,
        disk_path: PathBuf,
        version: oom_edit_core::DiskVersion,
        choice: ExternalSaveChoice,
    },
}

/// A dirty-buffer decision captures the tab and the exact version displayed.
#[derive(Debug)]
pub(crate) struct ConfirmDiskChange {
    target: usize,
    path: PathBuf,
    version: oom_edit_core::DiskVersion,
    selected: usize,
}

impl ConfirmDiskChange {
    pub(crate) fn new(target: usize, path: PathBuf, version: oom_edit_core::DiskVersion) -> Self {
        Self {
            target,
            path,
            version,
            selected: 0,
        }
    }

    pub(crate) fn resolve_key(&mut self, key: &KeyInput) -> Option<ConfirmationResolution> {
        use oom_edit_core::KeyCodeKind;
        let choice = match key.code.kind {
            KeyCodeKind::Char('k') if !key.mods.ctrl && !key.mods.alt && !key.mods.shift => {
                Some(DiskChangeChoice::KeepMine)
            }
            KeyCodeKind::Char('r') if !key.mods.ctrl && !key.mods.alt && !key.mods.shift => {
                Some(DiskChangeChoice::Reload)
            }
            KeyCodeKind::Esc => Some(DiskChangeChoice::Cancel),
            KeyCodeKind::Char('c') if key.mods.ctrl && !key.mods.alt => {
                Some(DiskChangeChoice::Cancel)
            }
            KeyCodeKind::Up => {
                self.selected = self.selected.saturating_sub(1);
                None
            }
            KeyCodeKind::Down => {
                self.selected = (self.selected + 1).min(2);
                None
            }
            KeyCodeKind::Enter => Some(match self.selected {
                0 => DiskChangeChoice::KeepMine,
                1 => DiskChangeChoice::Reload,
                _ => DiskChangeChoice::Cancel,
            }),
            _ => None,
        };
        choice.map(|choice| ConfirmationResolution::DiskChange {
            target: self.target,
            path: self.path.clone(),
            version: self.version.clone(),
            choice,
        })
    }

    pub(crate) fn render(&self, frame: &mut Frame<'_>) {
        let area = centered_area(44, 7, frame.area());
        let block = Block::default()
            .borders(Borders::ALL)
            .title(" Disk changed ");
        let inner = block.inner(area);
        frame.render_widget(block, area);
        let choices = ["Keep mine [k]", "Reload disk [r]", "Cancel [Esc]"];
        let lines = choices
            .iter()
            .enumerate()
            .map(|(index, label)| {
                let prefix = if index == self.selected { "> " } else { "  " };
                Line::raw(format!("{prefix}{label}"))
            })
            .collect::<Vec<_>>();
        frame.render_widget(Paragraph::new(lines), inner);
    }
}

/// Confirm quit overlay (dirty buffer).
///
/// Options: `Save and quit` (`y`/`w`) / `Quit without saving` (`n`) / `Cancel` (`Esc`).
#[derive(Debug)]
pub struct ConfirmQuit {
    action: CloseTabRequest,
    choices: QuitChoices,
}

#[derive(Debug)]
enum QuitChoices {
    Decision(usize),
    AwaitingSavePath,
}

impl ConfirmQuit {
    /// Open a new confirm-quit overlay.
    pub fn for_action(action: CloseTabRequest) -> Self {
        Self {
            action,
            choices: QuitChoices::Decision(0),
        }
    }

    pub(crate) fn for_save_path(action: CloseTabRequest) -> Self {
        Self {
            action,
            choices: QuitChoices::AwaitingSavePath,
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

        let mut selected = match self.choices {
            QuitChoices::Decision(selected) => selected,
            QuitChoices::AwaitingSavePath => {
                let choice = match key.code.kind {
                    KeyCodeKind::Esc => DirtyCloseChoice::Cancel,
                    KeyCodeKind::Char('c') if key.mods.ctrl && !key.mods.alt => {
                        DirtyCloseChoice::Cancel
                    }
                    KeyCodeKind::Char('n')
                        if !key.mods.ctrl && !key.mods.alt && !key.mods.shift =>
                    {
                        DirtyCloseChoice::Discard
                    }
                    _ => return None,
                };
                return Some(ConfirmationResolution::DirtyClose {
                    action: self.action,
                    choice,
                });
            }
        };

        let choice = match key.code.kind {
            KeyCodeKind::Char('y') if !key.mods.ctrl && !key.mods.alt && !key.mods.shift => {
                selected = 0;
                Some(DirtyCloseChoice::SaveAndClose)
            }
            KeyCodeKind::Char('w') if !key.mods.ctrl && !key.mods.alt && !key.mods.shift => {
                selected = 0;
                Some(DirtyCloseChoice::SaveAndClose)
            }
            KeyCodeKind::Char('n') if !key.mods.ctrl && !key.mods.alt && !key.mods.shift => {
                selected = 1;
                Some(DirtyCloseChoice::Discard)
            }
            KeyCodeKind::Esc => {
                selected = 2;
                Some(DirtyCloseChoice::Cancel)
            }
            KeyCodeKind::Char('c') if key.mods.ctrl && !key.mods.alt => {
                selected = 2;
                Some(DirtyCloseChoice::Cancel)
            }
            KeyCodeKind::Up => {
                selected = selected.saturating_sub(1);
                None
            }
            KeyCodeKind::Down => {
                selected = (selected + 1).min(2);
                None
            }
            KeyCodeKind::Enter => Some(self.selected_choice()),
            _ => None,
        };
        self.choices = QuitChoices::Decision(selected);
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
        match self.choices {
            QuitChoices::Decision(0) => DirtyCloseChoice::SaveAndClose,
            QuitChoices::Decision(1) => DirtyCloseChoice::Discard,
            _ => DirtyCloseChoice::Cancel,
        }
    }

    /// Render the overlay.
    pub fn render(&self, frame: &mut Frame<'_>) {
        let area = centered_area(40, 7, frame.area());
        let block = Block::default().borders(Borders::ALL).title(" Quit? ");

        frame.render_widget(block.clone(), area);

        let QuitChoices::Decision(selected) = self.choices else {
            frame.render_widget(
                Paragraph::new(vec![
                    Line::raw("Host save path pending"),
                    Line::raw("Discard [n]"),
                    Line::raw("Cancel [Esc]"),
                ]),
                block.inner(area),
            );
            return;
        };

        let compact = area.width < 40 || area.height < 7;
        let lines = if compact {
            vec![
                Line::raw("Save [y/w]"),
                Line::raw("Discard [n]"),
                Line::raw("Cancel [Esc]"),
            ]
        } else {
            vec![
                Line::raw(""),
                Line::raw("  Save and quit?    [y/w]"),
                Line::raw("  Quit without save [n]"),
                Line::raw("  Cancel            [Esc]"),
                Line::raw(""),
            ]
        };

        // Highlight the selected option in both full and compact layouts.
        let mut rendered_lines: Vec<Line<'_>> = Vec::new();
        for (i, line) in lines.iter().enumerate() {
            if i == selected + usize::from(!compact) {
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
        match self.choices {
            QuitChoices::Decision(_) => "y/w save+quit · n quit · Esc cancel",
            QuitChoices::AwaitingSavePath => "host save path pending · n discard · Esc cancel",
        }
    }
}

/// Confirm overwrite overlay (externally modified file).
///
/// Options: `Overwrite (:w!)` (`o`) / `Reload (:e!)` (`r`) / `Cancel` (`Esc`).
#[derive(Debug)]
pub struct ConfirmOverwrite {
    request: SaveRequest,
    disk_path: PathBuf,
    version: oom_edit_core::DiskVersion,
    /// Which option is currently highlighted (0=overwrite, 1=reload, 2=cancel).
    selected: usize,
}

impl ConfirmOverwrite {
    /// Open a new confirm-overwrite overlay.
    pub fn for_request(
        request: SaveRequest,
        disk_path: PathBuf,
        version: oom_edit_core::DiskVersion,
    ) -> Self {
        Self {
            request,
            disk_path,
            version,
            selected: 0,
        }
    }

    #[cfg(test)]
    pub fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("fixture.md");
        std::fs::write(&path, "fixture\n").unwrap();
        let version = oom_edit_core::DiskVersion::observe(&path).unwrap();
        Self::for_request(
            SaveRequest {
                target: 0,
                path: None,
                force: false,
                retarget: true,
                continuation: crate::lifecycle::SaveContinuation::StayOpen,
            },
            PathBuf::from("fixture.md"),
            version,
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
            version: self.version.clone(),
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
        let missing = self.version.is_missing();
        let block = Block::default().borders(Borders::ALL).title(if missing {
            " File Missing "
        } else {
            " File Modified "
        });

        frame.render_widget(block.clone(), area);

        let compact = area.width < 40 || area.height < 7;
        let lines = if compact {
            vec![
                Line::raw(if missing {
                    "Recreate [o]"
                } else {
                    "Overwrite [o]"
                }),
                Line::raw("Reload [r]"),
                Line::raw("Cancel [Esc]"),
            ]
        } else {
            vec![
                Line::raw(""),
                Line::raw(if missing {
                    "  Recreate file    [o]"
                } else {
                    "  Overwrite (:w!)   [o]"
                }),
                Line::raw("  Reload (:e!)      [r]"),
                Line::raw("  Cancel            [Esc]"),
                Line::raw(""),
            ]
        };

        // Highlight the selected option in both full and compact layouts.
        let mut rendered_lines: Vec<Line<'_>> = Vec::new();
        for (i, line) in lines.iter().enumerate() {
            if i == self.selected + usize::from(!compact) {
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
    let x = parent
        .x
        .saturating_add(parent.width.saturating_sub(width).saturating_sub(1) / 2);
    let y = parent
        .y
        .saturating_add(parent.height.saturating_sub(height).saturating_sub(1) / 2);
    ratatui::layout::Rect::new(
        x,
        y,
        width.min(parent.right().saturating_sub(x)),
        height.min(parent.bottom().saturating_sub(y)),
    )
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::layout::Rect;
    use ratatui::Terminal;

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

    fn compact_rows(render: impl FnOnce(&mut Frame<'_>)) -> Vec<String> {
        let mut terminal = Terminal::new(TestBackend::new(20, 5)).unwrap();
        terminal.draw(render).unwrap();
        let buffer = terminal.backend().buffer();
        (0..5)
            .map(|row| {
                (0..20)
                    .map(|column| buffer[(column, row)].symbol())
                    .collect::<String>()
            })
            .collect()
    }

    #[test]
    fn all_confirmation_actions_remain_visible_and_selectable_at_twenty_by_five() {
        let mut quit = ConfirmQuit::new();
        let lines = compact_rows(|frame| quit.render(frame));
        assert!(lines.iter().any(|line| line.contains("Save [y/w]")));
        assert!(lines.iter().any(|line| line.contains("Discard [n]")));
        assert!(lines.iter().any(|line| line.contains("Cancel [Esc]")));
        quit.resolve_key(&down_key());
        quit.resolve_key(&down_key());
        assert_eq!(quit.result(), ConfirmResult::Cancel);
        assert!(compact_rows(|frame| quit.render(frame))[3].contains("Cancel [Esc]"));

        let mut overwrite = ConfirmOverwrite::new();
        let lines = compact_rows(|frame| overwrite.render(frame));
        assert!(lines.iter().any(|line| line.contains("Overwrite [o]")));
        assert!(lines.iter().any(|line| line.contains("Reload [r]")));
        assert!(lines.iter().any(|line| line.contains("Cancel [Esc]")));
        overwrite.resolve_key(&down_key());
        overwrite.resolve_key(&down_key());
        assert_eq!(overwrite.result(), ConfirmResult::Cancel);
    }

    #[test]
    fn confirmation_geometry_offsets_the_host_origin_once() {
        assert_eq!(
            centered_area(40, 7, Rect::new(10, 20, 80, 24)),
            Rect::new(29, 28, 40, 7)
        );
    }
}
