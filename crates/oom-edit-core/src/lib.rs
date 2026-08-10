//! # oom-edit-core
//!
//! Embeddable markdown editing engine.  Provides a tree-sitter-highlighted
//! document model, rendered Normal/Select navigation, raw-source Insert
//! editing, and a renderer-agnostic style system.
//!
//! **Zero terminal dependencies** — this crate can be used headlessly or
//! embedded in any application.
//!
//! # Public API
//!
//! The public API is re-exported from [`session`] and [`style`]:
//! - [`session::EditorSession`] — the main editing session type
//! - [`session::Mode`] — exactly Normal, Insert, Select, and Command
//! - [`session::Effect`] — effects emitted by session operations
//! - [`session::Viewport`] — viewport specification for rendering
//! - [`style::SourceFrame`] — raw-source Insert frame
//! - [`style::RenderedLayout`] — rendered Normal/Select layout
//! - [`style::RenderedSelection`] — rendered Select rows and source range
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
//! // Normal is rendered Markdown. Hosts supply the actual text width.
//! let layout = session.render_layout(80);
//! assert!(!layout.lines.is_empty());
//! ```

#![deny(missing_docs)]
#![forbid(unsafe_code)]

pub mod clipboard;
pub mod document;
pub mod error;
pub mod frontmatter;
mod rendered;
pub mod session;
pub mod style;
pub mod syntax;
mod vim;

// ── Public re-exports ──────────────────────────────────────────────────────

// Complete public API surface.
//
// These re-exports form the public API of `oom-edit-core`. Nothing more
// is exported publicly; this `pub use` list *is* the API contract (FR-8.3).

pub use clipboard::{ClipboardError, ClipboardSink, RecordingClipboardSink};
pub use error::{OpenError, SaveError};
pub use session::EditorSession;
pub use session::{Effect, KeyCode, KeyCodeKind, KeyInput, Mode, Modifiers, Severity, Viewport};
pub use style::{
    JumpTarget, LineKind, RenderedLayout, RenderedLine, RenderedLineRole, RenderedPoint,
    RenderedSearch, RenderedSelection, RenderedSelectionRow, RenderedSourceAtom, SearchDirection,
    SelectionShape, SemanticStyle, SourceFrame, Span, StyledLine, TargetKind,
};
pub use syntax::Highlighter;

#[cfg(test)]
mod public_api_tests {
    use crate::Mode;

    #[test]
    fn public_modes_are_normal_insert_select_command() {
        fn label(mode: Mode) -> &'static str {
            match mode {
                Mode::Normal => "Normal",
                Mode::Insert => "Insert",
                Mode::Select => "Select",
                Mode::Command => "Command",
            }
        }

        let modes = [Mode::Normal, Mode::Insert, Mode::Select, Mode::Command];
        let labels: Vec<_> = modes.into_iter().map(label).collect();
        assert_eq!(labels, ["Normal", "Insert", "Select", "Command"]);
        assert_eq!(format!("{:?}", Mode::Select), "Select");
    }
}
