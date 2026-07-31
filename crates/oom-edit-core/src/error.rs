//! Error types for document I/O operations.
//!
//! See architecture §6 / plan §6.6 (FR-5.x) for the error semantics.

use std::path::PathBuf;
use thiserror::Error;

/// Error returned when opening a document fails.
///
/// Per FR-5.1: invalid UTF-8 is refused with the byte offset of the first
/// bad byte — no lossy conversion.
#[derive(Debug, Error)]
pub enum OpenError {
    /// The file contains invalid UTF-8 at the given byte offset.
    #[error("file contains invalid UTF-8 at byte offset {0}")]
    NotUtf8(usize),

    /// The file could not be read (I/O error).
    #[error("failed to read file: {0}")]
    Io(#[from] std::io::Error),
}

/// Error returned when saving a document fails.
#[derive(Debug, Error)]
pub enum SaveError {
    /// The file was externally modified since it was opened/last saved
    /// (FR-5.7). The path identifies the file that was checked.
    #[error("file externally modified: {0}")]
    ExternallyModified(PathBuf),

    /// Atomic write failed (temp file write, fsync, or rename).
    #[error("failed to save file: {0}")]
    Io(#[from] std::io::Error),
}

/// Error from parsing front matter.
///
/// Wraps the `gray_matter` parsing error. Parse failures degrade gracefully:
/// the document still opens, editing is unaffected, and the structural API
/// returns the error (FR-5.6).
#[derive(Debug, Error, PartialEq, Eq)]
#[error("front matter parse error: {0}")]
pub struct FmError(#[from] pub gray_matter::Error);
