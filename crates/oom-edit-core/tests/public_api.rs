//! Downstream-style guards for the curated root facade.

use oom_edit_core::{
    ClipboardError, ClipboardSink, EditorSession, Effect, FmError, FrontMatter, JumpTarget,
    KeyCode, KeyCodeKind, KeyInput, LineEnding, LineKind, Mode, Modifiers, Num, OpenError,
    RecordingClipboardSink, RenderedLayout, RenderedLine, RenderedLineRole, RenderedPoint,
    RenderedSearch, RenderedSelection, RenderedSelectionRow, RenderedSourceAtom, SaveError,
    SearchDirection, SelectionShape, SemanticStyle, Severity, SourceFrame, Span, StyledLine,
    TargetKind, Value, Viewport,
};

#[test]
fn public_facade_types_are_available_at_crate_root() {
    let mut session = EditorSession::from_text("# root\n");
    let _: Mode = session.mode();
    let _: &FrontMatter = session.front_matter();
    let _: LineEnding = session.line_ending();
    let _: RenderedPoint = session.rendered_cursor();
    let _: &RenderedLayout = session.render_layout(20);
    let _: Option<RenderedSelection> = session.rendered_selection();
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
        Span,
        StyledLine,
        TargetKind,
        Value,
    )>();
}

#[test]
fn implementation_modules_are_not_public() {
    let lib = include_str!("../src/lib.rs");
    for module in [
        "document",
        "frontmatter",
        "rendered",
        "session",
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
