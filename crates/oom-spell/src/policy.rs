use std::ops::Range;

use crate::{Token, MAX_CHECKED_WORD_BYTES};

/// Text-generic decision for one token candidate.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CandidateDecision<'a> {
    /// Do not consult the spelling engine for this token.
    Skip,
    /// Check the complete borrowed token.
    Check(&'a str),
    /// Check the listed absolute byte ranges independently.
    CheckParts(Vec<Range<usize>>),
}

/// Classify one token without Markdown or spelling-engine knowledge.
///
/// Acronyms, interior mixed case, identifiers, entities, single letters,
/// overlong candidates, and non-ASCII alphabetic candidates are skipped.
/// Eligible ASCII hyphenated candidates are split and checked part by part.
pub fn classify_candidate<'a>(token: &Token<'a>) -> CandidateDecision<'a> {
    let shape = token.shape;
    if token.text.len() > MAX_CHECKED_WORD_BYTES
        || shape.adjacent_to_word_char()
        || shape.entity_shaped()
        || shape.has_non_ascii_alpha()
    {
        return CandidateDecision::Skip;
    }

    if shape.contains_hyphen() {
        let ranges: Vec<_> = token
            .text
            .split('-')
            .scan(token.range.start, |offset, part| {
                let start = *offset;
                *offset += part.len() + 1;
                Some((part, start..start + part.len()))
            })
            .filter_map(|(part, range)| eligible_part(part).then_some(range))
            .collect();
        return if ranges.is_empty() {
            CandidateDecision::Skip
        } else {
            CandidateDecision::CheckParts(ranges)
        };
    }

    if shape.all_caps() || shape.mixed_case_interior() || policy_letter_count(token.text) == 1 {
        CandidateDecision::Skip
    } else {
        CandidateDecision::Check(token.text)
    }
}

fn eligible_part(part: &str) -> bool {
    if part.is_empty() || part.len() > MAX_CHECKED_WORD_BYTES || !part.is_ascii() {
        return false;
    }
    let letters: Vec<_> = part
        .chars()
        .filter(|character| character.is_alphabetic())
        .collect();
    if policy_letter_count(part) <= 1 {
        return false;
    }
    let all_caps = letters.len() >= 2 && letters.iter().all(|character| !character.is_lowercase());
    let mixed_case_interior = letters
        .iter()
        .skip(1)
        .any(|character| character.is_uppercase());
    !all_caps && !mixed_case_interior
}

fn policy_letter_count(text: &str) -> usize {
    let base = ["'s", "'S", "’s", "’S"]
        .into_iter()
        .find_map(|suffix| text.strip_suffix(suffix))
        .unwrap_or(text);
    base.chars()
        .filter(|character| character.is_alphabetic())
        .count()
}
