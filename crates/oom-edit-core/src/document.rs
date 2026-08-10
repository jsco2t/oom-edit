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

trait AtomicSaveOperations {
    type TempFile;
    type ParentDirectory;

    fn create_temp(&mut self, parent: &Path) -> io::Result<Self::TempFile>;
    fn write_all(&mut self, temp_file: &mut Self::TempFile, contents: &[u8]) -> io::Result<()>;
    fn set_permissions(
        &mut self,
        temp_file: &Self::TempFile,
        permissions: fs::Permissions,
    ) -> io::Result<()>;
    fn sync_file(&mut self, temp_file: &Self::TempFile) -> io::Result<()>;
    fn persist(&mut self, temp_file: Self::TempFile, target: &Path) -> io::Result<()>;
    fn open_parent_read_only(&mut self, parent: &Path) -> io::Result<Self::ParentDirectory>;
    fn sync_parent(&mut self, parent: &Self::ParentDirectory) -> io::Result<()>;
}

struct FileSystemAtomicSave;

impl AtomicSaveOperations for FileSystemAtomicSave {
    type TempFile = tempfile::NamedTempFile;
    type ParentDirectory = fs::File;

    fn create_temp(&mut self, parent: &Path) -> io::Result<Self::TempFile> {
        tempfile::NamedTempFile::new_in(parent)
    }

    fn write_all(&mut self, temp_file: &mut Self::TempFile, contents: &[u8]) -> io::Result<()> {
        temp_file.write_all(contents)
    }

    fn set_permissions(
        &mut self,
        temp_file: &Self::TempFile,
        permissions: fs::Permissions,
    ) -> io::Result<()> {
        fs::set_permissions(temp_file.path(), permissions)
    }

    fn sync_file(&mut self, temp_file: &Self::TempFile) -> io::Result<()> {
        temp_file.as_file().sync_all()
    }

    fn persist(&mut self, temp_file: Self::TempFile, target: &Path) -> io::Result<()> {
        temp_file
            .persist(target)
            .map(|_| ())
            .map_err(|e| io::Error::other(format!("failed to persist temp file: {e}")))
    }

    fn open_parent_read_only(&mut self, parent: &Path) -> io::Result<Self::ParentDirectory> {
        fs::File::open(parent)
    }

    fn sync_parent(&mut self, parent: &Self::ParentDirectory) -> io::Result<()> {
        parent.sync_all()
    }
}

fn parent_directory(path: &Path) -> &Path {
    path.parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."))
}

struct AtomicSaveError {
    error: io::Error,
    committed: bool,
}

impl AtomicSaveError {
    fn before_commit(error: io::Error) -> Self {
        Self {
            error,
            committed: false,
        }
    }

    fn after_commit(error: io::Error) -> Self {
        Self {
            error,
            committed: true,
        }
    }
}

fn atomic_save<O: AtomicSaveOperations>(
    operations: &mut O,
    parent: &Path,
    target: &Path,
    contents: &[u8],
    permissions: Option<fs::Permissions>,
) -> Result<(), AtomicSaveError> {
    let mut temp_file = operations
        .create_temp(parent)
        .map_err(AtomicSaveError::before_commit)?;
    operations
        .write_all(&mut temp_file, contents)
        .map_err(AtomicSaveError::before_commit)?;
    if let Some(permissions) = permissions {
        operations
            .set_permissions(&temp_file, permissions)
            .map_err(AtomicSaveError::before_commit)?;
    }
    operations
        .sync_file(&temp_file)
        .map_err(AtomicSaveError::before_commit)?;
    operations
        .persist(temp_file, target)
        .map_err(AtomicSaveError::before_commit)?;

    // Make the rename durable. Failure means the save did not meet FR-5.3.
    let parent_directory = operations
        .open_parent_read_only(parent)
        .map_err(AtomicSaveError::after_commit)?;
    operations
        .sync_parent(&parent_directory)
        .map_err(AtomicSaveError::after_commit)
}

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
                        let offset = e.utf8_error().valid_up_to();
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
        self.save_with_text_using(text, override_path, force, &mut FileSystemAtomicSave)
    }

    fn save_with_text_using<O: AtomicSaveOperations>(
        &mut self,
        text: &str,
        override_path: Option<&Path>,
        force: bool,
        operations: &mut O,
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
        let parent = parent_directory(&target_path);
        let save_result = atomic_save(
            operations,
            parent,
            &target_path,
            serialized.as_bytes(),
            Some(permissions),
        );

        if let Err(error) = save_result {
            if error.committed {
                self.record_disk_state(&target_path, serialized.len(), override_path.is_some());
            }
            return Err(error.error.into());
        }

        // Update recorded metadata. EditorSession synchronizes this
        // document's save point from the vim engine after a successful save.
        self.record_disk_state(&target_path, serialized.len(), override_path.is_some());

        // Re-parse front matter after save (cheap — always re-parse)
        self.front_matter = crate::frontmatter::parse_front_matter(text);

        Ok(())
    }

    /// Save a copy of the document to the given path without retargeting
    /// or clearing dirty (for `:w {path}`).
    ///
    /// Uses atomic write (temp file + fsync + rename) to prevent corruption
    /// on crash or interrupt.
    pub fn save_copy(&self, path: &Path) -> Result<(), SaveError> {
        self.save_copy_with_text(&self.text, path)
    }

    /// Save supplied live editor text as a copy without retargeting.
    pub(crate) fn save_copy_with_text(&self, text: &str, path: &Path) -> Result<(), SaveError> {
        self.save_copy_with_text_using(text, path, &mut FileSystemAtomicSave)
    }

    #[cfg(test)]
    fn save_copy_using<O: AtomicSaveOperations>(
        &self,
        path: &Path,
        operations: &mut O,
    ) -> Result<(), SaveError> {
        self.save_copy_with_text_using(&self.text, path, operations)
    }

    fn save_copy_with_text_using<O: AtomicSaveOperations>(
        &self,
        text: &str,
        path: &Path,
        operations: &mut O,
    ) -> Result<(), SaveError> {
        let serialized = self.serialize_text(text);

        // Atomic write: create temp file, write, fsync, rename
        let parent = parent_directory(path);
        atomic_save(operations, parent, path, serialized.as_bytes(), None)
            .map_err(|error| error.error)?;

        // Update recorded metadata to match the copy
        // Note: we don't update self.last_len or self.last_mtime because
        // this is a copy-out, not a retarget — dirty tracking stays relative
        // to the original file.
        let _ = fs::metadata(path);

        Ok(())
    }

    fn record_disk_state(&mut self, target: &Path, serialized_len: usize, retarget: bool) {
        self.last_len = Some(serialized_len);
        self.last_mtime = fs::metadata(target).ok().and_then(|m| m.modified().ok());
        if retarget {
            self.path = Some(target.to_path_buf());
        }
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

    /// Check if this document was opened from a nonexistent path (new file).
    ///
    /// Per FR-6.10: opening a nonexistent path creates a new-buffer session
    /// with empty text and the path retained. This method returns `true` when
    /// the file has never been saved.
    pub fn is_new(&self) -> bool {
        // A document is "new" if it has a path but was never saved (no metadata).
        // When opening an existing file, last_len and last_mtime are set.
        // When creating a new buffer, they are None.
        self.path.is_some() && self.last_len.is_none()
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
    #[cfg(test)]
    fn serialize(&self) -> String {
        self.serialize_text(&self.text)
    }

    /// Serialize the given text, restoring the recorded line ending
    /// and final-newline state.
    fn serialize_text(&self, text: &str) -> String {
        let mut result = text.to_string();

        // Restore final-newline
        if self.has_final_newline && !result.ends_with('\n') {
            result.push('\n');
        } else if !self.has_final_newline && result.ends_with('\n') {
            // Remove trailing newlines to match original
            result.truncate(result.trim_end_matches('\n').len());
        }

        // Restore line ending
        if self.line_ending == LineEnding::CrLf {
            result = result.replace('\n', "\r\n");
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

#[cfg(test)]
mod tests {
    use std::fs;
    use std::io;
    use std::path::{Path, PathBuf};

    use super::{parent_directory, AtomicSaveOperations, Document};

    #[derive(Debug, Clone, PartialEq, Eq)]
    enum AtomicSaveEvent {
        Write,
        FileSync,
        Rename,
        OpenParentReadOnly(PathBuf),
        DirectorySync,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Failure {
        FileSync,
        Persist,
        ParentOpen,
        ParentSync,
    }

    #[derive(Default)]
    struct RecordingAtomicSave {
        events: Vec<AtomicSaveEvent>,
        failure: Option<Failure>,
        commit_to_disk: bool,
    }

    impl RecordingAtomicSave {
        fn failing_at(failure: Failure) -> Self {
            Self {
                events: Vec::new(),
                failure: Some(failure),
                commit_to_disk: false,
            }
        }

        fn committing_and_failing_at(failure: Failure) -> Self {
            Self {
                events: Vec::new(),
                failure: Some(failure),
                commit_to_disk: true,
            }
        }

        fn fail(&self, failure: Failure) -> io::Result<()> {
            if self.failure == Some(failure) {
                Err(io::Error::other("injected atomic-save failure"))
            } else {
                Ok(())
            }
        }
    }

    impl AtomicSaveOperations for RecordingAtomicSave {
        type TempFile = Vec<u8>;
        type ParentDirectory = ();

        fn create_temp(&mut self, _parent: &Path) -> io::Result<Self::TempFile> {
            Ok(Vec::new())
        }

        fn write_all(&mut self, temp_file: &mut Self::TempFile, contents: &[u8]) -> io::Result<()> {
            self.events.push(AtomicSaveEvent::Write);
            temp_file.extend_from_slice(contents);
            Ok(())
        }

        fn set_permissions(
            &mut self,
            _temp_file: &Self::TempFile,
            _permissions: fs::Permissions,
        ) -> io::Result<()> {
            Ok(())
        }

        fn sync_file(&mut self, _temp_file: &Self::TempFile) -> io::Result<()> {
            self.events.push(AtomicSaveEvent::FileSync);
            self.fail(Failure::FileSync)
        }

        fn persist(&mut self, temp_file: Self::TempFile, target: &Path) -> io::Result<()> {
            self.events.push(AtomicSaveEvent::Rename);
            self.fail(Failure::Persist)?;
            if self.commit_to_disk {
                fs::write(target, temp_file)?;
            }
            Ok(())
        }

        fn open_parent_read_only(&mut self, parent: &Path) -> io::Result<Self::ParentDirectory> {
            self.events
                .push(AtomicSaveEvent::OpenParentReadOnly(parent.to_path_buf()));
            self.fail(Failure::ParentOpen)
        }

        fn sync_parent(&mut self, _parent: &Self::ParentDirectory) -> io::Result<()> {
            self.events.push(AtomicSaveEvent::DirectorySync);
            self.fail(Failure::ParentSync)
        }
    }

    fn expected_atomic_save_events(parent: &Path) -> Vec<AtomicSaveEvent> {
        vec![
            AtomicSaveEvent::Write,
            AtomicSaveEvent::FileSync,
            AtomicSaveEvent::Rename,
            AtomicSaveEvent::OpenParentReadOnly(parent.to_path_buf()),
            AtomicSaveEvent::DirectorySync,
        ]
    }

    #[test]
    fn both_document_save_paths_use_the_complete_atomic_sequence() {
        let mut document = Document::from_text("original");
        let mut save_operations = RecordingAtomicSave::default();
        document
            .save_with_text_using(
                "updated",
                Some(Path::new("document.md")),
                true,
                &mut save_operations,
            )
            .unwrap();
        assert_eq!(
            save_operations.events,
            expected_atomic_save_events(Path::new("."))
        );

        let mut copy_operations = RecordingAtomicSave::default();
        document
            .save_copy_using(Path::new("copy.md"), &mut copy_operations)
            .unwrap();
        assert_eq!(
            copy_operations.events,
            expected_atomic_save_events(Path::new("."))
        );
    }

    #[test]
    fn document_save_paths_propagate_sync_and_parent_open_failures() {
        for failure in [Failure::FileSync, Failure::ParentOpen, Failure::ParentSync] {
            let mut document = Document::from_text("contents");
            let mut save_operations = RecordingAtomicSave::failing_at(failure);
            assert!(document
                .save_with_text_using(
                    "updated",
                    Some(Path::new("document.md")),
                    true,
                    &mut save_operations,
                )
                .is_err());

            let mut copy_operations = RecordingAtomicSave::failing_at(failure);
            assert!(document
                .save_copy_using(Path::new("copy.md"), &mut copy_operations)
                .is_err());
        }
    }

    #[test]
    fn document_save_records_committed_disk_state_after_parent_durability_failure() {
        for failure in [Failure::ParentOpen, Failure::ParentSync] {
            let directory = tempfile::tempdir().unwrap();
            let target = directory.path().join("document.md");
            let mut document = Document::from_text("original");
            let mut operations = RecordingAtomicSave::committing_and_failing_at(failure);

            assert!(document
                .save_with_text_using("updated", Some(&target), true, &mut operations)
                .is_err());

            assert_eq!(document.path(), Some(target.as_path()));
            assert_eq!(document.last_len, Some("updated".len()));
            assert!(document.last_mtime.is_some());
        }
    }

    #[test]
    fn document_save_does_not_record_disk_state_before_rename() {
        for failure in [Failure::FileSync, Failure::Persist] {
            let mut document = Document::from_text("original");
            let mut operations = RecordingAtomicSave::failing_at(failure);

            assert!(document
                .save_with_text_using(
                    "updated",
                    Some(Path::new("document.md")),
                    true,
                    &mut operations,
                )
                .is_err());

            assert_eq!(document.path(), None);
            assert_eq!(document.last_len, None);
            assert_eq!(document.last_mtime, None);
        }
    }

    #[test]
    fn relative_targets_resolve_parent_to_current_directory() {
        assert_eq!(parent_directory(Path::new("document.md")), Path::new("."));
    }

    #[test]
    fn real_document_saves_commit_contents_in_a_temp_directory() {
        let directory = tempfile::tempdir().unwrap();
        let target = directory.path().join("document.md");
        let copy = directory.path().join("copy.md");
        let mut document = Document::from_text("durable contents");

        document.save(Some(&target), true).unwrap();
        document.save_copy(&copy).unwrap();

        assert_eq!(fs::read_to_string(target).unwrap(), "durable contents");
        assert_eq!(fs::read_to_string(copy).unwrap(), "durable contents");
    }

    #[test]
    fn serialize_crlf_without_final_newline_removes_entire_line_ending() {
        let mut document = Document::from_text("hello\r\nworld");
        document.set_text("hello\nworld\n");

        assert_eq!(document.serialize(), "hello\r\nworld");
    }

    #[test]
    fn serialize_crlf_with_final_newline_adds_crlf_ending() {
        let mut document = Document::from_text("hello\r\nworld\r\n");
        document.set_text("hello\nworld");

        assert_eq!(document.serialize(), "hello\r\nworld\r\n");
    }

    #[test]
    fn serialize_round_trips_crlf_without_final_newline() {
        let original = "hello\r\nworld";
        let document = Document::from_text(original);

        assert_eq!(document.serialize(), original);
    }

    #[test]
    fn serialize_empty_crlf_document_without_final_newline_is_empty() {
        let mut document = Document::from_text("hello\r\nworld");
        document.set_text("");

        assert_eq!(document.serialize(), "");
    }
}
