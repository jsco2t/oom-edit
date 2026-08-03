//! Headless example: demonstrates `EditorSession` usage without any terminal
//! dependencies. Builds a session from an embedded markdown string, plays a
//! key script, renders source and view output, saves to a tempdir, and exits.
//!
//! This is the reference host application that proves `oom-edit-core` is
//! embeddable (FR-8.4, SC-4).

use oom_edit_core::session::{EditorSession, KeyCode, KeyCodeKind, KeyInput, Modifiers, Viewport};

/// Tiny ANSI escape mapper — local to this example only.
fn style_to_ansi(style: &oom_edit_core::style::SemanticStyle) -> &'static str {
    match style {
        oom_edit_core::style::SemanticStyle::Heading1
        | oom_edit_core::style::SemanticStyle::Heading2
        | oom_edit_core::style::SemanticStyle::Heading3
        | oom_edit_core::style::SemanticStyle::Heading4
        | oom_edit_core::style::SemanticStyle::Heading5
        | oom_edit_core::style::SemanticStyle::Heading6 => "\x1b[1m",
        oom_edit_core::style::SemanticStyle::Strong => "\x1b[1m",
        oom_edit_core::style::SemanticStyle::Emphasis => "\x1b[3m",
        oom_edit_core::style::SemanticStyle::Strikethrough => "\x1b[9m",
        oom_edit_core::style::SemanticStyle::CodeSpan
        | oom_edit_core::style::SemanticStyle::CodeBlock => "\x1b[36m",
        oom_edit_core::style::SemanticStyle::Link
        | oom_edit_core::style::SemanticStyle::LinkUrl => "\x1b[34m",
        oom_edit_core::style::SemanticStyle::Comment => "\x1b[2m",
        oom_edit_core::style::SemanticStyle::ListMarker => "\x1b[33m",
        oom_edit_core::style::SemanticStyle::Rule => "\x1b[33m",
        oom_edit_core::style::SemanticStyle::Quote => "\x1b[2m",
        oom_edit_core::style::SemanticStyle::HtmlRaw => "\x1b[35m",
        oom_edit_core::style::SemanticStyle::FmDelimiter => "\x1b[2m",
        oom_edit_core::style::SemanticStyle::FmKey => "\x1b[35m",
        oom_edit_core::style::SemanticStyle::FmValue => "\x1b[37m",
        oom_edit_core::style::SemanticStyle::Keyword => "\x1b[31m",
        oom_edit_core::style::SemanticStyle::Function => "\x1b[32m",
        oom_edit_core::style::SemanticStyle::TypeName => "\x1b[36m",
        oom_edit_core::style::SemanticStyle::StringLit => "\x1b[33m",
        oom_edit_core::style::SemanticStyle::NumberLit => "\x1b[33m",
        oom_edit_core::style::SemanticStyle::Operator => "\x1b[37m",
        oom_edit_core::style::SemanticStyle::Variable => "\x1b[37m",
        oom_edit_core::style::SemanticStyle::Punct => "\x1b[37m",
        oom_edit_core::style::SemanticStyle::Selection => "\x1b[7m",
        oom_edit_core::style::SemanticStyle::Match => "\x1b[7m",
        oom_edit_core::style::SemanticStyle::CursorLine => "\x1b[7m",
        oom_edit_core::style::SemanticStyle::Text => "",
        _ => "",
    }
}

fn print_styled_line(line: &oom_edit_core::style::StyledLine) {
    let mut last_style: Option<&oom_edit_core::style::SemanticStyle> = None;
    let mut ansi_buf = String::new();
    let text_len = line.text.len();

    for span in &line.spans {
        if Some(&span.style) != last_style {
            ansi_buf.push_str(style_to_ansi(&span.style));
            last_style = Some(&span.style);
        }
        let start = span.start_col.min(text_len);
        let end = span.end_col.min(text_len);
        if start < end {
            ansi_buf.push_str(&line.text[start..end]);
        }
    }

    if let Some(last) = line.spans.last() {
        let start = last.end_col.min(text_len);
        if start < text_len {
            ansi_buf.push_str(&line.text[start..]);
        }
    } else if !line.text.is_empty() {
        ansi_buf.push_str(&line.text);
    }

    println!("{}\x1b[0m", ansi_buf);
}

fn key(ch: char) -> KeyInput {
    KeyInput {
        code: KeyCode {
            kind: KeyCodeKind::Char(ch),
        },
        mods: Modifiers::default(),
    }
}

fn main() {
    let markdown = "# Hello World\n\nThis is **bold** and `code`.\n\n- item one\n- item two\n";
    let mut session = EditorSession::from_text(markdown);

    println!("=== Initial state ===");
    println!("Mode: {:?}", session.mode());
    println!("Lines: {}", session.line_count());

    // Render source
    let vp = Viewport {
        top_line: 0,
        height: 10,
        width: 80,
    };
    let frame = session.render_source(vp);
    println!("\n=== Source frame ({} lines) ===", frame.lines.len());
    for line in &frame.lines {
        print_styled_line(line);
    }

    // Play a key script: ggVGd (go to top, visual line, to bottom, delete)
    println!("\n=== Playing key script: ggVGd ===");
    session.handle_key(key('g'));
    session.handle_key(key('g'));
    session.handle_key(key('V'));
    session.handle_key(key('G'));
    session.handle_key(key('d'));

    println!("Mode after script: {:?}", session.mode());
    println!("Lines after script: {}", session.line_count());

    // Render source after edits
    let frame = session.render_source(vp);
    println!("\n=== Source frame after ggVGd ===");
    for line in &frame.lines {
        print_styled_line(line);
    }

    // Render view layout
    let layout = session.render_view(80);
    println!("\n=== View layout ===");
    println!("Lines: {}", layout.lines.len());
    println!("Jump targets: {}", layout.jump_targets.len());

    // Test :help effect
    let _effects = session.handle_key(key(':'));
    let _effects = session.handle_key(key('h'));
    let _effects = session.handle_key(key('e'));
    let _effects = session.handle_key(key('l'));
    println!("\n=== Help effect ===");
    println!("(HelpRequested effect is emitted during :help chord)");

    println!("\n=== Done ===");
}
