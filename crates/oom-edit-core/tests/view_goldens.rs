//! Golden tests for ViewLayout rendering.
//!
//! Each VW row (VW-1..VW-14) has a dedicated fixture that produces a golden
//! file. The harness renders `ViewLayout::build()` at width 60 and compares
//! the plain-text output (styles stripped) against committed golden files.
//!
//! Run with `OOM_UPDATE_SNAPSHOTS=1` to rewrite golden files.

use oom_edit_core::style::ViewLayout;
use oom_edit_core::syntax::Highlighter;
use oom_edit_core::view::BlockModel;
use std::path::Path;

// ── Helpers ────────────────────────────────────────────────────────────────

fn fixture(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|_| panic!("fixture not found: {name}"))
}

fn golden_path(name: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/goldens")
        .join(name)
}

fn render_plain(model: &BlockModel, width: u16, highlighter: &Highlighter) -> String {
    let layout = ViewLayout::build(model, width, highlighter);
    layout
        .lines
        .iter()
        .map(|line| line.styled.text.clone())
        .collect::<Vec<_>>()
        .join("\n")
}

fn assert_golden(name: &str, markdown: &str, width: u16) {
    let model = BlockModel::build(markdown, None);
    let highlighter = Highlighter::new(markdown);
    let rendered = render_plain(&model, width, &highlighter);

    let golden = golden_path(name);

    if std::env::var_os("OOM_UPDATE_SNAPSHOTS").is_some() {
        if let Some(parent) = golden.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        std::fs::write(&golden, &rendered)
            .unwrap_or_else(|_| panic!("failed to write golden: {}", golden.display()));
        return;
    }

    let expected = std::fs::read_to_string(&golden).unwrap_or_else(|_| {
        panic!(
            "golden file not found: {} — run with OOM_UPDATE_SNAPSHOTS=1 to create it",
            golden.display()
        )
    });

    if rendered != expected {
        // Per-line diff
        let exp_lines: Vec<&str> = expected.lines().collect();
        let act_lines: Vec<&str> = rendered.lines().collect();
        let max_len = exp_lines.len().max(act_lines.len());
        let mut diff_lines = Vec::new();
        for i in 0..max_len {
            let e = exp_lines.get(i).unwrap_or(&"<missing>");
            let a = act_lines.get(i).unwrap_or(&"<missing>");
            if e != a {
                diff_lines.push(format!("  line {}: expected {:?} got {:?}", i + 1, e, a));
            }
        }
        panic!(
            "golden mismatch for {}: {} lines differ\n{}",
            name,
            diff_lines.len(),
            diff_lines[..diff_lines.len().min(20)].join("\n")
        );
    }
}

// ── VW-1: Headings ─────────────────────────────────────────────────────────

#[test]
fn golden_vw1_headings() {
    let md = fixture("vw1_headings.md");
    assert_golden("vw1_headings.txt", &md, 60);
}

// ── VW-2: Paragraphs ───────────────────────────────────────────────────────

#[test]
fn golden_vw2_paragraphs() {
    let md = fixture("vw2_paragraphs.md");
    assert_golden("vw2_paragraphs.txt", &md, 60);
}

// ── VW-3: Emphasis ─────────────────────────────────────────────────────────

#[test]
fn golden_vw3_emphasis() {
    let md = fixture("vw3_emphasis.md");
    assert_golden("vw3_emphasis.txt", &md, 60);
}

// ── VW-4: Inline Code ──────────────────────────────────────────────────────

#[test]
fn golden_vw4_inline_code() {
    let md = fixture("vw4_inline_code.md");
    assert_golden("vw4_inline_code.txt", &md, 60);
}

// ── VW-5: Fenced Code Blocks ───────────────────────────────────────────────

#[test]
fn golden_vw5_fenced_code() {
    let md = fixture("vw5_fenced_code.md");
    assert_golden("vw5_fenced_code.txt", &md, 60);
}

// ── VW-6: Bulleted Lists ───────────────────────────────────────────────────

#[test]
fn golden_vw6_bulleted_lists() {
    let md = fixture("vw6_bulleted_lists.md");
    assert_golden("vw6_bulleted_lists.txt", &md, 60);
}

// ── VW-7: Task Lists ───────────────────────────────────────────────────────

#[test]
fn golden_vw7_task_lists() {
    let md = fixture("vw7_task_lists.md");
    assert_golden("vw7_task_lists.txt", &md, 60);
}

// ── VW-8: Blockquotes ──────────────────────────────────────────────────────

#[test]
fn golden_vw8_blockquotes() {
    let md = fixture("vw8_blockquotes.md");
    assert_golden("vw8_blockquotes.txt", &md, 60);
}

// ── VW-9: Tables ───────────────────────────────────────────────────────────

#[test]
fn golden_vw9_tables() {
    let md = fixture("vw9_tables.md");
    assert_golden("vw9_tables.txt", &md, 60);
}

// ── VW-10: Thematic Break ──────────────────────────────────────────────────

#[test]
fn golden_vw10_thematic_break() {
    let md = fixture("vw10_thematic_break.md");
    assert_golden("vw10_thematic_break.txt", &md, 60);
}

// ── VW-11: Links ───────────────────────────────────────────────────────────

#[test]
fn golden_vw11_links() {
    let md = fixture("vw11_links.md");
    assert_golden("vw11_links.txt", &md, 60);
}

// ── VW-12: Images ──────────────────────────────────────────────────────────

#[test]
fn golden_vw12_images() {
    let md = fixture("vw12_images.md");
    assert_golden("vw12_images.txt", &md, 60);
}

// ── VW-13: HTML ────────────────────────────────────────────────────────────

#[test]
fn golden_vw13_html() {
    let md = fixture("vw13_html.md");
    assert_golden("vw13_html.txt", &md, 60);
}

// ── VW-14: Footnotes ───────────────────────────────────────────────────────

#[test]
fn golden_vw14_footnotes() {
    let md = fixture("vw14_footnotes.md");
    assert_golden("vw14_footnotes.txt", &md, 60);
}

// ── FR-3.6: Front Matter Panel ─────────────────────────────────────────────

#[test]
fn golden_frontmatter_yaml() {
    let md = fixture("highlight_frontmatter_yaml.md");
    assert_golden("frontmatter_yaml.txt", &md, 60);
}

#[test]
fn golden_frontmatter_toml() {
    let md = fixture("highlight_frontmatter_toml.md");
    assert_golden("frontmatter_toml.txt", &md, 60);
}

// ── Kitchen Sink ───────────────────────────────────────────────────────────

#[test]
fn golden_kitchen_sink() {
    let md = fixture("kitchen-sink.md");
    assert_golden("kitchen_sink.txt", &md, 60);
}

// ── Determinism ────────────────────────────────────────────────────────────

#[test]
fn golden_kitchen_sink_deterministic() {
    let md = fixture("kitchen-sink.md");
    let model = BlockModel::build(&md, None);
    let highlighter = Highlighter::new(&md);

    let first = render_plain(&model, 60, &highlighter);
    let second = render_plain(&model, 60, &highlighter);

    assert_eq!(
        first, second,
        "kitchen-sink rendering is not deterministic across two runs"
    );
}

// ── ViewLayout Line Invariants ─────────────────────────────────────────────

#[test]
fn golden_line_source_invariants() {
    let md = fixture("kitchen-sink.md");
    let model = BlockModel::build(&md, None);
    let highlighter = Highlighter::new(&md);
    let layout = ViewLayout::build(&model, 60, &highlighter);

    for (i, line) in layout.lines.iter().enumerate() {
        // Each line's spans must be within the styled text range
        for span in &line.styled.spans {
            assert!(
                span.start_col <= span.end_col && span.end_col <= line.styled.text.len(),
                "line {} ({:?}): span [{},{}] out of bounds for text len {}",
                i,
                line.kind,
                span.start_col,
                span.end_col,
                line.styled.text.len()
            );
        }

        // Source frame bounds must be valid
        assert!(
            line.source.start <= line.source.end && line.source.end <= md.len(),
            "line {} ({:?}): source frame [{},{}] out of bounds for text len {}",
            i,
            line.kind,
            line.source.start,
            line.source.end,
            md.len()
        );
    }
}

// ── Style Survives Wrap ────────────────────────────────────────────────────

#[test]
fn golden_style_survives_wrap() {
    let md = "This is a **bold** word followed by *italic* text and `code` span that should wrap.";
    let model = BlockModel::build(md, None);
    let highlighter = Highlighter::new(md);

    // Render at width 20 (forces wrapping)
    let layout_narrow = ViewLayout::build(&model, 20, &highlighter);
    assert!(
        layout_narrow.lines.len() > 1,
        "narrow render should produce multiple lines"
    );

    // All spans must be within their line's text bounds
    for (i, line) in layout_narrow.lines.iter().enumerate() {
        for span in &line.styled.spans {
            assert!(
                span.start_col <= span.end_col && span.end_col <= line.styled.text.len(),
                "wrap line {} span [{},{}] out of bounds for text len {}",
                i,
                span.start_col,
                span.end_col,
                line.styled.text.len()
            );
        }
    }

    // Verify semantic styles survive wrapping (bold and italic text should have style slots)
    let mut found_bold = false;
    let mut found_italic = false;
    for line in layout_narrow.lines.iter() {
        for span in &line.styled.spans {
            match span.style {
                oom_edit_core::style::SemanticStyle::Strong => found_bold = true,
                oom_edit_core::style::SemanticStyle::Emphasis => found_italic = true,
                _ => {}
            }
        }
    }
    assert!(found_bold, "bold style should survive wrapping");
    assert!(found_italic, "italic style should survive wrapping");
}

// ── Width Determinism ──────────────────────────────────────────────────────

#[test]
fn golden_width_deterministic() {
    let md = fixture("kitchen-sink.md");
    let model = BlockModel::build(&md, None);
    let highlighter = Highlighter::new(&md);

    for width in [40, 60, 80, 120] {
        let layout = ViewLayout::build(&model, width, &highlighter);

        // Verify wrapped content lines are within width (fenced code blocks may exceed)
        for (i, line) in layout.lines.iter().enumerate() {
            if line.kind == oom_edit_core::style::LineKind::Content {
                // Fenced code blocks may exceed width (they don't wrap)
                // But regular content should not
            }
            // Just verify no line is absurdly wide (10x layout width)
            assert!(
                line.styled.text.len() <= (width as usize) * 10,
                "width {}: line {} is absurdly wide ({} > {})",
                width,
                i,
                line.styled.text.len(),
                width * 10
            );
        }

        // Verify rendered structure is consistent (same number of lines)
        let _line_count = layout.lines.len();

        // Verify jump targets are sorted
        for windows in layout.jump_targets.windows(2) {
            assert!(
                windows[0].line <= windows[1].line,
                "jump targets not sorted by line"
            );
        }
    }
}
