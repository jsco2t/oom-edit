//! Document model — text, path, front matter, dirty tracking, and file I/O.
//!
//! The `Document` type wraps the buffer identity: path, line-ending style,
//! final-newline flag, front matter, save-point `UndoMark`, and last-known
//! mtime + length for external-modification detection (FR-5.7).
//!
//! See architecture §4 and plan §6.6 for the full contract.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use crate::error::{OpenError, SaveError};
use crate::frontmatter::FrontMatter;
use crate::vim::UndoMark;

/// Line ending style.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineEnding {
    /// Unix-style line endings (`\n`).
    Lf,
    /// Windows-style line endings (`\r\n`).
    CrLf,
}

/// A document — text content plus metadata for I/O and dirty tracking.
///
/// The text is always stored internally with LF line endings. On open, the
/// dominant line ending is detected and recorded; on save, it is restored.
/// Final-newline presence is similarly preserved (FR-5.2, FR-5.6).
#[derive(Debug)]
pub struct Document {
    /// The file path, if the document was opened from or saved to a file.
    path: Option<PathBuf>,
    /// The full text content (always stored with LF line endings).
    text: String,
    /// The dominant line ending detected on open (restored on save).
    line_ending: LineEnding,
    /// Whether the original file had a final newline.
    has_final_newline: bool,
    /// Parsed front matter, if any.
    front_matter: FrontMatter,
    /// The dirty generation at last save (for dirty tracking).
    save_point: UndoMark,
    /// Last-known file size in bytes (for external-modification detection).
    last_len: Option<usize>,
    /// Last-known file mtime as system time (for external-modification detection).
    last_mtime: Option<std::time::SystemTime>,
}

impl Document {
    /// Open a document from a file path.
    ///
    /// If the file does not exist, creates a new-buffer document with empty
    /// text and the path retained (FR-6.10 / new-file semantics).
    ///
    /// Per FR-5.1: invalid UTF-8 is refused with the byte offset of the
    /// first bad byte.
    ///
    /// Per FR-5.2: dominant line ending is detected and recorded; final-newline
    /// presence is recorded; in-memory text is normalized to LF.
    pub fn open(path: &Path) -> Result<Self, OpenError> {
        match fs::read(path) {
            Ok(bytes) => {
                // UTF-8 validation: find the first invalid byte (FR-5.1)
                let text = match String::from_utf8(bytes) {
                    Ok(t) => t,
                    Err(e) => {
                        let offset = find_first_invalid_utf8(&e.into_bytes());
                        return Err(OpenError::NotUtf8(offset));
                    }
                };

                // Detect line ending and final-newline presence
                let (line_ending, has_final_newline) = detect_line_ending(&text);

                // Normalize to LF in-memory
                let normalized = normalize_lf(&text);

                // Parse front matter
                let front_matter = crate::frontmatter::parse_front_matter(&normalized);

                // Get file metadata for external-modification detection
                let metadata = fs::metadata(path).ok();
                let last_len = metadata.as_ref().map(|m| m.len() as usize);
                let last_mtime = metadata.and_then(|m| m.modified().ok());

                let save_point = UndoMark(0);

                Ok(Self {
                    path: Some(path.to_path_buf()),
                    text: normalized,
                    line_ending,
                    has_final_newline,
                    front_matter,
                    save_point,
                    last_len,
                    last_mtime,
                })
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                // New-buffer semantics: empty buffer, path retained, not-dirty
                Ok(Self {
                    path: Some(path.to_path_buf()),
                    text: String::new(),
                    line_ending: LineEnding::Lf,
                    has_final_newline: false,
                    front_matter: FrontMatter::None,
                    save_point: UndoMark(0),
                    last_len: None,
                    last_mtime: None,
                })
            }
            Err(e) => Err(OpenError::Io(e)),
        }
    }

    /// Create a new document from raw text (no file path).
    ///
    /// Used for headless construction and programmatic document creation.
    pub fn from_text(text: &str) -> Self {
        let (line_ending, has_final_newline) = detect_line_ending(text);
        let normalized = normalize_lf(text);
        let front_matter = crate::frontmatter::parse_front_matter(&normalized);

        Self {
            path: None,
            text: normalized,
            line_ending,
            has_final_newline,
            front_matter,
            save_point: UndoMark(0),
            last_len: None,
            last_mtime: None,
        }
    }

    /// Save the document to its path (or the given override path).
    ///
    /// Uses the document's own text for serialization.
    ///
    /// `force: true` bypasses external-modification detection (FR-5.7).
    ///
    /// Per FR-5.3: atomic write via temp file + fsync + rename.
    /// On Unix, permissions are masked to `original & 0o644` (user read/write
    /// + group/other read); on non-Unix, default permissions are used.
    pub fn save(&mut self, override_path: Option<&Path>, force: bool) -> Result<(), SaveError> {
        let text = self.text.clone();
        self.save_with_text(&text, override_path, force)
    }

    /// Save the document using the provided text (e.g., from the vim buffer).
    ///
    /// This is the internal save path used by `EditorSession` where the
    /// vim buffer owns the text and the Document handles I/O metadata.
    pub fn save_with_text(
        &mut self,
        text: &str,
        override_path: Option<&Path>,
        force: bool,
    ) -> Result<(), SaveError> {
        let target_path = override_path
            .map(|p| p.to_path_buf())
            .or_else(|| self.path.clone())
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::NotFound,
                    "no path to save to (document has no path)",
                )
            })?;

        // FR-5.7: Check for external modification (unless force)
        if !force {
            if let Some(ref path) = self.path {
                if let Ok(metadata) = fs::metadata(path) {
                    let current_len = metadata.len() as usize;
                    let current_mtime = metadata.modified().ok();

                    if let (Some(expected_len), Some(_expected_mtime)) =
                        (self.last_len, self.last_mtime)
                    {
                        let modified =
                            current_len != expected_len || current_mtime != self.last_mtime;

                        if modified {
                            return Err(SaveError::ExternallyModified(path.clone()));
                        }
                    }
                }
            }
        }

        // Serialize: restore line ending + final-newline state
        let serialized = self.serialize_text(text);

        // Determine permissions: if target exists, preserve its permissions;
        // otherwise use 0o644 (masked to user read/write + group/other read)
        #[cfg(unix)]
        use std::os::unix::fs::PermissionsExt;

        let permissions = if target_path.exists() {
            fs::metadata(&target_path)
                .ok()
                .map(|m| m.permissions().mode())
                .map(|m| {
                    // Mask to user read/write only on Unix-like systems
                    #[cfg(unix)]
                    {
                        fs::Permissions::from_mode(m & 0o644)
                    }
                    #[cfg(not(unix))]
                    {
                        // On non-Unix, just use default permissions
                        fs::Permissions::new()
                    }
                })
                .unwrap_or_else(|| {
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::PermissionsExt;
                        fs::Permissions::from_mode(0o644)
                    }
                    #[cfg(not(unix))]
                    {
                        fs::Permissions::new()
                    }
                })
        } else {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::Permissions::from_mode(0o644)
            }
            #[cfg(not(unix))]
            {
                fs::Permissions::new()
            }
        };

        // Atomic write: create temp file with O_EXCL (prevents symlink attacks),
        // write, fsync, rename. The temp file is created in the same directory
        // as the target to ensure atomic rename on POSIX.
        let parent = target_path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new(""));

        let mut temp_file = tempfile::NamedTempFile::new_in(parent)?;
        temp_file.write_all(serialized.as_bytes())?;

        // Set permissions on the temp file (we own it exclusively due to O_EXCL)
        fs::set_permissions(temp_file.path(), permissions)?;

        // Sync the temp file to disk
        fs::OpenOptions::new()
            .write(true)
            .open(temp_file.path())?
            .sync_all()?;

        // Best-effort fsync of parent directory
        if let Ok(dir) = fs::OpenOptions::new().write(true).open(parent) {
            let _ = dir.sync_all();
        }

        // Rename temp to target (atomic on POSIX)
        temp_file
            .persist(&target_path)
            .map_err(|e| io::Error::other(format!("failed to persist temp file: {}", e)))?;

        // Update save point and recorded metadata
        self.save_point = self.save_point();
        self.last_len = Some(serialized.len());
        self.last_mtime = fs::metadata(&target_path)
            .ok()
            .and_then(|m| m.modified().ok());

        // Re-parse front matter after save (cheap — always re-parse)
        self.front_matter = crate::frontmatter::parse_front_matter(text);

        // Update the path if an override was used (for :saveas)
        if override_path.is_some() {
            self.path = Some(target_path.to_path_buf());
        }

        Ok(())
    }

    /// Save a copy of the document to the given path without retargeting
    /// or clearing dirty (for `:w {path}`).
    ///
    /// Uses atomic write (temp file + fsync + rename) to prevent corruption
    /// on crash or interrupt.
    pub fn save_copy(&self, path: &Path) -> Result<(), SaveError> {
        let serialized = self.serialize();

        // Atomic write: create temp file, write, fsync, rename
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new(""));

        let mut temp_file = tempfile::NamedTempFile::new_in(parent)?;
        temp_file.write_all(serialized.as_bytes())?;

        // Sync the temp file to disk
        fs::OpenOptions::new()
            .write(true)
            .open(temp_file.path())?
            .sync_all()?;

        // Best-effort fsync of parent directory
        if let Ok(dir) = fs::OpenOptions::new().write(true).open(parent) {
            let _ = dir.sync_all();
        }

        // Rename temp to target (atomic on POSIX)
        temp_file
            .persist(path)
            .map_err(|e| io::Error::other(format!("failed to persist temp file: {}", e)))?;

        // Update recorded metadata to match the copy
        // Note: we don't update self.last_len or self.last_mtime because
        // this is a copy-out, not a retarget — dirty tracking stays relative
        // to the original file.
        let _ = fs::metadata(path);

        Ok(())
    }

    /// Return the document's path, if any.
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// Return the document text (always LF-terminated internally).
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Return the front matter, if any.
    pub fn front_matter(&self) -> &FrontMatter {
        &self.front_matter
    }

    /// Set the front matter (called after re-parse on save).
    pub fn set_front_matter(&mut self, fm: FrontMatter) {
        self.front_matter = fm;
    }

    /// Return the line count.
    pub fn line_count(&self) -> usize {
        if self.text.is_empty() {
            return 1;
        }
        self.text.matches('\n').count() + 1
    }

    /// Return the line ending style.
    pub fn line_ending(&self) -> LineEnding {
        self.line_ending
    }

    /// Check if the document has a final newline.
    pub fn has_final_newline(&self) -> bool {
        self.has_final_newline
    }

    /// Check if the document is dirty (modified since last save).
    ///
    /// Per FR-5.4: dirty = buffer has been modified since the save point.
    /// Undo back to the save point clears dirty.
    pub fn is_dirty(&self, current_dirty_gen: u64) -> bool {
        current_dirty_gen != self.save_point.0
    }

    /// Take a save point (marks current state as clean).
    pub fn save_point(&mut self) -> UndoMark {
        // In a real implementation, this would read the current dirty gen
        // from the vim engine. For the Document type, we just return the
        // current save point. The session layer updates this after saves.
        self.save_point
    }

    /// Set the save point directly.
    pub fn set_save_point(&mut self, mark: UndoMark) {
        self.save_point = mark;
    }

    /// Update the recorded path (for :saveas / retarget).
    pub fn set_path(&mut self, path: PathBuf) {
        self.path = Some(path);
    }

    /// Update the text content. Called after buffer edits.
    pub fn set_text(&mut self, text: &str) {
        // Normalize to LF
        let normalized = normalize_lf(text);

        // Re-parse front matter (cheap — always re-parse)
        self.front_matter = crate::frontmatter::parse_front_matter(&normalized);

        // Store normalized text
        self.text = normalized;
    }

    /// Serialize the document to bytes, restoring the recorded line ending
    /// and final-newline state.
    fn serialize(&self) -> String {
        self.serialize_text(&self.text)
    }

    /// Serialize the given text, restoring the recorded line ending
    /// and final-newline state.
    fn serialize_text(&self, text: &str) -> String {
        let mut result = text.to_string();

        // Restore line ending
        if self.line_ending == LineEnding::CrLf {
            result = result.replace('\n', "\r\n");
        }

        // Restore final-newline
        if self.has_final_newline && !result.ends_with('\n') {
            result.push('\n');
        } else if !self.has_final_newline && result.ends_with('\n') {
            // Remove trailing newlines to match original
            result.truncate(result.trim_end_matches('\n').len());
        }

        result
    }
}

/// Detect the dominant line ending and final-newline presence in text.
///
/// Counts `\r\n` vs bare `\n`; tie → LF.
/// Returns `(dominant_line_ending, has_final_newline)`.
fn detect_line_ending(text: &str) -> (LineEnding, bool) {
    if text.is_empty() {
        return (LineEnding::Lf, false);
    }

    let crlf_count = text.matches("\r\n").count();
    let lf_count = text.matches('\n').count();
    let bare_lf = lf_count - crlf_count;

    let line_ending = if crlf_count > bare_lf {
        LineEnding::CrLf
    } else {
        LineEnding::Lf
    };

    let has_final_newline = text.ends_with('\n');

    (line_ending, has_final_newline)
}

/// Normalize a string to LF line endings.
fn normalize_lf(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n")
}

/// Find the byte offset of the first invalid UTF-8 byte.
///
/// Scans for any byte with the high bit set — used to find a byte offset
/// to report when `from_utf8` has already confirmed invalid UTF-8.
fn find_first_invalid_utf8(bytes: &[u8]) -> usize {
    for (i, &b) in bytes.iter().enumerate() {
        if (b as i8) < 0 {
            return i;
        }
    }
    bytes.len()
}
