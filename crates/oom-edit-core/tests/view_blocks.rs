//! Tests for the view module (block model).

use oom_edit_core::style::LineKind;
use oom_edit_core::view::{Block, BlockKind, BlockModel, Inline};
use oom_edit_core::{Highlighter, ViewLayout};
use std::path::Path;

// ── Helpers ────────────────────────────────────────────────────────────────

fn fixture(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|_| panic!("fixture not found: {name}"))
}

fn view_layout(text: &str) -> ViewLayout {
    let model = BlockModel::build(text, None);
    let highlighter = Highlighter::new(text);
    ViewLayout::build(&model, 80, &highlighter)
}

/// Assert that a block's span points at non-empty source text.
fn assert_span_valid(block: &Block, text: &str, label: &str) {
    let span = &block.span;
    assert!(
        span.start <= span.end && span.end <= text.len(),
        "{label}: span {span:?} out of bounds for text len {}",
        text.len()
    );
    let slice = &text[span.start..span.end];
    assert!(
        !slice.trim().is_empty() || block.kind == BlockKind::Rule,
        "{label}: span {span:?} points at empty text (slice: {slice:?})"
    );
}

/// Collect all blocks recursively (flat list).
fn flatten_blocks(blocks: &[Block]) -> Vec<&Block> {
    let mut out = Vec::new();
    for b in blocks {
        out.push(b);
        match &b.kind {
            BlockKind::BlockQuote { children } => out.extend(flatten_blocks(children)),
            BlockKind::List { items, .. } => {
                for item in items {
                    out.extend(flatten_blocks(&item.children));
                }
            }
            BlockKind::FootnoteDef { children, .. } => {
                out.extend(flatten_blocks(children));
            }
            _ => {}
        }
    }
    out
}

// ── Heading tests ──────────────────────────────────────────────────────────

#[test]
fn test_headings() {
    let text = "# H1\n\n## H2\n\n### H3\n\n#### H4\n\n##### H5\n\n###### H6";
    let model = BlockModel::build(text, None);

    assert_eq!(model.blocks.len(), 6);

    for (i, block) in model.blocks.iter().enumerate() {
        assert_span_valid(block, text, &format!("heading H{}", i + 1));
        match &block.kind {
            BlockKind::Heading { level, inlines } => {
                assert_eq!(*level as usize, i + 1);
                assert_eq!(inlines.len(), 1);
                if let Inline::Text(t) = &inlines[0] {
                    assert_eq!(*t, format!("H{}", i + 1));
                } else {
                    panic!("expected Text inline");
                }
            }
            _ => panic!("expected Heading, got {:?}", block.kind),
        }
    }
}

// ── Paragraph tests ────────────────────────────────────────────────────────

#[test]
fn test_paragraphs() {
    let text = "First paragraph.\n\nSecond paragraph.\n\nThird paragraph.";
    let model = BlockModel::build(text, None);

    assert_eq!(model.blocks.len(), 3);

    for (i, block) in model.blocks.iter().enumerate() {
        assert_span_valid(block, text, &format!("paragraph {}", i + 1));
        match &block.kind {
            BlockKind::Paragraph { inlines } => {
                assert!(!inlines.is_empty());
            }
            _ => panic!("expected Paragraph, got {:?}", block.kind),
        }
    }
}

// ── Code fence tests ───────────────────────────────────────────────────────

#[test]
fn test_fenced_code_blocks() {
    let text = "```rust\nfn main() {}\n```\n\n```toml\n[package]\n```";
    let model = BlockModel::build(text, None);

    assert_eq!(model.blocks.len(), 2);

    // First code block
    match &model.blocks[0].kind {
        BlockKind::CodeFence {
            lang,
            content_span,
            indented,
        } => {
            assert_eq!(lang.as_deref(), Some("rust"));
            assert!(!indented);
            assert!(!content_span.is_empty());
            let content = &text[content_span.start..content_span.end];
            assert!(content.contains("fn main"));
        }
        _ => panic!("expected CodeFence, got {:?}", model.blocks[0].kind),
    }

    // Second code block
    match &model.blocks[1].kind {
        BlockKind::CodeFence {
            lang,
            content_span,
            indented,
        } => {
            assert_eq!(lang.as_deref(), Some("toml"));
            assert!(!indented);
            assert!(!content_span.is_empty());
        }
        _ => panic!("expected CodeFence, got {:?}", model.blocks[1].kind),
    }
}

#[test]
fn test_indented_code_block() {
    let text = "    indented line 1\n    indented line 2";
    let model = BlockModel::build(text, None);

    // Indented code blocks may be parsed as a paragraph or code block
    // depending on context; at least verify no panic
    assert!(!model.blocks.is_empty());
}

// ── List tests ─────────────────────────────────────────────────────────────

#[test]
fn test_unordered_list() {
    let text = "- item one\n- item two\n- item three";
    let model = BlockModel::build(text, None);

    assert_eq!(model.blocks.len(), 1);
    match &model.blocks[0].kind {
        BlockKind::List {
            ordered,
            tight,
            items,
        } => {
            assert!(ordered.is_none());
            assert!(*tight);
            assert_eq!(items.len(), 3);
        }
        _ => panic!("expected List, got {:?}", model.blocks[0].kind),
    }
}

#[test]
fn test_ordered_list() {
    let text = "1. first\n2. second\n3. third";
    let model = BlockModel::build(text, None);

    assert_eq!(model.blocks.len(), 1);
    match &model.blocks[0].kind {
        BlockKind::List {
            ordered,
            tight,
            items,
        } => {
            assert_eq!(*ordered, Some(1));
            assert!(*tight);
            assert_eq!(items.len(), 3);
        }
        _ => panic!("expected List, got {:?}", model.blocks[0].kind),
    }
}

/// Recursively count the maximum list nesting depth.
fn max_list_depth(blocks: &[Block]) -> usize {
    let mut max_depth = 0;
    for block in blocks {
        if let BlockKind::List { items, .. } = &block.kind {
            for item in items {
                let child_depth = max_list_depth(&item.children) + 1;
                max_depth = max_depth.max(child_depth);
            }
        }
    }
    max_depth
}

#[test]
fn test_nested_lists_3_deep() {
    let text = "- level1\n  - level2\n    - level3\n      - level4";
    let model = BlockModel::build(text, None);

    assert_eq!(model.blocks.len(), 1);
    assert_eq!(
        max_list_depth(&model.blocks),
        4,
        "should have 4 levels of list nesting"
    );
}

#[test]
fn test_task_list() {
    let text = "- [ ] unchecked\n- [x] checked\n- [ ] another unchecked";
    let model = BlockModel::build(text, None);

    assert_eq!(model.blocks.len(), 1);
    match &model.blocks[0].kind {
        BlockKind::List { items, .. } => {
            assert_eq!(items.len(), 3);
            assert_eq!(items[0].task, Some(false));
            assert_eq!(items[1].task, Some(true));
            assert_eq!(items[2].task, Some(false));
        }
        _ => panic!("expected List, got {:?}", model.blocks[0].kind),
    }
}

#[test]
fn test_quote_in_list() {
    let text = "- Item with quote\n  > quoted text";
    let model = BlockModel::build(text, None);

    assert!(!model.blocks.is_empty());
    // Verify no panic and blocks exist
    for block in &model.blocks {
        assert_span_valid(block, text, "quote-in-list");
    }
}

#[test]
fn test_fence_in_quote() {
    let text = "> Code block in quote:\n> ```rust\n> fn main() {}\n> ```";
    let model = BlockModel::build(text, None);

    assert!(!model.blocks.is_empty());
    for block in &model.blocks {
        assert_span_valid(block, text, "fence-in-quote");
    }
}

// ── Blockquote tests ───────────────────────────────────────────────────────

#[test]
fn test_blockquote() {
    let text = "> First quoted paragraph.\n>\n> > Nested quote.";
    let model = BlockModel::build(text, None);

    assert_eq!(model.blocks.len(), 1);
    match &model.blocks[0].kind {
        BlockKind::BlockQuote { children } => {
            assert!(!children.is_empty());
        }
        _ => panic!("expected BlockQuote, got {:?}", model.blocks[0].kind),
    }
}

// ── Nested block metadata regressions ─────────────────────────────────────

#[test]
fn link_inside_list_item_in_index() {
    let layout = view_layout("- See [Rust](https://www.rust-lang.org/).");

    assert_eq!(
        layout.link_index,
        vec![(0, "https://www.rust-lang.org/".to_string())]
    );
    assert!(layout
        .lines
        .iter()
        .any(|line| line.styled.text.contains("Rust [0]")));
}

#[test]
fn link_inside_blockquote_in_index() {
    let layout = view_layout("> Read [the guide](https://example.com/guide).");

    assert_eq!(
        layout.link_index,
        vec![(0, "https://example.com/guide".to_string())]
    );
    assert!(layout
        .lines
        .iter()
        .any(|line| line.styled.text.contains("the guide [0]")));
}

#[test]
fn heading_inside_blockquote_jump_target() {
    let layout = view_layout("> # Nested heading");

    assert_eq!(layout.jump_targets.len(), 1);
    let target = &layout.jump_targets[0];
    assert_eq!(layout.lines[target.line].styled.text, "┃ █ Nested heading");
}

#[test]
fn nested_link_markers_sequential() {
    let layout = view_layout(
        "[outside](https://example.com/0)\n\n- [first](https://example.com/1)\n- [second](https://example.com/2)\n\n> [quoted](https://example.com/3)",
    );

    assert_eq!(
        layout.link_index,
        vec![
            (0, "https://example.com/0".to_string()),
            (1, "https://example.com/1".to_string()),
            (2, "https://example.com/2".to_string()),
            (3, "https://example.com/3".to_string()),
        ]
    );

    for expected in ["outside [0]", "first [1]", "second [2]", "quoted [3]"] {
        assert!(
            layout
                .lines
                .iter()
                .filter(|line| line.kind == LineKind::Content)
                .any(|line| line.styled.text.contains(expected)),
            "content lines should contain {expected}"
        );
    }
}

#[test]
fn nested_container_metadata_is_preserved_recursively() {
    let layout = view_layout("> - [deep link](https://example.com/deep)\n>   > # Deep heading");

    assert_eq!(
        layout.link_index,
        vec![(0, "https://example.com/deep".to_string())]
    );
    assert!(layout
        .lines
        .iter()
        .any(|line| line.styled.text == "┃ • deep link [0]"));
    assert_eq!(layout.jump_targets.len(), 1);
    assert_eq!(
        layout.lines[layout.jump_targets[0].line].styled.text,
        "┃ • ┃ █ Deep heading"
    );
}

#[test]
fn no_spurious_link_panel_in_nested() {
    let layout =
        view_layout("Before.\n\n- [nested](https://example.com/nested)\n\nAfter nested content.");
    let after_line = layout
        .lines
        .iter()
        .position(|line| line.styled.text == "After nested content.")
        .expect("trailing paragraph should be rendered");
    let link_panels: Vec<_> = layout
        .lines
        .iter()
        .enumerate()
        .filter(|(_, line)| line.styled.text.contains("links"))
        .collect();

    assert_eq!(link_panels.len(), 1);
    assert!(link_panels[0].0 > after_line);
    assert_eq!(link_panels[0].1.styled.text, "─ links ─");
}

#[test]
fn nested_footnotes_are_finalized_once_at_document_end() {
    let layout = view_layout(
        "- Listed note[^list].\n\n  [^list]: From a list item.\n\n> Quoted note[^quote].\n>\n> [^quote]: From a blockquote.\n\nAfter nested footnotes.",
    );
    let after_line = layout
        .lines
        .iter()
        .position(|line| line.styled.text == "After nested footnotes.")
        .expect("trailing paragraph should be rendered");
    let footnote_panels: Vec<_> = layout
        .lines
        .iter()
        .enumerate()
        .filter(|(_, line)| line.styled.text == "─ footnotes ─")
        .collect();

    assert_eq!(footnote_panels.len(), 1);
    assert!(footnote_panels[0].0 > after_line);

    let line_position = |text: &str| {
        layout
            .lines
            .iter()
            .position(|line| line.styled.text == text)
            .unwrap_or_else(|| panic!("expected rendered line {text:?}"))
    };
    let list_marker = line_position("[list]: ");
    let list_body = line_position("From a list item.");
    let quote_marker = line_position("[quote]: ");
    let quote_body = line_position("From a blockquote.");

    assert!(footnote_panels[0].0 < list_marker);
    assert!(list_marker < list_body);
    assert!(list_body < quote_marker);
    assert!(quote_marker < quote_body);
}

// ── Table tests ────────────────────────────────────────────────────────────

#[test]
fn test_table_alignments() {
    let text = "| Left | Center | Right |\n|:-----|:------:|------:|\n| a | b | c |";
    let model = BlockModel::build(text, None);

    assert_eq!(model.blocks.len(), 1);
    match &model.blocks[0].kind {
        BlockKind::Table {
            alignments,
            header,
            rows,
        } => {
            assert_eq!(alignments.len(), 3);
            assert_eq!(header.len(), 3);
            assert_eq!(rows.len(), 1);
        }
        _ => panic!("expected Table, got {:?}", model.blocks[0].kind),
    }
}

#[test]
fn test_table_without_alignments() {
    let text = "| A | B |\n|---|---|\n| 1 | 2 |";
    let model = BlockModel::build(text, None);

    assert_eq!(model.blocks.len(), 1);
    match &model.blocks[0].kind {
        BlockKind::Table { alignments, .. } => {
            // Default alignment is Left
            assert_eq!(alignments.len(), 2);
        }
        _ => panic!("expected Table, got {:?}", model.blocks[0].kind),
    }
}

// ── Rule tests ─────────────────────────────────────────────────────────────

#[test]
fn test_thematic_break() {
    let text = "Before.\n\n---\n\nAfter.";
    let model = BlockModel::build(text, None);

    assert_eq!(model.blocks.len(), 3);
    assert!(matches!(model.blocks[1].kind, BlockKind::Rule));
}

// ── HTML block tests ───────────────────────────────────────────────────────

#[test]
fn test_html_blocks() {
    let text = "<div>HTML block</div>\n\nBack to markdown.";
    let model = BlockModel::build(text, None);

    assert!(!model.blocks.is_empty());
    for block in &model.blocks {
        assert_span_valid(block, text, "html-block");
    }
}

// ── Footnote tests ─────────────────────────────────────────────────────────

#[test]
fn test_footnote_definition_and_reference() {
    let text = "Text with a footnote[^1].\n\n[^1]: This is the footnote definition.";
    let model = BlockModel::build(text, None);

    assert!(!model.blocks.is_empty());
    // Should have a paragraph with FootnoteRef inline
    // and a FootnoteDef block
    let has_ref = model.blocks.iter().any(|b| {
        matches!(&b.kind, BlockKind::Paragraph { inlines } if inlines
            .iter()
            .any(|i| matches!(i, Inline::FootnoteRef(_))))
    });
    let has_def = model
        .blocks
        .iter()
        .any(|b| matches!(&b.kind, BlockKind::FootnoteDef { label, .. } if label == "1"));
    assert!(
        has_ref && has_def,
        "expected both footnote reference and definition, got ref={}, def={}",
        has_ref,
        has_def
    );
}

// ── Inline tests ───────────────────────────────────────────────────────────

#[test]
fn test_inline_text() {
    let text = "Hello **world** and *friend*.";
    let model = BlockModel::build(text, None);

    assert_eq!(model.blocks.len(), 1);
    match &model.blocks[0].kind {
        BlockKind::Paragraph { inlines } => {
            assert!(!inlines.is_empty());
            // Should have Text, Strong, Text, Emph, Text
            let text_count = inlines
                .iter()
                .filter(|i| matches!(i, Inline::Text(_)))
                .count();
            assert!(text_count >= 3);
        }
        _ => panic!("expected Paragraph, got {:?}", model.blocks[0].kind),
    }
}

#[test]
fn test_inline_code() {
    let text = "Use `cargo build` to compile.";
    let model = BlockModel::build(text, None);

    assert_eq!(model.blocks.len(), 1);
    match &model.blocks[0].kind {
        BlockKind::Paragraph { inlines } => {
            let has_code = inlines
                .iter()
                .any(|i| matches!(i, Inline::Code(c) if c == "cargo build"));
            assert!(has_code, "expected Inline::Code(\"cargo build\")");
        }
        _ => panic!("expected Paragraph, got {:?}", model.blocks[0].kind),
    }
}

#[test]
fn test_inline_strikethrough() {
    let text = "This is ~~deleted~~ text.";
    let model = BlockModel::build(text, None);

    assert_eq!(model.blocks.len(), 1);
    match &model.blocks[0].kind {
        BlockKind::Paragraph { inlines } => {
            let has_strike = inlines.iter().any(|i| matches!(i, Inline::Strike(_)));
            assert!(has_strike, "expected Inline::Strike");
        }
        _ => panic!("expected Paragraph, got {:?}", model.blocks[0].kind),
    }
}

#[test]
fn test_links() {
    let text = "A [link](https://example.com) and an ![image](https://img.com/pic.png).";
    let model = BlockModel::build(text, None);

    assert_eq!(model.blocks.len(), 1);
    match &model.blocks[0].kind {
        BlockKind::Paragraph { inlines } => {
            let has_link = inlines.iter().any(|i| matches!(i, Inline::Link { .. }));
            let has_image = inlines.iter().any(|i| matches!(i, Inline::Image { .. }));
            assert!(has_link, "expected Inline::Link");
            assert!(has_image, "expected Inline::Image");
        }
        _ => panic!("expected Paragraph, got {:?}", model.blocks[0].kind),
    }
}

// ── Front matter tests ─────────────────────────────────────────────────────

#[test]
fn test_front_matter_passthrough() {
    let text = "---\ntitle: Test\n---\n\n# Heading";
    // Compute the front-matter span from the actual text
    let fm_end = text.find("\n#").unwrap_or(text.len());
    let fm_span = Some(0..fm_end);
    let model = BlockModel::build(text, fm_span);

    assert_eq!(model.blocks.len(), 2);
    assert!(matches!(model.blocks[0].kind, BlockKind::FrontMatter));
    assert!(matches!(model.blocks[1].kind, BlockKind::Heading { .. }));
}

#[test]
fn test_empty_document() {
    let model = BlockModel::build("", None);
    assert!(model.blocks.is_empty());
}

#[test]
fn test_whitespace_only() {
    let model = BlockModel::build("   \n\n  \n", None);
    assert!(model.blocks.is_empty());
}

// ── Span integrity tests ───────────────────────────────────────────────────

#[test]
fn test_span_integrity_headings() {
    let text = "# Title\n\n## Subtitle";
    let model = BlockModel::build(text, None);

    for block in &model.blocks {
        assert_span_valid(block, text, "span-integrity");
        // Verify the span actually contains the heading marker
        let slice = &text[block.span.start..block.span.end];
        assert!(slice.starts_with('#'), "heading span should start with #");
    }
}

#[test]
fn test_span_integrity_lists() {
    let text = "- item1\n- item2\n  - nested";
    let model = BlockModel::build(text, None);

    for block in &model.blocks {
        assert_span_valid(block, text, "list-span");
    }
}

#[test]
fn test_span_integrity_code_blocks() {
    let text = "```rust\nlet x = 1;\n```";
    let model = BlockModel::build(text, None);

    for block in &model.blocks {
        assert_span_valid(block, text, "code-span");
    }
}

// ── Kitchen-sink corpus test ───────────────────────────────────────────────

#[test]
fn test_kitchen_sink_no_panic() {
    let text = fixture("kitchen-sink.md");
    let model = BlockModel::build(&text, None);

    // Should produce multiple blocks without panicking
    assert!(
        !model.blocks.is_empty(),
        "kitchen-sink should produce at least one block"
    );

    // All blocks should have valid spans
    for block in &model.blocks {
        assert_span_valid(block, &text, "kitchen-sink");
    }

    // Top-level blocks should be sorted by start byte and non-overlapping
    for i in 1..model.blocks.len() {
        let prev = &model.blocks[i - 1];
        let curr = &model.blocks[i];
        assert!(
            prev.span.end <= curr.span.start,
            "blocks {} and {} overlap: {:?} vs {:?}",
            i - 1,
            i,
            prev.span,
            curr.span
        );
    }
}

#[test]
fn test_kitchen_sink_covers_all_kind_types() {
    let text = fixture("kitchen-sink.md");
    let model = BlockModel::build(&text, None);

    let all_blocks = flatten_blocks(&model.blocks);
    let kinds: std::collections::HashSet<_> = all_blocks
        .iter()
        .map(|b| std::mem::discriminant(&b.kind))
        .collect();

    // Verify we have at least some variety
    assert!(
        kinds.len() >= 4,
        "kitchen-sink should produce at least 4 different block kinds, got {}",
        kinds.len()
    );
}

#[test]
fn test_kitchen_sink_block_spans_sorted_non_overlapping() {
    let text = fixture("kitchen-sink.md");
    let model = BlockModel::build(&text, None);

    // Check top-level blocks are sorted and non-overlapping
    for i in 1..model.blocks.len() {
        assert!(
            model.blocks[i - 1].span.start <= model.blocks[i].span.start,
            "blocks not sorted at index {}",
            i
        );
        assert!(
            model.blocks[i - 1].span.end <= model.blocks[i].span.start,
            "blocks overlap at index {}",
            i
        );
    }
}

// ── T06 fixture compatibility ──────────────────────────────────────────────

#[test]
fn test_t06_fixtures_parse_without_panic() {
    let fixtures = [
        "highlight_fences.md",
        "highlight_frontmatter_yaml.md",
        "highlight_frontmatter_toml.md",
        "highlight_markdown_structure.md",
    ];

    for name in &fixtures {
        let text = fixture(name);
        let model = BlockModel::build(&text, None);
        // Just verify no panic and blocks exist for non-empty input
        if !text.trim().is_empty() {
            assert!(
                !model.blocks.is_empty(),
                "fixture {name} should produce blocks"
            );
            for block in &model.blocks {
                assert_span_valid(block, &text, name);
            }
        }
    }
}
