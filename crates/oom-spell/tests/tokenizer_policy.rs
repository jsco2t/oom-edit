use std::ops::Range;

use oom_spell::{
    classify_candidate, tokenize_chunk, CandidateDecision, Token, TokenizerState,
    MAX_CHECKED_WORD_BYTES,
};

fn collect_with_budget<'a>(
    text: &'a str,
    range: Range<usize>,
    exclusions: &[Range<usize>],
    budget: usize,
) -> Vec<Token<'a>> {
    let mut state = TokenizerState::new(range);
    let mut tokens = Vec::new();
    while !state.is_complete() {
        let before = state.position();
        let chunk = tokenize_chunk(text, exclusions, &mut state, budget);
        let after = state.position();
        assert_eq!(chunk.consumed(), before..after);
        assert!(after > before, "tokenizer must make progress");
        assert!(after - before <= budget, "tokenizer exceeded its budget");
        tokens.extend(chunk.into_tokens());
    }
    tokens
}

fn decision_ranges(token: &Token<'_>) -> Vec<Range<usize>> {
    match classify_candidate(token) {
        CandidateDecision::Skip => Vec::new(),
        CandidateDecision::Check(checked) => {
            assert_eq!(checked, token.text, "Check must borrow the complete token");
            vec![token.range.clone()]
        }
        CandidateDecision::CheckParts(ranges) => ranges,
    }
}

#[test]
fn tokenizer_emits_byte_exact_utf8_ranges_and_shapes() {
    let text = "Well-known can't O’Reilly naïve foo_bar abc123 123xyz A rock-’-roll";
    let tokens = collect_with_budget(text, 0..text.len(), &[], 1);
    let observed: Vec<_> = tokens
        .iter()
        .map(|token| {
            (
                token.text,
                token.range.clone(),
                token.shape.contains_hyphen(),
                token.shape.all_caps(),
                token.shape.mixed_case_interior(),
                token.shape.adjacent_to_word_char(),
                token.shape.has_non_ascii_alpha(),
            )
        })
        .collect();

    let expected_text = [
        "Well-known",
        "can't",
        "O’Reilly",
        "naïve",
        "foo",
        "bar",
        "abc",
        "xyz",
        "A",
        "rock",
        "roll",
    ];
    assert_eq!(
        observed
            .iter()
            .map(|row| (row.0, row.1.clone()))
            .collect::<Vec<_>>(),
        expected_text
            .map(|word| (word, range_of(text, word)))
            .to_vec()
    );
    assert!(observed[0].2);
    assert!(observed[2].4);
    assert!(observed[3].6);
    for row in observed.iter().take(8).skip(4) {
        assert!(row.5, "identifier fragment should be marked");
    }
    assert!(!observed[8].3, "one letter is not an acronym");
}

#[test]
fn tokenization_is_identical_across_chunk_sizes_and_exclusions() {
    let text = "alpha naïve hidden-token can't omega_2 final";
    let hidden = text.find("hidden-token").unwrap();
    let exclusions: Vec<_> = std::iter::once(hidden..hidden + "hidden-token".len()).collect();
    let expected = collect_with_budget(text, 0..text.len(), &exclusions, 4096);
    let expected_tokens = ["alpha", "naïve", "can't", "omega", "final"]
        .map(|word| (word, range_of(text, word)))
        .to_vec();
    assert_eq!(
        expected
            .iter()
            .map(|token| (token.text, token.range.clone()))
            .collect::<Vec<_>>(),
        expected_tokens
    );
    assert!(expected.iter().all(|token| {
        exclusions
            .iter()
            .all(|range| token.range.end <= range.start || token.range.start >= range.end)
    }));

    for budget in [1, 2, 3, 4, 7, 64, 4096] {
        assert_eq!(
            collect_with_budget(text, 0..text.len(), &exclusions, budget),
            expected,
            "tokenization changed at budget {budget}"
        );
    }
}

#[test]
fn overlong_token_completion_does_not_break_byte_budgeting() {
    let text = format!("{} tail", "a".repeat(1024 * 1024));
    let tokens = collect_with_budget(&text, 0..text.len(), &[], 4096);
    assert_eq!(tokens.len(), 2);
    assert_eq!(tokens[0].range, 0..1024 * 1024);
    assert!(matches!(
        classify_candidate(&tokens[0]),
        CandidateDecision::Skip
    ));
    assert_eq!(tokens[1].text, "tail");
}

#[test]
fn candidate_policy_is_exhaustive_and_table_driven() {
    let exact_limit = "a".repeat(MAX_CHECKED_WORD_BYTES);
    let overlong = "a".repeat(MAX_CHECKED_WORD_BYTES + 1);
    let cases = [
        ("word", vec!["word"]),
        ("Word", vec!["Word"]),
        ("can't", vec!["can't"]),
        ("NASA", vec![]),
        ("CamelCase", vec![]),
        ("A", vec![]),
        ("a's", vec![]),
        ("A’s", vec![]),
        ("naïve", vec![]),
        ("&amp;", vec![]),
        ("foo_bar", vec![]),
        ("abc123", vec![]),
        ("well-known", vec!["well", "known"]),
        ("state-of-the-art", vec!["state", "of", "the", "art"]),
        ("X-ray", vec!["ray"]),
        ("NASA-word", vec!["word"]),
        ("CamelCase-word", vec!["word"]),
        (exact_limit.as_str(), vec![exact_limit.as_str()]),
        (overlong.as_str(), vec![]),
    ];

    for (text, expected) in cases {
        let tokens = collect_with_budget(text, 0..text.len(), &[], 4);
        let checked: Vec<_> = tokens
            .iter()
            .flat_map(|token| decision_ranges(token))
            .map(|range| &text[range])
            .collect();
        assert_eq!(checked, expected, "wrong decision for {text:?}");
    }
}

#[test]
fn punctuation_is_internal_only_between_letters() {
    let text = "'start end' two--parts rock-’-roll mother-in-law l’esprit";
    let tokens = collect_with_budget(text, 0..text.len(), &[], 2);
    assert_eq!(
        tokens.iter().map(|token| token.text).collect::<Vec<_>>(),
        [
            "start",
            "end",
            "two",
            "parts",
            "rock",
            "roll",
            "mother-in-law",
            "l’esprit",
        ]
    );
}

fn range_of(text: &str, needle: &str) -> Range<usize> {
    let start = text
        .find(needle)
        .unwrap_or_else(|| panic!("missing {needle:?}"));
    start..start + needle.len()
}
