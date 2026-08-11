//! Headless example: demonstrates `EditorSession` usage without any terminal
//! dependencies. Builds a session from an embedded markdown string, plays a
//! source-mapped rendered selections, enters raw-source Insert, renders both models, and
//! exits.
//!
//! This is the reference host application that proves `oom-edit-core` is
//! embeddable (FR-8.4, SC-4).

use oom_edit_core::{EditorSession, KeyCode, KeyCodeKind, KeyInput, Modifiers, Viewport};

/// Tiny ANSI escape mapper — local to this example only.
fn style_to_ansi(style: &oom_edit_core::SemanticStyle) -> &'static str {
    match style {
        oom_edit_core::SemanticStyle::Heading1
        | oom_edit_core::SemanticStyle::Heading2
        | oom_edit_core::SemanticStyle::Heading3
        | oom_edit_core::SemanticStyle::Heading4
        | oom_edit_core::SemanticStyle::Heading5
        | oom_edit_core::SemanticStyle::Heading6 => "\x1b[1m",
        oom_edit_core::SemanticStyle::Strong => "\x1b[1m",
        oom_edit_core::SemanticStyle::Emphasis => "\x1b[3m",
        oom_edit_core::SemanticStyle::Strikethrough => "\x1b[9m",
        oom_edit_core::SemanticStyle::CodeSpan | oom_edit_core::SemanticStyle::CodeBlock => {
            "\x1b[36m"
        }
        oom_edit_core::SemanticStyle::Link | oom_edit_core::SemanticStyle::LinkUrl => "\x1b[34m",
        oom_edit_core::SemanticStyle::Comment => "\x1b[2m",
        oom_edit_core::SemanticStyle::ListMarker => "\x1b[33m",
        oom_edit_core::SemanticStyle::Rule => "\x1b[33m",
        oom_edit_core::SemanticStyle::Quote => "\x1b[2m",
        oom_edit_core::SemanticStyle::HtmlRaw => "\x1b[35m",
        oom_edit_core::SemanticStyle::FmDelimiter => "\x1b[2m",
        oom_edit_core::SemanticStyle::FmKey => "\x1b[35m",
        oom_edit_core::SemanticStyle::FmValue => "\x1b[37m",
        oom_edit_core::SemanticStyle::Keyword => "\x1b[31m",
        oom_edit_core::SemanticStyle::Function => "\x1b[32m",
        oom_edit_core::SemanticStyle::TypeName => "\x1b[36m",
        oom_edit_core::SemanticStyle::StringLit => "\x1b[33m",
        oom_edit_core::SemanticStyle::NumberLit => "\x1b[33m",
        oom_edit_core::SemanticStyle::Operator => "\x1b[37m",
        oom_edit_core::SemanticStyle::Variable => "\x1b[37m",
        oom_edit_core::SemanticStyle::Punct => "\x1b[37m",
        oom_edit_core::SemanticStyle::Selection => "\x1b[7m",
        oom_edit_core::SemanticStyle::Match => "\x1b[7m",
        oom_edit_core::SemanticStyle::CursorLine => "\x1b[7m",
        oom_edit_core::SemanticStyle::Text => "",
        _ => "",
    }
}

fn print_styled_line(line: &oom_edit_core::StyledLine) {
    let mut last_style: Option<&oom_edit_core::SemanticStyle> = None;
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

fn esc() -> KeyInput {
    KeyInput {
        code: KeyCode {
            kind: KeyCodeKind::Esc,
        },
        mods: Modifiers::default(),
    }
}

fn ctrl(ch: char) -> KeyInput {
    let mut input = key(ch);
    input.mods.ctrl = true;
    input
}

fn main() {
    let markdown = "# Hello World\n\nThis is **bold** and `code`.\n\n- item one\n- item two\n";
    let mut session = EditorSession::from_text(markdown);

    println!("=== Initial state ===");
    println!("Mode: {:?}", session.mode());
    println!("Lines: {}", session.line_count());

    // Render Normal at the host's actual text width.
    let layout = session.render_layout(80);
    println!("\n=== Rendered Normal layout ===");
    println!("Lines: {}", layout.lines.len());
    println!("Jump targets: {}", layout.jump_targets.len());
    println!("Rendered cursor: {:?}", session.rendered_cursor());

    // Character-wise Select exposes display geometry and raw-source ranges.
    let before_yank = session.document();
    session.handle_key(key('v'));
    println!("Mode during selection: {:?}", session.mode());
    session.handle_key(key('l'));
    let character = session.rendered_selection().expect("character selection");
    println!(
        "Character selection: {:?} {:?}",
        character.shape, character.source_ranges
    );
    session.handle_key(key('y'));
    println!(
        "Yank preserved document: {}",
        session.document() == before_yank
    );

    session.render_layout(80);
    session.handle_key(key('V'));
    println!(
        "Line selection: {:?}",
        session.rendered_selection().expect("line selection").shape
    );
    session.handle_key(esc());

    session.handle_key(ctrl('v'));
    println!(
        "Block selection: {:?}",
        session.rendered_selection().expect("block selection").shape
    );
    session.handle_key(esc());

    // Enter Insert at the mapped source position, add one marker, and return
    // to rendered Normal.
    session.handle_key(key('i'));
    println!("Mode during source edit: {:?}", session.mode());
    session.handle_key(key('!'));
    session.handle_key(esc());
    println!("Mode after Insert: {:?}", session.mode());

    // Render the raw source surface by entering Insert once more.
    session.handle_key(key('i'));
    let vp = Viewport {
        top_line: 0,
        height: 10,
        width: 80,
        wrap: true,
        left_col: 0,
        skip_rows: 0,
    };
    let frame = session.render_source(vp);
    println!(
        "\n=== Insert source frame ({} lines) ===",
        frame.lines.len()
    );
    for line in &frame.lines {
        print_styled_line(line);
    }

    session.handle_key(esc());

    // Test :help effect
    let _effects = session.handle_key(key(':'));
    let _effects = session.handle_key(key('h'));
    let _effects = session.handle_key(key('e'));
    let _effects = session.handle_key(key('l'));
    let _effects = session.handle_key(key('p'));
    println!("\n=== Help effect ===");
    let effects = session.handle_key(KeyInput {
        code: KeyCode {
            kind: KeyCodeKind::Enter,
        },
        mods: Modifiers::default(),
    });
    println!(
        "Help requested: {}",
        effects
            .iter()
            .any(|effect| matches!(effect, oom_edit_core::Effect::HelpRequested))
    );

    println!("\n=== Done ===");
}
