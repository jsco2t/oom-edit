//! # oom-edit-core
//!
//! Embeddable markdown editing engine.  Provides a tree-sitter-highlighted
//! document model, Vim-style modal editing, and a renderer-agnostic style
//! system.
//!
//! **Zero terminal dependencies** — this crate can be used headlessly or
//! embedded in any application.
//!
//! # Public API
//!
//! The public API is re-exported from [`session`] and [`style`]:
//! - [`session::EditorSession`] — the main editing session type
//! - [`session::Mode`] — editor mode (Normal, Insert, Visual, View, etc.)
//! - [`session::Effect`] — effects emitted by session operations
//! - [`session::Viewport`] — viewport specification for rendering
//! - [`style::SourceFrame`] — rendered source frame
//! - [`style::ViewLayout`] — rendered view layout
//! - [`style::SemanticStyle`] — renderer-agnostic style slots
//!
//! # Example
//!
//! ```
//! use oom_edit_core::session::EditorSession;
//!
//! let mut session = EditorSession::from_text("# Hello\n\nWorld\n");
//! assert_eq!(session.mode(), oom_edit_core::session::Mode::Normal);
//! assert_eq!(session.line_count(), 4);
//!
//! // Render the source editor
//! let vp = oom_edit_core::session::Viewport {
//!     top_line: 0,
//!     height: 10,
//!     width: 80,
//! };
//! let frame = session.render_source(vp);
//! assert_eq!(frame.lines.len(), 10);
//! ```

#![deny(missing_docs)]
#![forbid(unsafe_code)]

pub mod document;
pub mod error;
pub mod frontmatter;
pub mod session;
pub mod style;
pub mod syntax;
pub mod view;
mod vim;

// ── Public re-exports ──────────────────────────────────────────────────────

// Complete public API surface.
//
// These re-exports form the public API of `oom-edit-core`. Nothing more
// is exported publicly; this `pub use` list *is* the API contract (FR-8.3).

pub use session::EditorSession;
pub use session::{Effect, KeyCode, KeyCodeKind, KeyInput, Mode, Modifiers, Severity, Viewport};
pub use style::{SemanticStyle, SourceFrame, Span, StyledLine, ViewLayout};
pub use syntax::Highlighter;
