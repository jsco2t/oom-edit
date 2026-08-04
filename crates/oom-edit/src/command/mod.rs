//! Command registry — the single source of truth for app chrome commands.
//!
//! [`Command`] is a closed enum (one variant per discrete user-facing action).
//! [`COMMANDS`] carries each command's kebab-case name, human description,
//! available [`Contexts`], hint-bar order, and quick-bar eligibility.
//!
//! [`Keymap`] owns triggers; [`App`](crate::app) owns dispatch.
//! Projections (which-key, palette, hint bar) read metadata from this module.

pub mod keymap;
pub mod registry;

pub use keymap::Keymap;
pub use registry::*;
