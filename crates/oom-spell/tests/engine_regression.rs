use oom_spell::{
    normalize_dictionary_entry, AddWordOutcome, BuildProgress, DictionaryEntryError, SpellEngine,
    SpellEngineBuilder, MAX_CHECKED_WORD_BYTES,
};

fn finish_with_budget(lists: Vec<String>, budget: usize) -> SpellEngine {
    let mut builder = SpellEngineBuilder::new(lists);
    while builder.step(budget) == BuildProgress::Pending {}
    builder.finish().expect("completed builder should finish")
}

fn tiny_engine() -> SpellEngine {
    finish_with_budget(
        vec!["the\ncat\ndog\ncoat\ncost\ncast\nfrom\nform\nhello\nworld\n".into()],
        4,
    )
}

fn encoded_word(mut value: usize, len: usize) -> String {
    let mut bytes = vec![b'a'; len];
    for byte in bytes.iter_mut().rev() {
        *byte = b'a' + (value % 26) as u8;
        value /= 26;
    }
    String::from_utf8(bytes).expect("generated words are ASCII")
}

#[test]
fn builder_one_byte_steps_equal_one_shot_build() {
    let lists = vec![
        "# primary\r\nThe\r\ncat\nDUPLICATE\n".to_owned(),
        format!(
            "duplicate\n{}\nworld",
            "x".repeat(MAX_CHECKED_WORD_BYTES + 1)
        ),
    ];
    let one_byte = finish_with_budget(lists.clone(), 1);
    let one_shot = finish_with_budget(lists, usize::MAX);

    assert_eq!(one_byte.word_count(), one_shot.word_count());
    assert_eq!(one_byte.word_count(), 4);
    for word in ["the", "THE", "cat", "duplicate", "world"] {
        assert_eq!(one_byte.check(word), one_shot.check(word), "{word}");
    }
    assert!(!one_byte.check(&"x".repeat(MAX_CHECKED_WORD_BYTES + 1)));
}

#[test]
fn builder_step_never_completes_before_its_exact_input_budget() {
    let mut builder = SpellEngineBuilder::new(vec!["ab\n".into(), String::new(), "cd".into()]);
    for consumed in 1..5 {
        assert_eq!(
            builder.step(1),
            BuildProgress::Pending,
            "builder completed after only {consumed} of 5 bytes"
        );
    }
    assert_eq!(builder.step(1), BuildProgress::Pending);
    assert!(builder.finish().is_err());

    let mut builder = SpellEngineBuilder::new(vec!["ab\n".into(), String::new(), "cd".into()]);
    assert_eq!(builder.step(5), BuildProgress::Pending);
    assert_eq!(builder.step(usize::MAX), BuildProgress::Complete);
    assert_eq!(builder.step(1), BuildProgress::Complete);
    let engine = builder.finish().expect("all five bytes were consumed");
    assert!(engine.check("ab"));
    assert!(engine.check("cd"));
}

#[test]
fn builder_budget_zero_does_not_advance_and_premature_finish_is_an_error() {
    let mut builder = SpellEngineBuilder::new(vec!["hello\n".into()]);
    assert_eq!(builder.step(0), BuildProgress::Pending);
    assert!(builder.finish().is_err());

    let empty = SpellEngineBuilder::new(Vec::new());
    let engine = empty.finish().expect("an empty input is already complete");
    assert_eq!(engine.word_count(), 0);
    assert_eq!(engine.generation(), 1);
}

#[test]
fn normalization_is_the_single_dictionary_entry_policy() {
    assert_eq!(
        normalize_dictionary_entry("  HeLLo  ").unwrap(),
        Some("hello".into())
    );
    assert_eq!(normalize_dictionary_entry("# comment").unwrap(), None);
    assert_eq!(normalize_dictionary_entry("   ").unwrap(), None);
    assert_eq!(
        normalize_dictionary_entry("can't").unwrap(),
        Some("can't".into())
    );
    assert!(matches!(
        normalize_dictionary_entry("naïve"),
        Err(DictionaryEntryError::NonAscii)
    ));
    assert!(matches!(
        normalize_dictionary_entry("word2"),
        Err(DictionaryEntryError::InvalidCharacter)
    ));
    assert!(matches!(
        normalize_dictionary_entry(&"a".repeat(MAX_CHECKED_WORD_BYTES + 1)),
        Err(DictionaryEntryError::TooLong)
    ));
}

#[test]
fn normalize_builder_and_add_share_trim_and_length_boundaries() {
    let exact = "a".repeat(MAX_CHECKED_WORD_BYTES);
    let overlong = "b".repeat(MAX_CHECKED_WORD_BYTES + 1);
    let padded_exact = format!(" \t{exact}\r ");
    let padded_overlong = format!(" \t{overlong}\r ");

    assert_eq!(
        normalize_dictionary_entry(&padded_exact).unwrap(),
        Some(exact.clone())
    );
    assert!(matches!(
        normalize_dictionary_entry(&padded_overlong),
        Err(DictionaryEntryError::TooLong)
    ));

    let mut engine = finish_with_budget(vec![format!("{padded_exact}\n{padded_overlong}\n")], 1);
    assert_eq!(engine.word_count(), 1);
    assert!(engine.check(&exact));
    assert_eq!(
        engine.add_word(&padded_exact).unwrap(),
        AddWordOutcome::AlreadyPresent {
            normalized: exact.clone()
        }
    );
    assert!(matches!(
        engine.add_word(&padded_overlong),
        Err(DictionaryEntryError::TooLong)
    ));
}

#[test]
fn check_covers_exact_fold_and_straight_or_curly_possessives() {
    let engine = finish_with_budget(vec!["cat\nDog\n".into()], 1);
    for accepted in [
        "cat", "Cat", "CAT", "dog", "Dog", "dog's", "Dog’s", "DOG'S", "DOG’S",
    ] {
        assert!(
            engine.check(accepted),
            "expected {accepted:?} to be accepted"
        );
    }
    for rejected in ["dogs", "dog'", "cat’", "unknown", "café"] {
        assert!(
            !engine.check(rejected),
            "expected {rejected:?} to be rejected"
        );
    }
}

#[test]
fn add_word_deduplicates_and_only_real_insertions_advance_generation() {
    let mut engine = tiny_engine();
    let initial = engine.generation();

    assert_eq!(
        engine.add_word("  NewWord ").unwrap(),
        AddWordOutcome::Inserted {
            normalized: "newword".into()
        }
    );
    assert_eq!(engine.generation(), initial + 1);
    assert_eq!(
        engine.add_word("NEWWORD").unwrap(),
        AddWordOutcome::AlreadyPresent {
            normalized: "newword".into()
        }
    );
    assert_eq!(engine.generation(), initial + 1);
    assert_eq!(
        engine.add_word(" # ignored ").unwrap(),
        AddWordOutcome::Ignored
    );
    assert_eq!(engine.generation(), initial + 1);
    assert!(engine.add_word("not valid!").is_err());
    assert_eq!(engine.generation(), initial + 1);
}

#[test]
fn suggestions_are_bounded_ranked_deterministic_and_case_restored() {
    let engine = tiny_engine();
    assert_eq!(engine.suggest("teh", 9), ["the"]);
    assert_eq!(engine.suggest("Teh", 9), ["The"]);
    assert_eq!(engine.suggest("TEH", 9), ["THE"]);
    assert_eq!(
        engine.suggest("cta", 9).first().map(String::as_str),
        Some("cat")
    );
    assert_eq!(engine.suggest("cot", 3), ["cat", "coat", "cost"]);
    assert!(engine.suggest("cot", 0).is_empty());
    assert_eq!(engine.suggest("cot", 1), ["cat"]);
    assert!(engine.suggest("zzzzzzzz", 99).len() <= 9);
}

#[test]
fn suggestions_enforce_exact_maximums_against_more_than_nine_candidates() {
    let candidates = [
        "aaab", "aaac", "aaad", "aaae", "aaaf", "aaag", "aaah", "aaai", "aaaj", "aaak", "aaal",
    ];
    let engine = finish_with_budget(vec![candidates.join("\n")], 2);

    assert!(engine.suggest("aaaa", 0).is_empty());
    assert_eq!(engine.suggest("aaaa", 1), ["aaab"]);
    assert_eq!(engine.suggest("aaaa", 9), candidates[..9]);
    assert_eq!(engine.suggest("aaaa", 99), candidates[..9]);
}

#[test]
fn builder_skips_an_overlong_remainder_without_losing_the_next_line() {
    let list = format!("{}\nkept\n", "x".repeat(32 * 1024));
    let engine = finish_with_budget(vec![list], 1);
    assert_eq!(engine.word_count(), 1);
    assert!(engine.check("kept"));
}

#[test]
fn multi_run_finalization_preserves_every_unique_word_and_search_order() {
    let mut expected: Vec<_> = (0..9_000).map(|index| encoded_word(index, 6)).collect();
    let mut shuffled = expected.clone();
    let mut state = 0x5eed_f1a1_12e5_u64;
    for index in (1..shuffled.len()).rev() {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1);
        shuffled.swap(index, state as usize % (index + 1));
    }
    let early_duplicates: Vec<_> = shuffled[..128].to_vec();
    shuffled.splice(4_096..4_096, early_duplicates);
    shuffled.extend(expected.iter().take(128).cloned());
    let list = shuffled.join("\n");

    let one_byte = finish_with_budget(vec![list.clone()], 1);
    let one_shot = finish_with_budget(vec![list], usize::MAX);
    assert_eq!(one_byte.word_count(), expected.len());
    assert_eq!(one_shot.word_count(), expected.len());
    for word in &expected {
        assert!(one_byte.check(word), "one-byte build lost {word}");
        assert!(one_shot.check(word), "one-shot build lost {word}");
    }

    let mut engine = one_byte;
    let existing = expected.pop().expect("fixture should be non-empty");
    assert!(matches!(
        engine.add_word(&existing).unwrap(),
        AddWordOutcome::AlreadyPresent { .. }
    ));
    assert!(matches!(
        engine.add_word("zzzzzz").unwrap(),
        AddWordOutcome::Inserted { .. }
    ));
    assert!(engine.check("zzzzzz"));
    assert_eq!(engine.word_count(), 9_001);
}

#[test]
fn suggestions_obey_64_byte_boundary() {
    let exact = "a".repeat(MAX_CHECKED_WORD_BYTES);
    let mut near = exact.clone();
    near.replace_range(MAX_CHECKED_WORD_BYTES - 1..MAX_CHECKED_WORD_BYTES, "b");
    let engine = finish_with_budget(vec![format!("{exact}\n")], 7);

    assert_eq!(engine.suggest(&near, 9), [exact]);
    assert!(engine
        .suggest(&"a".repeat(MAX_CHECKED_WORD_BYTES + 1), 9)
        .is_empty());
}
