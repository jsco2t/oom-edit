//! Command registry — the single source of truth for app chrome commands.
//!
//! Static rows separate registry identity, executable App commands, and
//! visibility-only core bindings.
//! [`COMMANDS`] carries each command's kebab-case name, human description,
//! available [`Contexts`], hint-bar order, and quick-bar eligibility.
//!
//! Pure transitions own App-prefix routing; [`App`](crate::app) owns dispatch.
//! Projections (which-key, palette, hint bar) read metadata from this module.

pub mod keymap;
pub mod registry;

pub use registry::*;
