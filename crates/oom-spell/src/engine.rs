use std::error::Error;
use std::fmt;

/// Maximum byte length accepted for a dictionary entry or checked word.
pub const MAX_CHECKED_WORD_BYTES: usize = 64;

const DISTANCE_CUTOFF: usize = 2;
const MAX_SUGGESTIONS: usize = 9;
const FINALIZE_BUCKET_BUDGET: usize = 4 * 1024;
const FINALIZE_ITEMS_PER_UNIT: usize = 4 * 1024;

#[derive(Debug)]
struct MergeState {
    source: Vec<String>,
    output: Vec<String>,
    width: usize,
    pair_start: usize,
    left: usize,
    left_end: usize,
    right: usize,
    right_end: usize,
}

#[derive(Debug)]
struct DedupState {
    source: Vec<String>,
    output: Vec<String>,
    offset: usize,
}

impl DedupState {
    fn new(source: Vec<String>) -> Self {
        Self {
            output: Vec::with_capacity(source.len()),
            source,
            offset: 0,
        }
    }
}

impl MergeState {
    fn new(source: Vec<String>) -> Self {
        let mut state = Self {
            output: Vec::with_capacity(source.len()),
            source,
            width: FINALIZE_ITEMS_PER_UNIT,
            pair_start: 0,
            left: 0,
            left_end: 0,
            right: 0,
            right_end: 0,
        };
        state.reset_pair();
        state
    }

    fn reset_pair(&mut self) {
        self.left = self.pair_start;
        self.left_end = (self.pair_start + self.width).min(self.source.len());
        self.right = self.left_end;
        self.right_end = (self.pair_start + 2 * self.width).min(self.source.len());
    }
}

/// Progress returned by one budgeted builder step.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BuildProgress {
    /// Input consumption or budgeted finalization work remains.
    Pending,
    /// Every input list and finalization phase is complete.
    Complete,
}

/// Error returned when a builder is finished before input and finalization are complete.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BuildIncomplete;

impl fmt::Display for BuildIncomplete {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("spell engine build is incomplete")
    }
}

impl Error for BuildIncomplete {}

/// Reason a personal or plain-wordlist entry cannot be normalized.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DictionaryEntryError {
    /// The entry contains a non-ASCII byte.
    NonAscii,
    /// The entry contains something other than ASCII letters or an internal apostrophe.
    InvalidCharacter,
    /// The trimmed entry exceeds [`MAX_CHECKED_WORD_BYTES`].
    TooLong,
}

impl fmt::Display for DictionaryEntryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NonAscii => formatter.write_str("dictionary entry must be ASCII"),
            Self::InvalidCharacter => formatter.write_str(
                "dictionary entry must contain only ASCII letters and internal apostrophes",
            ),
            Self::TooLong => write!(
                formatter,
                "dictionary entry exceeds {MAX_CHECKED_WORD_BYTES} bytes"
            ),
        }
    }
}

impl Error for DictionaryEntryError {}

/// Result of adding one normalized word to an existing engine.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AddWordOutcome {
    /// A new word was inserted.
    Inserted {
        /// The lowercase word stored by the engine.
        normalized: String,
    },
    /// The normalized word was already present.
    AlreadyPresent {
        /// The lowercase word already stored by the engine.
        normalized: String,
    },
    /// The input was blank or a comment.
    Ignored,
}

/// Normalize one plain-wordlist or personal-dictionary entry.
///
/// Leading and trailing ASCII whitespace is removed. Blank entries and lines
/// whose first non-whitespace byte is `#` return `Ok(None)`. Accepted entries
/// contain only ASCII letters and internal straight apostrophes, are at most 64
/// bytes, and are returned in ASCII lowercase.
pub fn normalize_dictionary_entry(entry: &str) -> Result<Option<String>, DictionaryEntryError> {
    let entry = entry.trim_matches(|character: char| character.is_ascii_whitespace());
    if entry.is_empty() || entry.starts_with('#') {
        return Ok(None);
    }
    if !entry.is_ascii() {
        return Err(DictionaryEntryError::NonAscii);
    }
    if entry.len() > MAX_CHECKED_WORD_BYTES {
        return Err(DictionaryEntryError::TooLong);
    }

    let bytes = entry.as_bytes();
    let valid = bytes.iter().enumerate().all(|(index, byte)| {
        byte.is_ascii_alphabetic()
            || (*byte == b'\''
                && index > 0
                && index + 1 < bytes.len()
                && bytes[index - 1].is_ascii_alphabetic()
                && bytes[index + 1].is_ascii_alphabetic())
    });
    if !valid {
        return Err(DictionaryEntryError::InvalidCharacter);
    }
    Ok(Some(entry.to_ascii_lowercase()))
}

/// Incremental builder for a [`SpellEngine`].
///
/// Input lists are UTF-8 strings containing one entry per line. Calls to
/// [`step`](Self::step) consume no more than the supplied number of input
/// bytes. Physical lines that grow beyond the entry limit are skipped without
/// retaining their remainder.
#[derive(Debug)]
pub struct SpellEngineBuilder {
    lists: Vec<String>,
    list_index: usize,
    offset: usize,
    partial_line: Vec<u8>,
    pending_whitespace: usize,
    skipping_overlong: bool,
    buckets: [Vec<String>; MAX_CHECKED_WORD_BYTES + 1],
    finalize_bucket: usize,
    finalize_offset: usize,
    finalize_credit: usize,
    merge: Option<MergeState>,
    dedup: Option<DedupState>,
    word_count: usize,
    complete: bool,
}

impl SpellEngineBuilder {
    /// Create a builder over owned plain word lists.
    pub fn new(lists: Vec<String>) -> Self {
        let complete = lists.iter().all(String::is_empty);
        Self {
            lists,
            list_index: 0,
            offset: 0,
            partial_line: Vec::with_capacity(MAX_CHECKED_WORD_BYTES + 1),
            pending_whitespace: 0,
            skipping_overlong: false,
            buckets: std::array::from_fn(|_| Vec::new()),
            finalize_bucket: 0,
            finalize_offset: 0,
            finalize_credit: 0,
            merge: None,
            dedup: None,
            word_count: 0,
            complete,
        }
    }

    /// Consume at most `max_bytes` input bytes and report whether work remains.
    pub fn step(&mut self, max_bytes: usize) -> BuildProgress {
        if self.complete {
            return BuildProgress::Complete;
        }
        if max_bytes == 0 {
            return BuildProgress::Pending;
        }

        let mut remaining = max_bytes;
        while remaining > 0 && !self.complete {
            self.settle_finished_lists();
            if self.list_index < self.lists.len() {
                let byte = self.lists[self.list_index].as_bytes()[self.offset];
                self.offset += 1;
                remaining -= 1;
                self.consume_byte(byte);
            } else {
                self.advance_finalization(&mut remaining);
            }
        }
        self.settle_finished_lists();
        self.progress()
    }

    /// Finish a builder whose input and budgeted finalization are complete.
    pub fn finish(self) -> Result<SpellEngine, BuildIncomplete> {
        if !self.complete {
            return Err(BuildIncomplete);
        }
        Ok(SpellEngine {
            buckets: self.buckets,
            word_count: self.word_count,
            generation: 1,
        })
    }

    fn progress(&self) -> BuildProgress {
        if self.complete {
            BuildProgress::Complete
        } else {
            BuildProgress::Pending
        }
    }

    fn settle_finished_lists(&mut self) {
        while self.list_index < self.lists.len() && self.offset == self.lists[self.list_index].len()
        {
            self.finish_line();
            self.list_index += 1;
            self.offset = 0;
        }
    }

    fn advance_finalization(&mut self, remaining: &mut usize) {
        let needed = FINALIZE_BUCKET_BUDGET - self.finalize_credit;
        let consumed = (*remaining).min(needed);
        self.finalize_credit += consumed;
        *remaining -= consumed;
        if self.finalize_credit != FINALIZE_BUCKET_BUDGET {
            return;
        }

        self.finalize_credit = 0;
        self.finalize_unit();
    }

    fn finalize_unit(&mut self) {
        if self.dedup.is_some() {
            self.dedup_unit();
            return;
        }
        if self.merge.is_some() {
            self.merge_unit();
            return;
        }

        let bucket = &mut self.buckets[self.finalize_bucket];
        if bucket.is_empty() {
            self.complete_finalized_bucket();
            return;
        }

        let end = (self.finalize_offset + FINALIZE_ITEMS_PER_UNIT).min(bucket.len());
        bucket[self.finalize_offset..end].sort_unstable();
        self.finalize_offset = end;
        if end != bucket.len() {
            return;
        }

        if bucket.len() <= FINALIZE_ITEMS_PER_UNIT {
            self.dedup = Some(DedupState::new(std::mem::take(bucket)));
            return;
        }

        self.merge = Some(MergeState::new(std::mem::take(bucket)));
    }

    fn merge_unit(&mut self) {
        let merge = self.merge.as_mut().expect("merge state should exist");
        for _ in 0..FINALIZE_ITEMS_PER_UNIT {
            while merge.left == merge.left_end && merge.right == merge.right_end {
                merge.pair_start += 2 * merge.width;
                if merge.pair_start < merge.source.len() {
                    merge.reset_pair();
                    continue;
                }
                if 2 * merge.width >= merge.source.len() {
                    let merge = self.merge.take().expect("completed merge should exist");
                    self.dedup = Some(DedupState::new(merge.output));
                    return;
                }
                merge.source = std::mem::take(&mut merge.output);
                merge.output = Vec::with_capacity(merge.source.len());
                merge.width *= 2;
                merge.pair_start = 0;
                merge.reset_pair();
            }

            let take_left = merge.right == merge.right_end
                || (merge.left < merge.left_end
                    && merge.source[merge.left] <= merge.source[merge.right]);
            let source_index = if merge.left == merge.left_end {
                let index = merge.right;
                merge.right += 1;
                index
            } else if take_left {
                let index = merge.left;
                merge.left += 1;
                index
            } else {
                let index = merge.right;
                merge.right += 1;
                index
            };
            merge
                .output
                .push(std::mem::take(&mut merge.source[source_index]));
        }
    }

    fn dedup_unit(&mut self) {
        let dedup = self.dedup.as_mut().expect("dedup state should exist");
        let end = (dedup.offset + FINALIZE_ITEMS_PER_UNIT).min(dedup.source.len());
        for index in dedup.offset..end {
            let word = std::mem::take(&mut dedup.source[index]);
            if dedup.output.last() != Some(&word) {
                dedup.output.push(word);
            }
        }
        dedup.offset = end;
        if end != dedup.source.len() {
            return;
        }
        let dedup = self.dedup.take().expect("completed dedup should exist");
        self.buckets[self.finalize_bucket] = dedup.output;
        self.complete_finalized_bucket();
    }

    fn complete_finalized_bucket(&mut self) {
        self.word_count += self.buckets[self.finalize_bucket].len();
        self.finalize_bucket += 1;
        self.finalize_offset = 0;
        if self.finalize_bucket == self.buckets.len() {
            self.complete = true;
        }
    }

    fn consume_byte(&mut self, byte: u8) {
        if byte == b'\n' {
            self.finish_line();
            return;
        }
        if self.skipping_overlong {
            return;
        }

        if byte.is_ascii_whitespace() {
            if !self.partial_line.is_empty() {
                self.pending_whitespace = self
                    .pending_whitespace
                    .saturating_add(1)
                    .min(MAX_CHECKED_WORD_BYTES + 1);
            }
            return;
        }

        let new_len = self
            .partial_line
            .len()
            .saturating_add(self.pending_whitespace)
            .saturating_add(1);
        if new_len > MAX_CHECKED_WORD_BYTES {
            self.partial_line.clear();
            self.pending_whitespace = 0;
            self.skipping_overlong = true;
            return;
        }
        self.partial_line
            .extend(std::iter::repeat_n(b' ', self.pending_whitespace));
        self.pending_whitespace = 0;
        self.partial_line.push(byte);
    }

    fn finish_line(&mut self) {
        if !self.skipping_overlong && !self.partial_line.is_empty() {
            if let Ok(line) = std::str::from_utf8(&self.partial_line) {
                if let Ok(Some(word)) = normalize_dictionary_entry(line) {
                    self.buckets[word.len()].push(word);
                }
            }
        }
        self.partial_line.clear();
        self.pending_whitespace = 0;
        self.skipping_overlong = false;
    }
}

/// Immutable dictionary engine with mutable personal-word insertion.
#[derive(Debug)]
pub struct SpellEngine {
    buckets: [Vec<String>; MAX_CHECKED_WORD_BYTES + 1],
    word_count: usize,
    generation: u64,
}

impl SpellEngine {
    /// Return whether `word` is known by exact, ASCII-folded, or possessive lookup.
    pub fn check(&self, word: &str) -> bool {
        if word.is_empty() || word.len() > MAX_CHECKED_WORD_BYTES {
            return false;
        }
        if self.contains_exact_or_folded(word) {
            return true;
        }
        let folded = word.to_ascii_lowercase();
        let base = folded
            .strip_suffix("'s")
            .or_else(|| folded.strip_suffix("’s"));
        base.is_some_and(|base| !base.is_empty() && self.contains_exact_or_folded(base))
    }

    /// Return deterministic distance-two suggestions, capped at nine results.
    pub fn suggest(&self, word: &str, max: usize) -> Vec<String> {
        let limit = max.min(MAX_SUGGESTIONS);
        if limit == 0 || word.is_empty() || word.len() > MAX_CHECKED_WORD_BYTES || !word.is_ascii()
        {
            return Vec::new();
        }

        let folded = word.to_ascii_lowercase();
        let minimum = folded.len().saturating_sub(DISTANCE_CUTOFF);
        let maximum = (folded.len() + DISTANCE_CUTOFF).min(MAX_CHECKED_WORD_BYTES);
        let mut best: Vec<(usize, &str)> = Vec::with_capacity(limit);

        for length in minimum..=maximum {
            for candidate in &self.buckets[length] {
                let Some(distance) =
                    bounded_osa_distance(folded.as_bytes(), candidate.as_bytes(), DISTANCE_CUTOFF)
                else {
                    continue;
                };
                let key = (distance, candidate.as_str());
                let position = best.binary_search(&key).unwrap_or_else(|index| index);
                if position < limit {
                    best.insert(position, key);
                    if best.len() > limit {
                        best.pop();
                    }
                }
            }
        }

        let casing = Casing::of(word);
        best.into_iter()
            .map(|(_, candidate)| casing.restore(candidate))
            .collect()
    }

    /// Normalize and add one personal-dictionary word.
    pub fn add_word(&mut self, word: &str) -> Result<AddWordOutcome, DictionaryEntryError> {
        let Some(normalized) = normalize_dictionary_entry(word)? else {
            return Ok(AddWordOutcome::Ignored);
        };
        let bucket = &mut self.buckets[normalized.len()];
        match bucket.binary_search(&normalized) {
            Ok(_) => Ok(AddWordOutcome::AlreadyPresent { normalized }),
            Err(index) => {
                bucket.insert(index, normalized.clone());
                self.word_count += 1;
                self.generation = self.generation.wrapping_add(1).max(1);
                Ok(AddWordOutcome::Inserted { normalized })
            }
        }
    }

    /// Return the dictionary generation, starting at one.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Return the number of unique normalized words.
    pub fn word_count(&self) -> usize {
        self.word_count
    }

    fn contains_exact_or_folded(&self, word: &str) -> bool {
        if word.len() > MAX_CHECKED_WORD_BYTES {
            return false;
        }
        let bucket = &self.buckets[word.len()];
        if bucket
            .binary_search_by(|candidate| candidate.as_str().cmp(word))
            .is_ok()
        {
            return true;
        }
        if !word.is_ascii() {
            return false;
        }
        let folded = word.to_ascii_lowercase();
        bucket
            .binary_search_by(|candidate| candidate.as_str().cmp(&folded))
            .is_ok()
    }
}

#[derive(Clone, Copy)]
enum Casing {
    Lower,
    InitialCapital,
    AllCaps,
}

impl Casing {
    fn of(word: &str) -> Self {
        let letters: Vec<u8> = word.bytes().filter(u8::is_ascii_alphabetic).collect();
        if !letters.is_empty() && letters.iter().all(u8::is_ascii_uppercase) {
            Self::AllCaps
        } else if word.as_bytes().first().is_some_and(u8::is_ascii_uppercase)
            && letters.iter().skip(1).all(u8::is_ascii_lowercase)
        {
            Self::InitialCapital
        } else {
            Self::Lower
        }
    }

    fn restore(self, candidate: &str) -> String {
        let mut bytes = candidate.as_bytes().to_vec();
        match self {
            Self::Lower => {}
            Self::InitialCapital => {
                if let Some(first) = bytes.first_mut() {
                    first.make_ascii_uppercase();
                }
            }
            Self::AllCaps => bytes.make_ascii_uppercase(),
        }
        String::from_utf8(bytes).expect("dictionary candidates are normalized ASCII")
    }
}

fn bounded_osa_distance(left: &[u8], right: &[u8], cutoff: usize) -> Option<usize> {
    if left.len().abs_diff(right.len()) > cutoff {
        return None;
    }
    let unreachable = cutoff + 1;
    let mut previous_previous = [unreachable; MAX_CHECKED_WORD_BYTES + 1];
    let mut previous = [unreachable; MAX_CHECKED_WORD_BYTES + 1];
    let mut current = [unreachable; MAX_CHECKED_WORD_BYTES + 1];
    for (column, value) in previous
        .iter_mut()
        .enumerate()
        .take(right.len().min(cutoff) + 1)
    {
        *value = column;
    }

    for row in 1..=left.len() {
        current[..=right.len()].fill(unreachable);
        if row <= cutoff {
            current[0] = row;
        }
        let start = row.saturating_sub(cutoff).max(1);
        let end = (row + cutoff).min(right.len());
        for column in start..=end {
            let substitution_cost = usize::from(left[row - 1] != right[column - 1]);
            let mut distance = (previous[column] + 1)
                .min(current[column - 1] + 1)
                .min(previous[column - 1] + substitution_cost);
            if row > 1
                && column > 1
                && left[row - 1] == right[column - 2]
                && left[row - 2] == right[column - 1]
            {
                distance = distance.min(previous_previous[column - 2] + 1);
            }
            current[column] = distance.min(unreachable);
        }
        std::mem::swap(&mut previous_previous, &mut previous);
        std::mem::swap(&mut previous, &mut current);
    }

    (previous[right.len()] <= cutoff).then_some(previous[right.len()])
}

#[cfg(test)]
mod tests {
    use super::bounded_osa_distance;

    fn full_matrix_osa(left: &[u8], right: &[u8]) -> usize {
        let mut matrix = vec![vec![0; right.len() + 1]; left.len() + 1];
        for (row, cells) in matrix.iter_mut().enumerate() {
            cells[0] = row;
        }
        for (column, cell) in matrix[0].iter_mut().enumerate() {
            *cell = column;
        }
        for row in 1..=left.len() {
            for column in 1..=right.len() {
                let cost = usize::from(left[row - 1] != right[column - 1]);
                matrix[row][column] = (matrix[row - 1][column] + 1)
                    .min(matrix[row][column - 1] + 1)
                    .min(matrix[row - 1][column - 1] + cost);
                if row > 1
                    && column > 1
                    && left[row - 1] == right[column - 2]
                    && left[row - 2] == right[column - 1]
                {
                    matrix[row][column] = matrix[row][column].min(matrix[row - 2][column - 2] + 1);
                }
            }
        }
        matrix[left.len()][right.len()]
    }

    fn words(alphabet: &[u8], max_len: usize) -> Vec<Vec<u8>> {
        fn extend(output: &mut Vec<Vec<u8>>, prefix: &mut Vec<u8>, alphabet: &[u8], max: usize) {
            output.push(prefix.clone());
            if prefix.len() == max {
                return;
            }
            for byte in alphabet {
                prefix.push(*byte);
                extend(output, prefix, alphabet, max);
                prefix.pop();
            }
        }
        let mut output = Vec::new();
        extend(&mut output, &mut Vec::new(), alphabet, max_len);
        output
    }

    #[test]
    fn banded_distance_matches_full_oracle_exhaustively() {
        let words = words(b"abc", 4);
        for left in &words {
            for right in &words {
                let expected = full_matrix_osa(left, right);
                assert_eq!(
                    bounded_osa_distance(left, right, 2),
                    (expected <= 2).then_some(expected),
                    "left={:?}, right={:?}",
                    String::from_utf8_lossy(left),
                    String::from_utf8_lossy(right)
                );
            }
        }
    }

    #[test]
    fn banded_distance_matches_full_oracle_for_seeded_random_words() {
        let mut state = 0x5eed_cafe_f00d_beefu64;
        for _ in 0..10_000 {
            let mut next_word = || {
                state = state
                    .wrapping_mul(6_364_136_223_846_793_005)
                    .wrapping_add(1);
                let len = (state as usize >> 8) % 12;
                let mut word = Vec::with_capacity(len);
                for _ in 0..len {
                    state = state
                        .wrapping_mul(6_364_136_223_846_793_005)
                        .wrapping_add(1);
                    word.push(b'a' + ((state >> 32) % 5) as u8);
                }
                word
            };
            let left = next_word();
            let right = next_word();
            let expected = full_matrix_osa(&left, &right);
            assert_eq!(
                bounded_osa_distance(&left, &right, 2),
                (expected <= 2).then_some(expected)
            );
        }
    }

    #[test]
    fn overlong_physical_line_keeps_partial_storage_bounded() {
        use super::{BuildProgress, SpellEngineBuilder, MAX_CHECKED_WORD_BYTES};

        let mut builder =
            SpellEngineBuilder::new(vec![format!("{}\nkept\n", "x".repeat(1024 * 1024))]);
        while builder.step(4 * 1024) == BuildProgress::Pending {
            assert!(builder.partial_line.len() <= MAX_CHECKED_WORD_BYTES);
            assert!(builder.partial_line.capacity() <= MAX_CHECKED_WORD_BYTES + 1);
        }
        let engine = builder.finish().expect("builder should complete");
        assert_eq!(engine.word_count(), 1);
        assert!(engine.check("kept"));
    }
}
