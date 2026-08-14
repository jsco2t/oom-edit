mod exclusions;
mod state;

use std::error::Error;
use std::fmt;
use std::ops::Range;

pub(crate) use state::SpellState;

/// A provider-neutral document diagnostic.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Diagnostic {
    /// Subsystem that produced this diagnostic.
    pub provider: DiagnosticProvider,
    /// Presentation-neutral severity.
    pub severity: DiagnosticSeverity,
    /// Exact half-open UTF-8 source byte range.
    pub range: Range<usize>,
    /// Exact source text observed when the diagnostic was produced.
    pub source_text: String,
    /// Provider-authored human-readable message.
    pub message: String,
}

/// Closed set of diagnostic providers supported by this release.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticProvider {
    /// Built-in English spelling provider.
    Spell,
}

/// Provider-neutral diagnostic severity.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticSeverity {
    /// Error-level diagnostic.
    Error,
    /// Warning-level diagnostic.
    Warning,
    /// Informational diagnostic.
    Info,
    /// Hint-level diagnostic.
    Hint,
}

/// Renderer-neutral decoration payload shared by source and rendered geometry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DecorationKind {
    /// Decoration for a provider-authored document diagnostic.
    Diagnostic {
        /// Subsystem that produced the diagnostic.
        provider: DiagnosticProvider,
        /// Presentation-neutral diagnostic severity.
        severity: DiagnosticSeverity,
    },
}

/// One diagnostic interval in rendered display-cell coordinates.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiagnosticDecorationRow {
    /// Zero-based row in the complete rendered layout.
    pub row: usize,
    /// Half-open display-cell interval on `row`.
    pub columns: Range<usize>,
    /// Shared decoration payload.
    pub kind: DecorationKind,
}

/// Zero-based source position whose column counts Unicode scalar values.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TextPosition {
    /// Zero-based physical line.
    pub line: usize,
    /// Zero-based Unicode-scalar column.
    pub column: usize,
}

/// Error returned when a source byte offset cannot name a cursor position.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PositionError {
    /// The offset is beyond the document's logical EOF.
    OutOfBounds,
    /// The offset falls in the middle of a UTF-8 scalar value.
    NotCharBoundary,
}

impl fmt::Display for PositionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OutOfBounds => formatter.write_str("source offset is beyond end of document"),
            Self::NotCharBoundary => {
                formatter.write_str("source offset is not a UTF-8 character boundary")
            }
        }
    }
}

impl Error for PositionError {}

pub(crate) fn normalize_reference_label(label: &str) -> Option<String> {
    let label = label
        .strip_prefix('[')
        .and_then(|label| label.strip_suffix(']'))
        .unwrap_or(label);
    if label.is_empty() || label.len() > 999 {
        return None;
    }
    let normalized: String = label
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase();
    (!normalized.is_empty()).then_some(normalized)
}
