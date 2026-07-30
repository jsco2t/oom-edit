//! Downstream-style guards for the curated root facade.

use oom_edit_core::{
    ClipboardError, ClipboardSink, DecorationKind, Diagnostic, DiagnosticDecorationRow,
    DiagnosticProvider, DiagnosticSeverity, EditorSession, Effect, FmError, FrontMatter,
    JumpTarget, KeyCode, KeyCodeKind, KeyInput, LineEnding, LineKind, Mode, Modifiers, Num,
    OpenError, PositionError, RecordingClipboardSink, RenderedLayout, RenderedLine,
    RenderedLineRole, RenderedPoint, RenderedSearch, RenderedSelection, RenderedSelectionRow,
    RenderedSourceAtom, SaveError, SearchDirection, SelectionShape, SemanticStyle, Severity,
    SourceDecoration, SourceFrame, Span, StyledLine, TargetKind, TextPosition, Value, Viewport,
};
use std::path::Path;

fn public_use_declarations(source: &str) -> Vec<String> {
    let mut declarations = Vec::new();
    let mut current = String::new();

    for line in source.lines() {
        let trimmed = line.trim();
        if current.is_empty() {
            if !trimmed.starts_with("pub use ") {
                continue;
            }
            current.push_str(trimmed);
        } else {
            current.push(' ');
            current.push_str(trimmed);
        }

        if trimmed.ends_with(';') {
            declarations.push(std::mem::take(&mut current));
        }
    }

    assert!(current.is_empty(), "unterminated pub use declaration");
    declarations
}

fn assert_no_exported_macros(path: &Path) {
    for entry in std::fs::read_dir(path).expect("core source directory should be readable") {
        let path = entry.expect("core source entry should be readable").path();
        if path.is_dir() {
            assert_no_exported_macros(&path);
        } else if path.extension().and_then(|extension| extension.to_str()) == Some("rs") {
            let source =
                std::fs::read_to_string(&path).expect("core Rust source should be readable");
            let compact_source: String = source.split_whitespace().collect();
            assert!(
                !compact_source.contains("#[macro_export"),
                "exported macro bypasses the curated crate-root facade: {}",
                path.display()
            );
        }
    }
}

#[test]
fn public_facade_types_are_available_at_crate_root() {
    let mut session = EditorSession::from_text("# root\n");
    let _: Mode = session.mode();
    let _: &FrontMatter = session.front_matter();
    let _: LineEnding = session.line_ending();
    let _: RenderedPoint = session.rendered_cursor();
    let _: &RenderedLayout = session.render_layout(20);
    let _: Option<RenderedSelection> = session.rendered_selection();
    let _: RenderedLineRole = RenderedLineRole::CodeFence;
    let _: SourceFrame = session.render_source(Viewport {
        top_line: 0,
        height: 2,
        width: 20,
        wrap: true,
        left_col: 0,
        skip_rows: 0,
    });

    let mut clipboard = RecordingClipboardSink::default();
    ClipboardSink::copy(&mut clipboard, "root").unwrap();

    let _nameable = std::any::TypeId::of::<(
        ClipboardError,
        DecorationKind,
        Diagnostic,
        DiagnosticDecorationRow,
        DiagnosticProvider,
        DiagnosticSeverity,
        Effect,
        FmError,
        JumpTarget,
        KeyCode,
        KeyCodeKind,
        KeyInput,
        LineKind,
        Modifiers,
        Num,
        OpenError,
        PositionError,
        RenderedLine,
        RenderedLineRole,
        RenderedSearch,
        RenderedSelectionRow,
        RenderedSourceAtom,
        SaveError,
        SearchDirection,
        SelectionShape,
        SemanticStyle,
        Severity,
        SourceDecoration,
        Span,
        StyledLine,
        TargetKind,
        TextPosition,
        Value,
    )>();
}

#[test]
fn public_facade_remains_curated_without_partial_spell_reexports() {
    let source = include_str!("../src/lib.rs");
    let declarations = public_use_declarations(source);
    assert_eq!(
        declarations,
        [
            "pub use clipboard::{ClipboardError, ClipboardSink, RecordingClipboardSink};",
            "pub use document::LineEnding;",
            "pub use error::{FmError, OpenError, SaveError};",
            "pub use frontmatter::{FrontMatter, Num, Value};",
            "pub use input::{KeyCode, KeyCodeKind, KeyInput, Modifiers};",
            "pub use session::EditorSession;",
            "pub use session::{Effect, Mode, Severity, Viewport};",
            "pub use spell::{ DecorationKind, Diagnostic, DiagnosticDecorationRow, DiagnosticProvider, DiagnosticSeverity, PositionError, TextPosition, };",
            "pub use style::{ JumpTarget, LineKind, RenderedLayout, RenderedLine, RenderedLineRole, RenderedPoint, RenderedSearch, RenderedSelection, RenderedSelectionRow, RenderedSourceAtom, SearchDirection, SelectionShape, SemanticStyle, SourceDecoration, SourceFrame, Span, StyledLine, TargetKind, };",
        ],
        "crate-root facade changed; update the API contract and this guard together"
    );

    let unexpected_public_items: Vec<_> = source
        .lines()
        .filter(|line| line.starts_with("pub ") && !line.starts_with("pub use "))
        .collect();
    assert!(
        unexpected_public_items.is_empty(),
        "crate-root facade must consist only of the curated re-exports: {unexpected_public_items:?}"
    );
    assert_no_exported_macros(Path::new(env!("CARGO_MANIFEST_DIR")).join("src").as_path());
    assert!(!source.contains("pub use oom_spell"));
    assert!(include_str!("../src/session.rs").contains("pub fn diagnostic_decoration_rows"));
}

#[test]
fn implementation_modules_are_not_public() {
    let lib = include_str!("../src/lib.rs");
    for module in [
        "document",
        "frontmatter",
        "rendered",
        "session",
        "spell",
        "style",
        "syntax",
        "vim",
    ] {
        assert!(!lib.contains(&format!("pub mod {module};")));
    }
    assert!(!lib.contains("pub use syntax::Highlighter"));
}

#[test]
fn malformed_front_matter_exposes_stable_owned_error() {
    let session = EditorSession::from_text("---\ninvalid: [\n---\n");
    let FrontMatter::Yaml(Err(error)) = session.front_matter() else {
        panic!("expected malformed YAML diagnostic");
    };
    assert!(!error.message().is_empty());
    assert!(error.to_string().starts_with("front matter parse error:"));
}

#[test]
fn front_matter_error_does_not_export_gray_matter() {
    let error_source = include_str!("../src/error.rs");
    assert!(!error_source.contains("pub gray_matter"));
    assert!(!error_source.contains("impl From<gray_matter"));
}
