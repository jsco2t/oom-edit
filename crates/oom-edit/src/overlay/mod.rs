//! Overlay system — single-slot overlay with take-and-return-bool key protocol.
//!
//! [`Overlay`] is a flat enum with one active slot. Each overlay variant
//! implements the key protocol: `handle_key(&mut self, key) -> bool` where
//! `true` means "consumed" and `false` means "pass through".
//!
//! `centered()` returns the preferred geometry; `clear()` resets state.

pub mod palette;

pub use palette::PaletteState;

use crate::command::Command;

/// An active overlay slot. Only one overlay can be open at a time.
#[derive(Debug, Default)]
pub enum Overlay {
    /// No overlay is open.
    #[default]
    None,
    /// The command palette (FR-6.6/6.7).
    Palette(PaletteState),
    /// Confirm quit overlay (T16).
    #[allow(dead_code)]
    ConfirmQuit,
    /// Confirm overwrite overlay (T16).
    #[allow(dead_code)]
    ConfirmOverwrite,
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
    pub fn open_palette() -> Self {
        Overlay::Palette(PaletteState::default())
    }

    /// Close the current overlay, returning the old value.
    pub fn close(&mut self) -> Overlay {
        std::mem::replace(self, Overlay::None)
    }

    /// Handle a key event. Returns `true` if consumed.
    pub fn handle_key(&mut self, key: &oom_edit_core::session::KeyInput) -> bool {
        match self {
            Overlay::Palette(p) => p.handle_key(key),
            Overlay::ConfirmQuit => false,
            Overlay::ConfirmOverwrite => false,
            Overlay::None => false,
        }
    }

    /// Render the overlay.
    pub fn render(&self, frame: &mut ratatui::Frame<'_>) {
        if let Overlay::Palette(p) = self {
            p.render(frame);
        }
    }

    /// Preferred centered geometry (width, height) in character cells.
    #[allow(dead_code)]
    pub fn geometry(&self) -> (u16, u16) {
        match self {
            Overlay::Palette(p) => p.geometry(),
            Overlay::ConfirmQuit => (40, 7),
            Overlay::ConfirmOverwrite => (40, 7),
            Overlay::None => (0, 0),
        }
    }

    /// Hint string for the bottom bar.
    #[allow(dead_code)]
    pub fn hints(&self) -> &'static str {
        match self {
            Overlay::Palette(p) => p.hints(),
            Overlay::ConfirmQuit => "y/n to confirm",
            Overlay::ConfirmOverwrite => "y/n to confirm",
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
}
