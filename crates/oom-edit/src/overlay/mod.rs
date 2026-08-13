//! Overlay system — one exclusive active slot.
//!
//! [`Overlay`] is a flat enum with one active slot. Each overlay variant
//! Palette input uses a consumed/pass-through protocol. Confirmation variants
//! own their complete lifecycle request and return a semantic resolution.
//!
//! `centered()` returns the preferred geometry; `clear()` resets state.

pub mod confirm;
pub mod palette;

pub use confirm::{
    ConfirmOverwrite, ConfirmQuit, ConfirmationResolution, DirtyCloseChoice, ExternalSaveChoice,
};
pub use palette::PaletteState;

use crate::command::{AppCommand, Contexts};
use crate::lifecycle::{CloseTabRequest, SaveRequest};
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
    pub fn open_confirm_quit(action: CloseTabRequest) -> Self {
        Overlay::ConfirmQuit(ConfirmQuit::for_action(action))
    }

    /// Open the confirm-overwrite overlay.
    pub fn open_confirm_overwrite(request: SaveRequest, disk_path: std::path::PathBuf) -> Self {
        Overlay::ConfirmOverwrite(ConfirmOverwrite::for_request(request, disk_path))
    }

    /// Close the current overlay, returning the old value.
    pub fn close(&mut self) -> Overlay {
        std::mem::replace(self, Overlay::None)
    }

    /// Handle a key event. Returns `true` if consumed.
    pub fn handle_key(&mut self, key: &oom_edit_core::KeyInput) -> bool {
        match self {
            Overlay::Palette(p) => p.handle_key(key),
            Overlay::ConfirmQuit(_) | Overlay::ConfirmOverwrite(_) => true,
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
    pub fn selected_command(&self) -> Option<AppCommand> {
        if let Overlay::Palette(p) = self {
            p.selected_command()
        } else {
            None
        }
    }

    pub fn is_confirmation(&self) -> bool {
        matches!(self, Overlay::ConfirmQuit(_) | Overlay::ConfirmOverwrite(_))
    }

    /// Handle confirmation input exclusively and return a semantic resolution.
    pub fn handle_confirmation_key(
        &mut self,
        key: &oom_edit_core::KeyInput,
    ) -> Option<ConfirmationResolution> {
        match self {
            Overlay::ConfirmQuit(q) => q.resolve_key(key),
            Overlay::ConfirmOverwrite(o) => o.resolve_key(key),
            _ => None,
        }
    }
}
