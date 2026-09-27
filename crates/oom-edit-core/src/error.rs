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

    /// A previously backed path disappeared; an explicit version-bound choice is required.
    #[error("file is missing on disk: {0}")]
    Missing(PathBuf),

    /// Replacement occurred, but durability or the final disk state could not be confirmed.
    #[error("file replacement committed but final state is uncertain: {0}")]
    CommittedUncertain(std::io::Error),

    /// Atomic write failed (temp file write, fsync, or rename).
    #[error("failed to save file: {0}")]
    Io(#[from] std::io::Error),
}

/// Rejection of a version-bound external-change decision.
#[derive(Debug, Error)]
pub enum DiskDecisionError {
    /// The observed version is no longer current or belongs to another path.
    #[error("disk version is stale")]
    StaleVersion,
    /// The decision does not apply to the current disk state.
    #[error("disk decision does not apply to the current state")]
    WrongState,
    /// Disk observation failed.
    #[error("failed to inspect disk state: {0}")]
    Io(#[from] std::io::Error),
}

/// Error from atomic reload preparation.
#[derive(Debug, Error)]
pub enum ReloadError {
    /// No file is attached to this session.
    #[error("session has no disk path")]
    Unbacked,
    /// The expected disk version no longer matches.
    #[error("disk version is stale")]
    StaleVersion,
    /// The candidate contains invalid UTF-8 at this byte.
    #[error("file contains invalid UTF-8 at byte offset {0}")]
    NotUtf8(usize),
    /// The candidate could not be read.
    #[error("failed to read reload candidate: {0}")]
    Io(#[from] std::io::Error),
}

/// Error from a guarded path retarget.
#[derive(Debug, Error)]
pub enum RetargetError {
    /// No file is attached to this session.
    #[error("session has no disk path")]
    Unbacked,
    /// The destination version changed since validation.
    #[error("destination disk version is stale")]
    StaleVersion,
    /// Destination bytes or identity conflict with the source baseline.
    #[error("destination does not match the moved file")]
    ConflictingDestination,
    /// The destination could not be read.
    #[error("failed to inspect destination: {0}")]
    Io(#[from] std::io::Error),
}

/// Error from parsing front matter.
///
/// Owns a stable parser-neutral diagnostic. Parse failures degrade
/// gracefully: the document still opens and editing remains available.
#[derive(Debug, Error, PartialEq, Eq)]
#[error("front matter parse error: {message}")]
pub struct FmError {
    message: String,
}

impl FmError {
    pub(crate) fn from_parser(error: gray_matter::Error) -> Self {
        Self {
            message: error.to_string(),
        }
    }

    /// Return the owned parser-neutral diagnostic text.
    pub fn message(&self) -> &str {
        &self.message
    }
}
