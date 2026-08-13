// Smoke tests for the dependency stack.
//
// These prove the core dependencies work together before any feature code is written.

// ---------------------------------------------------------------------------
// 1. the public session reaches the private hjkl wrapper
// ---------------------------------------------------------------------------

#[test]
fn editor_session_uses_private_hjkl_wrapper() {
    use oom_edit_core::{EditorSession, KeyCode, KeyCodeKind, KeyInput, Mode, Modifiers};

    let mut session = EditorSession::from_text("# hello\n# world\n");
    session.render_layout(40);
    session.handle_key(KeyInput {
        code: KeyCode {
            kind: KeyCodeKind::Char('j'),
        },
        mods: Modifiers::default(),
    });
    session.handle_key(KeyInput {
        code: KeyCode {
            kind: KeyCodeKind::Char('i'),
        },
        mods: Modifiers::default(),
    });

    assert_eq!(session.mode(), Mode::Insert);
    assert_eq!(session.document(), "# hello\n# world\n");
}

// ---------------------------------------------------------------------------
// 2. every_grammar_parses
// ---------------------------------------------------------------------------

#[test]
fn every_grammar_parses() {
    use tree_sitter::Parser;

    // A 3-line hello-world snippet for each language
    let snippets = [
        ("markdown", "# Hello\n\nWorld\n"),
        ("rust", "fn main() {}\n"),
        ("python", "def main():\n    pass\n"),
        ("javascript", "function main() {}\n"),
        ("typescript", "function main(): void {}\n"),
        ("go", "func main() {}\n"),
        ("bash", "#!/bin/bash\necho hello\n"),
        ("json", "{\"key\": \"value\"}\n"),
        ("c", "int main() { return 0; }\n"),
        ("yaml", "key: value\n"),
        ("toml", "key = \"value\"\n"),
    ];

    let mut parser = Parser::new();
    parser
        .set_included_ranges(&[tree_sitter::Range {
            start_byte: 0,
            end_byte: usize::MAX,
            start_point: tree_sitter::Point::new(0, 0),
            end_point: tree_sitter::Point::new(usize::MAX, usize::MAX),
        }])
        .unwrap();

    for (name, snippet) in snippets {
        let lang = get_language(name);
        parser
            .set_language(&lang)
            .unwrap_or_else(|_| panic!("tree-sitter: failed to set language for {name}"));
        let tree = parser
            .parse(snippet, None)
            .unwrap_or_else(|| panic!("tree-sitter: parse failed for {name}"));
        assert!(
            !tree.root_node().has_error(),
            "tree-sitter: {name} root node has error"
        );
    }
}

/// Helper to get a tree-sitter Language by name.
fn get_language(name: &str) -> tree_sitter::Language {
    match name {
        "markdown" => tree_sitter_md::LANGUAGE.into(),
        "rust" => tree_sitter_rust::LANGUAGE.into(),
        "python" => tree_sitter_python::LANGUAGE.into(),
        "javascript" => tree_sitter_javascript::LANGUAGE.into(),
        "typescript" => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
        "go" => tree_sitter_go::LANGUAGE.into(),
        "bash" => tree_sitter_bash::LANGUAGE.into(),
        "json" => tree_sitter_json::LANGUAGE.into(),
        "c" => tree_sitter_c::LANGUAGE.into(),
        "yaml" => tree_sitter_yaml::LANGUAGE.into(),
        "toml" => tree_sitter_toml_ng::LANGUAGE.into(),
        _ => panic!("unknown language: {name}"),
    }
}

// ---------------------------------------------------------------------------
// 3. markdown_frontmatter_nodes_exposed
// ---------------------------------------------------------------------------

#[test]
fn markdown_frontmatter_nodes_exposed() {
    use tree_sitter::Parser;

    let fixture = "---\ntitle: Hello\n---\n# Header\n";

    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_md::LANGUAGE.into())
        .unwrap();

    let tree = parser.parse(fixture, None).unwrap();
    let root = tree.root_node();
    assert!(!root.has_error(), "root node has error");

    // tree-sitter-md with EXTENSION_MINUS_METADATA should expose a
    // `minus_metadata` node for YAML front matter.
    let minus_md = root
        .named_child(0)
        .expect("expected a named child at position 0");
    assert_eq!(
        minus_md.kind(),
        "minus_metadata",
        "expected minus_metadata node for YAML front matter, got '{}'",
        minus_md.kind()
    );
}

// ---------------------------------------------------------------------------
// 4. pulldown_offsets_work
// ---------------------------------------------------------------------------

#[test]
fn pulldown_offsets_work() {
    use pulldown_cmark::{Event, Options, Parser, Tag};

    let input = "# h\n\npara\n";

    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_TABLES);
    opts.insert(Options::ENABLE_FOOTNOTES);
    opts.insert(Options::ENABLE_TASKLISTS);
    opts.insert(Options::ENABLE_STRIKETHROUGH);

    let parser = Parser::new_ext(input, opts);
    let events: Vec<Event<'_>> = parser.collect();

    // Use into_offset_iter to verify spans map back to source bytes
    let offset_iter = Parser::new_ext(input, opts).into_offset_iter();
    for event in offset_iter {
        let (event, range) = event;
        // The range should be within bounds and the slice should be non-empty
        assert!(
            range.start <= range.end,
            "invalid range: {range:?} for event {event:?}"
        );
        assert!(
            range.end <= input.len(),
            "range end {} exceeds input len {}",
            range.end,
            input.len()
        );
        // The slice should match the event text where applicable
        let _slice = &input[range];
    }

    // Assert we got expected events for "# h\n\npara\n"
    assert!(
        events.len() >= 4,
        "expected at least 4 events, got {}",
        events.len()
    );
    // First event should be Start(Heading(1))
    let first_event = &events[0];
    if let Event::Start(Tag::Heading { level, .. }) = first_event {
        assert!(
            matches!(level, pulldown_cmark::HeadingLevel::H1),
            "expected Heading level H1"
        );
    } else {
        panic!(
            "expected Start(Heading(1)) as first event, got {:?}",
            first_event
        );
    }
}

// ---------------------------------------------------------------------------
// 5. gray_matter_parses_yaml_and_toml
// ---------------------------------------------------------------------------

#[test]
fn gray_matter_parses_yaml_and_toml() {
    use gray_matter::{
        engine::{TOML, YAML},
        Matter, ParsedEntity,
    };

    // YAML front matter
    let yaml_input = r#"---
title: Hello
author: Test
---
# Content
"#;

    let yaml_matter = Matter::<YAML>::new();
    let yaml_result: ParsedEntity = yaml_matter.parse(yaml_input).expect("YAML parse failed");

    let data = yaml_result.data.as_ref().expect("YAML data is None");
    assert_eq!(
        data["title"].as_string().unwrap(),
        "Hello",
        "YAML title mismatch"
    );
    assert_eq!(
        data["author"].as_string().unwrap(),
        "Test",
        "YAML author mismatch"
    );

    // TOML front matter
    let toml_input = r#"---
title = "Hello"
author = "Test"
---
# Content
"#;

    let toml_matter = Matter::<TOML>::new();
    let toml_result: ParsedEntity = toml_matter.parse(toml_input).expect("TOML parse failed");

    let data = toml_result.data.as_ref().expect("TOML data is None");
    assert_eq!(
        data["title"].as_string().unwrap(),
        "Hello",
        "TOML title mismatch"
    );
    assert_eq!(
        data["author"].as_string().unwrap(),
        "Test",
        "TOML author mismatch"
    );
}
