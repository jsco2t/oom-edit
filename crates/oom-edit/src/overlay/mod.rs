//! Overlay system — single-slot overlay with take-and-return-bool key protocol.
//!
//! [`Overlay`] is a flat enum with one active slot. Each overlay variant
//! implements the key protocol: `handle_key(&mut self, key) -> bool` where
//! `true` means "consumed" and `false` means "pass through".
//!
//! `centered()` returns the preferred geometry; `clear()` resets state.

pub mod confirm;
pub mod palette;

pub use confirm::{ConfirmOverwrite, ConfirmQuit, ConfirmResult};
pub use palette::PaletteState;

use crate::command::{Command, Contexts};
use crate::theme::{Theme, Tier};

/// An active overlay slot. Only one overlay can be open at a time.
#[derive(Default, Debug)]
pub enum Overlay {
    /// No overlay is open.
    #[default]
    None,
    /// The command palette (FR-6.6/6.7).
    Palette(PaletteState),
    /// Confirm quit overlay (T16).
    ConfirmQuit(ConfirmQuit),
    /// Confirm overwrite overlay (T16).
    ConfirmOverwrite(ConfirmOverwrite),
}

impl Overlay {
    /// Is any overlay open?
    pub fn is_some(&self) -> bool {
        !matches!(self, Overlay::None)
    }

    /// Is the palette open?
    #[allow(dead_code)]
    pub fn is_palette(&self) -> bool {
        matches!(self, Overlay::Palette(_))
    }

    /// Open the command palette.
    #[allow(dead_code)]
    pub fn open_palette(context: Contexts) -> Self {
        Overlay::Palette(PaletteState::new(context))
    }

    /// Open the confirm-quit overlay.
    pub fn open_confirm_quit() -> Self {
        Overlay::ConfirmQuit(ConfirmQuit::new())
    }

    /// Open the confirm-overwrite overlay.
    pub fn open_confirm_overwrite() -> Self {
        Overlay::ConfirmOverwrite(ConfirmOverwrite::new())
    }

    /// Close the current overlay, returning the old value.
    pub fn close(&mut self) -> Overlay {
        std::mem::replace(self, Overlay::None)
    }

    /// Handle a key event. Returns `true` if consumed.
    pub fn handle_key(&mut self, key: &oom_edit_core::session::KeyInput) -> bool {
        match self {
            Overlay::Palette(p) => p.handle_key(key),
            Overlay::ConfirmQuit(q) => q.handle_key(key),
            Overlay::ConfirmOverwrite(o) => o.handle_key(key),
            Overlay::None => false,
        }
    }

    /// Render the overlay.
    pub fn render(&self, frame: &mut ratatui::Frame<'_>, theme: &Theme, tier: Tier) {
        match self {
            Overlay::Palette(p) => p.render(frame, theme, tier),
            Overlay::ConfirmQuit(q) => q.render(frame),
            Overlay::ConfirmOverwrite(o) => o.render(frame),
            Overlay::None => {}
        }
    }

    /// Preferred centered geometry (width, height) in character cells.
    #[allow(dead_code)]
    pub fn geometry(&self) -> (u16, u16) {
        match self {
            Overlay::Palette(p) => p.geometry(),
            Overlay::ConfirmQuit(q) => q.geometry(),
            Overlay::ConfirmOverwrite(o) => o.geometry(),
            Overlay::None => (0, 0),
        }
    }

    /// Hint string for the bottom bar.
    pub fn hints(&self) -> &'static str {
        match self {
            Overlay::Palette(p) => p.hints(),
            Overlay::ConfirmQuit(_) => "y/w save+quit · n quit · Esc cancel",
            Overlay::ConfirmOverwrite(_) => "o overwrite · r reload · Esc cancel",
            Overlay::None => "",
        }
    }

    /// Get the command to execute (if the selected row is a Command).
    pub fn selected_command(&self) -> Option<Command> {
        if let Overlay::Palette(p) = self {
            p.selected_command()
        } else {
            None
        }
    }

    /// Get the confirm result (if a confirm overlay is open).
    pub fn confirm_result(&self) -> Option<ConfirmResult> {
        match self {
            Overlay::ConfirmQuit(q) => Some(q.result()),
            Overlay::ConfirmOverwrite(o) => Some(o.result()),
            _ => None,
        }
    }
}
