use oom_edit_core::{analyze_markdown, EditorSession, FrontMatter, Value};

#[test]
fn yaml_toml_absent_and_malformed_front_matter_match_the_editor() {
    for (source, body) in [
        ("---\ntitle: Café\n---\n# Body\n", "# Body\n"),
        ("+++\ntitle = \"Café\"\n+++\n# Body\n", "# Body\n"),
        ("# No metadata\n", "# No metadata\n"),
        (
            "---\ntitle: [\n---\n# Body\n",
            "---\ntitle: [\n---\n# Body\n",
        ),
        (
            "+++\ntitle = [\n+++\n# Body\n",
            "+++\ntitle = [\n+++\n# Body\n",
        ),
    ] {
        let analysis = analyze_markdown(source);
        let editor = EditorSession::from_text(source);
        assert_eq!(analysis.front_matter, *editor.front_matter());
        assert_eq!(analysis.body, body);
        assert_eq!(&source[analysis.body_span.clone()], body);
        if matches!(
            analysis.front_matter,
            FrontMatter::Yaml(Err(_)) | FrontMatter::Toml(Err(_))
        ) {
            assert_eq!(analysis.diagnostics.len(), 1);
            assert_eq!(analysis.body_span, 0..source.len());
        } else {
            assert!(analysis.diagnostics.is_empty());
        }
    }
}

#[test]
fn metadata_spans_are_exact_for_crlf_and_unicode() {
    let source = "---\r\ntitle: Café\r\n---\r\n# Über\r\n";
    let analysis = analyze_markdown(source);
    let expected_end = source.find("# Über").unwrap();
    assert_eq!(analysis.front_matter_span, Some(0..expected_end));
    assert_eq!(analysis.body_span, expected_end..source.len());
    assert_eq!(analysis.body, "# Über\r\n");
    let FrontMatter::Yaml(Ok(metadata)) = analysis.front_matter else {
        panic!("YAML should parse")
    };
    assert_eq!(metadata.get("title"), Some(&Value::str("Café".into())));
}

#[test]
fn first_top_level_h1_decodes_atx_and_setext_and_skips_fences() {
    for (source, expected, raw) in [
        (
            "```md\n# fake\n```\n# A &amp; B \\* C\n",
            "A & B * C",
            "# A &amp; B \\* C",
        ),
        (
            "> # nested\n\nReal &amp; Title\n================\n",
            "Real & Title",
            "Real &amp; Title\n================",
        ),
        (
            "# Café **bold** `code`\n",
            "Café bold code",
            "# Café **bold** `code`",
        ),
    ] {
        let analysis = analyze_markdown(source);
        let heading = analysis.first_h1.expect("expected first H1");
        assert_eq!(heading.text, expected);
        assert_eq!(source[heading.source_span].trim_end_matches('\n'), raw);
    }
}

#[test]
fn analysis_has_borrowed_body_and_no_editing_engine_path() {
    let source = String::from("---\ntitle: Owned\n---\n# Hello\n");
    let analysis = analyze_markdown(&source);
    assert_eq!(
        analysis.body.as_ptr(),
        source[analysis.body_span.clone()].as_ptr()
    );
    let module = include_str!("../src/analysis.rs");
    for forbidden in [
        "EditorSession",
        "LiveDocument",
        "VimCore",
        "Highlighter",
        "SpellState",
        "ratatui",
        "crossterm",
    ] {
        assert!(
            !module.contains(forbidden),
            "analysis module must not construct {forbidden}"
        );
    }
}

#[test]
fn repeated_unicode_heading_text_keeps_the_first_exact_source_span() {
    let source = "Prelude Résumé\n\n# Résumé &amp; Résumé\n\n# Résumé\n";
    let analysis = analyze_markdown(source);
    let heading = analysis.first_h1.unwrap();
    assert_eq!(heading.text, "Résumé & Résumé");
    let start = source.find("# Résumé").unwrap();
    assert_eq!(heading.source_span.start, start);
    assert_eq!(&source[heading.source_span], "# Résumé &amp; Résumé\n");
}
