//! Tests for the rendered block model and layout.

use crate::rendered::{Block, BlockKind, BlockModel, Inline, ListItem};
use crate::style::{LineKind, RenderedLineRole, SemanticStyle};
use crate::syntax::Highlighter;
use crate::{RenderedLayout, TargetKind};
use std::path::Path;

// ── Helpers ────────────────────────────────────────────────────────────────

fn fixture(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|_| panic!("fixture not found: {name}"))
}

fn rendered_layout(text: &str) -> RenderedLayout {
    rendered_layout_at_width(text, 80)
}

fn rendered_layout_at_width(text: &str, width: u16) -> RenderedLayout {
    let model = BlockModel::build(text, None);
    let highlighter = Highlighter::new(text);
    RenderedLayout::build(&model, width, &highlighter)
}

fn source_backed_ranges(layout: &RenderedLayout, text: &str) -> Vec<std::ops::Range<usize>> {
    layout
        .lines
        .iter()
        .flat_map(|line| &line.atoms)
        .filter_map(|atom| atom.source.clone())
        .filter(|source| !text[source.clone()].chars().all(char::is_whitespace))
        .collect()
}

#[test]
fn repeated_transformed_text_keeps_leaf_local_source_order() {
    // The removed global candidate queue could consume an identical label
    // from a different parser leaf. Each displayed group must now remain
    // inside the exact repeated occurrence that produced it.
    let text =
        "same [same](one) ![same *same*](two)\n\n> - same\n\n| a | b |\n|---|---|\n| same | same |";
    let layout = rendered_layout_at_width(text, 24);
    let occurrences = text
        .match_indices("same")
        .map(|(start, value)| start..start + value.len())
        .collect::<Vec<_>>();
    let ranges = source_backed_ranges(&layout, text);

    for occurrence in occurrences {
        let owned = ranges
            .iter()
            .filter(|range| occurrence.start <= range.start && range.end <= occurrence.end)
            .map(|range| &text[range.clone()])
            .collect::<String>();
        assert_eq!(owned, "same", "wrong ownership for {occurrence:?}");
    }
}

#[test]
fn nested_inline_provenance_survives_wrapping_and_prefixes() {
    // Prefix correction used to happen after wrapping and could accidentally
    // lend list/quote/image decoration a neighboring leaf's bytes.
    let text = "> - **alpha [beta](dest) `gamma`** delta";
    let layout = rendered_layout_at_width(text, 13);
    assert!(layout.lines.len() > 1);
    for line in &layout.lines {
        for atom in &line.atoms {
            if atom.columns.start < 4 {
                let visible = line
                    .styled
                    .text
                    .chars()
                    .nth(atom.columns.start)
                    .unwrap_or(' ');
                if matches!(visible, '┃' | '•' | ' ') {
                    assert_eq!(atom.source, None);
                }
            }
            if let Some(source) = &atom.source {
                assert!(source.end <= text.len());
                assert!(text.is_char_boundary(source.start));
                assert!(text.is_char_boundary(source.end));
            }
        }
    }
}

#[test]
fn table_cell_provenance_survives_alignment_padding_and_wrap() {
    // The old synthetic-column side table corrected padding only after table
    // layout and could not prove repeated cell ownership was row-local.
    let long = "東京".repeat(22);
    let text = format!(
        "| left | center | right |\n|:---|:---:|---:|\n| same | same | same |\n| {long} |  | same |"
    );
    let data_rows = [
        "| same | same | same |".to_string(),
        format!("| {long} |  | same |"),
    ];

    for width in [24, 120] {
        let layout = rendered_layout_at_width(&text, width);
        for line in layout
            .lines
            .iter()
            .filter(|line| line.styled.text.contains('│'))
        {
            for atom in &line.atoms {
                if let Some(source) = &atom.source {
                    assert!(line.source.start <= source.start);
                    assert!(source.end <= line.source.end);
                }
            }
            for (character, atom) in line.styled.text.chars().zip(&line.atoms) {
                if matches!(character, '│' | ' ') {
                    assert_eq!(atom.source, None, "table layout cell was source-backed");
                }
            }
        }

        for row in &data_rows {
            let row_start = text.find(row).unwrap();
            let delimiters = row
                .match_indices('|')
                .map(|(offset, _)| offset)
                .collect::<Vec<_>>();
            for pair in delimiters.windows(2) {
                let raw_start = pair[0] + 1;
                let raw_end = pair[1];
                let raw = &row[raw_start..raw_end];
                let leading = raw.len() - raw.trim_start().len();
                let trailing = raw.len() - raw.trim_end().len();
                let cell = row_start + raw_start + leading..row_start + raw_end - trailing;
                if cell.is_empty() {
                    continue;
                }

                let owned = layout
                    .lines
                    .iter()
                    .flat_map(|line| &line.atoms)
                    .filter_map(|atom| atom.source.clone())
                    .filter(|source| cell.start <= source.start && source.end <= cell.end)
                    .collect::<Vec<_>>();
                assert!(
                    !owned.is_empty(),
                    "cell {cell:?} lost its source at width {width}"
                );
                assert_eq!(owned.first().unwrap().start, cell.start);
                assert_eq!(owned.last().unwrap().end, cell.end);
                for adjacent in owned.windows(2) {
                    assert_eq!(
                        adjacent[0].end, adjacent[1].start,
                        "cell bytes were duplicated, skipped, or reordered at width {width}"
                    );
                }
                assert_eq!(
                    owned
                        .iter()
                        .map(|source| &text[source.clone()])
                        .collect::<String>(),
                    text[cell.clone()],
                    "cell did not reconstruct from its own bytes at width {width}"
                );
            }
        }
    }
}

#[test]
fn entity_and_escape_atoms_are_constructed_from_complete_tokens() {
    // Candidate search previously recovered these transforms after rendering;
    // construction-time atoms must own the complete raw token instead.
    let text = r"a &amp; b \* c &#x1F642;";
    let layout = rendered_layout(text);
    let raw = layout
        .lines
        .iter()
        .flat_map(|line| &line.atoms)
        .filter_map(|atom| atom.source.as_ref())
        .map(|source| &text[source.clone()])
        .collect::<Vec<_>>();
    assert!(raw.contains(&"&amp;"));
    assert!(raw.contains(&r"\*"));
    assert!(raw.contains(&"&#x1F642;"));
}

#[test]
fn inline_code_normalization_keeps_each_display_group_on_its_exact_raw_token() {
    let text = "`alpha\nbeta`\n\n| code |\n|---|\n| `left\\|right` |";
    let model = BlockModel::build(text, None);
    let BlockKind::Paragraph { inlines } = &model.blocks[0].kind else {
        panic!("expected paragraph");
    };
    let Inline::Code(code) = &inlines[0] else {
        panic!("expected code");
    };
    assert_eq!(code.text, "alpha beta");
    assert_eq!(
        code.atoms
            .iter()
            .map(|atom| atom.source.clone())
            .collect::<Vec<_>>(),
        (1..6)
            .map(|start| start..start + 1)
            .chain(std::iter::once(6..7))
            .chain((7..11).map(|start| start..start + 1))
            .collect::<Vec<_>>()
    );
    let layout = rendered_layout_at_width(text, 40);
    let atoms = layout
        .lines
        .iter()
        .flat_map(|line| &line.atoms)
        .filter_map(|atom| atom.source.clone())
        .collect::<Vec<_>>();

    let newline = text.find('\n').unwrap();
    assert!(
        atoms.contains(&(newline..newline + 1)),
        "normalized code newline lost its exact source: {atoms:?}"
    );

    let beta = text.find("beta").unwrap();
    for offset in 0.."beta".len() {
        assert!(atoms
            .iter()
            .any(|source| { source.start == beta + offset && source.end == beta + offset + 1 }));
    }

    let escaped_pipe = text.find("\\|").unwrap();
    assert!(atoms.contains(&(escaped_pipe..escaped_pipe + 2)));
}

#[test]
fn ordinary_inline_code_preserves_literal_escaped_pipe_byte_ownership() {
    let text = "`left\\|right`";
    let model = BlockModel::build(text, None);
    let BlockKind::Paragraph { inlines } = &model.blocks[0].kind else {
        panic!("expected paragraph");
    };
    let Inline::Code(code) = &inlines[0] else {
        panic!("expected code");
    };
    assert_eq!(code.text, "left\\|right");
    assert_eq!(
        code.atoms
            .iter()
            .map(|atom| atom.source.clone())
            .collect::<Vec<_>>(),
        (1..12).map(|start| start..start + 1).collect::<Vec<_>>()
    );
    assert_eq!(
        source_backed_ranges(&rendered_layout_at_width(text, 40), text),
        (1..12).map(|start| start..start + 1).collect::<Vec<_>>()
    );
}

#[test]
fn prose_non_escape_preserves_backslash_and_following_byte_ownership() {
    let text = "left\\qright";
    let model = BlockModel::build(text, None);
    let BlockKind::Paragraph { inlines } = &model.blocks[0].kind else {
        panic!("expected paragraph");
    };
    let Inline::Text(prose) = &inlines[0] else {
        panic!("expected prose");
    };
    assert_eq!(prose.text, text);
    let expected = (0..text.len())
        .map(|start| start..start + 1)
        .collect::<Vec<_>>();
    assert_eq!(
        prose
            .atoms
            .iter()
            .map(|atom| atom.source.clone())
            .collect::<Vec<_>>(),
        expected
    );
    assert_eq!(
        source_backed_ranges(&rendered_layout_at_width(text, 40), text),
        expected
    );
}

#[test]
fn rendered_provenance_is_width_invariant() {
    // Post-hoc recovery depended on final line order. Rewrapping now moves the
    // same mapped fragments, so non-whitespace ownership is width invariant.
    let text = r"repeat **repeat** 東京 &amp; \* [repeat](dest) repeat";
    let baseline = source_backed_ranges(&rendered_layout_at_width(text, 80), text);
    for width in [7, 11, 19, 37] {
        assert_eq!(
            source_backed_ranges(&rendered_layout_at_width(text, width), text),
            baseline,
            "source ownership changed at width {width}"
        );
    }
}

#[test]
fn table_provenance_is_width_invariant() {
    let text = "| first | second |\n|---|---|\n| same | same |\n| 東京東京東京東京 | same |";
    let baseline = source_backed_ranges(&rendered_layout_at_width(text, 80), text);
    for width in [18, 27, 43] {
        assert_eq!(
            source_backed_ranges(&rendered_layout_at_width(text, width), text),
            baseline,
            "table source ownership changed at width {width}"
        );
    }
}

#[test]
fn rendered_provenance_pipeline_has_no_posthoc_reconstruction() {
    // This narrow dependency guard complements the behavioral assertions and
    // rejects only the removed recovery architecture.
    let rendered_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/rendered");
    for file in ["mod.rs", "blocks.rs", "table.rs", "wrap.rs"] {
        let source = std::fs::read_to_string(rendered_dir.join(file)).unwrap();
        for removed in [
            "SourceCandidate",
            "populate_source_atoms",
            "inline_sources",
            "synthetic_columns",
        ] {
            assert!(!source.contains(removed), "{file} restored {removed}");
        }
    }
}

#[test]
fn inline_source_atoms_cover_markdown_transformations() {
    let cases = [
        ("plain text", "plain text"),
        (r"escaped \* punctuation", r"escaped \* punctuation"),
        ("*emphasis* and `code`", "emphasis and code"),
        ("[link label](https://example.test)", "link label"),
        ("![image alt](image.png)", "image alt"),
        ("- list item", "list item"),
        ("- [x] completed task", "completed task"),
        ("repeated aaa aaa", "repeated aaa aaa"),
        ("wide 東京 and cafe\u{301}", "wide 東京 and cafe\u{301}"),
    ];
    for (text, expected_raw) in cases {
        let layout = rendered_layout_at_width(text, 80);
        let sources: Vec<_> = layout
            .lines
            .iter()
            .flat_map(|line| &line.atoms)
            .filter_map(|atom| atom.source.as_ref())
            .collect();
        assert!(!sources.is_empty(), "no source atoms for {text:?}");
        for source in &sources {
            assert!(source.start < source.end, "empty atom for {text:?}");
            assert!(source.end <= text.len(), "out-of-bounds atom for {text:?}");
            assert!(text.is_char_boundary(source.start));
            assert!(text.is_char_boundary(source.end));
        }
        assert_eq!(
            sources
                .iter()
                .map(|source| &text[(**source).clone()])
                .collect::<String>(),
            expected_raw,
            "wrong rendered-atom source ownership for {text:?}"
        );
    }

    let table = "| phrase | tag |\n|---|---|\n| hello world | x |";
    let layout = rendered_layout(table);
    let row = layout
        .lines
        .iter()
        .find(|line| line.styled.text.contains("hello world"))
        .expect("rendered table body");
    let byte_start = row.styled.text.find("hello world").unwrap();
    let start = row.styled.text[..byte_start]
        .chars()
        .map(|character| unicode_width::UnicodeWidthChar::width(character).unwrap_or(0))
        .sum::<usize>();
    let raw = row
        .atoms
        .iter()
        .filter(|atom| atom.columns.start >= start && atom.columns.start < start + 11)
        .map(|atom| {
            atom.source
                .as_ref()
                .map_or("", |source| &table[source.clone()])
        })
        .collect::<String>();
    let ownership = row
        .atoms
        .iter()
        .map(|atom| {
            (
                atom.columns.clone(),
                atom.source
                    .as_ref()
                    .map(|source| table[source.clone()].to_string()),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(raw, "hello world", "{ownership:?}");
    assert_eq!(
        row.atoms
            .iter()
            .find(|atom| atom.columns.start == start + 5)
            .and_then(|atom| atom.source.as_ref())
            .map(|source| &table[source.clone()]),
        Some(" ")
    );
}

#[test]
fn synthetic_layout_cells_are_explicitly_unmapped() {
    let text = "- item\n\n[link](https://example.test)\n\n| a | b |\n|---|---|\n| c | d |\n\n```rust\nfn main() {}\n```\n";
    let layout = rendered_layout(text);
    assert!(layout
        .lines
        .iter()
        .flat_map(|line| &line.atoms)
        .any(|atom| { atom.source.is_none() && atom.columns.start < atom.columns.end }));
    assert!(layout
        .lines
        .iter()
        .flat_map(|line| &line.atoms)
        .any(|atom| atom.source.is_some()));

    for line in &layout.lines {
        for (character, atom) in line.styled.text.chars().zip(&line.atoms) {
            if matches!(character, '•' | '│' | '┌' | '┐' | '└' | '┘' | '─' | '▏') {
                assert_eq!(atom.source, None, "synthetic {character:?} was mapped");
            }
        }
        if line.styled.text.starts_with("▏ ") {
            assert_eq!(line.atoms[1].source, None);
        }
        if let Some(marker) = line.styled.text.find("[0]") {
            assert!(line.atoms[marker..marker + 3]
                .iter()
                .all(|atom| atom.source.is_none()));
        }
    }
}

#[test]
fn wrapped_source_atoms_keep_their_byte_ownership() {
    let text = "alpha beta gamma delta epsilon 東京 cafe\u{301}";
    let wide = rendered_layout_at_width(text, 80);
    let narrow = rendered_layout_at_width(text, 9);
    let ranges = |layout: &RenderedLayout| {
        let mut ranges: Vec<_> = layout
            .lines
            .iter()
            .flat_map(|line| &line.atoms)
            .filter_map(|atom| atom.source.clone())
            .filter(|range| !text[range.clone()].trim().is_empty())
            .collect();
        ranges.sort_by_key(|range| (range.start, range.end));
        ranges.dedup();
        ranges
    };
    assert_eq!(ranges(&wide), ranges(&narrow));
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

fn inline_text(inlines: &[Inline]) -> String {
    let mut text = String::new();
    for inline in inlines {
        match inline {
            Inline::Text(value) | Inline::Code(value) | Inline::Html(value) => {
                text.push_str(&value.text);
            }
            Inline::SoftBreak(value) | Inline::HardBreak(value) => text.push_str(&value.text),
            Inline::Emph(children)
            | Inline::Strong(children)
            | Inline::Strike(children)
            | Inline::Link { text: children, .. } => text.push_str(&inline_text(children)),
            Inline::Image { alt, .. } => text.push_str(&inline_text(alt)),
            Inline::FootnoteRef(label) => text.push_str(&label.text),
        }
    }
    text
}

fn paragraph_text(block: &Block) -> Option<String> {
    match &block.kind {
        BlockKind::Paragraph { inlines } => Some(inline_text(inlines)),
        _ => None,
    }
}

fn collect_list_labels(items: &[ListItem], labels: &mut Vec<String>) {
    for item in items {
        for child in &item.children {
            if let Some(text) = paragraph_text(child) {
                labels.push(text);
            }
            if let BlockKind::List { items, .. } = &child.kind {
                collect_list_labels(items, labels);
            }
        }
    }
}

fn assert_list_item_spans(items: &[ListItem]) {
    for item in items {
        for child in &item.children {
            assert!(
                item.span.start <= child.span.start && child.span.end <= item.span.end,
                "child span {:?} must be bounded by item span {:?}",
                child.span,
                item.span
            );
        }
        for children in item.children.windows(2) {
            assert!(
                children[0].span.end <= children[1].span.start,
                "item children must be ordered and non-overlapping: {:?} before {:?}",
                children[0].span,
                children[1].span
            );
        }
        for child in &item.children {
            if let BlockKind::List { items, .. } = &child.kind {
                assert_list_item_spans(items);
            }
        }
    }
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
                    assert_eq!(t.text, format!("H{}", i + 1));
                } else {
                    panic!("expected Text inline");
                }
            }
            _ => panic!("expected Heading, got {:?}", block.kind),
        }
    }
}

#[test]
fn heading_jump_target_wrapping() {
    let layout = rendered_layout_at_width(
        "Intro paragraph.\n\n# This is a heading that wraps across multiple lines",
        20,
    );

    assert_eq!(layout.jump_targets.len(), 1);
    let first_heading_line = layout
        .lines
        .iter()
        .position(|line| line.styled.text.starts_with("█ "))
        .expect("rendered heading should have a first line");
    assert_eq!(
        layout.lines[first_heading_line + 1].source,
        layout.lines[first_heading_line].source,
        "heading should wrap onto another rendered line",
    );
    assert_eq!(layout.jump_targets[0].line, first_heading_line);
}

#[test]
fn heading_jump_target_no_wrap() {
    let layout = rendered_layout_at_width("Intro paragraph.\n\n# Short heading", 80);

    assert_eq!(layout.jump_targets.len(), 1);
    let first_heading_line = layout
        .lines
        .iter()
        .position(|line| line.styled.text == "█ Short heading")
        .expect("rendered heading should be present");
    assert_eq!(layout.jump_targets[0].line, first_heading_line);
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
fn fenced_layout_marks_complete_surface_role() {
    let text = "```rust\nfn main() {}\n\nlet value = \"ok\";\n```\n```mystery\nopaque\n```\n```\n```\n\n    indented\n";
    let layout = rendered_layout(text);
    let rendered = layout
        .lines
        .iter()
        .map(|line| line.styled.text.as_str())
        .collect::<Vec<_>>();

    assert_eq!(
        rendered,
        [
            "▏ rust",
            "▏ fn main() {}",
            "▏ ",
            "▏ let value = \"ok\";",
            "▏",
            "",
            "▏ mystery",
            "▏ opaque",
            "▏",
            "",
            "▏ ",
            "▏ ",
            "▏",
            "",
            "indented",
        ]
    );

    let expected_roles = [
        RenderedLineRole::CodeFence,
        RenderedLineRole::CodeFence,
        RenderedLineRole::CodeFence,
        RenderedLineRole::CodeFence,
        RenderedLineRole::CodeFence,
        RenderedLineRole::Document,
        RenderedLineRole::CodeFence,
        RenderedLineRole::CodeFence,
        RenderedLineRole::CodeFence,
        RenderedLineRole::Document,
        RenderedLineRole::CodeFence,
        RenderedLineRole::CodeFence,
        RenderedLineRole::CodeFence,
        RenderedLineRole::Document,
        RenderedLineRole::Document,
    ];
    let expected_kinds = [
        LineKind::Synthetic,
        LineKind::Content,
        LineKind::Content,
        LineKind::Content,
        LineKind::Synthetic,
        LineKind::Synthetic,
        LineKind::Synthetic,
        LineKind::Content,
        LineKind::Synthetic,
        LineKind::Synthetic,
        LineKind::Synthetic,
        LineKind::Content,
        LineKind::Synthetic,
        LineKind::Synthetic,
        LineKind::Content,
    ];
    assert_eq!(
        layout
            .lines
            .iter()
            .map(|line| line.role)
            .collect::<Vec<_>>(),
        expected_roles
    );
    assert_eq!(
        layout
            .lines
            .iter()
            .map(|line| line.kind)
            .collect::<Vec<_>>(),
        expected_kinds
    );

    let known_body = &layout.lines[1];
    assert!(known_body
        .styled
        .spans
        .iter()
        .any(|span| span.style == SemanticStyle::Keyword));
    let unknown_body = &layout.lines[7];
    assert_eq!(
        unknown_body
            .styled
            .spans
            .iter()
            .map(|span| span.style)
            .collect::<Vec<_>>(),
        [SemanticStyle::Muted, SemanticStyle::CodeBlock],
        "unknown-language content must use the fenced-code fallback role"
    );
    assert_eq!(&text[known_body.source.clone()], "fn main() {}\n");
    assert_eq!(&text[unknown_body.source.clone()], "opaque");
    assert_eq!(&text[layout.lines[14].source.clone()], "indented");

    for line in &layout.lines {
        assert!(line.source.end <= text.len());
        assert!(text.is_char_boundary(line.source.start));
        assert!(text.is_char_boundary(line.source.end));
        let mut previous_end = 0;
        for atom in &line.atoms {
            assert!(atom.columns.start >= previous_end);
            assert!(atom.columns.start <= atom.columns.end);
            previous_end = atom.columns.end;
            if let Some(source) = &atom.source {
                assert!(source.end <= text.len());
                assert!(text.is_char_boundary(source.start));
                assert!(text.is_char_boundary(source.end));
            }
        }
    }
}

#[test]
fn nested_fence_surface_role_survives_container_prefixes() {
    let text = "- Before fence\n  ```rust\n  let value = 1;\n  ```\n  After fence\n\n> Before quote fence\n> ```text\n> quoted code\n> ```\n> After quote fence";
    let layout = rendered_layout(text);

    let fence_rows = layout
        .lines
        .iter()
        .filter(|line| line.styled.text.contains('▏'))
        .collect::<Vec<_>>();
    assert_eq!(fence_rows.len(), 8);
    assert!(fence_rows
        .iter()
        .all(|line| line.role == RenderedLineRole::CodeFence));
    assert!(fence_rows
        .iter()
        .any(|line| line.styled.text.starts_with("  ▏ rust")));
    assert!(fence_rows
        .iter()
        .any(|line| line.styled.text.starts_with("┃ ▏ text")));

    for label in [
        "Before fence",
        "After fence",
        "Before quote fence",
        "After quote fence",
    ] {
        let line = layout
            .lines
            .iter()
            .find(|line| line.styled.text.contains(label))
            .unwrap_or_else(|| panic!("missing neighboring prose {label:?}"));
        assert_eq!(line.role, RenderedLineRole::Document);
    }

    for line in &layout.lines {
        for atom in &line.atoms {
            if let Some(source) = &atom.source {
                assert!(source.end <= text.len());
                assert!(text.is_char_boundary(source.start));
                assert!(text.is_char_boundary(source.end));
            }
        }
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
fn tight_list_item_children_follow_source_order() {
    let text = "Nested unordered lists:\n\n- Top-level item\n  - Second-level item\n    - Third-level item\n      - Fourth-level item\n  - Another second-level item\n- Back to top level";
    let model = BlockModel::build(text, None);
    let BlockKind::List { tight, items, .. } = &model.blocks[1].kind else {
        panic!("expected the reported top-level list");
    };

    assert!(tight, "the blank-line-free reporter list must remain tight");
    assert_eq!(items.len(), 2);
    assert!(matches!(
        items[0].children[0].kind,
        BlockKind::Paragraph { .. }
    ));
    assert!(matches!(items[0].children[1].kind, BlockKind::List { .. }));

    let mut labels = Vec::new();
    collect_list_labels(items, &mut labels);
    assert_eq!(
        labels,
        [
            "Top-level item",
            "Second-level item",
            "Third-level item",
            "Fourth-level item",
            "Another second-level item",
            "Back to top level",
        ]
    );
    assert_list_item_spans(items);
}

#[test]
fn loose_and_multiblock_list_items_preserve_event_order() {
    let text = "- First paragraph.\n\n  Second paragraph before nested list.\n\n  - Nested child\n\n  Trailing paragraph after nested list.\n\n- Sibling paragraph.\n\n  > Quoted block\n\n  Final sibling paragraph.";
    let model = BlockModel::build(text, None);
    let BlockKind::List { tight, items, .. } = &model.blocks[0].kind else {
        panic!("expected a loose top-level list");
    };

    assert!(!tight, "explicit item paragraphs must make the list loose");
    assert_eq!(items.len(), 2);
    assert_eq!(items[0].children.len(), 4);
    assert_eq!(
        paragraph_text(&items[0].children[0]).as_deref(),
        Some("First paragraph.")
    );
    assert_eq!(
        paragraph_text(&items[0].children[1]).as_deref(),
        Some("Second paragraph before nested list.")
    );
    assert!(matches!(items[0].children[2].kind, BlockKind::List { .. }));
    assert_eq!(
        paragraph_text(&items[0].children[3]).as_deref(),
        Some("Trailing paragraph after nested list.")
    );

    assert_eq!(items[1].children.len(), 3);
    assert_eq!(
        paragraph_text(&items[1].children[0]).as_deref(),
        Some("Sibling paragraph.")
    );
    assert!(matches!(
        items[1].children[1].kind,
        BlockKind::BlockQuote { .. }
    ));
    assert_eq!(
        paragraph_text(&items[1].children[2]).as_deref(),
        Some("Final sibling paragraph.")
    );
    assert_list_item_spans(items);
}

#[test]
fn tight_multiblock_item_keeps_trailing_text_after_child_block() {
    let text = "- Before fence\n  ```text\n  code\n  ```\n  After fence";
    let model = BlockModel::build(text, None);
    let BlockKind::List { tight, items, .. } = &model.blocks[0].kind else {
        panic!("expected a tight top-level list");
    };

    assert!(tight);
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].children.len(), 3);
    assert_eq!(
        paragraph_text(&items[0].children[0]).as_deref(),
        Some("Before fence")
    );
    assert!(matches!(
        items[0].children[1].kind,
        BlockKind::CodeFence { .. }
    ));
    assert_eq!(
        paragraph_text(&items[0].children[2]).as_deref(),
        Some("After fence")
    );
    assert_list_item_spans(items);
}

#[test]
fn nested_unordered_layout_preserves_preorder_and_vw6_markers() {
    let text = "Nested unordered lists:\n\n- Top-level item\n  - Second-level item\n    - Third-level item\n      - Fourth-level item\n  - Another second-level item\n- Back to top level";
    let layout = rendered_layout(text);
    let lines: Vec<_> = layout
        .lines
        .iter()
        .map(|line| line.styled.text.as_str())
        .collect();

    assert_eq!(
        lines,
        [
            "Nested unordered lists:",
            "",
            "• Top-level item",
            "  ◦ Second-level item",
            "    ▪ Third-level item",
            "      ▪ Fourth-level item",
            "  ◦ Another second-level item",
            "• Back to top level",
        ]
    );
}

#[test]
fn ordered_lists_render_declared_start_and_nested_restart() {
    let text = "10. Tenth item\n11. Eleventh item\n\n    3. Nested third\n    4. Nested fourth";
    let layout = rendered_layout(text);
    let lines: Vec<_> = layout
        .lines
        .iter()
        .map(|line| line.styled.text.as_str())
        .collect();

    assert_eq!(
        lines,
        [
            "10. Tenth item",
            "11. Eleventh item",
            "  3. Nested third",
            "  4. Nested fourth",
        ]
    );
}

#[test]
fn wrapped_list_continuations_align_after_marker() {
    let layout = rendered_layout_at_width("10. alpha beta gamma delta", 14);
    let lines: Vec<_> = layout
        .lines
        .iter()
        .map(|line| line.styled.text.as_str())
        .collect();

    assert_eq!(lines, ["10. alpha beta", "    gamma", "    delta"]);
    assert_eq!(
        lines.iter().filter(|line| line.contains("10.")).count(),
        1,
        "the marker must appear only on the first rendered line"
    );
}

#[test]
fn nested_task_lists_keep_checkbox_marker_at_depth() {
    let layout = rendered_layout("- Parent\n  - [ ] Pending child\n  - [x] Completed child");
    let lines: Vec<_> = layout
        .lines
        .iter()
        .map(|line| line.styled.text.as_str())
        .collect();

    assert_eq!(
        lines,
        ["• Parent", "  ☐ Pending child", "  ☑ Completed child"]
    );
    let completed = &layout.lines[2].styled;
    assert!(completed.spans.iter().any(|span| {
        span.style == SemanticStyle::Muted && span.start_col == 4 && span.end_col == 19
    }));
}

#[test]
fn nested_list_only_item_keeps_parent_marker() {
    let layout = rendered_layout("-\n  - child");
    let lines: Vec<_> = layout
        .lines
        .iter()
        .map(|line| line.styled.text.as_str())
        .collect();

    assert_eq!(lines, ["•", "  ◦ child"]);
    assert_eq!(layout.lines[0].source, 0..11);
    assert!(layout.lines[0].styled.spans.iter().any(|span| {
        span.start_col == 0 && span.end_col == 1 && span.style == SemanticStyle::ListMarker
    }));
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
    let layout = rendered_layout("- See [Rust](https://www.rust-lang.org/).");

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
    let layout = rendered_layout("> Read [the guide](https://example.com/guide).");

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
    let layout = rendered_layout("> # Nested heading");

    assert_eq!(layout.jump_targets.len(), 1);
    let target = &layout.jump_targets[0];
    assert_eq!(layout.lines[target.line].styled.text, "┃ █ Nested heading");
}

#[test]
fn nested_link_markers_sequential() {
    let layout = rendered_layout(
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
    let layout = rendered_layout("> - [deep link](https://example.com/deep)\n>   > # Deep heading");

    assert_eq!(
        layout.link_index,
        vec![(0, "https://example.com/deep".to_string())]
    );
    assert!(layout
        .lines
        .iter()
        .any(|line| line.styled.text == "┃ • deep link [0]"));
    assert_eq!(layout.jump_targets.len(), 2);
    let link = layout
        .jump_targets
        .iter()
        .find(|target| target.kind == TargetKind::Link(0))
        .expect("nested link should be navigable");
    assert_eq!(layout.lines[link.line].styled.text, "┃ • deep link [0]");
    let heading = layout
        .jump_targets
        .iter()
        .find(|target| target.kind == TargetKind::Heading(1))
        .expect("nested heading should be navigable");
    assert_eq!(
        layout.lines[heading.line].styled.text,
        "┃   ┃ █ Deep heading"
    );
}

#[test]
fn no_spurious_link_panel_in_nested() {
    let layout = rendered_layout(
        "Before.\n\n- [nested](https://example.com/nested)\n\nAfter nested content.",
    );
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
    let layout = rendered_layout(
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
                .any(|i| matches!(i, Inline::Code(c) if c.text == "cargo build"));
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
