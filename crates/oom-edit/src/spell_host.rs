//! TUI-owned spell-engine lifecycle, dictionary loading, and persistence.

use std::collections::BTreeSet;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use oom_spell::{
    normalize_dictionary_entry, AddWordOutcome, BuildProgress, SpellEngine, SpellEngineBuilder,
};

use crate::config::SpellConfig;

const WORDLIST_UNIT_BYTES: usize = 4 * 1024;
const MAX_ADDITIONAL_DICTIONARY_BYTES: usize = 16 * 1024 * 1024;

const EN_US: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/assets/dict/en_US.txt"
));
const EN_CA: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/assets/dict/en_CA.txt"
));
const EN_AU: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/assets/dict/en_AU.txt"
));

/// Fully resolved engine inputs. Construction performs no I/O.
pub(crate) struct WordlistSource {
    builtin: String,
    additional_dictionaries: Vec<PathBuf>,
}

impl WordlistSource {
    #[cfg(test)]
    pub(crate) fn testing(words: impl Into<String>) -> Self {
        Self {
            builtin: words.into(),
            additional_dictionaries: Vec::new(),
        }
    }

    #[cfg(test)]
    fn with_additional(words: impl Into<String>, paths: Vec<PathBuf>) -> Self {
        Self {
            builtin: words.into(),
            additional_dictionaries: paths,
        }
    }
}

pub(crate) struct WordlistResolution {
    pub(crate) source: WordlistSource,
    pub(crate) warning: Option<String>,
}

/// Select the built-in dialect and resolve additional paths relative to the
/// directory containing `config.toml`.
pub(crate) fn resolve_wordlist_source(
    config: &SpellConfig,
    config_path: &Path,
) -> WordlistResolution {
    let (builtin, warning) = match config.language.as_str() {
        "en_US" => (EN_US, None),
        "en_CA" => (EN_CA, None),
        "en_AU" => (EN_AU, None),
        language => (
            EN_US,
            Some(format!(
                "invalid spell language '{language}'; falling back to en_US"
            )),
        ),
    };
    let config_directory = config_path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let additional_dictionaries = config
        .additional_dictionaries
        .iter()
        .map(|path| {
            if path.is_absolute() {
                path.clone()
            } else {
                config_directory.join(path)
            }
        })
        .collect();

    WordlistResolution {
        source: WordlistSource {
            // Resolve the embedded source to owned host input before the event
            // loop starts, so the first idle unit only moves this allocation.
            builtin: builtin.to_string(),
            additional_dictionaries,
        },
        warning,
    }
}

struct OpenedWordlist {
    reader: Box<dyn Read>,
    advertised_len: Option<u64>,
}

trait WordlistReaderFactory {
    fn open(&mut self, path: &Path) -> io::Result<OpenedWordlist>;
}

struct FileWordlistReaderFactory;

impl WordlistReaderFactory for FileWordlistReaderFactory {
    fn open(&mut self, path: &Path) -> io::Result<OpenedWordlist> {
        let file = File::open(path)?;
        let advertised_len = file.metadata().ok().map(|metadata| metadata.len());
        Ok(OpenedWordlist {
            reader: Box::new(file),
            advertised_len,
        })
    }
}

#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "Phase 6 owns the tested persistence boundary; command routing is Phase 7"
    )
)]
trait PersonalDictionaryStore {
    fn load(&mut self, max_bytes: usize) -> Result<PersonalLoadProgress, String>;
    fn save(&mut self, words: &[String]) -> Result<(), String>;
}

enum PersonalLoadProgress {
    Pending,
    Complete(Vec<String>),
}

#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "Phase 6 owns the tested atomic-save boundary; command routing is Phase 7"
    )
)]
trait PersonalSaveOperations {
    type ParentDirectory;

    fn create_dir_all(&mut self, path: &Path) -> io::Result<()>;
    fn write_temp(&mut self, path: &Path, contents: &[u8]) -> io::Result<()>;
    fn rename(&mut self, source: &Path, target: &Path) -> io::Result<()>;
    fn open_parent(&mut self, path: &Path) -> io::Result<Self::ParentDirectory>;
    fn sync_parent(&mut self, directory: &Self::ParentDirectory) -> io::Result<()>;
}

#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "Phase 6 owns the tested atomic-save boundary; command routing is Phase 7"
    )
)]
struct FilePersonalSaveOperations;

impl PersonalSaveOperations for FilePersonalSaveOperations {
    type ParentDirectory = File;

    fn create_dir_all(&mut self, path: &Path) -> io::Result<()> {
        fs::create_dir_all(path)
    }

    fn write_temp(&mut self, path: &Path, contents: &[u8]) -> io::Result<()> {
        let mut file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(path)?;
        file.write_all(contents)?;
        file.sync_all()
    }

    fn rename(&mut self, source: &Path, target: &Path) -> io::Result<()> {
        fs::rename(source, target)
    }

    fn open_parent(&mut self, path: &Path) -> io::Result<Self::ParentDirectory> {
        File::open(path)
    }

    fn sync_parent(&mut self, directory: &Self::ParentDirectory) -> io::Result<()> {
        directory.sync_all()
    }
}

struct FilePersonalDictionaryStore {
    path: PathBuf,
    load_state: PersonalFileLoadState,
}

enum PersonalFileLoadState {
    NotStarted,
    Reading {
        reader: File,
        line: PersonalLine,
        line_number: usize,
        words: BTreeSet<String>,
    },
    Finalizing {
        remaining: std::collections::btree_set::IntoIter<String>,
        completed: Vec<String>,
    },
}

#[derive(Default)]
struct PersonalLine {
    utf8_pending: Vec<u8>,
    candidate: Vec<u8>,
    trimmed_len: usize,
    pending_whitespace: usize,
    started: bool,
    comment: bool,
    saw_byte: bool,
}

impl PersonalLine {
    fn push(&mut self, byte: u8, path: &Path) -> Result<(), String> {
        self.saw_byte = true;
        self.utf8_pending.push(byte);
        match std::str::from_utf8(&self.utf8_pending) {
            Ok(_) => self.utf8_pending.clear(),
            Err(error) if error.error_len().is_some() => {
                return Err(format!(
                    "personal dictionary '{}' is not valid UTF-8",
                    path.display()
                ));
            }
            Err(_) => {}
        }

        if self.comment {
            return Ok(());
        }
        if byte.is_ascii_whitespace() {
            if self.started {
                self.pending_whitespace += 1;
            }
            return Ok(());
        }

        if !self.started {
            self.started = true;
            self.comment = byte == b'#';
            if self.comment {
                return Ok(());
            }
        }
        self.commit_pending_whitespace();
        self.trimmed_len += 1;
        if self.candidate.len() < 65 {
            self.candidate.push(byte);
        }
        Ok(())
    }

    fn commit_pending_whitespace(&mut self) {
        self.trimmed_len = self.trimmed_len.saturating_add(self.pending_whitespace);
        let retained = self
            .pending_whitespace
            .min(65_usize.saturating_sub(self.candidate.len()));
        self.candidate.extend(std::iter::repeat_n(b' ', retained));
        self.pending_whitespace = 0;
    }

    fn finish(
        self,
        path: &Path,
        line_number: usize,
        words: &mut BTreeSet<String>,
    ) -> Result<(), String> {
        if !self.utf8_pending.is_empty() {
            return Err(format!(
                "personal dictionary '{}' is not valid UTF-8",
                path.display()
            ));
        }
        let candidate = if self.comment {
            "#"
        } else if self.trimmed_len > 64 {
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        } else {
            std::str::from_utf8(&self.candidate)
                .expect("incremental UTF-8 validation accepted the personal entry")
        };
        match normalize_dictionary_entry(candidate) {
            Ok(Some(word)) => {
                words.insert(word);
                Ok(())
            }
            Ok(None) => Ok(()),
            Err(error) => Err(format!(
                "invalid personal dictionary entry at '{}':{line_number}: {error}",
                path.display()
            )),
        }
    }
}

#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "Phase 6 owns the tested atomic-save boundary; command routing is Phase 7"
    )
)]
impl FilePersonalDictionaryStore {
    fn new(path: PathBuf) -> Self {
        Self {
            path,
            load_state: PersonalFileLoadState::NotStarted,
        }
    }

    fn save_using<O: PersonalSaveOperations>(
        &self,
        words: &[String],
        operations: &mut O,
    ) -> Result<(), String> {
        let parent = self
            .path
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        operations.create_dir_all(parent).map_err(|error| {
            format!(
                "failed to create personal dictionary directory '{}': {error}",
                parent.display()
            )
        })?;
        sync_created_directory_ancestors(operations, parent)?;

        let mut normalized = words.to_vec();
        normalized.sort_unstable();
        normalized.dedup();
        let mut contents = normalized.join("\n");
        if !contents.is_empty() {
            contents.push('\n');
        }

        let temp_path = self.path.with_extension("txt.tmp");
        operations
            .write_temp(&temp_path, contents.as_bytes())
            .map_err(|error| format!("failed to write personal dictionary: {error}"))?;
        operations
            .rename(&temp_path, &self.path)
            .map_err(|error| format!("failed to replace personal dictionary: {error}"))?;
        let parent_handle = operations
            .open_parent(parent)
            .map_err(|error| format!("failed to open personal dictionary directory: {error}"))?;
        operations
            .sync_parent(&parent_handle)
            .map_err(|error| format!("failed to sync personal dictionary directory: {error}"))
    }
}

fn sync_created_directory_ancestors<O: PersonalSaveOperations>(
    operations: &mut O,
    parent: &Path,
) -> Result<(), String> {
    let mut ancestor = containing_directory(parent).unwrap_or(parent).to_path_buf();
    loop {
        let handle = operations
            .open_parent(&ancestor)
            .map_err(|error| format!("failed to open personal dictionary directory: {error}"))?;
        operations
            .sync_parent(&handle)
            .map_err(|error| format!("failed to sync personal dictionary directory: {error}"))?;
        match containing_directory(&ancestor) {
            Some(next) if next != ancestor => ancestor = next.to_path_buf(),
            _ => break,
        }
    }
    Ok(())
}

fn containing_directory(path: &Path) -> Option<&Path> {
    path.parent().map(|parent| {
        if parent.as_os_str().is_empty() {
            Path::new(".")
        } else {
            parent
        }
    })
}

impl PersonalDictionaryStore for FilePersonalDictionaryStore {
    fn load(&mut self, max_bytes: usize) -> Result<PersonalLoadProgress, String> {
        if max_bytes == 0 {
            return Ok(PersonalLoadProgress::Pending);
        }
        if matches!(self.load_state, PersonalFileLoadState::NotStarted) {
            let reader = match File::open(&self.path) {
                Ok(reader) => reader,
                Err(error) if error.kind() == io::ErrorKind::NotFound => {
                    return Ok(PersonalLoadProgress::Complete(Vec::new()));
                }
                Err(error) => {
                    return Err(format!(
                        "failed to read personal dictionary '{}': {error}",
                        self.path.display()
                    ));
                }
            };
            self.load_state = PersonalFileLoadState::Reading {
                reader,
                line: PersonalLine::default(),
                line_number: 1,
                words: BTreeSet::new(),
            };
        }

        if let PersonalFileLoadState::Finalizing {
            remaining,
            completed,
        } = &mut self.load_state
        {
            let mut consumed = 0;
            while consumed < max_bytes {
                let Some(word) = remaining.next() else {
                    return Ok(PersonalLoadProgress::Complete(std::mem::take(completed)));
                };
                consumed += word.len().max(1);
                completed.push(word);
            }
            return Ok(PersonalLoadProgress::Pending);
        }

        let PersonalFileLoadState::Reading {
            reader,
            line,
            line_number,
            words,
        } = &mut self.load_state
        else {
            unreachable!("personal load state was initialized above")
        };
        let mut chunk = vec![0_u8; max_bytes];
        let read = match reader.read(&mut chunk) {
            Ok(read) => read,
            Err(error) => {
                return Err(format!(
                    "failed to read personal dictionary '{}': {error}",
                    self.path.display()
                ));
            }
        };
        if read == 0 {
            if line.saw_byte {
                std::mem::take(line).finish(&self.path, *line_number, words)?;
            }
            let remaining = std::mem::take(words).into_iter();
            self.load_state = PersonalFileLoadState::Finalizing {
                remaining,
                completed: Vec::new(),
            };
            return Ok(PersonalLoadProgress::Pending);
        }

        for byte in &chunk[..read] {
            if *byte == b'\n' {
                std::mem::take(line).finish(&self.path, *line_number, words)?;
                *line_number += 1;
            } else {
                line.push(*byte, &self.path)?;
            }
        }
        Ok(PersonalLoadProgress::Pending)
    }

    fn save(&mut self, words: &[String]) -> Result<(), String> {
        self.save_using(words, &mut FilePersonalSaveOperations)
    }
}

pub(crate) struct UnbuiltState {
    source: WordlistSource,
    readers: Box<dyn WordlistReaderFactory>,
    personal: Box<dyn PersonalDictionaryStore>,
}

struct ActiveWordlist {
    path: PathBuf,
    reader: Box<dyn Read>,
    contents: String,
    pending_utf8: Vec<u8>,
    bytes_read: usize,
}

pub(crate) struct WordlistLoader {
    lists: Vec<String>,
    additional: Vec<PathBuf>,
    next_path: usize,
    active: Option<ActiveWordlist>,
    readers: Box<dyn WordlistReaderFactory>,
    personal: Box<dyn PersonalDictionaryStore>,
    personal_words: Vec<String>,
}

pub(crate) struct BuildingState {
    builder: SpellEngineBuilder,
    personal: Box<dyn PersonalDictionaryStore>,
    personal_words: Vec<String>,
}

pub(crate) struct ReadyState {
    engine: SpellEngine,
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "Phase 6 persistence is consumed by the Phase 7 command surface"
        )
    )]
    personal: Box<dyn PersonalDictionaryStore>,
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "Phase 6 persistence is consumed by the Phase 7 command surface"
        )
    )]
    personal_words: Vec<String>,
}

/// One TUI-owned spell engine with closed, non-retrying lifecycle states.
pub(crate) enum SpellHost {
    Unbuilt(UnbuiltState),
    Loading(WordlistLoader),
    Building(Box<BuildingState>),
    Ready(Box<ReadyState>),
    Unavailable {
        reason: String,
        warning_pending: bool,
    },
}

impl SpellHost {
    pub(crate) fn production(source: WordlistSource, personal_path: PathBuf) -> Self {
        Self::new(
            source,
            Box::new(FileWordlistReaderFactory),
            Box::new(FilePersonalDictionaryStore::new(personal_path)),
        )
    }

    fn new(
        source: WordlistSource,
        readers: Box<dyn WordlistReaderFactory>,
        personal: Box<dyn PersonalDictionaryStore>,
    ) -> Self {
        Self::Unbuilt(UnbuiltState {
            source,
            readers,
            personal,
        })
    }

    #[cfg(test)]
    pub(crate) fn testing(words: impl Into<String>) -> Self {
        Self::new(
            WordlistSource::testing(words),
            Box::new(FileWordlistReaderFactory),
            Box::new(MemoryPersonalDictionaryStore::default()),
        )
    }

    #[cfg(test)]
    pub(crate) fn testing_unavailable(reason: impl Into<String>) -> Self {
        Self::Unavailable {
            reason: reason.into(),
            warning_pending: true,
        }
    }

    /// Advance at most one bounded loading/building unit.
    pub(crate) fn advance(&mut self, enabled: bool, max_bytes: usize) -> bool {
        if !enabled || max_bytes == 0 || matches!(self, Self::Ready(_) | Self::Unavailable { .. }) {
            return false;
        }

        let current = std::mem::replace(
            self,
            Self::Unavailable {
                reason: "spell host transition failed".to_string(),
                warning_pending: true,
            },
        );
        let (next, worked) = current.advance_owned(max_bytes.min(WORDLIST_UNIT_BYTES));
        *self = next;
        worked
    }

    fn advance_owned(self, max_bytes: usize) -> (Self, bool) {
        match self {
            Self::Unbuilt(mut state) => match state.personal.load(max_bytes) {
                Ok(PersonalLoadProgress::Pending) => (Self::Unbuilt(state), true),
                Ok(PersonalLoadProgress::Complete(personal_words)) => (
                    Self::Loading(WordlistLoader {
                        lists: vec![state.source.builtin],
                        additional: state.source.additional_dictionaries,
                        next_path: 0,
                        active: None,
                        readers: state.readers,
                        personal: state.personal,
                        personal_words,
                    }),
                    true,
                ),
                Err(reason) => (
                    Self::Unavailable {
                        reason,
                        warning_pending: true,
                    },
                    true,
                ),
            },
            Self::Loading(mut loader) => match loader.step(max_bytes) {
                LoaderStep::Pending => (Self::Loading(loader), true),
                LoaderStep::Complete => {
                    if !loader.personal_words.is_empty() {
                        let mut personal_list = loader.personal_words.join("\n");
                        personal_list.push('\n');
                        loader.lists.push(personal_list);
                    }
                    (
                        Self::Building(Box::new(BuildingState {
                            builder: SpellEngineBuilder::new(loader.lists),
                            personal: loader.personal,
                            personal_words: loader.personal_words,
                        })),
                        true,
                    )
                }
                LoaderStep::Failed(reason) => (
                    Self::Unavailable {
                        reason,
                        warning_pending: true,
                    },
                    true,
                ),
            },
            Self::Building(state) => {
                let mut state = *state;
                match state.builder.step(max_bytes) {
                    BuildProgress::Pending => (Self::Building(Box::new(state)), true),
                    BuildProgress::Complete => match state.builder.finish() {
                        Ok(engine) => (
                            Self::Ready(Box::new(ReadyState {
                                engine,
                                personal: state.personal,
                                personal_words: state.personal_words,
                            })),
                            true,
                        ),
                        Err(error) => (
                            Self::Unavailable {
                                reason: format!("spell engine build failed: {error}"),
                                warning_pending: true,
                            },
                            true,
                        ),
                    },
                }
            }
            ready @ Self::Ready(_) => (ready, false),
            unavailable @ Self::Unavailable { .. } => (unavailable, false),
        }
    }

    pub(crate) fn engine(&self) -> Option<&SpellEngine> {
        match self {
            Self::Ready(state) => Some(&state.engine),
            _ => None,
        }
    }

    /// Consume the one warning emitted when a host first becomes unavailable.
    pub(crate) fn take_unavailable_warning(&mut self) -> Option<String> {
        let Self::Unavailable {
            reason,
            warning_pending,
        } = self
        else {
            return None;
        };
        if !*warning_pending {
            return None;
        }
        *warning_pending = false;
        Some(format!("spell unavailable: {reason}"))
    }

    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "Phase 6 state messages are consumed by the Phase 7 command surface"
        )
    )]
    pub(crate) fn status_message(&self) -> Option<String> {
        match self {
            Self::Unbuilt(_) | Self::Loading(_) => Some("spell dictionaries loading".to_string()),
            Self::Building(_) => Some("spell dictionary building".to_string()),
            Self::Ready(_) => None,
            Self::Unavailable { reason, .. } => Some(format!("spell unavailable: {reason}")),
        }
    }

    #[cfg(test)]
    fn phase(&self) -> HostPhase {
        match self {
            Self::Unbuilt(_) => HostPhase::Unbuilt,
            Self::Loading(_) => HostPhase::Loading,
            Self::Building(_) => HostPhase::Building,
            Self::Ready(_) => HostPhase::Ready,
            Self::Unavailable { .. } => HostPhase::Unavailable,
        }
    }

    #[cfg(test)]
    pub(crate) fn phase_name(&self) -> &'static str {
        match self.phase() {
            HostPhase::Unbuilt => "Unbuilt",
            HostPhase::Loading => "Loading",
            HostPhase::Building => "Building",
            HostPhase::Ready => "Ready",
            HostPhase::Unavailable => "Unavailable",
        }
    }

    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "Phase 6 persistence is consumed by the Phase 7 command surface"
        )
    )]
    pub(crate) fn add_personal_word(&mut self, word: &str) -> Result<AddWordOutcome, String> {
        let Self::Ready(state) = self else {
            return Err(self
                .status_message()
                .unwrap_or_else(|| "spell dictionary is not ready".to_string()));
        };
        let Some(normalized) =
            normalize_dictionary_entry(word).map_err(|error| error.to_string())?
        else {
            return Ok(AddWordOutcome::Ignored);
        };

        match state.personal_words.binary_search(&normalized) {
            Ok(_) => state
                .engine
                .add_word(&normalized)
                .map_err(|e| e.to_string()),
            Err(index) => {
                let mut updated = state.personal_words.clone();
                updated.insert(index, normalized.clone());
                state.personal.save(&updated)?;
                let outcome = state
                    .engine
                    .add_word(&normalized)
                    .map_err(|e| e.to_string())?;
                state.personal_words = updated;
                Ok(outcome)
            }
        }
    }
}

enum LoaderStep {
    Pending,
    Complete,
    Failed(String),
}

impl WordlistLoader {
    fn step(&mut self, max_bytes: usize) -> LoaderStep {
        if self.active.is_none() {
            let Some(path) = self.additional.get(self.next_path).cloned() else {
                return LoaderStep::Complete;
            };
            let opened = match self.readers.open(&path) {
                Ok(opened) => opened,
                Err(error) => {
                    return LoaderStep::Failed(format!(
                        "cannot open additional dictionary '{}': {error}",
                        path.display()
                    ))
                }
            };
            if opened.advertised_len.is_some_and(|length| {
                length > u64::try_from(MAX_ADDITIONAL_DICTIONARY_BYTES).unwrap()
            }) {
                return LoaderStep::Failed(format!(
                    "additional dictionary '{}' exceeds 16 MiB",
                    path.display()
                ));
            }
            self.active = Some(ActiveWordlist {
                path,
                reader: opened.reader,
                contents: String::new(),
                pending_utf8: Vec::new(),
                bytes_read: 0,
            });
        }

        let active = self.active.as_mut().expect("active reader was just opened");
        let remaining_capacity = MAX_ADDITIONAL_DICTIONARY_BYTES
            .saturating_add(1)
            .saturating_sub(active.bytes_read);
        let read_len = max_bytes.min(remaining_capacity).max(1);
        let mut chunk = vec![0_u8; read_len];
        match active.reader.read(&mut chunk) {
            Ok(0) => {
                let active = self.active.take().expect("completed reader must exist");
                let path = active.path;
                if active.bytes_read > MAX_ADDITIONAL_DICTIONARY_BYTES {
                    return LoaderStep::Failed(format!(
                        "additional dictionary '{}' exceeds 16 MiB",
                        path.display()
                    ));
                }
                if !active.pending_utf8.is_empty() {
                    return LoaderStep::Failed(format!(
                        "additional dictionary '{}' is not valid UTF-8",
                        path.display()
                    ));
                }
                self.lists.push(active.contents);
                self.next_path += 1;
                LoaderStep::Pending
            }
            Ok(read) => {
                active.bytes_read += read;
                if active.bytes_read > MAX_ADDITIONAL_DICTIONARY_BYTES {
                    return LoaderStep::Failed(format!(
                        "additional dictionary '{}' exceeds 16 MiB",
                        active.path.display()
                    ));
                }
                active.pending_utf8.extend_from_slice(&chunk[..read]);
                match std::str::from_utf8(&active.pending_utf8) {
                    Ok(valid) => {
                        active.contents.push_str(valid);
                        active.pending_utf8.clear();
                        LoaderStep::Pending
                    }
                    Err(error) if error.error_len().is_some() => LoaderStep::Failed(format!(
                        "additional dictionary '{}' is not valid UTF-8",
                        active.path.display()
                    )),
                    Err(error) => {
                        let valid_up_to = error.valid_up_to();
                        let remainder = active.pending_utf8.split_off(valid_up_to);
                        let valid = std::str::from_utf8(&active.pending_utf8)
                            .expect("UTF-8 validator reported a valid prefix");
                        active.contents.push_str(valid);
                        active.pending_utf8 = remainder;
                        LoaderStep::Pending
                    }
                }
            }
            Err(error) => LoaderStep::Failed(format!(
                "failed reading additional dictionary '{}': {error}",
                active.path.display()
            )),
        }
    }
}

#[cfg(test)]
#[derive(Default)]
struct MemoryPersonalDictionaryStore {
    words: Vec<String>,
}

#[cfg(test)]
impl PersonalDictionaryStore for MemoryPersonalDictionaryStore {
    fn load(&mut self, _max_bytes: usize) -> Result<PersonalLoadProgress, String> {
        Ok(PersonalLoadProgress::Complete(self.words.clone()))
    }

    fn save(&mut self, words: &[String]) -> Result<(), String> {
        self.words = words.to_vec();
        Ok(())
    }
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum HostPhase {
    Unbuilt,
    Loading,
    Building,
    Ready,
    Unavailable,
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::collections::VecDeque;
    use std::rc::Rc;

    use super::*;

    struct ScriptedReader {
        steps: VecDeque<io::Result<Vec<u8>>>,
        remainder: Vec<u8>,
    }

    impl ScriptedReader {
        fn new(steps: Vec<io::Result<Vec<u8>>>) -> Self {
            Self {
                steps: steps.into(),
                remainder: Vec::new(),
            }
        }
    }

    impl Read for ScriptedReader {
        fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
            if self.remainder.is_empty() {
                match self.steps.pop_front() {
                    Some(Ok(bytes)) => self.remainder = bytes,
                    Some(Err(error)) => return Err(error),
                    None => return Ok(0),
                }
            }
            let count = output.len().min(self.remainder.len());
            output[..count].copy_from_slice(&self.remainder[..count]);
            self.remainder.drain(..count);
            Ok(count)
        }
    }

    struct RepeatingReader {
        remaining: usize,
    }

    impl Read for RepeatingReader {
        fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
            let count = output.len().min(self.remaining);
            output[..count].fill(b'a');
            self.remaining -= count;
            Ok(count)
        }
    }

    struct ScriptedFactory {
        opened: Rc<RefCell<Vec<PathBuf>>>,
        scripts: VecDeque<io::Result<OpenedWordlist>>,
    }

    impl WordlistReaderFactory for ScriptedFactory {
        fn open(&mut self, path: &Path) -> io::Result<OpenedWordlist> {
            self.opened.borrow_mut().push(path.to_path_buf());
            self.scripts
                .pop_front()
                .expect("unexpected scripted dictionary open")
        }
    }

    #[derive(Default)]
    struct SharedPersonalState {
        words: Vec<String>,
        fail_load: bool,
        fail_save: bool,
        loads: usize,
        saves: usize,
    }

    struct SharedPersonalStore(Rc<RefCell<SharedPersonalState>>);

    impl PersonalDictionaryStore for SharedPersonalStore {
        fn load(&mut self, _max_bytes: usize) -> Result<PersonalLoadProgress, String> {
            let mut state = self.0.borrow_mut();
            state.loads += 1;
            if state.fail_load {
                Err("scripted personal load failure".to_string())
            } else {
                Ok(PersonalLoadProgress::Complete(state.words.clone()))
            }
        }

        fn save(&mut self, words: &[String]) -> Result<(), String> {
            let mut state = self.0.borrow_mut();
            state.saves += 1;
            if state.fail_save {
                Err("scripted personal save failure".to_string())
            } else {
                state.words = words.to_vec();
                Ok(())
            }
        }
    }

    fn opened(contents: &[u8]) -> io::Result<OpenedWordlist> {
        Ok(OpenedWordlist {
            reader: Box::new(ScriptedReader::new(vec![Ok(contents.to_vec())])),
            advertised_len: Some(contents.len() as u64),
        })
    }

    fn host_with(
        source: WordlistSource,
        scripts: Vec<io::Result<OpenedWordlist>>,
        personal: Rc<RefCell<SharedPersonalState>>,
        opened_paths: Rc<RefCell<Vec<PathBuf>>>,
    ) -> SpellHost {
        SpellHost::new(
            source,
            Box::new(ScriptedFactory {
                opened: opened_paths,
                scripts: scripts.into(),
            }),
            Box::new(SharedPersonalStore(personal)),
        )
    }

    fn drain(host: &mut SpellHost) {
        for _ in 0..100_000 {
            if host.phase() == HostPhase::Ready || host.phase() == HostPhase::Unavailable {
                return;
            }
            assert!(host.advance(true, WORDLIST_UNIT_BYTES));
        }
        panic!("spell host did not reach a terminal state");
    }

    fn drain_personal_store(
        store: &mut dyn PersonalDictionaryStore,
    ) -> Result<Vec<String>, String> {
        for _ in 0..100_000 {
            match store.load(WORDLIST_UNIT_BYTES)? {
                PersonalLoadProgress::Pending => {}
                PersonalLoadProgress::Complete(words) => return Ok(words),
            }
        }
        panic!("personal dictionary load did not complete");
    }

    #[test]
    fn source_resolution_preserves_order_and_resolves_relative_paths() {
        let config = SpellConfig {
            enabled: true,
            language: "en_CA".to_string(),
            additional_dictionaries: vec![
                PathBuf::from("team.txt"),
                PathBuf::from("/opt/shared.txt"),
                PathBuf::from("later.txt"),
            ],
        };
        let resolved = resolve_wordlist_source(&config, Path::new("/tmp/config/config.toml"));
        assert!(resolved.warning.is_none());
        assert!(resolved.source.builtin.contains("colour"));
        assert_eq!(
            resolved.source.additional_dictionaries,
            [
                PathBuf::from("/tmp/config/team.txt"),
                PathBuf::from("/opt/shared.txt"),
                PathBuf::from("/tmp/config/later.txt"),
            ]
        );
    }

    #[test]
    fn language_resolver_spot_words_and_invalid_fallback() {
        for (language, present, absent) in [
            ("en_US", "color", "colour"),
            ("en_CA", "colour", "color"),
            ("en_AU", "colour", "color"),
        ] {
            let config = SpellConfig {
                language: language.to_string(),
                ..SpellConfig::default()
            };
            let resolved = resolve_wordlist_source(&config, Path::new("config.toml"));
            let entries: Vec<_> = resolved.source.builtin.lines().collect();
            assert!(entries.contains(&present), "{language} lacks {present}");
            assert!(
                !entries.contains(&absent),
                "{language} unexpectedly has {absent}"
            );
        }

        let config = SpellConfig {
            language: "xx_YY".to_string(),
            ..SpellConfig::default()
        };
        let resolved = resolve_wordlist_source(&config, Path::new("config.toml"));
        assert!(resolved.source.builtin.lines().any(|line| line == "color"));
        assert_eq!(
            resolved.warning.as_deref(),
            Some("invalid spell language 'xx_YY'; falling back to en_US")
        );
    }

    #[test]
    fn disabled_host_does_not_advance_unbuilt_loading_or_building() {
        let personal = Rc::new(RefCell::new(SharedPersonalState::default()));
        let opened_paths = Rc::new(RefCell::new(Vec::new()));
        let mut host = host_with(
            WordlistSource::with_additional("known\n", vec![PathBuf::from("extra")]),
            vec![opened(b"added\n")],
            personal,
            opened_paths.clone(),
        );
        assert!(!host.advance(false, WORDLIST_UNIT_BYTES));
        assert_eq!(host.phase(), HostPhase::Unbuilt);
        assert!(opened_paths.borrow().is_empty());

        assert!(host.advance(true, WORDLIST_UNIT_BYTES));
        assert_eq!(host.phase(), HostPhase::Loading);
        assert!(!host.advance(false, WORDLIST_UNIT_BYTES));
        assert_eq!(host.phase(), HostPhase::Loading);
        assert!(opened_paths.borrow().is_empty());

        for _ in 0..1_000 {
            if host.phase() != HostPhase::Loading {
                break;
            }
            assert!(host.advance(true, WORDLIST_UNIT_BYTES));
        }
        assert_eq!(host.phase(), HostPhase::Building);
        assert!(!host.advance(false, WORDLIST_UNIT_BYTES));
        assert_eq!(host.phase(), HostPhase::Building);
    }

    #[test]
    fn additional_dictionaries_use_short_reads_in_declaration_order_and_deduplicate() {
        let opened_paths = Rc::new(RefCell::new(Vec::new()));
        let personal = Rc::new(RefCell::new(SharedPersonalState::default()));
        let first = OpenedWordlist {
            reader: Box::new(ScriptedReader::new(vec![
                Ok(b"du".to_vec()),
                Ok(b"pe\nfirst\n".to_vec()),
            ])),
            advertised_len: None,
        };
        let second = OpenedWordlist {
            reader: Box::new(ScriptedReader::new(vec![Ok(b"dupe\nsecond\n".to_vec())])),
            advertised_len: None,
        };
        let mut host = host_with(
            WordlistSource::with_additional(
                "base\ndupe\n",
                vec![PathBuf::from("first"), PathBuf::from("second")],
            ),
            vec![Ok(first), Ok(second)],
            personal,
            opened_paths.clone(),
        );
        drain(&mut host);
        assert_eq!(
            *opened_paths.borrow(),
            [PathBuf::from("first"), PathBuf::from("second")]
        );
        let engine = host.engine().unwrap();
        assert_eq!(engine.word_count(), 4);
        for word in ["base", "dupe", "first", "second"] {
            assert!(engine.check(word));
        }
    }

    #[test]
    fn additional_dictionary_utf8_validation_spans_short_read_boundaries() {
        let opened_paths = Rc::new(RefCell::new(Vec::new()));
        let personal = Rc::new(RefCell::new(SharedPersonalState::default()));
        let split_utf8 = OpenedWordlist {
            reader: Box::new(ScriptedReader::new(vec![
                Ok(b"caf\xc3".to_vec()),
                Ok(b"\xa9\nvalid\n".to_vec()),
            ])),
            advertised_len: None,
        };
        let mut host = host_with(
            WordlistSource::with_additional("base\n", vec![PathBuf::from("split-utf8")]),
            vec![Ok(split_utf8)],
            personal,
            opened_paths,
        );

        drain(&mut host);
        assert_eq!(host.phase(), HostPhase::Ready);
        assert!(host.engine().unwrap().check("valid"));
    }

    #[test]
    fn every_additional_dictionary_failure_is_terminal_and_non_retrying() {
        let cases: Vec<(&str, io::Result<OpenedWordlist>, &str)> = vec![
            (
                "missing",
                Err(io::Error::new(io::ErrorKind::NotFound, "missing")),
                "cannot open",
            ),
            (
                "unreadable",
                Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "permission denied",
                )),
                "cannot open",
            ),
            (
                "read",
                Ok(OpenedWordlist {
                    reader: Box::new(ScriptedReader::new(vec![Err(io::Error::other(
                        "read failed",
                    ))])),
                    advertised_len: None,
                }),
                "failed reading",
            ),
            ("utf8", opened(&[0xff]), "not valid UTF-8"),
            (
                "size",
                Ok(OpenedWordlist {
                    reader: Box::new(io::empty()),
                    advertised_len: Some(MAX_ADDITIONAL_DICTIONARY_BYTES as u64 + 1),
                }),
                "exceeds 16 MiB",
            ),
        ];

        for (name, script, expected) in cases {
            let opened_paths = Rc::new(RefCell::new(Vec::new()));
            let personal = Rc::new(RefCell::new(SharedPersonalState::default()));
            let mut host = host_with(
                WordlistSource::with_additional("base\n", vec![PathBuf::from(name)]),
                vec![script],
                personal,
                opened_paths.clone(),
            );
            drain(&mut host);
            assert_eq!(host.phase(), HostPhase::Unavailable);
            assert!(host.status_message().unwrap().contains(expected));
            let opens = opened_paths.borrow().len();
            assert!(!host.advance(true, WORDLIST_UNIT_BYTES));
            assert_eq!(opened_paths.borrow().len(), opens, "{name} retried");
        }
    }

    #[test]
    fn streamed_additional_dictionary_enforces_exact_size_boundary_without_metadata() {
        for (size, expected_phase) in [
            (MAX_ADDITIONAL_DICTIONARY_BYTES, HostPhase::Ready),
            (MAX_ADDITIONAL_DICTIONARY_BYTES + 1, HostPhase::Unavailable),
        ] {
            let opened_paths = Rc::new(RefCell::new(Vec::new()));
            let personal = Rc::new(RefCell::new(SharedPersonalState::default()));
            let streamed = OpenedWordlist {
                reader: Box::new(RepeatingReader { remaining: size }),
                advertised_len: None,
            };
            let mut host = host_with(
                WordlistSource::with_additional(
                    "base\n",
                    vec![PathBuf::from(format!("streamed-{size}"))],
                ),
                vec![Ok(streamed)],
                personal,
                opened_paths.clone(),
            );

            drain(&mut host);
            assert_eq!(host.phase(), expected_phase, "streamed size {size}");
            assert_eq!(opened_paths.borrow().len(), 1);
            if expected_phase == HostPhase::Unavailable {
                assert!(host.status_message().unwrap().contains("exceeds 16 MiB"));
                assert!(!host.advance(true, WORDLIST_UNIT_BYTES));
                assert_eq!(opened_paths.borrow().len(), 1);
            }
        }
    }

    #[test]
    fn personal_dictionary_missing_malformed_and_roundtrip_behave_exactly() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("dictionary.txt");
        let mut store = FilePersonalDictionaryStore::new(path.clone());
        assert_eq!(
            drain_personal_store(&mut store).unwrap(),
            Vec::<String>::new()
        );

        fs::write(&path, b"valid\nnot-valid!\n").unwrap();
        let mut store = FilePersonalDictionaryStore::new(path.clone());
        assert!(drain_personal_store(&mut store)
            .unwrap_err()
            .contains(":2:"));

        fs::write(&path, [0xff]).unwrap();
        let mut store = FilePersonalDictionaryStore::new(path.clone());
        assert!(drain_personal_store(&mut store)
            .unwrap_err()
            .contains("not valid UTF-8"));

        let mut store = FilePersonalDictionaryStore::new(path.clone());
        store
            .save(&[
                "zebra".to_string(),
                "apple".to_string(),
                "apple".to_string(),
            ])
            .unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "apple\nzebra\n");
        let mut store = FilePersonalDictionaryStore::new(path);
        assert_eq!(
            drain_personal_store(&mut store).unwrap(),
            ["apple", "zebra"]
        );
    }

    #[test]
    fn large_personal_dictionary_load_remains_resumable_in_four_kib_units() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("dictionary.txt");
        let contents = "word\n".repeat((1024 * 1024) / 5);
        fs::write(&path, contents).unwrap();
        let mut host = SpellHost::new(
            WordlistSource::testing("known\n"),
            Box::new(FileWordlistReaderFactory),
            Box::new(FilePersonalDictionaryStore::new(path)),
        );

        assert!(host.advance(true, WORDLIST_UNIT_BYTES));
        assert_eq!(
            host.phase(),
            HostPhase::Unbuilt,
            "one unit must not synchronously consume the personal dictionary"
        );
        drain(&mut host);
        assert!(host.engine().unwrap().check("word"));
    }

    #[test]
    fn personal_load_failure_is_terminal_and_never_retried() {
        let shared = Rc::new(RefCell::new(SharedPersonalState {
            fail_load: true,
            ..SharedPersonalState::default()
        }));
        let mut host = host_with(
            WordlistSource::testing("known\n"),
            Vec::new(),
            shared.clone(),
            Rc::new(RefCell::new(Vec::new())),
        );
        assert!(host.advance(true, WORDLIST_UNIT_BYTES));
        assert_eq!(host.phase(), HostPhase::Unavailable);
        assert_eq!(shared.borrow().loads, 1);
        assert!(!host.advance(true, WORDLIST_UNIT_BYTES));
        assert_eq!(shared.borrow().loads, 1);
    }

    #[test]
    fn personal_add_is_disk_first_duplicate_stable_and_survives_new_host() {
        let shared = Rc::new(RefCell::new(SharedPersonalState::default()));
        let mut first = host_with(
            WordlistSource::testing("known\n"),
            Vec::new(),
            shared.clone(),
            Rc::new(RefCell::new(Vec::new())),
        );
        drain(&mut first);
        assert_eq!(
            first.add_personal_word(" Added ").unwrap(),
            AddWordOutcome::Inserted {
                normalized: "added".to_string()
            }
        );
        assert!(first.engine().unwrap().check("added"));
        assert_eq!(shared.borrow().words, ["added"]);
        assert_eq!(shared.borrow().saves, 1);

        assert_eq!(
            first.add_personal_word("ADDED").unwrap(),
            AddWordOutcome::AlreadyPresent {
                normalized: "added".to_string()
            }
        );
        assert_eq!(shared.borrow().saves, 1);

        let mut reloaded = host_with(
            WordlistSource::testing("known\n"),
            Vec::new(),
            shared,
            Rc::new(RefCell::new(Vec::new())),
        );
        drain(&mut reloaded);
        assert!(reloaded.engine().unwrap().check("added"));
    }

    #[test]
    fn file_backed_personal_add_survives_a_fresh_host_reload() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("dictionary.txt");
        let make_host = || {
            SpellHost::new(
                WordlistSource::testing("known\n"),
                Box::new(FileWordlistReaderFactory),
                Box::new(FilePersonalDictionaryStore::new(path.clone())),
            )
        };

        let mut first = make_host();
        drain(&mut first);
        first.add_personal_word(" Added ").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "added\n");

        let mut reloaded = make_host();
        drain(&mut reloaded);
        assert!(reloaded.engine().unwrap().check("added"));
    }

    #[test]
    fn personal_atomic_failure_never_mutates_engine() {
        let shared = Rc::new(RefCell::new(SharedPersonalState {
            fail_save: true,
            ..SharedPersonalState::default()
        }));
        let mut host = host_with(
            WordlistSource::testing("known\n"),
            Vec::new(),
            shared.clone(),
            Rc::new(RefCell::new(Vec::new())),
        );
        drain(&mut host);
        let generation = host.engine().unwrap().generation();
        assert!(host.add_personal_word("added").is_err());
        assert!(!host.engine().unwrap().check("added"));
        assert_eq!(host.engine().unwrap().generation(), generation);
        assert!(shared.borrow().words.is_empty());
    }

    #[test]
    fn host_state_messages_distinguish_loading_building_ready_and_unavailable() {
        let mut host = SpellHost::testing("known\n");
        assert_eq!(
            host.status_message().as_deref(),
            Some("spell dictionaries loading")
        );
        host.advance(true, WORDLIST_UNIT_BYTES);
        host.advance(true, WORDLIST_UNIT_BYTES);
        assert_eq!(host.phase(), HostPhase::Building);
        assert_eq!(
            host.status_message().as_deref(),
            Some("spell dictionary building")
        );
        drain(&mut host);
        assert!(host.status_message().is_none());

        let mut unavailable = host_with(
            WordlistSource::with_additional("known\n", vec![PathBuf::from("missing")]),
            vec![Err(io::Error::new(io::ErrorKind::NotFound, "gone"))],
            Rc::new(RefCell::new(SharedPersonalState::default())),
            Rc::new(RefCell::new(Vec::new())),
        );
        drain(&mut unavailable);
        assert!(unavailable
            .status_message()
            .unwrap()
            .starts_with("spell unavailable:"));
    }

    #[test]
    fn personal_atomic_save_runs_exact_sequence_and_stops_at_every_failure() {
        #[derive(Clone, Debug, Eq, PartialEq)]
        enum SaveEvent {
            CreateDirectory(PathBuf),
            WriteTemp(PathBuf, Vec<u8>),
            Rename(PathBuf, PathBuf),
            OpenDirectory(PathBuf),
            SyncDirectory(PathBuf),
        }

        #[derive(Default)]
        struct RecordingOperations {
            events: Vec<SaveEvent>,
            fail_at: Option<usize>,
        }
        impl RecordingOperations {
            fn record(&mut self, event: SaveEvent) -> io::Result<()> {
                let index = self.events.len();
                self.events.push(event);
                if self.fail_at == Some(index) {
                    Err(io::Error::other(format!("scripted failure at {index}")))
                } else {
                    Ok(())
                }
            }
        }
        impl PersonalSaveOperations for RecordingOperations {
            type ParentDirectory = PathBuf;
            fn create_dir_all(&mut self, path: &Path) -> io::Result<()> {
                self.record(SaveEvent::CreateDirectory(path.to_path_buf()))
            }
            fn write_temp(&mut self, path: &Path, contents: &[u8]) -> io::Result<()> {
                self.record(SaveEvent::WriteTemp(path.to_path_buf(), contents.to_vec()))
            }
            fn rename(&mut self, source: &Path, target: &Path) -> io::Result<()> {
                self.record(SaveEvent::Rename(
                    source.to_path_buf(),
                    target.to_path_buf(),
                ))
            }
            fn open_parent(&mut self, path: &Path) -> io::Result<Self::ParentDirectory> {
                self.record(SaveEvent::OpenDirectory(path.to_path_buf()))?;
                Ok(path.to_path_buf())
            }
            fn sync_parent(&mut self, directory: &Self::ParentDirectory) -> io::Result<()> {
                self.record(SaveEvent::SyncDirectory(directory.clone()))
            }
        }

        let store = FilePersonalDictionaryStore::new(PathBuf::from("cfg/dictionary.txt"));
        let mut operations = RecordingOperations::default();
        store
            .save_using(&["zebra".to_string(), "apple".to_string()], &mut operations)
            .unwrap();
        let expected = vec![
            SaveEvent::CreateDirectory(PathBuf::from("cfg")),
            SaveEvent::OpenDirectory(PathBuf::from(".")),
            SaveEvent::SyncDirectory(PathBuf::from(".")),
            SaveEvent::WriteTemp(
                PathBuf::from("cfg/dictionary.txt.tmp"),
                b"apple\nzebra\n".to_vec(),
            ),
            SaveEvent::Rename(
                PathBuf::from("cfg/dictionary.txt.tmp"),
                PathBuf::from("cfg/dictionary.txt"),
            ),
            SaveEvent::OpenDirectory(PathBuf::from("cfg")),
            SaveEvent::SyncDirectory(PathBuf::from("cfg")),
        ];
        assert_eq!(operations.events, expected);

        for fail_at in 0..expected.len() {
            let mut operations = RecordingOperations {
                events: Vec::new(),
                fail_at: Some(fail_at),
            };
            let error = store
                .save_using(&["apple".to_string(), "zebra".to_string()], &mut operations)
                .unwrap_err();
            assert!(
                error.contains("scripted failure"),
                "failure {fail_at}: {error}"
            );
            assert_eq!(operations.events, expected[..=fail_at]);
        }
    }
}
