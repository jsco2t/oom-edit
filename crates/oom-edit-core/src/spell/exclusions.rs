use std::ops::Range;

/// Resumable Markdown non-prose exclusion discovery.
pub(super) struct ProseExclusionScanner {
    scan: Range<usize>,
    cursor: usize,
    block_ranges: Vec<Range<usize>>,
    reference_labels: Vec<String>,
    block_index: usize,
    block_recorded: bool,
    ranges: Vec<Range<usize>>,
    mode: InlineMode,
    email_run: Option<EmailRun>,
    escape_next: bool,
    bracket_depth: usize,
    link_label_closed: bool,
}

#[derive(Clone, Debug)]
enum InlineMode {
    Normal,
    CodeOpening {
        start: usize,
        ticks: usize,
    },
    Code {
        start: usize,
        delimiter: usize,
        closing_ticks: usize,
    },
    Angle {
        start: usize,
        terminator: AngleTerminator,
        raw_html: bool,
        tag: HtmlTagState,
        uri_valid: bool,
        email: EmailRun,
        has_whitespace: bool,
    },
    LinkDestination {
        content_start: usize,
        depth: usize,
        escaped: bool,
        validation: LinkValidation,
    },
    ReferenceLabel {
        content_start: usize,
        escaped: bool,
    },
    BareUrl {
        start: usize,
        trimmed_end: usize,
    },
}

#[derive(Clone, Debug)]
struct EmailRun {
    start: usize,
    at_count: usize,
    local_len: usize,
    domain_len: usize,
    suffix_len: usize,
    has_domain_dot: bool,
    domain_ends_alphanumeric: bool,
    angle_valid_end: Option<usize>,
    bare_valid_end: Option<usize>,
    invalid: bool,
}

#[derive(Clone, Debug, Default)]
struct LinkValidation {
    seen_destination: bool,
    after_destination_whitespace: bool,
    pointy_destination: bool,
    pointy_destination_open: bool,
    title_quote: Option<u8>,
    title_closed: bool,
    valid: bool,
}

impl EmailRun {
    fn consume(&mut self, byte: u8, end: usize) {
        if byte == b'@' {
            if self.at_count != 0 || self.local_len == 0 || self.invalid {
                self.invalid = true;
            }
            self.at_count += 1;
            self.domain_len = 0;
            self.suffix_len = 0;
            self.has_domain_dot = false;
            self.domain_ends_alphanumeric = false;
            self.angle_valid_end = None;
            self.bare_valid_end = None;
        } else if self.at_count == 0 {
            if is_email_local_byte(byte) {
                self.local_len += 1;
            } else {
                self.invalid = true;
            }
        } else if byte == b'.' {
            if self.domain_ends_alphanumeric {
                self.has_domain_dot = true;
                self.suffix_len = 0;
                self.domain_ends_alphanumeric = false;
            } else {
                self.invalid = true;
            }
        } else if byte.is_ascii_alphanumeric() {
            self.domain_len += 1;
            self.domain_ends_alphanumeric = true;
            if self.has_domain_dot {
                self.suffix_len += 1;
            }
        } else if byte == b'-' && self.domain_ends_alphanumeric {
            self.domain_len += 1;
            self.domain_ends_alphanumeric = false;
            if self.has_domain_dot {
                self.suffix_len += 1;
            }
        } else {
            self.invalid = true;
        }
        if self.is_valid() {
            self.angle_valid_end = Some(end);
            if self.has_domain_dot && self.suffix_len >= 2 {
                self.bare_valid_end = Some(end);
            }
        }
    }

    fn is_valid(&self) -> bool {
        self.at_count == 1
            && !self.invalid
            && self.local_len > 0
            && self.domain_len > 0
            && self.domain_ends_alphanumeric
    }
}

impl LinkValidation {
    fn consume(&mut self, byte: u8) {
        self.seen_destination = true;
        self.valid &= byte != b'<';
    }

    fn is_valid(&self) -> bool {
        self.valid
            && self.seen_destination
            && !self.pointy_destination_open
            && self.title_quote.is_none()
    }
}

#[derive(Clone, Copy, Debug)]
enum AngleTerminator {
    Tag,
    Comment,
    Processing,
    Cdata,
}

#[derive(Clone, Copy, Debug)]
enum HtmlTagState {
    None,
    OpenName,
    BeforeAttribute,
    AttributeName,
    AfterAttributeName,
    BeforeAttributeValue,
    UnquotedAttributeValue,
    QuotedAttributeValue(u8),
    SelfClosing,
    ClosingSlash,
    ClosingName,
    ClosingWhitespace,
    Complete,
    Invalid,
}

impl HtmlTagState {
    fn consume(self, byte: u8) -> Self {
        match self {
            Self::OpenName => match byte {
                byte if is_tag_name_byte(byte) => Self::OpenName,
                byte if byte.is_ascii_whitespace() => Self::BeforeAttribute,
                b'/' => Self::SelfClosing,
                b'>' => Self::Complete,
                _ => Self::Invalid,
            },
            Self::BeforeAttribute => match byte {
                byte if byte.is_ascii_whitespace() => Self::BeforeAttribute,
                byte if is_attribute_name_start(byte) => Self::AttributeName,
                b'/' => Self::SelfClosing,
                b'>' => Self::Complete,
                _ => Self::Invalid,
            },
            Self::AttributeName => match byte {
                byte if is_attribute_name_byte(byte) => Self::AttributeName,
                byte if byte.is_ascii_whitespace() => Self::AfterAttributeName,
                b'=' => Self::BeforeAttributeValue,
                b'/' => Self::SelfClosing,
                b'>' => Self::Complete,
                _ => Self::Invalid,
            },
            Self::AfterAttributeName => match byte {
                byte if byte.is_ascii_whitespace() => Self::AfterAttributeName,
                byte if is_attribute_name_start(byte) => Self::AttributeName,
                b'=' => Self::BeforeAttributeValue,
                b'/' => Self::SelfClosing,
                b'>' => Self::Complete,
                _ => Self::Invalid,
            },
            Self::BeforeAttributeValue => match byte {
                byte if byte.is_ascii_whitespace() => Self::BeforeAttributeValue,
                b'\'' | b'"' => Self::QuotedAttributeValue(byte),
                byte if is_unquoted_attribute_value_byte(byte) => Self::UnquotedAttributeValue,
                _ => Self::Invalid,
            },
            Self::UnquotedAttributeValue => match byte {
                byte if byte.is_ascii_whitespace() => Self::BeforeAttribute,
                b'>' => Self::Complete,
                byte if is_unquoted_attribute_value_byte(byte) => Self::UnquotedAttributeValue,
                _ => Self::Invalid,
            },
            Self::QuotedAttributeValue(quote) => {
                if byte == quote {
                    Self::BeforeAttribute
                } else {
                    Self::QuotedAttributeValue(quote)
                }
            }
            Self::SelfClosing => {
                if byte == b'>' {
                    Self::Complete
                } else {
                    Self::Invalid
                }
            }
            Self::ClosingSlash => {
                if byte == b'/' {
                    Self::ClosingName
                } else {
                    Self::Invalid
                }
            }
            Self::ClosingName => match byte {
                byte if is_tag_name_byte(byte) => Self::ClosingName,
                byte if byte.is_ascii_whitespace() => Self::ClosingWhitespace,
                b'>' => Self::Complete,
                _ => Self::Invalid,
            },
            Self::ClosingWhitespace => match byte {
                byte if byte.is_ascii_whitespace() => Self::ClosingWhitespace,
                b'>' => Self::Complete,
                _ => Self::Invalid,
            },
            Self::None | Self::Complete | Self::Invalid => self,
        }
    }

    fn is_possible(self) -> bool {
        !matches!(self, Self::None | Self::Invalid)
    }

    fn is_valid(self) -> bool {
        matches!(self, Self::Complete)
    }
}

impl ProseExclusionScanner {
    pub(super) fn new(
        text: &str,
        scan: Range<usize>,
        block_ranges: Vec<Range<usize>>,
        reference_labels: Vec<String>,
    ) -> Self {
        assert!(scan.start <= scan.end && scan.end <= text.len());
        let mut block_ranges: Vec<_> = block_ranges
            .into_iter()
            .filter_map(|range| {
                let start = range.start.max(scan.start);
                let end = range.end.min(scan.end);
                (start < end).then_some(start..end)
            })
            .collect();
        block_ranges = normalize_ranges(block_ranges);
        Self {
            cursor: scan.start,
            scan,
            block_ranges,
            reference_labels,
            block_index: 0,
            block_recorded: false,
            ranges: Vec::new(),
            mode: InlineMode::Normal,
            email_run: None,
            escape_next: false,
            bracket_depth: 0,
            link_label_closed: false,
        }
    }

    /// Advance by at most `max_bytes`; true means the configured range is complete.
    pub(super) fn step(&mut self, text: &str, max_bytes: usize) -> bool {
        assert!(self.scan.end <= text.len(), "exclusion scan exceeds text");
        if self.is_complete() || max_bytes == 0 {
            return self.is_complete();
        }
        let limit = self.cursor.saturating_add(max_bytes).min(self.scan.end);
        while self.cursor < limit {
            self.advance_block_index();
            if self.inside_current_block() {
                self.enter_or_continue_block(limit);
                continue;
            }
            self.advance_inline(text, limit);
        }
        if self.cursor == self.scan.end {
            self.finish_at_end();
        }
        self.is_complete()
    }

    pub(super) fn add_block_ranges(&mut self, ranges: Vec<Range<usize>>) {
        for range in ranges {
            let start = range.start.max(self.cursor).max(self.scan.start);
            let end = range.end.min(self.scan.end);
            if start >= end {
                continue;
            }
            if let Some(last_index) = self.block_ranges.len().checked_sub(1) {
                if start <= self.block_ranges[last_index].end {
                    self.block_ranges[last_index].end = self.block_ranges[last_index].end.max(end);
                    if self.block_recorded && self.block_index == last_index {
                        let recorded = self
                            .ranges
                            .last_mut()
                            .expect("recorded block must have an exclusion range");
                        recorded.end = recorded.end.max(end);
                    }
                    continue;
                }
            }
            self.block_ranges.push(start..end);
        }
    }

    pub(super) fn position(&self) -> usize {
        self.cursor
    }

    pub(super) fn is_complete(&self) -> bool {
        self.cursor == self.scan.end
    }

    pub(super) fn into_ranges(self) -> Vec<Range<usize>> {
        assert!(
            self.is_complete(),
            "cannot finalize an incomplete prose exclusion scan"
        );
        normalize_ranges(self.ranges)
    }

    fn advance_block_index(&mut self) {
        while self.block_index < self.block_ranges.len()
            && self.block_ranges[self.block_index].end <= self.cursor
        {
            self.block_index += 1;
            self.block_recorded = false;
        }
    }

    fn inside_current_block(&self) -> bool {
        self.block_ranges
            .get(self.block_index)
            .is_some_and(|range| range.start <= self.cursor && self.cursor < range.end)
    }

    fn enter_or_continue_block(&mut self, limit: usize) {
        let range = self.block_ranges[self.block_index].clone();
        if !self.block_recorded {
            self.finish_email_run(range.start);
            self.mode = InlineMode::Normal;
            self.escape_next = false;
            self.bracket_depth = 0;
            self.link_label_closed = false;
            self.ranges.push(range.clone());
            self.block_recorded = true;
        }
        self.cursor = range.end.min(limit);
    }

    fn advance_inline(&mut self, text: &str, limit: usize) {
        match self.mode.clone() {
            InlineMode::Normal => self.advance_normal(text),
            InlineMode::CodeOpening { start, ticks } => {
                if self.byte(text) == b'`' {
                    self.cursor += 1;
                    self.mode = InlineMode::CodeOpening {
                        start,
                        ticks: ticks + 1,
                    };
                } else {
                    self.mode = InlineMode::Code {
                        start,
                        delimiter: ticks,
                        closing_ticks: 0,
                    };
                }
            }
            InlineMode::Code {
                start,
                delimiter,
                closing_ticks,
            } => {
                if self.byte(text) == b'`' {
                    self.cursor += 1;
                    self.mode = InlineMode::Code {
                        start,
                        delimiter,
                        closing_ticks: closing_ticks + 1,
                    };
                } else if closing_ticks == delimiter {
                    self.ranges.push(start..self.cursor);
                    self.mode = InlineMode::Normal;
                } else {
                    self.cursor += 1;
                    self.mode = InlineMode::Code {
                        start,
                        delimiter,
                        closing_ticks: 0,
                    };
                }
            }
            InlineMode::Angle {
                start,
                terminator,
                raw_html,
                mut tag,
                mut uri_valid,
                mut email,
                mut has_whitespace,
            } => {
                let byte = self.byte(text);
                self.cursor += 1;
                tag = tag.consume(byte);
                let is_terminator = match terminator {
                    AngleTerminator::Tag => byte == b'>',
                    _ => self.angle_terminator_matches(text, terminator),
                };
                if !is_terminator {
                    email.consume(byte, self.cursor);
                    has_whitespace |= byte.is_ascii_whitespace();
                    uri_valid &= is_uri_autolink_byte(byte);
                }
                let tag_terminator = !matches!(
                    tag,
                    HtmlTagState::QuotedAttributeValue(_) | HtmlTagState::UnquotedAttributeValue
                );
                if is_terminator && tag_terminator {
                    let raw_html = raw_html || tag.is_valid();
                    let autolink = !has_whitespace
                        && (uri_valid || email.angle_valid_end == Some(self.cursor - 1));
                    if raw_html || autolink {
                        self.ranges.push(start..self.cursor);
                    }
                    self.mode = InlineMode::Normal;
                    return;
                } else if !raw_html && !tag.is_possible() && byte == b'\n' {
                    self.mode = InlineMode::Normal;
                    return;
                }
                self.mode = InlineMode::Angle {
                    start,
                    terminator,
                    raw_html,
                    tag,
                    uri_valid,
                    email,
                    has_whitespace,
                };
            }
            InlineMode::LinkDestination {
                content_start,
                mut depth,
                mut escaped,
                mut validation,
            } => {
                let byte = self.byte(text);
                if escaped {
                    escaped = false;
                    if validation.pointy_destination_open {
                        validation.valid &= !matches!(byte, b'\n' | b'\r');
                    } else {
                        validation.consume(byte);
                    }
                    self.cursor += 1;
                } else if byte == b'\\' {
                    escaped = true;
                    self.cursor += 1;
                } else if validation.pointy_destination_open {
                    if byte == b'>' {
                        validation.pointy_destination_open = false;
                        validation.after_destination_whitespace = true;
                    } else {
                        validation.valid &= !matches!(byte, b'<' | b'\n' | b'\r');
                    }
                    self.cursor += 1;
                } else if let Some(quote) = validation.title_quote {
                    if byte == quote {
                        validation.title_quote = None;
                        validation.title_closed = true;
                    }
                    self.cursor += 1;
                } else if validation.title_closed {
                    if byte == b')' {
                        if validation.is_valid() && content_start < self.cursor {
                            self.ranges.push(content_start..self.cursor);
                        }
                        self.cursor += 1;
                        self.mode = InlineMode::Normal;
                        return;
                    }
                    validation.valid &= byte.is_ascii_whitespace();
                    self.cursor += 1;
                } else if validation.pointy_destination
                    && !validation.after_destination_whitespace
                    && byte != b')'
                {
                    validation.after_destination_whitespace = byte.is_ascii_whitespace();
                    validation.valid &= byte.is_ascii_whitespace();
                    self.cursor += 1;
                } else if !validation.seen_destination && byte == b'<' {
                    validation.seen_destination = true;
                    validation.pointy_destination = true;
                    validation.pointy_destination_open = true;
                    self.cursor += 1;
                } else if byte.is_ascii_whitespace() {
                    validation.after_destination_whitespace |= validation.seen_destination;
                    self.cursor += 1;
                } else if validation.after_destination_whitespace && byte == b')' {
                    if validation.is_valid() && content_start < self.cursor {
                        self.ranges.push(content_start..self.cursor);
                    }
                    self.cursor += 1;
                    self.mode = InlineMode::Normal;
                    return;
                } else if validation.after_destination_whitespace
                    && matches!(byte, b'\'' | b'"' | b'(')
                {
                    validation.title_quote = Some(if byte == b'(' { b')' } else { byte });
                    self.cursor += 1;
                } else if validation.after_destination_whitespace {
                    validation.valid = false;
                    self.cursor += 1;
                } else if byte == b'(' {
                    validation.seen_destination = true;
                    depth += 1;
                    self.cursor += 1;
                } else if byte == b')' {
                    depth -= 1;
                    if depth == 0 {
                        if validation.is_valid() && content_start < self.cursor {
                            self.ranges.push(content_start..self.cursor);
                        }
                        self.cursor += 1;
                        self.mode = InlineMode::Normal;
                        return;
                    }
                    self.cursor += 1;
                } else {
                    validation.consume(byte);
                    self.cursor += 1;
                }
                self.mode = InlineMode::LinkDestination {
                    content_start,
                    depth,
                    escaped,
                    validation,
                };
            }
            InlineMode::ReferenceLabel {
                content_start,
                mut escaped,
            } => {
                let byte = self.byte(text);
                if escaped {
                    escaped = false;
                    self.cursor += 1;
                } else if byte == b'\\' {
                    escaped = true;
                    self.cursor += 1;
                } else if byte == b']' {
                    if content_start < self.cursor
                        && self.cursor - content_start <= 999
                        && crate::spell::normalize_reference_label(
                            &text[content_start..self.cursor],
                        )
                        .is_some_and(|label| self.reference_labels.binary_search(&label).is_ok())
                    {
                        self.ranges.push(content_start..self.cursor);
                    }
                    self.cursor += 1;
                    self.mode = InlineMode::Normal;
                    return;
                } else {
                    self.cursor += 1;
                }
                self.mode = InlineMode::ReferenceLabel {
                    content_start,
                    escaped,
                };
            }
            InlineMode::BareUrl {
                start,
                mut trimmed_end,
            } => {
                if is_bare_url_terminator(self.byte(text)) {
                    self.finish_bare_url(start, trimmed_end);
                    self.mode = InlineMode::Normal;
                } else {
                    if !is_bare_url_trailing_punctuation(self.byte(text)) {
                        trimmed_end = self.cursor + 1;
                    }
                    self.cursor += 1;
                    self.mode = InlineMode::BareUrl { start, trimmed_end };
                }
            }
        }
        debug_assert!(self.cursor <= limit || self.cursor == self.scan.end);
    }

    fn advance_normal(&mut self, text: &str) {
        let byte = self.byte(text);
        if self.escape_next {
            self.escape_next = false;
            self.link_label_closed = false;
            self.extend_email_run(byte);
            self.cursor += 1;
            return;
        }
        if byte == b'\\' {
            self.finish_email_run(self.cursor);
            self.escape_next = true;
            self.link_label_closed = false;
            self.cursor += 1;
            return;
        }
        if self.starts_bare_url(text) {
            self.finish_email_run(self.cursor);
            self.link_label_closed = false;
            self.mode = InlineMode::BareUrl {
                start: self.cursor,
                trimmed_end: self.cursor,
            };
            return;
        }
        match byte {
            b'`' => {
                self.finish_email_run(self.cursor);
                self.link_label_closed = false;
                self.mode = InlineMode::CodeOpening {
                    start: self.cursor,
                    ticks: 0,
                };
            }
            b'<' => {
                self.finish_email_run(self.cursor);
                self.link_label_closed = false;
                let rest = &text.as_bytes()[self.cursor..self.scan.end];
                let terminator = if rest.starts_with(b"<!--") {
                    AngleTerminator::Comment
                } else if rest.starts_with(b"<?") {
                    AngleTerminator::Processing
                } else if rest.starts_with(b"<![CDATA[") {
                    AngleTerminator::Cdata
                } else {
                    AngleTerminator::Tag
                };
                let raw_html = rest.starts_with(b"<!--")
                    || rest.starts_with(b"<?")
                    || rest.starts_with(b"<![CDATA[")
                    || (rest.starts_with(b"<!")
                        && rest.get(2).is_some_and(u8::is_ascii_alphabetic));
                let tag = if rest.get(1).is_some_and(u8::is_ascii_alphabetic) {
                    HtmlTagState::OpenName
                } else if rest.starts_with(b"</")
                    && rest.get(2).is_some_and(u8::is_ascii_alphabetic)
                {
                    HtmlTagState::ClosingSlash
                } else {
                    HtmlTagState::None
                };
                self.mode = InlineMode::Angle {
                    start: self.cursor,
                    terminator,
                    raw_html,
                    tag,
                    uri_valid: starts_with_uri_scheme(rest),
                    email: EmailRun {
                        start: self.cursor + 1,
                        at_count: 0,
                        local_len: 0,
                        domain_len: 0,
                        suffix_len: 0,
                        has_domain_dot: false,
                        domain_ends_alphanumeric: false,
                        angle_valid_end: None,
                        bare_valid_end: None,
                        invalid: false,
                    },
                    has_whitespace: false,
                };
                self.cursor += 1;
            }
            b'(' if self.link_label_closed => {
                self.finish_email_run(self.cursor);
                self.link_label_closed = false;
                self.cursor += 1;
                self.mode = InlineMode::LinkDestination {
                    content_start: self.cursor,
                    depth: 1,
                    escaped: false,
                    validation: LinkValidation {
                        valid: true,
                        ..LinkValidation::default()
                    },
                };
            }
            b'[' if self.link_label_closed => {
                self.finish_email_run(self.cursor);
                self.link_label_closed = false;
                self.cursor += 1;
                self.mode = InlineMode::ReferenceLabel {
                    content_start: self.cursor,
                    escaped: false,
                };
            }
            b'[' => {
                self.finish_email_run(self.cursor);
                self.bracket_depth += 1;
                self.link_label_closed = false;
                self.cursor += 1;
            }
            b']' => {
                self.finish_email_run(self.cursor);
                if self.bracket_depth > 0 {
                    self.bracket_depth -= 1;
                    self.link_label_closed = true;
                } else {
                    self.link_label_closed = false;
                }
                self.cursor += 1;
            }
            _ => {
                self.link_label_closed = false;
                self.extend_email_run(byte);
                self.cursor += 1;
            }
        }
    }

    fn extend_email_run(&mut self, byte: u8) {
        if is_email_run_byte(byte) {
            let run = self.email_run.get_or_insert(EmailRun {
                start: self.cursor,
                at_count: 0,
                local_len: 0,
                domain_len: 0,
                suffix_len: 0,
                has_domain_dot: false,
                domain_ends_alphanumeric: false,
                angle_valid_end: None,
                bare_valid_end: None,
                invalid: false,
            });
            run.consume(byte, self.cursor + 1);
        } else {
            self.finish_email_run(self.cursor);
        }
    }

    fn finish_email_run(&mut self, end: usize) {
        let Some(run) = self.email_run.take() else {
            return;
        };
        if let Some(valid_end) = run.bare_valid_end {
            self.ranges.push(run.start..valid_end.min(end));
        }
    }

    fn finish_bare_url(&mut self, start: usize, end: usize) {
        if start < end {
            self.ranges.push(start..end);
        }
    }

    fn finish_at_end(&mut self) {
        match self.mode.clone() {
            InlineMode::Normal => self.finish_email_run(self.scan.end),
            InlineMode::Code {
                start,
                delimiter,
                closing_ticks,
            } if delimiter == closing_ticks => self.ranges.push(start..self.scan.end),
            InlineMode::BareUrl { start, trimmed_end } => self.finish_bare_url(start, trimmed_end),
            _ => {}
        }
        self.mode = InlineMode::Normal;
        self.email_run = None;
    }

    fn starts_bare_url(&self, text: &str) -> bool {
        let rest = &text.as_bytes()[self.cursor..self.scan.end];
        let prefix = rest.starts_with(b"https://")
            || rest.starts_with(b"http://")
            || rest.starts_with(b"www.")
            || rest.starts_with(b"mailto:");
        prefix
            && self
                .previous_byte(text)
                .is_none_or(|byte| !byte.is_ascii_alphanumeric() && byte != b'_')
    }

    fn byte(&self, text: &str) -> u8 {
        text.as_bytes()[self.cursor]
    }

    fn angle_terminator_matches(&self, text: &str, terminator: AngleTerminator) -> bool {
        let consumed = &text.as_bytes()[self.scan.start..self.cursor];
        match terminator {
            AngleTerminator::Tag => consumed.ends_with(b">"),
            AngleTerminator::Comment => consumed.ends_with(b"-->"),
            AngleTerminator::Processing => consumed.ends_with(b"?>"),
            AngleTerminator::Cdata => consumed.ends_with(b"]]>"),
        }
    }

    fn previous_byte(&self, text: &str) -> Option<u8> {
        self.cursor
            .checked_sub(1)
            .filter(|offset| *offset >= self.scan.start)
            .map(|offset| text.as_bytes()[offset])
    }
}

fn starts_with_uri_scheme(source: &[u8]) -> bool {
    let Some(body) = source.strip_prefix(b"<") else {
        return false;
    };
    let Some(colon) = body.iter().take(33).position(|byte| *byte == b':') else {
        return false;
    };
    (2..=32).contains(&colon)
        && body[0].is_ascii_alphabetic()
        && body[..colon]
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'-' | b'.'))
}

fn is_uri_autolink_byte(byte: u8) -> bool {
    !byte.is_ascii_control() && !byte.is_ascii_whitespace() && !matches!(byte, b'<' | b'>')
}

fn is_email_local_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric()
        || matches!(
            byte,
            b'.' | b'!'
                | b'#'
                | b'$'
                | b'%'
                | b'&'
                | b'\''
                | b'*'
                | b'+'
                | b'/'
                | b'='
                | b'?'
                | b'^'
                | b'_'
                | b'`'
                | b'{'
                | b'|'
                | b'}'
                | b'~'
                | b'-'
        )
}

fn is_tag_name_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'-'
}

fn is_attribute_name_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || matches!(byte, b'_' | b':')
}

fn is_attribute_name_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.' | b':' | b'-')
}

fn is_unquoted_attribute_value_byte(byte: u8) -> bool {
    !byte.is_ascii_whitespace() && !matches!(byte, b'"' | b'\'' | b'=' | b'<' | b'>' | b'`')
}

fn is_bare_url_terminator(byte: u8) -> bool {
    byte.is_ascii_whitespace() || matches!(byte, b'<' | b'>' | b'"' | b'\'' | b'`')
}

fn is_email_run_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'+' | b'-' | b'%' | b'@')
}

fn is_bare_url_trailing_punctuation(byte: u8) -> bool {
    matches!(
        byte,
        b'.' | b',' | b';' | b':' | b'!' | b'?' | b')' | b']' | b'}'
    )
}

fn normalize_ranges(mut ranges: Vec<Range<usize>>) -> Vec<Range<usize>> {
    ranges.sort_by_key(|range| (range.start, range.end));
    let mut normalized: Vec<Range<usize>> = Vec::with_capacity(ranges.len());
    for range in ranges {
        if let Some(last) = normalized.last_mut() {
            if range.start <= last.end {
                last.end = last.end.max(range.end);
                continue;
            }
        }
        normalized.push(range);
    }
    normalized
}

#[cfg(test)]
mod tests {
    use std::ops::Range;

    use super::ProseExclusionScanner;
    use crate::syntax::Highlighter;

    fn range_of(text: &str, needle: &str) -> Range<usize> {
        let start = text
            .find(needle)
            .unwrap_or_else(|| panic!("missing {needle:?}"));
        start..start + needle.len()
    }

    fn ranges_with_budget(text: &str, scan: Range<usize>, budget: usize) -> Vec<Range<usize>> {
        let highlighter = Highlighter::new(text);
        let blocks = highlighter.spell_block_exclusion_ranges(scan.clone());
        let reference_labels = highlighter.spell_reference_labels();
        ranges_with_blocks_budget(text, scan, &blocks, &reference_labels, budget)
    }

    fn ranges_with_blocks_budget(
        text: &str,
        scan: Range<usize>,
        blocks: &[Range<usize>],
        reference_labels: &[String],
        budget: usize,
    ) -> Vec<Range<usize>> {
        let mut scanner =
            ProseExclusionScanner::new(text, scan, blocks.to_vec(), reference_labels.to_vec());
        while !scanner.is_complete() {
            let before = scanner.position();
            scanner.step(text, budget);
            let after = scanner.position();
            assert!(after > before, "scanner must make progress");
            assert!(after - before <= budget, "scanner exceeded its budget");
        }
        scanner.into_ranges()
    }

    fn ranges_with_lazy_blocks_budget(
        text: &str,
        scan: Range<usize>,
        budget: usize,
    ) -> Vec<Range<usize>> {
        let highlighter = Highlighter::new(text);
        let reference_labels = highlighter.spell_reference_labels();
        let mut scanner =
            ProseExclusionScanner::new(text, scan.clone(), Vec::new(), reference_labels);
        let mut ticks = 0;
        while !scanner.is_complete() {
            let before = scanner.position();
            let end = before.saturating_add(budget).min(scan.end);
            scanner.add_block_ranges(highlighter.spell_block_exclusion_ranges(before..end));
            scanner.step(text, budget);
            let after = scanner.position();
            assert!(after > before, "scanner must make progress");
            assert!(after - before <= budget, "scanner exceeded its budget");
            ticks += 1;
            assert!(ticks <= scan.len(), "scanner exceeded its progress bound");
        }
        scanner.into_ranges()
    }

    fn intersects(left: &Range<usize>, right: &Range<usize>) -> bool {
        left.start < right.end && right.start < left.end
    }

    #[test]
    fn construct_matrix_has_byte_exact_checked_and_excluded_evidence() {
        let text = concat!(
            "---\ntitle: hidden_front\n---\n",
            "# heading_checked\n\n",
            "paragraph_checked *emphasis_checked* **strong_checked**.\n\n",
            "- list_checked\n> quote_checked\n\n",
            "| table_checked | second_checked |\n| --- | --- |\n| cell_checked | more_checked |\n\n",
            "[visible_checked](https://inline.example/path \"hidden title\") and ",
            "[reference_checked][reference_target] and ![alt_checked](image.png).\n\n",
            "`hidden_inline_code` <https://auto.example/path> ",
            "<person@example.com> https://bare.example/path?q=hidden.\n\n",
            "before_checked <span data-name=\"hidden\">inside_checked</span> after_checked\n\n",
            "[reference_target]: https://definition.example/path \"hidden definition\"\n\n",
            "```rust\nhidden_fence\n```\n\n",
            "    hidden_indented\n\n",
            "<div>\nhidden_html_block\n</div>\n",
        );
        let ranges = ranges_with_budget(text, 0..text.len(), 7);

        let expected = [
            "---\ntitle: hidden_front\n---\n",
            "https://inline.example/path \"hidden title\"",
            "reference_target",
            "image.png",
            "`hidden_inline_code`",
            "<https://auto.example/path>",
            "<person@example.com>",
            "https://bare.example/path?q=hidden",
            "<span data-name=\"hidden\">",
            "</span>",
            "[reference_target]: https://definition.example/path \"hidden definition\"\n",
            "```rust\nhidden_fence\n```\n",
            "    hidden_indented\n\n<div>\nhidden_html_block\n</div>\n",
        ]
        .map(|excluded| range_of(text, excluded));
        assert_eq!(ranges, expected, "construct exclusions must be byte-exact");

        for checked in [
            "heading_checked",
            "paragraph_checked",
            "emphasis_checked",
            "strong_checked",
            "list_checked",
            "quote_checked",
            "table_checked",
            "cell_checked",
            "visible_checked",
            "reference_checked",
            "alt_checked",
            "inside_checked",
            "after_checked",
        ] {
            let target = range_of(text, checked);
            assert!(
                ranges.iter().all(|range| !intersects(range, &target)),
                "visible prose {checked:?} overlapped an exclusion in {ranges:?}"
            );
        }

        let toml = "+++\ntitle = \"hidden_toml\"\n+++\nvisible\n";
        assert_eq!(
            ranges_with_budget(toml, 0..toml.len(), 3),
            [range_of(toml, "+++\ntitle = \"hidden_toml\"\n+++\n")]
        );
    }

    #[test]
    fn chunk_sizes_from_one_byte_to_four_kib_are_equivalent() {
        let text = concat!(
            "Visible `same hidden` and [same visible](https://example.test/a).\n",
            "Escaped \\`same visible\\` plus [nested **visible**][target].\n",
            "<span title=\"same hidden\">same visible</span> ",
            "user@example.test and www.example.test/path.\n\n",
            "[target]: <https://definition.test>\n",
        );
        let highlighter = Highlighter::new(text);
        let scan = 0..text.len();
        let blocks = highlighter.spell_block_exclusion_ranges(scan.clone());
        let reference_labels = highlighter.spell_reference_labels();
        let expected = [
            range_of(text, "`same hidden`"),
            range_of(text, "https://example.test/a"),
            range_of(text, "target"),
            range_of(text, "<span title=\"same hidden\">"),
            range_of(text, "</span>"),
            range_of(text, "user@example.test"),
            range_of(text, "www.example.test/path"),
            range_of(text, "[target]: <https://definition.test>\n"),
        ];
        for budget in 1..=4096 {
            assert_eq!(
                ranges_with_blocks_budget(text, scan.clone(), &blocks, &reference_labels, budget,),
                expected,
                "ranges changed at chunk size {budget}"
            );
        }
        for occurrence in 0..3 {
            let target = nth_range_of(text, "same visible", occurrence);
            assert!(
                expected.iter().all(|range| !intersects(range, &target)),
                "visible repeated occurrence {occurrence} was excluded"
            );
        }
    }

    #[test]
    fn lazily_supplied_block_ranges_match_eager_discovery() {
        let text = concat!(
            "---\ntitle: hidden_front\n---\nvisible\n\n",
            "```text\nhidden_fence\n```\n\n",
            "[target]: https://definition.test\n\n",
            "<div>\nhidden_html\n</div>\n",
        );
        let expected = ranges_with_budget(text, 0..text.len(), 3);
        assert_eq!(
            ranges_with_lazy_blocks_budget(text, 0..text.len(), 3),
            expected
        );
    }

    #[test]
    fn inline_html_quotes_and_only_real_link_labels_open_exclusions() {
        let text = concat!(
            "before <span title=\"hidden > leaked\" data-x='also > hidden'>inside</span> after\n",
            "literal ](checked) and escaped [label\\](checked_too) but ",
            "[visible](hidden_destination) and ",
            "[visible_space](space_destination ) and ",
            "[visible_title](destination \"escaped \\\" title\") and ",
            "[visible_pointy](</my uri>) and ",
            "[not_a_link](mispelled ordinary words) and ",
            "[visible_reference][missing_reference] and ",
            "[invalid_pointy](<foo\\>) and ",
            "[invalid_cr](<foo\rbar>) and [invalid_escaped_cr](<foo\\\rbar>)\n",
        );
        let expected = [
            range_of(
                text,
                "<span title=\"hidden > leaked\" data-x='also > hidden'>",
            ),
            range_of(text, "</span>"),
            range_of(text, "hidden_destination"),
            range_of(text, "space_destination "),
            range_of(text, "destination \"escaped \\\" title\""),
            range_of(text, "</my uri>"),
        ];
        for budget in [1, 2, 7, 4096] {
            assert_eq!(
                ranges_with_budget(text, 0..text.len(), budget),
                expected,
                "quoted HTML/link recognition changed at budget {budget}"
            );
        }
        for checked in [
            "leaked",
            "inside",
            "checked",
            "checked_too",
            "visible",
            "mispelled",
            "ordinary",
            "words",
            "missing_reference",
            "foo",
            "bar",
        ] {
            let target = range_of(text, checked);
            let should_be_hidden = checked == "leaked";
            assert_eq!(
                expected.iter().any(|range| intersects(range, &target)),
                should_be_hidden,
                "wrong prose visibility for {checked:?}"
            );
        }
    }

    #[test]
    fn inline_raw_html_forms_remain_opaque_across_greater_than_bytes() {
        let text = concat!(
            "before <!-- hidden > comment --> after\n",
            "before <!--> short comment after\n",
            "before <!---> short hyphen comment after\n",
            "before <!DOCTYPE note SYSTEM \"hidden declaration\"> after\n",
            "before <!doctype lower_hidden> after\n",
            "before <?target hidden > processing?> after\n",
            "before <![CDATA[hidden > cdata]]> after\n",
            "before <span\n title=\"hidden multiline\">inside</span> after\n",
            "before <!-- hidden\n > multiline comment --> after\n",
            "invalid <mispelled!> and <person@> remain visible\n",
            "invalid <span !mispelled> and <person()@example.com> remain visible\n",
            "invalid <http://foo<bar> remains visible\n",
            "valid <foo@bar> remains opaque\n",
            "bare single@label remains visible\n",
        );
        let expected = [
            range_of(text, "<!-- hidden > comment -->"),
            range_of(text, "<!-->"),
            range_of(text, "<!--->"),
            range_of(text, "<!DOCTYPE note SYSTEM \"hidden declaration\">"),
            range_of(text, "<!doctype lower_hidden>"),
            range_of(text, "<?target hidden > processing?>"),
            range_of(text, "<![CDATA[hidden > cdata]]>"),
            range_of(text, "<span\n title=\"hidden multiline\">"),
            range_of(text, "</span>"),
            range_of(text, "<!-- hidden\n > multiline comment -->"),
            range_of(text, "<foo@bar>"),
        ];
        for budget in [1, 2, 3, 7, 4096] {
            assert_eq!(
                ranges_with_budget(text, 0..text.len(), budget),
                expected,
                "raw HTML recognition changed at budget {budget}"
            );
        }
        for checked in ["mispelled", "person", "foo", "bar", "single", "label"] {
            let target = range_of(text, checked);
            assert!(
                expected.iter().all(|range| !intersects(range, &target)),
                "invalid raw construct hid {checked:?}"
            );
        }
    }

    #[test]
    fn bare_email_excludes_only_its_valid_endpoint_before_sentence_punctuation() {
        let text = "Contact sentence@example.test. Then continue.";
        let expected = [range_of(text, "sentence@example.test")];
        for budget in [1, 2, 7, 4096] {
            assert_eq!(
                ranges_with_budget(text, 0..text.len(), budget),
                expected,
                "email endpoint changed at budget {budget}"
            );
        }
    }

    #[test]
    fn degenerate_documents_and_unclosed_inline_constructs_are_safe() {
        for (text, expected) in [
            ("", Vec::new()),
            ("plain", Vec::new()),
            (
                "---\nonly: front\n---",
                std::iter::once(0.."---\nonly: front\n---".len()).collect(),
            ),
            ("`unclosed code", Vec::new()),
            ("<unclosed", Vec::new()),
        ] {
            let ranges = ranges_with_budget(text, 0..text.len(), 1);
            assert_eq!(ranges, expected, "wrong exclusions for {text:?}");
        }
    }

    #[test]
    fn small_range_does_not_advance_into_unrelated_suffix() {
        let text = format!("short text{}", "x".repeat(1024 * 1024));
        let highlighter = Highlighter::new(&text);
        let scan = 0.."short text".len();
        let blocks = highlighter.spell_block_exclusion_ranges(scan.clone());
        let reference_labels = highlighter.spell_reference_labels();
        let mut scanner = ProseExclusionScanner::new(&text, scan.clone(), blocks, reference_labels);
        while !scanner.is_complete() {
            scanner.step(&text, 3);
            assert!(scanner.position() <= scan.end);
        }
        assert_eq!(scanner.position(), scan.end);
        assert!(scanner.into_ranges().is_empty());
    }

    #[test]
    fn one_mib_line_is_interruptible_after_one_chunk() {
        let text = format!("https://example.test/{}", "a".repeat(1024 * 1024));
        let highlighter = Highlighter::new(&text);
        let blocks = highlighter.spell_block_exclusion_ranges(0..text.len());
        let reference_labels = highlighter.spell_reference_labels();
        let mut scanner =
            ProseExclusionScanner::new(&text, 0..text.len(), blocks, reference_labels);
        assert!(!scanner.step(&text, 4096));
        assert_eq!(scanner.position(), 4096);
        assert!(!scanner.is_complete());
    }

    fn nth_range_of(text: &str, needle: &str, occurrence: usize) -> Range<usize> {
        let start = text
            .match_indices(needle)
            .nth(occurrence)
            .map(|(start, _)| start)
            .unwrap_or_else(|| panic!("missing occurrence {occurrence} of {needle:?}"));
        start..start + needle.len()
    }
}
