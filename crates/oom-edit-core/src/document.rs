//! Private file identity, serialization policy and versioned disk operations.
//! Live text and its derived caches belong exclusively to LiveDocument.

use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use sha2::{Digest, Sha256};

use crate::error::{DiskDecisionError, OpenError, ReloadError, RetargetError, SaveError};

/// Content-validated identity of one path observation. Fields are intentionally opaque.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiskVersion {
    path: PathBuf,
    snapshot: Option<FileSnapshot>,
}

/// Captured file versions for a host-managed move; does not mutate the session.
#[derive(Debug)]
pub struct RetargetPreparation {
    source: DiskVersion,
    baseline: Option<DiskVersion>,
    destination: DiskVersion,
}

impl RetargetPreparation {
    /// Version at the destination before the host changes the filesystem.
    pub fn destination_version(&self) -> &DiskVersion {
        &self.destination
    }
}

/// Validated, IO-free identity update. Text, cursor and undo are unaffected.
#[derive(Debug)]
pub struct RetargetBinding {
    source: PathBuf,
    baseline: Option<DiskVersion>,
    destination: PathBuf,
    observed: DiskVersion,
}

impl RetargetBinding {
    /// Coherent disk state at validation, retaining the pre-move baseline.
    pub fn disk_state(&self) -> DiskState {
        let mut baseline = self.baseline.clone();
        if let Some(baseline) = &mut baseline {
            baseline.path = self.destination.clone();
        }
        match (&baseline, self.observed.is_missing()) {
            (None, true) => DiskState::NeverCreated {
                version: self.observed.clone(),
            },
            (Some(_), true) => DiskState::Missing {
                version: self.observed.clone(),
            },
            (Some(baseline), false) if baseline == &self.observed => DiskState::Unchanged {
                version: self.observed.clone(),
            },
            _ => DiskState::Modified {
                version: self.observed.clone(),
            },
        }
    }
}

impl DiskVersion {
    /// Path whose contents or absence were validated.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Whether the validated path was absent.
    pub fn is_missing(&self) -> bool {
        self.snapshot.is_none()
    }
    /// Validate the current content version of a path without opening an editor session.
    pub fn observe(path: &Path) -> Result<Self, DiskIoError> {
        read_version(path)
            .map(|(_, version)| version)
            .map_err(Into::into)
    }
}

/// Result of a content-validated disk observation.
#[derive(Debug)]
pub enum DiskState {
    /// The session has no path.
    Unbacked,
    /// The named path has never existed for this session.
    NeverCreated {
        /// Version-bound absence at the named path.
        version: DiskVersion,
    },
    /// The bytes and file identity match the last accepted baseline.
    Unchanged {
        /// Validated current file version.
        version: DiskVersion,
    },
    /// The path exists but does not match the accepted baseline.
    Modified {
        /// Validated current file version.
        version: DiskVersion,
    },
    /// A previously backed path no longer exists.
    Missing {
        /// Version-bound absence at the formerly backed path.
        version: DiskVersion,
    },
    /// The path could not be observed; errors other than NotFound are not Missing.
    IoError(DiskIoError),
}

/// A metadata-only hint suitable for bounded routine polling.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiskHint {
    /// No path.
    Unbacked,
    /// A never-created path remains absent.
    NeverCreated,
    /// Metadata still matches; this is not proof of byte identity.
    Unchanged,
    /// Metadata or existence changed; content validation is needed.
    Changed,
    /// A previously backed path is absent.
    Missing,
    /// The metadata probe failed for another reason.
    IoError,
}

/// Owned I/O diagnostic from a disk observation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiskIoError {
    kind: DiskIoErrorKind,
    message: String,
}

impl DiskIoError {
    /// Build an owned diagnostic for an injected disk-service failure.
    pub fn new(kind: DiskIoErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }
    /// Stable category for presentation and policy.
    pub fn kind(&self) -> DiskIoErrorKind {
        self.kind
    }

    /// Human-readable operating-system detail.
    pub fn message(&self) -> &str {
        &self.message
    }
}

/// Project-owned disk I/O error category.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiskIoErrorKind {
    /// The process is not allowed to inspect the path.
    PermissionDenied,
    /// The path is a directory rather than a readable file.
    NotAFile,
    /// Any other I/O failure.
    Other,
}

impl From<io::Error> for DiskIoError {
    fn from(error: io::Error) -> Self {
        let kind = match error.kind() {
            io::ErrorKind::PermissionDenied => DiskIoErrorKind::PermissionDenied,
            io::ErrorKind::IsADirectory => DiskIoErrorKind::NotAFile,
            _ => DiskIoErrorKind::Other,
        };
        Self {
            kind,
            message: error.to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FileSnapshot {
    len: u64,
    modified: Option<SystemTime>,
    identity: FileIdentity,
    digest: [u8; 32],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FileIdentity {
    #[cfg(unix)]
    device: u64,
    #[cfg(unix)]
    inode: u64,
}

impl FileIdentity {
    fn from_metadata(metadata: &fs::Metadata) -> Self {
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            Self {
                device: metadata.dev(),
                inode: metadata.ino(),
            }
        }
        #[cfg(not(unix))]
        {
            let _ = metadata;
            Self {}
        }
    }
}

fn snapshot_from_bytes(bytes: &[u8], metadata: &fs::Metadata) -> FileSnapshot {
    FileSnapshot {
        len: metadata.len(),
        modified: metadata.modified().ok(),
        identity: FileIdentity::from_metadata(metadata),
        digest: Sha256::digest(bytes).into(),
    }
}

fn read_version(path: &Path) -> io::Result<(Option<Vec<u8>>, DiskVersion)> {
    let mut file = match fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Ok((
                None,
                DiskVersion {
                    path: path.to_path_buf(),
                    snapshot: None,
                },
            ));
        }
        Err(error) => return Err(error),
    };
    let before = file.metadata()?;
    if !before.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::IsADirectory,
            "path is not a regular file",
        ));
    }
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;
    let after = file.metadata()?;
    let at_path = fs::metadata(path)?;
    let stable = |left: &fs::Metadata, right: &fs::Metadata| {
        left.len() == right.len()
            && left.modified().ok() == right.modified().ok()
            && FileIdentity::from_metadata(left) == FileIdentity::from_metadata(right)
    };
    if !stable(&before, &after) || !stable(&after, &at_path) {
        return Err(io::Error::other("file changed during disk observation"));
    }
    let snapshot = snapshot_from_bytes(&bytes, &after);
    Ok((
        Some(bytes),
        DiskVersion {
            path: path.to_path_buf(),
            snapshot: Some(snapshot),
        },
    ))
}

pub(crate) trait AtomicSaveOperations {
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
    fn before_replace(&mut self, _target: &Path) -> io::Result<()> {
        Ok(())
    }
    fn persist(&mut self, temp_file: Self::TempFile, target: &Path) -> io::Result<()>;
    fn open_parent_read_only(&mut self, parent: &Path) -> io::Result<Self::ParentDirectory>;
    fn sync_parent(&mut self, parent: &Self::ParentDirectory) -> io::Result<()>;
}

pub(crate) struct FileSystemAtomicSave;

/// Auditable save boundary at which a host can observe or reject an operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveBoundary {
    /// The temporary file is synced; the target has not yet been replaced.
    BeforeReplace,
    /// Replacement succeeded; directory durability is not yet confirmed.
    ConfirmDurability,
}

/// Injected save-boundary service. A rejection after replacement is
/// committed-uncertain, never rollback or a successful durable save.
pub trait SaveObserver {
    /// Observe one save boundary for the actual target path.
    fn observe(&mut self, boundary: SaveBoundary, path: &Path) -> Result<(), DiskIoError>;
}

pub(crate) struct ObservedAtomicSave<'a> {
    pub(crate) observer: &'a mut dyn SaveObserver,
    pub(crate) target: PathBuf,
}

impl AtomicSaveOperations for ObservedAtomicSave<'_> {
    type TempFile = tempfile::NamedTempFile;
    type ParentDirectory = fs::File;
    fn create_temp(&mut self, parent: &Path) -> io::Result<Self::TempFile> {
        FileSystemAtomicSave.create_temp(parent)
    }
    fn write_all(&mut self, temp: &mut Self::TempFile, contents: &[u8]) -> io::Result<()> {
        FileSystemAtomicSave.write_all(temp, contents)
    }
    fn set_permissions(
        &mut self,
        temp: &Self::TempFile,
        permissions: fs::Permissions,
    ) -> io::Result<()> {
        FileSystemAtomicSave.set_permissions(temp, permissions)
    }
    fn sync_file(&mut self, temp: &Self::TempFile) -> io::Result<()> {
        FileSystemAtomicSave.sync_file(temp)
    }
    fn before_replace(&mut self, target: &Path) -> io::Result<()> {
        self.observer
            .observe(SaveBoundary::BeforeReplace, target)
            .map_err(|error| io::Error::other(error.message))?;
        Ok(())
    }
    fn persist(&mut self, temp: Self::TempFile, target: &Path) -> io::Result<()> {
        FileSystemAtomicSave.persist(temp, target)
    }
    fn open_parent_read_only(&mut self, parent: &Path) -> io::Result<Self::ParentDirectory> {
        FileSystemAtomicSave.open_parent_read_only(parent)
    }
    fn sync_parent(&mut self, parent: &Self::ParentDirectory) -> io::Result<()> {
        self.observer
            .observe(SaveBoundary::ConfirmDurability, &self.target)
            .map_err(|error| io::Error::other(error.message))?;
        FileSystemAtomicSave.sync_parent(parent)
    }
}

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
        temp_file.persist(target).map(|_| ()).map_err(|error| {
            io::Error::new(
                error.error.kind(),
                format!("failed to persist temp file: {error}"),
            )
        })
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
    expected: &DiskVersion,
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
        .before_replace(target)
        .map_err(AtomicSaveError::before_commit)?;
    let (_, current) = read_version(target).map_err(AtomicSaveError::before_commit)?;
    if &current != expected {
        return Err(AtomicSaveError::before_commit(io::Error::other(
            "file changed before atomic replacement",
        )));
    }
    operations
        .persist(temp_file, target)
        .map_err(AtomicSaveError::before_commit)?;

    // A replacement is not durably saved until the parent directory is synced.
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

/// Private file identity and serialization policy for a live session.
#[derive(Debug)]
pub struct Document {
    /// The file path, if the document was opened from or saved to a file.
    path: Option<PathBuf>,
    /// The dominant line ending detected on open (restored on save).
    line_ending: LineEnding,
    /// Whether the original file had a final newline.
    has_final_newline: bool,
    /// Last accepted content-validated disk baseline, if the path has existed.
    baseline: Option<DiskVersion>,
    /// One explicitly accepted external observation, invalidated by later changes.
    acknowledgement: Option<Acknowledgement>,
}

#[derive(Debug, Clone)]
enum Acknowledgement {
    KeepMine(DiskVersion),
    Recreate(DiskVersion),
}

/// Fully validated candidate; no live editor state changes until this is committed.
pub(crate) struct PreparedReload {
    pub(crate) text: String,
    line_ending: LineEnding,
    has_final_newline: bool,
    version: DiskVersion,
}

/// One normalized text value paired with its non-text file metadata.
pub(crate) struct OpenedDocument {
    text: String,
    metadata: Document,
}

impl OpenedDocument {
    pub(crate) fn into_parts(self) -> (String, Document) {
        (self.text, self.metadata)
    }
}

impl std::ops::Deref for OpenedDocument {
    type Target = Document;

    fn deref(&self) -> &Self::Target {
        &self.metadata
    }
}

impl std::ops::DerefMut for OpenedDocument {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.metadata
    }
}

#[cfg(test)]
impl OpenedDocument {
    fn set_text(&mut self, text: &str) {
        self.text = normalize_lf(text);
    }

    fn save(&mut self, path: Option<&Path>, force: bool) -> Result<(), SaveError> {
        let text = self.text.clone();
        self.metadata
            .save_with_text_using(&text, path, force, &mut FileSystemAtomicSave)
    }

    fn save_copy(&self, path: &Path) -> Result<(), SaveError> {
        self.metadata.save_copy_with_text(&self.text, path)
    }

    fn save_copy_using<O: AtomicSaveOperations>(
        &self,
        path: &Path,
        operations: &mut O,
    ) -> Result<(), SaveError> {
        self.metadata
            .save_copy_with_text_using(&self.text, path, operations)
    }

    fn serialize(&self) -> String {
        self.metadata.serialize_text(&self.text)
    }
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
    pub(crate) fn open(path: &Path) -> Result<OpenedDocument, OpenError> {
        Self::open_with_missing_policy(path, true)
    }

    pub(crate) fn open_existing(path: &Path) -> Result<OpenedDocument, OpenError> {
        Self::open_with_missing_policy(path, false)
    }

    fn open_with_missing_policy(
        path: &Path,
        allow_missing: bool,
    ) -> Result<OpenedDocument, OpenError> {
        match read_version(path) {
            Ok((Some(bytes), version)) => {
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

                Ok(OpenedDocument {
                    text: normalized,
                    metadata: Self {
                        path: Some(path.to_path_buf()),
                        line_ending,
                        has_final_newline,
                        baseline: Some(version),
                        acknowledgement: None,
                    },
                })
            }
            Ok((None, _)) if allow_missing => {
                // New-buffer semantics: empty buffer, path retained, not-dirty
                Ok(OpenedDocument {
                    text: String::new(),
                    metadata: Self {
                        path: Some(path.to_path_buf()),
                        line_ending: LineEnding::Lf,
                        has_final_newline: false,
                        baseline: None,
                        acknowledgement: None,
                    },
                })
            }
            Ok((None, _)) => Err(OpenError::Io(io::Error::new(
                io::ErrorKind::NotFound,
                "file not found",
            ))),
            Err(e) => Err(OpenError::Io(e)),
        }
    }

    /// Create a new document from raw text (no file path).
    ///
    /// Used for headless construction and programmatic document creation.
    pub(crate) fn from_text(text: &str) -> OpenedDocument {
        let (line_ending, has_final_newline) = detect_line_ending(text);
        let normalized = normalize_lf(text);
        OpenedDocument {
            text: normalized,
            metadata: Self {
                path: None,
                line_ending,
                has_final_newline,
                baseline: None,
                acknowledgement: None,
            },
        }
    }

    /// Observe content and identity, including replacements with unchanged metadata.
    pub(crate) fn disk_state(&self) -> DiskState {
        let Some(path) = &self.path else {
            return DiskState::Unbacked;
        };
        let version = match read_version(path) {
            Ok((_, version)) => version,
            Err(error) => return DiskState::IoError(error.into()),
        };
        match (&self.baseline, &version.snapshot) {
            (None, None) => DiskState::NeverCreated { version },
            (Some(_), None) => DiskState::Missing { version },
            (Some(baseline), Some(_)) if baseline == &version => DiskState::Unchanged { version },
            _ => DiskState::Modified { version },
        }
    }

    /// Poll metadata without reading file content. Unchanged is only a hint.
    pub(crate) fn disk_hint(&self) -> DiskHint {
        let Some(path) = &self.path else {
            return DiskHint::Unbacked;
        };
        match fs::metadata(path) {
            Ok(metadata) if !metadata.is_file() => DiskHint::IoError,
            Ok(metadata) => match &self.baseline {
                Some(baseline)
                    if baseline.snapshot.as_ref().is_some_and(|snapshot| {
                        snapshot.len == metadata.len()
                            && snapshot.modified == metadata.modified().ok()
                            && snapshot.identity == FileIdentity::from_metadata(&metadata)
                    }) =>
                {
                    DiskHint::Unchanged
                }
                _ => DiskHint::Changed,
            },
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                if self.baseline.is_some() {
                    DiskHint::Missing
                } else {
                    DiskHint::NeverCreated
                }
            }
            Err(_) => DiskHint::IoError,
        }
    }

    pub(crate) fn acknowledge_keep_mine(
        &mut self,
        expected: &DiskVersion,
    ) -> Result<(), DiskDecisionError> {
        match self.disk_state() {
            DiskState::Modified { version } if &version == expected => {
                self.acknowledgement = Some(Acknowledgement::KeepMine(version));
                Ok(())
            }
            DiskState::IoError(error) => {
                Err(DiskDecisionError::Io(io::Error::other(error.message)))
            }
            DiskState::Modified { .. } => Err(DiskDecisionError::StaleVersion),
            _ => Err(DiskDecisionError::WrongState),
        }
    }

    pub(crate) fn authorize_recreation(
        &mut self,
        expected: &DiskVersion,
    ) -> Result<(), DiskDecisionError> {
        match self.disk_state() {
            DiskState::Missing { version } if &version == expected => {
                self.acknowledgement = Some(Acknowledgement::Recreate(version));
                Ok(())
            }
            DiskState::IoError(error) => {
                Err(DiskDecisionError::Io(io::Error::other(error.message)))
            }
            DiskState::Missing { .. } => Err(DiskDecisionError::StaleVersion),
            _ => Err(DiskDecisionError::WrongState),
        }
    }

    pub(crate) fn prepare_reload(
        &self,
        expected: &DiskVersion,
    ) -> Result<PreparedReload, ReloadError> {
        let path = self.path.as_ref().ok_or(ReloadError::Unbacked)?;
        let (bytes, version) = read_version(path)?;
        if &version != expected {
            return Err(ReloadError::StaleVersion);
        }
        let bytes = bytes.ok_or(ReloadError::StaleVersion)?;
        let text = String::from_utf8(bytes)
            .map_err(|error| ReloadError::NotUtf8(error.utf8_error().valid_up_to()))?;
        let (line_ending, has_final_newline) = detect_line_ending(&text);
        Ok(PreparedReload {
            text: normalize_lf(&text),
            line_ending,
            has_final_newline,
            version,
        })
    }

    pub(crate) fn commit_reload(&mut self, candidate: &PreparedReload) {
        self.line_ending = candidate.line_ending;
        self.has_final_newline = candidate.has_final_newline;
        self.baseline = Some(candidate.version.clone());
        self.acknowledgement = None;
    }

    pub(crate) fn retarget(
        &mut self,
        destination: &Path,
        expected: &DiskVersion,
    ) -> Result<(), RetargetError> {
        let source = self.path.as_ref().ok_or(RetargetError::Unbacked)?;
        let (_, current) = read_version(destination)?;
        if &current != expected || expected.path != destination {
            return Err(RetargetError::StaleVersion);
        }
        let (Some(source_version), Some(destination_snapshot)) =
            (&self.baseline, &current.snapshot)
        else {
            return Err(RetargetError::ConflictingDestination);
        };
        let source_snapshot = source_version
            .snapshot
            .as_ref()
            .ok_or(RetargetError::ConflictingDestination)?;
        if destination_snapshot.identity != source_snapshot.identity
            || destination_snapshot.digest != source_snapshot.digest
        {
            return Err(RetargetError::ConflictingDestination);
        }
        if source != destination {
            self.path = Some(destination.to_path_buf());
            let mut baseline = source_version.clone();
            baseline.path = destination.to_path_buf();
            self.baseline = Some(baseline);
            self.acknowledgement = None;
        }
        Ok(())
    }

    pub(crate) fn prepare_retarget(
        &self,
        destination: &Path,
        expected_source: &DiskVersion,
    ) -> Result<RetargetPreparation, RetargetError> {
        let source = self.path.as_ref().ok_or(RetargetError::Unbacked)?;
        let (_, current) = read_version(source)?;
        if &current != expected_source {
            return Err(RetargetError::StaleVersion);
        }
        let (_, destination) = read_version(destination)?;
        Ok(RetargetPreparation {
            source: current,
            baseline: self.baseline.clone(),
            destination,
        })
    }

    pub(crate) fn validate_retarget(
        &self,
        preparation: &RetargetPreparation,
    ) -> Result<RetargetBinding, RetargetError> {
        if self.path.as_ref() != Some(&preparation.source.path)
            || self.baseline != preparation.baseline
        {
            return Err(RetargetError::StaleVersion);
        }
        let (_, current) = read_version(&preparation.destination.path)?;
        let matches_moved = match (&preparation.source.snapshot, &current.snapshot) {
            (Some(source), Some(destination)) => {
                source.identity == destination.identity && source.digest == destination.digest
            }
            (None, None) => true,
            _ => false,
        };
        if !matches_moved {
            return Err(RetargetError::ConflictingDestination);
        }
        Ok(RetargetBinding {
            source: preparation.source.path.clone(),
            baseline: preparation.baseline.clone(),
            destination: current.path.clone(),
            observed: current,
        })
    }

    pub(crate) fn can_commit_retarget(&self, binding: &RetargetBinding) -> bool {
        self.path.as_ref() == Some(&binding.source) && self.baseline == binding.baseline
    }

    pub(crate) fn commit_retarget(
        &mut self,
        binding: RetargetBinding,
    ) -> Result<(), RetargetError> {
        if !self.can_commit_retarget(&binding) {
            return Err(RetargetError::StaleVersion);
        }
        self.path = Some(binding.destination.clone());
        self.baseline = binding.baseline.map(|mut baseline| {
            baseline.path = binding.destination;
            baseline
        });
        self.acknowledgement = None;
        Ok(())
    }

    /// Save authoritative editor text through the atomic replacement protocol.
    pub(crate) fn save_with_text_using<O: AtomicSaveOperations>(
        &mut self,
        text: &str,
        override_path: Option<&Path>,
        force: bool,
        operations: &mut O,
    ) -> Result<(), SaveError> {
        self.save_with_text_expected_using(text, override_path, force, None, operations)
    }

    pub(crate) fn save_with_text_expected_using<O: AtomicSaveOperations>(
        &mut self,
        text: &str,
        override_path: Option<&Path>,
        force: bool,
        expected: Option<&DiskVersion>,
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

        // Validate content immediately before replacement. Portable filesystems
        // cannot make this a compare-and-swap against noncooperating writers.
        let (_, current) = read_version(&target_path)?;
        if expected.is_some_and(|version| version != &current) {
            return Err(if current.is_missing() {
                SaveError::Missing(target_path)
            } else {
                SaveError::ExternallyModified(target_path)
            });
        }
        if !force {
            let same_path = self.path.as_deref() == Some(target_path.as_path());
            if same_path {
                match (&self.baseline, &current.snapshot) {
                    (None, None) => {}
                    (Some(baseline), Some(_)) if baseline == &current => {}
                    (Some(_), Some(_))
                        if matches!(&self.acknowledgement,
                        Some(Acknowledgement::KeepMine(accepted)) if accepted == &current) => {}
                    (Some(_), None)
                        if matches!(&self.acknowledgement,
                        Some(Acknowledgement::Recreate(accepted)) if accepted == &current) => {}
                    (Some(_), None) => return Err(SaveError::Missing(target_path)),
                    _ => return Err(SaveError::ExternallyModified(target_path)),
                }
            } else if current.snapshot.is_some() {
                return Err(SaveError::ExternallyModified(target_path));
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
            &current,
        );

        if let Err(error) = save_result {
            if error.committed {
                let _ = self.record_disk_state(
                    &target_path,
                    serialized.as_bytes(),
                    override_path.is_some(),
                );
                return Err(SaveError::CommittedUncertain(error.error));
            }
            return Err(SaveError::Io(error.error));
        }

        // EditorSession synchronizes its save point only after durable success.
        self.record_disk_state(&target_path, serialized.as_bytes(), override_path.is_some())
            .map_err(SaveError::CommittedUncertain)?;

        Ok(())
    }

    /// Save supplied live editor text as a copy without retargeting.
    pub(crate) fn save_copy_with_text(&self, text: &str, path: &Path) -> Result<(), SaveError> {
        self.save_copy_with_text_using(text, path, &mut FileSystemAtomicSave)
    }

    pub(crate) fn save_copy_with_text_using<O: AtomicSaveOperations>(
        &self,
        text: &str,
        path: &Path,
        operations: &mut O,
    ) -> Result<(), SaveError> {
        let serialized = self.serialize_text(text);

        // Atomic write: create temp file, write, fsync, rename
        let parent = parent_directory(path);
        let (_, expected) = read_version(path)?;
        atomic_save(
            operations,
            parent,
            path,
            serialized.as_bytes(),
            None,
            &expected,
        )
        .map_err(|error| {
            if error.committed {
                SaveError::CommittedUncertain(error.error)
            } else {
                SaveError::Io(error.error)
            }
        })?;

        Ok(())
    }

    fn record_disk_state(
        &mut self,
        target: &Path,
        serialized: &[u8],
        retarget: bool,
    ) -> io::Result<()> {
        self.baseline = None;
        self.acknowledgement = None;
        if retarget {
            self.path = Some(target.to_path_buf());
        }
        let (bytes, version) = read_version(target)?;
        if bytes.as_deref() != Some(serialized) {
            return Err(io::Error::other("target changed after atomic replacement"));
        }
        self.baseline = Some(version);
        Ok(())
    }

    /// Return the document's path, if any.
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// Return the line ending style.
    pub fn line_ending(&self) -> LineEnding {
        self.line_ending
    }

    /// Check if the document has a final newline.
    pub fn has_final_newline(&self) -> bool {
        self.has_final_newline
    }

    /// Check if this document was opened from a nonexistent path (new file).
    ///
    /// Per FR-6.10: opening a nonexistent path creates a new-buffer session
    /// with empty text and the path retained. This method returns `true` when
    /// the file has never been saved.
    pub fn is_new(&self) -> bool {
        self.path.is_some() && self.baseline.is_none()
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

    use super::{parent_directory, AtomicSaveOperations, DiskIoError, DiskIoErrorKind, Document};
    use crate::SaveError;

    #[test]
    fn permission_and_non_file_errors_are_not_missing() {
        let denied = DiskIoError::from(io::Error::from(io::ErrorKind::PermissionDenied));
        assert_eq!(denied.kind(), DiskIoErrorKind::PermissionDenied);
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("folder.md");
        fs::create_dir(&path).unwrap();
        let document = Document::open(&path);
        assert!(document.is_err());
        let error = DiskIoError::from(super::read_version(&path).unwrap_err());
        assert_eq!(error.kind(), DiskIoErrorKind::NotAFile);
    }

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
        let directory = tempfile::tempdir().unwrap();
        let target = directory.path().join("document.md");
        let copy = directory.path().join("copy.md");
        let mut document = Document::from_text("original");
        let mut save_operations = RecordingAtomicSave {
            commit_to_disk: true,
            ..RecordingAtomicSave::default()
        };
        document
            .save_with_text_using("updated", Some(&target), true, &mut save_operations)
            .unwrap();
        assert_eq!(
            save_operations.events,
            expected_atomic_save_events(directory.path())
        );

        let mut copy_operations = RecordingAtomicSave::default();
        document
            .save_copy_using(&copy, &mut copy_operations)
            .unwrap();
        assert_eq!(
            copy_operations.events,
            expected_atomic_save_events(directory.path())
        );
    }

    #[test]
    fn document_save_paths_propagate_sync_and_parent_open_failures() {
        for failure in [Failure::FileSync, Failure::ParentOpen, Failure::ParentSync] {
            let mut document = Document::from_text("contents");
            let mut save_operations = RecordingAtomicSave::failing_at(failure);
            let save_error = document
                .save_with_text_using(
                    "updated",
                    Some(Path::new("document.md")),
                    true,
                    &mut save_operations,
                )
                .unwrap_err();
            assert_eq!(
                matches!(save_error, SaveError::CommittedUncertain(_)),
                failure != Failure::FileSync
            );

            let mut copy_operations = RecordingAtomicSave::failing_at(failure);
            let copy_error = document
                .save_copy_using(Path::new("copy.md"), &mut copy_operations)
                .unwrap_err();
            assert_eq!(
                matches!(copy_error, SaveError::CommittedUncertain(_)),
                failure != Failure::FileSync
            );
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
            let snapshot = document
                .baseline
                .as_ref()
                .unwrap()
                .snapshot
                .as_ref()
                .unwrap();
            assert_eq!(snapshot.len, "updated".len() as u64);
            assert!(snapshot.modified.is_some());
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
            assert!(document.baseline.is_none());
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
