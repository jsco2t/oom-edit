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
//! The crate root is the complete supported facade:
//! - [`EditorSession`] — the main editing session type
//! - [`Mode`] — exactly Normal, Insert, Select, and Command
//! - [`Effect`] — typed requests emitted by session operations
//! - [`KeyInput`] — the shared terminal-neutral input model
//! - [`Viewport`] and [`SourceFrame`] — raw-source Insert rendering
//! - [`RenderedLayout`] and [`RenderedSelection`] — rendered Normal/Select output
//! - [`SemanticStyle`] — renderer-agnostic style slots
//!
//! # Example
//!
//! ```
//! use oom_edit_core::EditorSession;
//!
//! let mut session = EditorSession::from_text("# Hello\n\nWorld\n");
//! assert_eq!(session.mode(), oom_edit_core::Mode::Normal);
//! assert_eq!(session.line_count(), 4);
//!
//! // Normal is rendered Markdown. Hosts supply the actual text width.
//! let layout = session.render_layout(80);
//! assert!(!layout.lines.is_empty());
//! ```
//!
//! Implementation modules are intentionally not part of the facade:
//!
//! ```compile_fail
//! use oom_edit_core::document::Document;
//! ```
//! ```compile_fail
//! use oom_edit_core::session::EditorSession;
//! ```
//! ```compile_fail
//! use oom_edit_core::syntax::LANGUAGES;
//! ```
//! ```compile_fail
//! use oom_edit_core::Highlighter;
//! ```

#![deny(missing_docs)]
#![forbid(unsafe_code)]

mod clipboard;
mod document;
mod error;
mod frontmatter;
mod input;
mod rendered;
mod session;
mod spell;
mod style;
mod syntax;
mod vim;

// ── Public re-exports ──────────────────────────────────────────────────────

// Complete public API surface.
//
// These re-exports form the public API of `oom-edit-core`. Nothing more
// is exported publicly; this `pub use` list *is* the API contract (FR-8.3).

pub use clipboard::{ClipboardError, ClipboardSink, RecordingClipboardSink};
pub use document::LineEnding;
pub use error::{FmError, OpenError, SaveError};
pub use frontmatter::{FrontMatter, Num, Value};
pub use input::{KeyCode, KeyCodeKind, KeyInput, Modifiers};
pub use session::EditorSession;
pub use session::{Effect, Mode, Severity, Viewport};
pub use style::{
    JumpTarget, LineKind, RenderedLayout, RenderedLine, RenderedLineRole, RenderedPoint,
    RenderedSearch, RenderedSelection, RenderedSelectionRow, RenderedSourceAtom, SearchDirection,
    SelectionShape, SemanticStyle, SourceFrame, Span, StyledLine, TargetKind,
};

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
