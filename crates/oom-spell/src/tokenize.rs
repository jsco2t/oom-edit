use std::ops::Range;

/// Shape facts computed for one candidate token.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TokenShape {
    contains_hyphen: bool,
    all_caps: bool,
    mixed_case_interior: bool,
    adjacent_to_word_char: bool,
    has_non_ascii_alpha: bool,
    entity_shaped: bool,
}

impl TokenShape {
    /// Whether the token contains an internal ASCII hyphen.
    pub fn contains_hyphen(self) -> bool {
        self.contains_hyphen
    }

    /// Whether the token has at least two letters and every cased letter is uppercase.
    pub fn all_caps(self) -> bool {
        self.all_caps
    }

    /// Whether an uppercase letter occurs after the first letter.
    pub fn mixed_case_interior(self) -> bool {
        self.mixed_case_interior
    }

    /// Whether the token touches an underscore or ASCII digit.
    pub fn adjacent_to_word_char(self) -> bool {
        self.adjacent_to_word_char
    }

    /// Whether the token contains a non-ASCII alphabetic character.
    pub fn has_non_ascii_alpha(self) -> bool {
        self.has_non_ascii_alpha
    }

    /// Whether the token is the name in a Markdown/HTML entity-shaped run.
    pub fn entity_shaped(self) -> bool {
        self.entity_shaped
    }
}

/// One borrowed token and its absolute UTF-8 byte range.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Token<'a> {
    /// Absolute half-open UTF-8 byte range in the source text.
    pub range: Range<usize>,
    /// Borrowed token text.
    pub text: &'a str,
    /// Text-generic shape facts used by candidate policy.
    pub shape: TokenShape,
}

/// Resumable state for [`tokenize_chunk`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TokenizerState {
    scan: Range<usize>,
    cursor: usize,
    token_start: Option<usize>,
    pending_connector: Option<Range<usize>>,
    shape: PartialShape,
    partial_scalar: Option<Range<usize>>,
    exclusion_index: usize,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct PartialShape {
    letter_count: usize,
    has_lowercase: bool,
    mixed_case_interior: bool,
    has_non_ascii_alpha: bool,
    contains_hyphen: bool,
}

impl TokenizerState {
    /// Create tokenizer state for one absolute source range.
    pub fn new(scan: Range<usize>) -> Self {
        Self {
            cursor: scan.start,
            scan,
            token_start: None,
            pending_connector: None,
            shape: PartialShape::default(),
            partial_scalar: None,
            exclusion_index: 0,
        }
    }

    /// Return the next source byte that has not been consumed.
    pub fn position(&self) -> usize {
        self.cursor
    }

    /// Return true once the complete configured scan range has been consumed.
    pub fn is_complete(&self) -> bool {
        self.cursor == self.scan.end && self.token_start.is_none() && self.partial_scalar.is_none()
    }
}

/// Tokens and byte progress produced by one budgeted tokenizer call.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TokenChunk<'a> {
    tokens: Vec<Token<'a>>,
    consumed: Range<usize>,
}

impl<'a> TokenChunk<'a> {
    /// Borrow the complete tokens emitted by this call.
    pub fn tokens(&self) -> &[Token<'a>] {
        &self.tokens
    }

    /// Consume the chunk and return its complete tokens.
    pub fn into_tokens(self) -> Vec<Token<'a>> {
        self.tokens
    }

    /// Return the absolute byte range consumed by this call.
    pub fn consumed(&self) -> Range<usize> {
        self.consumed.clone()
    }
}

/// Advance UTF-8 tokenization by at most `max_bytes` source bytes.
///
/// A token is a maximal alphabetic run with straight/curly apostrophes and
/// ASCII hyphens retained only when they occur between letters. Underscores
/// and digits separate tokens. `exclusions` must be sorted, non-overlapping,
/// and use character-boundary endpoints; excluded bytes are consumed but do
/// not produce tokens. Arbitrary byte budgets, including one byte in the
/// middle of a UTF-8 scalar, are supported.
pub fn tokenize_chunk<'a>(
    text: &'a str,
    exclusions: &[Range<usize>],
    state: &mut TokenizerState,
    max_bytes: usize,
) -> TokenChunk<'a> {
    validate_inputs(text, exclusions, state);
    let call_start = state.cursor;
    let limit = state.cursor.saturating_add(max_bytes).min(state.scan.end);
    let mut tokens = Vec::new();

    while state.cursor < limit {
        if let Some(scalar) = state.partial_scalar.clone() {
            let consumed_end = scalar.end.min(limit);
            state.cursor = consumed_end;
            if consumed_end == scalar.end {
                state.partial_scalar = None;
                let character = text[scalar.clone()]
                    .chars()
                    .next()
                    .expect("partial scalar must contain one character");
                process_character(text, scalar, character, state, &mut tokens);
            }
            continue;
        }

        while state.exclusion_index < exclusions.len()
            && exclusions[state.exclusion_index].end <= state.cursor
        {
            state.exclusion_index += 1;
        }
        if let Some(exclusion) = exclusions.get(state.exclusion_index) {
            if exclusion.start <= state.cursor && state.cursor < exclusion.end {
                finish_token(text, exclusion.start, state, &mut tokens);
                state.cursor = exclusion.end.min(limit);
                continue;
            }
        }

        let scalar_start = state.cursor;
        let character = text[scalar_start..state.scan.end]
            .chars()
            .next()
            .expect("cursor before scan end must name a character");
        let scalar = scalar_start..scalar_start + character.len_utf8();
        let consumed_end = scalar.end.min(limit);
        state.cursor = consumed_end;
        if consumed_end == scalar.end {
            process_character(text, scalar, character, state, &mut tokens);
        } else {
            state.partial_scalar = Some(scalar);
        }
    }

    if state.cursor == state.scan.end && state.partial_scalar.is_none() {
        let end = state.scan.end;
        finish_token(text, end, state, &mut tokens);
    }

    TokenChunk {
        tokens,
        consumed: call_start..state.cursor,
    }
}

fn validate_inputs(text: &str, exclusions: &[Range<usize>], state: &TokenizerState) {
    assert!(
        state.scan.start <= state.scan.end,
        "reversed tokenizer range"
    );
    assert!(state.scan.end <= text.len(), "tokenizer range exceeds text");
    assert!(
        text.is_char_boundary(state.scan.start) && text.is_char_boundary(state.scan.end),
        "tokenizer range endpoints must be UTF-8 boundaries"
    );
    let mut previous_end = 0;
    for exclusion in exclusions {
        assert!(exclusion.start <= exclusion.end, "reversed exclusion");
        assert!(exclusion.end <= text.len(), "exclusion exceeds text");
        assert!(
            text.is_char_boundary(exclusion.start) && text.is_char_boundary(exclusion.end),
            "exclusion endpoints must be UTF-8 boundaries"
        );
        assert!(
            exclusion.start >= previous_end,
            "exclusions must be sorted and non-overlapping"
        );
        previous_end = exclusion.end;
    }
}

fn process_character<'a>(
    text: &'a str,
    scalar: Range<usize>,
    character: char,
    state: &mut TokenizerState,
    tokens: &mut Vec<Token<'a>>,
) {
    if character.is_alphabetic() {
        if state.token_start.is_none() {
            state.token_start = Some(scalar.start);
        }
        if let Some(connector) = state.pending_connector.as_ref() {
            state.shape.contains_hyphen |= &text[connector.clone()] == "-";
        }
        state.pending_connector = None;
        state.shape.mixed_case_interior |= state.shape.letter_count > 0 && character.is_uppercase();
        state.shape.letter_count += 1;
        state.shape.has_lowercase |= character.is_lowercase();
        state.shape.has_non_ascii_alpha |= !character.is_ascii();
        return;
    }

    if matches!(character, '\'' | '’' | '-') && state.token_start.is_some() {
        if state.pending_connector.is_none() {
            state.pending_connector = Some(scalar);
        } else {
            let end = state
                .pending_connector
                .as_ref()
                .expect("pending connector exists")
                .start;
            finish_token(text, end, state, tokens);
        }
        return;
    }

    let end = state
        .pending_connector
        .as_ref()
        .map_or(scalar.start, |connector| connector.start);
    finish_token(text, end, state, tokens);
}

fn finish_token<'a>(
    text: &'a str,
    fallback_end: usize,
    state: &mut TokenizerState,
    tokens: &mut Vec<Token<'a>>,
) {
    let Some(start) = state.token_start.take() else {
        state.pending_connector = None;
        return;
    };
    let end = state
        .pending_connector
        .take()
        .map_or(fallback_end, |connector| connector.start);
    if start >= end {
        state.shape = PartialShape::default();
        return;
    }
    let token_text = &text[start..end];
    let partial = std::mem::take(&mut state.shape);
    tokens.push(Token {
        range: start..end,
        text: token_text,
        shape: TokenShape {
            contains_hyphen: partial.contains_hyphen,
            all_caps: partial.letter_count >= 2 && !partial.has_lowercase,
            mixed_case_interior: partial.mixed_case_interior,
            adjacent_to_word_char: preceding_char(text, start).is_some_and(is_identifier_joiner)
                || following_char(text, end).is_some_and(is_identifier_joiner),
            has_non_ascii_alpha: partial.has_non_ascii_alpha,
            entity_shaped: preceding_char(text, start) == Some('&')
                && following_char(text, end) == Some(';'),
        },
    });
}

fn preceding_char(text: &str, offset: usize) -> Option<char> {
    text.get(..offset)?.chars().next_back()
}

fn following_char(text: &str, offset: usize) -> Option<char> {
    text.get(offset..)?.chars().next()
}

fn is_identifier_joiner(character: char) -> bool {
    character == '_' || character.is_ascii_digit()
}
