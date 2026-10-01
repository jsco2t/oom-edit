//! Keep the manual large-document fixture representative and intact.

#[test]
fn kitchen_sink_large_has_two_substantial_code_fences() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../examples/kitchen-sink-1mb.md"
    );
    let document = std::fs::read_to_string(path).expect("large kitchen-sink example is present");

    assert!((1024 * 1024..1024 * 1024 + 2048).contains(&document.len()));
    assert!(document.contains("## Inline Formatting"));
    assert!(document.contains("## Tables"));
    assert!(document.contains("### Mixed Markdown Checkpoints"));

    for (heading, language) in [("Large Rust", "rust"), ("Large Go", "go")] {
        let opener = format!("### {heading} Fence\n\n```{language}\n");
        let (_, after_opener) = document
            .split_once(&opener)
            .unwrap_or_else(|| panic!("missing {language} fence"));
        let (code, _) = after_opener
            .split_once("\n```\n")
            .unwrap_or_else(|| panic!("unclosed {language} fence"));
        assert!(code.len() >= 390 * 1024, "{language} fence is too small");
    }
}
