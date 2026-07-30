use std::ops::Range;

use oom_spell::{classify_candidate, CandidateDecision, SpellEngine, TokenizerState};

use super::exclusions::ProseExclusionScanner;
use super::{Diagnostic, DiagnosticProvider, DiagnosticSeverity};
use crate::syntax::Highlighter;
use crate::vim::TextEdit;

pub(crate) enum InvalidationPlan {
    Full,
    Local {
        old_range: Range<usize>,
        edit_start: usize,
        new_text_len: usize,
        delta: isize,
    },
}

enum ScanCursor {
    Exclusions(Box<ProseExclusionScanner>),
    Tokens {
        exclusions: Vec<Range<usize>>,
        tokenizer: TokenizerState,
        found: Vec<Diagnostic>,
    },
}

enum ScanState {
    Clean,
    Dirty {
        range: Range<usize>,
    },
    Scanning {
        range: Range<usize>,
        cursor: ScanCursor,
    },
}

pub(crate) struct SpellState {
    enabled: bool,
    generation: u64,
    diagnostics: Vec<Diagnostic>,
    exclusions: Vec<Range<usize>>,
    scan: ScanState,
}

impl SpellState {
    pub(crate) fn new(text_len: usize) -> Self {
        Self {
            enabled: true,
            generation: 0,
            diagnostics: Vec::new(),
            exclusions: Vec::new(),
            scan: ScanState::Dirty { range: 0..text_len },
        }
    }

    pub(crate) fn enabled(&self) -> bool {
        self.enabled
    }

    pub(crate) fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    pub(crate) fn diagnostics(&self) -> &[Diagnostic] {
        if self.enabled {
            &self.diagnostics
        } else {
            &[]
        }
    }

    pub(crate) fn pending(&self) -> bool {
        self.enabled && !matches!(self.scan, ScanState::Clean)
    }

    pub(crate) fn prepare_invalidation(
        &self,
        old_text: &str,
        edits: &[TextEdit],
    ) -> InvalidationPlan {
        let [edit] = edits else {
            return InvalidationPlan::Full;
        };
        let Some(old_source) = old_text.get(edit.range.clone()) else {
            return InvalidationPlan::Full;
        };
        let old_range = containing_line(old_text, edit.range.start, edit.range.end);
        if !locally_safe(old_source)
            || !locally_safe(&edit.new_text)
            || has_context_marker(&old_text[old_range.clone()])
            || self
                .exclusions
                .iter()
                .any(|exclusion| edit_touches(exclusion, &edit.range))
        {
            return InvalidationPlan::Full;
        }
        InvalidationPlan::Local {
            old_range,
            edit_start: edit.range.start,
            new_text_len: edit.new_text_len,
            delta: edit.new_text_len as isize - edit.range.len() as isize,
        }
    }

    pub(crate) fn invalidate(&mut self, plan: InvalidationPlan, new_text: &str) {
        if !matches!(self.scan, ScanState::Clean) {
            self.diagnostics.clear();
            self.exclusions.clear();
            self.scan = ScanState::Dirty {
                range: 0..new_text.len(),
            };
            return;
        }

        match plan {
            InvalidationPlan::Full => {
                self.diagnostics.clear();
                self.exclusions.clear();
                self.scan = ScanState::Dirty {
                    range: 0..new_text.len(),
                };
            }
            InvalidationPlan::Local {
                old_range,
                edit_start,
                new_text_len,
                delta,
            } => {
                self.diagnostics.retain_mut(|diagnostic| {
                    if diagnostic.range.end <= old_range.start {
                        return true;
                    }
                    if diagnostic.range.start >= old_range.end {
                        diagnostic.range = shift_range(&diagnostic.range, delta);
                        return true;
                    }
                    false
                });
                self.exclusions.retain_mut(|exclusion| {
                    if exclusion.end <= old_range.start {
                        return true;
                    }
                    if exclusion.start >= old_range.end {
                        *exclusion = shift_range(exclusion, delta);
                        return true;
                    }
                    false
                });
                let new_end = edit_start.saturating_add(new_text_len).min(new_text.len());
                self.scan = ScanState::Dirty {
                    range: containing_line(new_text, edit_start.min(new_text.len()), new_end),
                };
            }
        }
    }

    pub(crate) fn tick(
        &mut self,
        text: &str,
        highlighter: &Highlighter,
        engine: &SpellEngine,
        max_bytes: usize,
    ) -> bool {
        if !self.enabled || max_bytes == 0 {
            return false;
        }

        let mut worked = false;
        if self.generation != engine.generation() {
            self.generation = engine.generation();
            self.diagnostics.clear();
            self.exclusions.clear();
            self.scan = ScanState::Dirty {
                range: 0..text.len(),
            };
            worked = true;
        }

        let mut remaining = max_bytes;
        while remaining > 0 {
            let state = std::mem::replace(&mut self.scan, ScanState::Clean);
            match state {
                ScanState::Clean => break,
                ScanState::Dirty { range } => {
                    if range.is_empty() {
                        self.scan = ScanState::Clean;
                        worked = true;
                        continue;
                    }
                    let scanner = ProseExclusionScanner::new(
                        text,
                        range.clone(),
                        Vec::new(),
                        highlighter.spell_reference_labels(),
                    );
                    self.scan = ScanState::Scanning {
                        range,
                        cursor: ScanCursor::Exclusions(Box::new(scanner)),
                    };
                }
                ScanState::Scanning {
                    range,
                    cursor: ScanCursor::Exclusions(mut scanner),
                } => {
                    let before = scanner.position();
                    let step_end = before.saturating_add(remaining).min(range.end);
                    scanner.add_block_ranges(
                        highlighter.spell_block_exclusion_ranges(before..step_end),
                    );
                    let complete = scanner.step(text, remaining);
                    let consumed = scanner.position().saturating_sub(before);
                    remaining = remaining.saturating_sub(consumed);
                    worked |= consumed > 0;
                    if complete {
                        let exclusions = (*scanner).into_ranges();
                        let tokenizer = TokenizerState::new(range.clone());
                        self.scan = ScanState::Scanning {
                            range,
                            cursor: ScanCursor::Tokens {
                                exclusions,
                                tokenizer,
                                found: Vec::new(),
                            },
                        };
                    } else {
                        self.scan = ScanState::Scanning {
                            range,
                            cursor: ScanCursor::Exclusions(scanner),
                        };
                    }
                }
                ScanState::Scanning {
                    range,
                    cursor:
                        ScanCursor::Tokens {
                            exclusions,
                            mut tokenizer,
                            mut found,
                        },
                } => {
                    let before = tokenizer.position();
                    let chunk =
                        oom_spell::tokenize_chunk(text, &exclusions, &mut tokenizer, remaining);
                    let consumed = tokenizer.position().saturating_sub(before);
                    remaining = remaining.saturating_sub(consumed);
                    worked |= consumed > 0;
                    for token in chunk.into_tokens() {
                        match classify_candidate(&token) {
                            CandidateDecision::Skip => {}
                            CandidateDecision::Check(word) => {
                                if let Some(diagnostic) =
                                    unknown_diagnostic(engine, token.range, word)
                                {
                                    found.push(diagnostic);
                                }
                            }
                            CandidateDecision::CheckParts(parts) => {
                                for part in parts {
                                    let word = &text[part.clone()];
                                    if let Some(diagnostic) = unknown_diagnostic(engine, part, word)
                                    {
                                        found.push(diagnostic);
                                    }
                                }
                            }
                        }
                    }
                    if tokenizer.is_complete() {
                        self.exclusions.extend(exclusions);
                        self.exclusions
                            .sort_by_key(|exclusion| (exclusion.start, exclusion.end));
                        self.diagnostics.extend(found);
                        self.diagnostics
                            .sort_by_key(|diagnostic| diagnostic.range.start);
                        self.scan = ScanState::Clean;
                    } else {
                        self.scan = ScanState::Scanning {
                            range,
                            cursor: ScanCursor::Tokens {
                                exclusions,
                                tokenizer,
                                found,
                            },
                        };
                    }
                }
            }
        }
        worked
    }
}

fn unknown_diagnostic(engine: &SpellEngine, range: Range<usize>, word: &str) -> Option<Diagnostic> {
    (!engine.check(word)).then(|| Diagnostic {
        provider: DiagnosticProvider::Spell,
        severity: DiagnosticSeverity::Warning,
        range,
        source_text: word.to_string(),
        message: format!("Possible misspelling: {word}"),
    })
}

fn locally_safe(text: &str) -> bool {
    text.chars()
        .all(|character| character.is_ascii_alphanumeric() || matches!(character, '_' | '\'' | '’'))
}

fn has_context_marker(text: &str) -> bool {
    text.bytes().any(|byte| {
        matches!(
            byte,
            b'`' | b'~' | b'<' | b'>' | b'[' | b']' | b'(' | b')' | b'\\'
        )
    })
}

fn edit_touches(exclusion: &Range<usize>, edit: &Range<usize>) -> bool {
    if edit.is_empty() {
        exclusion.start <= edit.start && edit.start <= exclusion.end
    } else {
        exclusion.start < edit.end && edit.start < exclusion.end
    }
}

fn containing_line(text: &str, start: usize, end: usize) -> Range<usize> {
    let line_start = text[..start]
        .rfind('\n')
        .map_or(0, |newline| newline.saturating_add(1));
    let line_end = text[end..]
        .find('\n')
        .map_or(text.len(), |relative| end + relative);
    line_start..line_end
}

fn shift_range(range: &Range<usize>, delta: isize) -> Range<usize> {
    range.start.saturating_add_signed(delta)..range.end.saturating_add_signed(delta)
}
