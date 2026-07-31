//! Markdown block model — typed block tree with byte-accurate source spans.
//!
//! This module parses markdown text into a structured `BlockModel` using
//! `pulldown-cmark`'s offset iterator. The block tree is the input to the
//! layout renderer (T08) and position mapping (T09).
//!
//! Pure data — no styling, no wrapping.
//!
//! See architecture §6.4 and plan §6.3.2 for the element inventory.

mod blocks;

pub use blocks::*;
