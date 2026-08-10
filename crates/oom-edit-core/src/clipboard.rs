//! Clipboard sink trait for the core.
//!
//! The core emits `Effect::ClipboardWrite` and the host routes it to its
//! [`ClipboardSink`] implementation. The TUI provides `Osc52Clipboard`
//! (architecture §7.2).
//!
//! See plan V-R3.

use std::fmt;

/// Error returned by clipboard operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClipboardError {
    /// The payload exceeds the practical OSC 52 limit (100 KB).
    TooLarge,
    /// The terminal refused the write (e.g. not supported).
    NotSupported,
    /// A generic I/O or encoding error.
    Other(String),
}

impl fmt::Display for ClipboardError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ClipboardError::TooLarge => write!(f, "payload exceeds 100 KB clipboard limit"),
            ClipboardError::NotSupported => write!(f, "clipboard not supported by terminal"),
            ClipboardError::Other(msg) => write!(f, "{msg}"),
        }
    }
}

impl std::error::Error for ClipboardError {}

/// A sink for clipboard write operations.
///
/// The TUI implements this via OSC 52 escape emission; headless hosts may
/// write to stdout, a file, or drop the data.
pub trait ClipboardSink {
    /// Write `text` to the system clipboard.
    fn copy(&mut self, text: &str) -> Result<(), ClipboardError>;
}

/// A recording clipboard sink for tests — captures every copy call.
///
/// Implements [`ClipboardSink`] by storing each payload in a `Vec<String>`.
#[derive(Debug, Default)]
pub struct RecordingClipboardSink {
    captures: Vec<String>,
}

impl RecordingClipboardSink {
    /// Return all captured payloads in order.
    pub fn captures(&self) -> &[String] {
        &self.captures
    }

    /// Clear all captured payloads.
    pub fn clear(&mut self) {
        self.captures.clear();
    }
}

impl ClipboardSink for RecordingClipboardSink {
    fn copy(&mut self, text: &str) -> Result<(), ClipboardError> {
        // Recording sink accepts anything (no size limit).
        self.captures.push(text.to_string());
        Ok(())
    }
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recording_sink_captures_payload() {
        let mut sink = RecordingClipboardSink::default();
        sink.copy("hello").unwrap();
        sink.copy("world").unwrap();
        assert_eq!(sink.captures(), &["hello", "world"]);
    }

    #[test]
    fn recording_sink_clear() {
        let mut sink = RecordingClipboardSink::default();
        sink.copy("hello").unwrap();
        sink.clear();
        assert!(sink.captures().is_empty());
    }

    #[test]
    fn clipboard_error_display() {
        assert_eq!(
            format!("{}", ClipboardError::TooLarge),
            "payload exceeds 100 KB clipboard limit"
        );
        assert_eq!(
            format!("{}", ClipboardError::NotSupported),
            "clipboard not supported by terminal"
        );
        assert_eq!(
            format!("{}", ClipboardError::Other("boom".to_string())),
            "boom"
        );
    }
}
