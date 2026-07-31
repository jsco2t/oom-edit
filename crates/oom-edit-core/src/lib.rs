//! # oom-edit-core
//!
//! Embeddable markdown editing engine.  Provides a tree-sitter-highlighted
//! document model, Vim-style modal editing, and a renderer-agnostic style
//! system.
//!
//! **Zero terminal dependencies** — this crate can be used headlessly or
//! embedded in any application.

#![deny(missing_docs)]
#![forbid(unsafe_code)]

pub mod session;
pub mod style;
mod vim;
