//! Configuration — `$XDG_CONFIG_HOME/oom-edit/config.toml` (fallback
//! `~/.config/oom-edit/config.toml`).
//!
//! `[theme]`, `[editor]`, `[clipboard]`, and `[spell]` sections.
//! Load-with-defaults on missing/partial config. Atomic write on change.
//! Never fail startup on malformed config (warn to stderr, use defaults).

use std::io::{self, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize, Serializer};

pub(crate) const DEFAULT_WRAP_WIDTH: u16 = 100;

/// A configuration store that performs no I/O.
#[cfg(test)]
#[derive(Debug, Default)]
pub(crate) struct DisabledConfigStore;

#[cfg(test)]
impl ThemePersistenceSink for DisabledConfigStore {
    fn persist_theme(&mut self, _slot: ThemeSlot, _name: &str) -> Result<(), ConfigPersistError> {
        Ok(())
    }
}

#[cfg(test)]
trait ConfigSaveOperations {
    type ParentDirectory;

    fn create_dir_all(&mut self, path: &Path) -> io::Result<()>;
    fn write(&mut self, path: &Path, contents: &[u8]) -> io::Result<()>;
    fn sync_file(&mut self, path: &Path) -> io::Result<()>;
    fn rename(&mut self, source: &Path, target: &Path) -> io::Result<()>;
    fn open_parent_read_only(&mut self, parent: &Path) -> io::Result<Self::ParentDirectory>;
    fn sync_parent(&mut self, parent: &Self::ParentDirectory) -> io::Result<()>;
}

#[cfg(test)]
struct FileSystemConfigSave;

#[cfg(test)]
impl ConfigSaveOperations for FileSystemConfigSave {
    type ParentDirectory = std::fs::File;

    fn create_dir_all(&mut self, path: &Path) -> io::Result<()> {
        std::fs::create_dir_all(path)
    }

    fn write(&mut self, path: &Path, contents: &[u8]) -> io::Result<()> {
        std::fs::write(path, contents)
    }

    fn sync_file(&mut self, path: &Path) -> io::Result<()> {
        std::fs::File::open(path)?.sync_all()
    }

    fn rename(&mut self, source: &Path, target: &Path) -> io::Result<()> {
        std::fs::rename(source, target)
    }

    fn open_parent_read_only(&mut self, parent: &Path) -> io::Result<Self::ParentDirectory> {
        std::fs::File::open(parent)
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

#[cfg(test)]
fn containing_directory(path: &Path) -> Option<&Path> {
    path.parent().map(|parent| {
        if parent.as_os_str().is_empty() {
            Path::new(".")
        } else {
            parent
        }
    })
}

#[cfg(test)]
fn ensure_parent_directory<O: ConfigSaveOperations>(
    operations: &mut O,
    parent: &Path,
) -> Result<(), String> {
    operations.create_dir_all(parent).map_err(|e| {
        format!(
            "failed to create config directory '{}': {e}",
            parent.display()
        )
    })?;

    // Re-establish directory-entry durability on every attempt so a retry
    // also completes any ancestor sync that failed after an earlier mkdir.
    let mut ancestor = containing_directory(parent).unwrap_or(parent).to_path_buf();
    loop {
        let directory = operations
            .open_parent_read_only(&ancestor)
            .map_err(|e| format!("failed to open config directory: {e}"))?;
        operations
            .sync_parent(&directory)
            .map_err(|e| format!("failed to sync config directory: {e}"))?;

        match containing_directory(&ancestor) {
            Some(next) if next != ancestor.as_path() => {
                ancestor = next.to_path_buf();
            }
            _ => break,
        }
    }

    Ok(())
}

#[cfg(test)]
fn atomic_save_config<O: ConfigSaveOperations>(
    operations: &mut O,
    path: &Path,
    contents: &[u8],
) -> Result<(), String> {
    let temp_path = path.with_extension("toml.tmp");
    operations
        .write(&temp_path, contents)
        .map_err(|e| format!("failed to write config file: {e}"))?;
    operations
        .sync_file(&temp_path)
        .map_err(|e| format!("failed to sync config file: {e}"))?;
    operations
        .rename(&temp_path, path)
        .map_err(|e| format!("failed to rename config file: {e}"))?;

    let parent = parent_directory(path);
    let parent_directory = operations
        .open_parent_read_only(parent)
        .map_err(|e| format!("failed to open config directory: {e}"))?;
    operations
        .sync_parent(&parent_directory)
        .map_err(|e| format!("failed to sync config directory: {e}"))
}

/// Config file path: `$XDG_CONFIG_HOME/oom-edit/config.toml`, falling back to
/// `~/.config/oom-edit/config.toml`.
pub fn config_path() -> PathBuf {
    if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME") {
        let p = PathBuf::from(xdg);
        return p.join("oom-edit").join("config.toml");
    }
    // Fallback: ~/.config/oom-edit/config.toml
    if let Some(home) = dirs_home() {
        return home.join(".config").join("oom-edit").join("config.toml");
    }
    PathBuf::from("config.toml")
}

/// Get the user's home directory.
fn dirs_home() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

/// Configuration loaded from disk (or defaults if missing/malformed).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Config {
    /// Whether rendered Normal, Select, and Command use hybrid-relative numbers.
    #[serde(default)]
    pub relative_line_numbers: bool,
    #[serde(default)]
    /// Theme appearance and name configuration.
    pub theme: ThemeConfig,
    #[serde(default)]
    /// Editing presentation configuration.
    pub editor: EditorConfig,
    #[serde(default)]
    /// Clipboard representation configuration.
    pub clipboard: ClipboardConfig,
    #[serde(default)]
    /// Spelling resource and runtime configuration.
    pub spell: SpellConfig,
}

/// Clipboard representation written to the host system clipboard.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ClipboardCopyFormat {
    /// Preserve exact Markdown source syntax.
    #[default]
    Markdown,
    /// Write rendered text with Markdown syntax removed.
    PlainText,
}

/// The `[clipboard]` section of the config.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClipboardConfig {
    /// Representation used for outgoing system-clipboard writes.
    #[serde(default)]
    pub copy_format: ClipboardCopyFormat,
}

/// The `[spell]` section of the config.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpellConfig {
    /// Whether newly-created sessions start with spell checking enabled.
    #[serde(default = "default_spell_enabled")]
    pub enabled: bool,
    /// Built-in English dictionary dialect (`en_US`, `en_CA`, or `en_AU`).
    #[serde(default = "default_spell_language")]
    pub language: String,
    /// Additional UTF-8 plain-wordlist dictionaries, in declaration order.
    #[serde(default)]
    pub additional_dictionaries: Vec<PathBuf>,
}

impl Default for SpellConfig {
    fn default() -> Self {
        Self {
            enabled: default_spell_enabled(),
            language: default_spell_language(),
            additional_dictionaries: Vec::new(),
        }
    }
}

fn default_spell_enabled() -> bool {
    true
}

fn default_spell_language() -> String {
    "en_US".to_string()
}

/// The `[editor]` section of the config.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EditorConfig {
    /// Whether long source lines wrap. Defaults to `true`.
    #[serde(default = "default_wrap")]
    pub wrap: bool,
    /// Maximum display columns used to lay out wrapped source and prose.
    #[serde(default = "default_wrap_width")]
    pub wrap_width: u16,
    /// Whether the terminal cursor shape follows the active mode.
    #[serde(default = "default_cursor_shapes")]
    pub cursor_shapes: bool,
}

impl Default for EditorConfig {
    fn default() -> Self {
        Self {
            wrap: default_wrap(),
            wrap_width: default_wrap_width(),
            cursor_shapes: default_cursor_shapes(),
        }
    }
}

fn default_wrap() -> bool {
    true
}

fn default_wrap_width() -> u16 {
    DEFAULT_WRAP_WIDTH
}

fn default_cursor_shapes() -> bool {
    true
}

/// The `[theme]` section of the config.
#[derive(Debug, Clone)]
pub struct ThemeConfig {
    /// Display mode override: `"light"` or `"dark"`. If `None`, the editor
    /// uses the `COLORFGBG` heuristic (or defaults to dark).
    pub mode: Option<String>,
    /// Theme name for dark mode. Defaults to `"default-dark"`.
    /// Use [`Self::set_dark`] to mark an explicit selection of that default.
    pub dark: String,
    /// Theme name for light mode. Defaults to `"default-light"`.
    /// Use [`Self::set_light`] to mark an explicit selection of that default.
    pub light: String,
    explicit_dark: bool,
    explicit_light: bool,
}

impl Default for ThemeConfig {
    fn default() -> Self {
        Self {
            mode: None,
            dark: default_dark(),
            light: default_light(),
            explicit_dark: false,
            explicit_light: false,
        }
    }
}

impl ThemeConfig {
    /// Set an explicitly authored dark-mode theme slot.
    pub fn set_dark(&mut self, name: String) {
        self.dark = name;
        self.explicit_dark = true;
    }

    /// Set an explicitly authored light-mode theme slot.
    pub fn set_light(&mut self, name: String) {
        self.light = name;
        self.explicit_light = true;
    }

    /// Whether the dark slot has been explicitly selected by a host or file.
    pub fn dark_is_explicit(&self) -> bool {
        self.explicit_dark || self.dark != default_dark()
    }

    /// Whether the light slot has been explicitly selected by a host or file.
    pub fn light_is_explicit(&self) -> bool {
        self.explicit_light || self.light != default_light()
    }
}

impl PartialEq for ThemeConfig {
    fn eq(&self, other: &Self) -> bool {
        self.mode == other.mode
            && self.dark == other.dark
            && self.light == other.light
            && self.dark_is_explicit() == other.dark_is_explicit()
            && self.light_is_explicit() == other.light_is_explicit()
    }
}

impl Eq for ThemeConfig {}

#[derive(Deserialize)]
struct ThemeConfigWire {
    #[serde(default)]
    mode: Option<String>,
    dark: Option<String>,
    light: Option<String>,
}

impl<'de> Deserialize<'de> for ThemeConfig {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let wire = ThemeConfigWire::deserialize(deserializer)?;
        Ok(Self {
            mode: wire.mode,
            explicit_dark: wire.dark.is_some(),
            explicit_light: wire.light.is_some(),
            dark: wire.dark.unwrap_or_else(default_dark),
            light: wire.light.unwrap_or_else(default_light),
        })
    }
}

impl Serialize for ThemeConfig {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        #[derive(Serialize)]
        struct Wire<'a> {
            #[serde(skip_serializing_if = "Option::is_none")]
            mode: Option<&'a str>,
            #[serde(skip_serializing_if = "Option::is_none")]
            dark: Option<&'a str>,
            #[serde(skip_serializing_if = "Option::is_none")]
            light: Option<&'a str>,
        }
        Wire {
            mode: self.mode.as_deref(),
            dark: self.dark_is_explicit().then_some(self.dark.as_str()),
            light: self.light_is_explicit().then_some(self.light.as_str()),
        }
        .serialize(serializer)
    }
}

fn default_dark() -> String {
    "default-dark".to_string()
}

fn default_light() -> String {
    "default-light".to_string()
}

impl Config {
    /// Load the standalone configuration, using defaults if it is unavailable.
    pub(crate) fn load_production() -> Self {
        Self::load_from_path(&config_path())
    }

    pub(crate) fn load_from_path(path: &Path) -> Self {
        match std::fs::read_to_string(path) {
            Ok(contents) => match toml::from_str::<Self>(&contents) {
                Ok(config) => config,
                Err(error) => {
                    eprintln!("oom-edit: config parse error: {error}, using defaults");
                    Self::default()
                }
            },
            Err(error) => {
                if error.kind() != io::ErrorKind::NotFound {
                    eprintln!("oom-edit: config read error: {error}, using defaults");
                }
                Self::default()
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn save_to_path(&self, path: &Path) -> Result<(), String> {
        self.save_to_path_using(path, &mut FileSystemConfigSave)
    }

    #[cfg(test)]
    fn save_to_path_using<O: ConfigSaveOperations>(
        &self,
        path: &Path,
        operations: &mut O,
    ) -> Result<(), String> {
        let parent = parent_directory(path);
        ensure_parent_directory(operations, parent)?;

        let contents =
            toml::to_string_pretty(self).map_err(|e| format!("failed to serialize config: {e}"))?;

        atomic_save_config(operations, path, contents.as_bytes())
    }

    /// Validate user-controlled settings using a supplied base directory.
    /// The original configuration is not changed.
    pub fn validate(&self, base_directory: &Path) -> ConfigValidation {
        let mut effective = self.clone();
        let mut warnings = Vec::new();
        if effective.editor.wrap_width == 0 {
            effective.editor.wrap_width = DEFAULT_WRAP_WIDTH;
            warnings.push(ConfigWarning::WrapWidthZero);
        }
        if let Some(mode) = &effective.theme.mode {
            if mode != "dark" && mode != "light" {
                warnings.push(ConfigWarning::InvalidThemeMode(mode.clone()));
                effective.theme.mode = Some("dark".to_string());
            }
        }
        if !matches!(
            effective.spell.language.as_str(),
            "en_US" | "en_CA" | "en_AU"
        ) {
            warnings.push(ConfigWarning::UnknownSpellLanguage(
                effective.spell.language.clone(),
            ));
            effective.spell.language = default_spell_language();
        }
        for path in self.additional_dictionary_paths(base_directory) {
            if let Err(error) = std::fs::File::open(&path).and_then(|file| {
                if file.metadata()?.is_file() {
                    Ok(())
                } else {
                    Err(io::Error::new(io::ErrorKind::InvalidInput, "not a file"))
                }
            }) {
                effective.spell.enabled = false;
                warnings.push(ConfigWarning::UnreadableDictionary {
                    path,
                    detail: error.to_string(),
                });
            }
        }
        ConfigValidation {
            effective,
            warnings,
        }
    }

    /// Resolve additional dictionary paths relative to the supplied base.
    pub fn additional_dictionary_paths(&self, base_directory: &Path) -> Vec<PathBuf> {
        self.spell
            .additional_dictionaries
            .iter()
            .map(|path| {
                if path.is_absolute() {
                    path.clone()
                } else {
                    base_directory.join(path)
                }
            })
            .collect()
    }
}

/// Typed validation outcomes; callers may display warnings without parsing text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigWarning {
    /// Zero cannot be used as a wrap width; the default is used.
    WrapWidthZero,
    /// Unknown theme mode was replaced with dark.
    InvalidThemeMode(String),
    /// Unknown built-in dictionary dialect was replaced with en_US.
    UnknownSpellLanguage(String),
    /// One declared additional dictionary could not be opened as a file.
    UnreadableDictionary {
        /// Resolved path that failed.
        path: PathBuf,
        /// Operating-system detail.
        detail: String,
    },
}

/// Effective configuration and all non-fatal validation warnings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigValidation {
    /// Safe values to use for this run.
    pub effective: Config,
    /// Every fallback applied in declaration order.
    pub warnings: Vec<ConfigWarning>,
}

/// Named configuration setting in a live-update report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigField {
    /// Theme appearance mode.
    ThemeMode,
    /// Dark appearance's configured theme name and presence.
    ThemeDark,
    /// Light appearance's configured theme name and presence.
    ThemeLight,
    /// Whether source and rendered lines wrap.
    Wrap,
    /// Whether gutter numbers are relative to the cursor.
    RelativeLineNumbers,
    /// Whether the host should use mode-specific cursor shapes.
    CursorShapes,
    /// Selected clipboard representation.
    ClipboardCopyFormat,
    /// Whether spelling is enabled for this session.
    SpellEnabled,
    /// Construction-time wrapping width.
    WrapWidth,
    /// Construction-time bundled dictionary dialect.
    SpellLanguage,
    /// Construction-time additional dictionary paths.
    AdditionalDictionaries,
}

/// Exact changes that can apply now and those that need a new pane.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ConfigApplyReport {
    /// Runtime-safe changes in stable field order.
    pub applied_now: Vec<ConfigField>,
    /// Resource/layout settings requiring new pane construction.
    pub requires_new_pane: Vec<ConfigField>,
}

impl ConfigApplyReport {
    /// Compare a running configuration with a candidate value.
    pub fn between(old: &Config, new: &Config) -> Self {
        let mut report = Self::default();
        if old.theme.mode != new.theme.mode {
            report.applied_now.push(ConfigField::ThemeMode);
        }
        if old.theme.dark != new.theme.dark
            || old.theme.dark_is_explicit() != new.theme.dark_is_explicit()
        {
            report.applied_now.push(ConfigField::ThemeDark);
        }
        if old.theme.light != new.theme.light
            || old.theme.light_is_explicit() != new.theme.light_is_explicit()
        {
            report.applied_now.push(ConfigField::ThemeLight);
        }
        if old.editor.wrap != new.editor.wrap {
            report.applied_now.push(ConfigField::Wrap);
        }
        if old.relative_line_numbers != new.relative_line_numbers {
            report.applied_now.push(ConfigField::RelativeLineNumbers);
        }
        if old.editor.cursor_shapes != new.editor.cursor_shapes {
            report.applied_now.push(ConfigField::CursorShapes);
        }
        if old.clipboard.copy_format != new.clipboard.copy_format {
            report.applied_now.push(ConfigField::ClipboardCopyFormat);
        }
        if old.spell.enabled != new.spell.enabled {
            report.applied_now.push(ConfigField::SpellEnabled);
        }
        if old.editor.wrap_width != new.editor.wrap_width {
            report.requires_new_pane.push(ConfigField::WrapWidth);
        }
        if old.spell.language != new.spell.language {
            report.requires_new_pane.push(ConfigField::SpellLanguage);
        }
        if old.spell.additional_dictionaries != new.spell.additional_dictionaries {
            report
                .requires_new_pane
                .push(ConfigField::AdditionalDictionaries);
        }
        report
    }
}

/// Active appearance slot selected for theme-name persistence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeSlot {
    /// Dark appearance.
    Dark,
    /// Light appearance.
    Light,
}

impl ThemeSlot {
    fn key(self) -> &'static str {
        match self {
            Self::Dark => "dark",
            Self::Light => "light",
        }
    }
}

/// Typed failure from a host-provided theme persistence sink.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigPersistError {
    /// The existing file is not valid UTF-8/TOML or has an invalid theme table.
    Malformed(String),
    /// The file changed during read/prepare; no replacement was attempted.
    ConcurrentChange,
    /// An I/O step failed before replacement.
    Io(String),
    /// Replacement committed but directory durability could not be confirmed.
    CommittedUncertain(String),
}

impl std::fmt::Display for ConfigPersistError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Malformed(message) => write!(formatter, "invalid config file: {message}"),
            Self::ConcurrentChange => write!(formatter, "config file changed while saving"),
            Self::Io(message) => write!(formatter, "config I/O error: {message}"),
            Self::CommittedUncertain(message) => {
                write!(
                    formatter,
                    "config replacement committed but durability is uncertain: {message}"
                )
            }
        }
    }
}

impl std::error::Error for ConfigPersistError {}

/// Host-injected persistence boundary for a user-initiated theme cycle.
pub trait ThemePersistenceSink {
    /// Persist only the selected appearance/name pair.
    fn persist_theme(&mut self, slot: ThemeSlot, name: &str) -> Result<(), ConfigPersistError>;
}

/// Standalone file-backed theme sink. Hosts may supply a different sink.
#[derive(Debug, Clone)]
pub struct FileThemeSink {
    path: PathBuf,
}

impl FileThemeSink {
    /// Bind this sink to a standalone config file path.
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    fn persist_with_hook(
        &mut self,
        slot: ThemeSlot,
        name: &str,
        before_revalidation: impl FnOnce(),
    ) -> Result<(), ConfigPersistError> {
        let expected = oom_edit_core::DiskVersion::observe(&self.path)
            .map_err(|error| ConfigPersistError::Io(error.message().to_string()))?;
        let bytes = match std::fs::read(&self.path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == io::ErrorKind::NotFound => Vec::new(),
            Err(error) => return Err(ConfigPersistError::Io(error.to_string())),
        };
        let after_read = oom_edit_core::DiskVersion::observe(&self.path)
            .map_err(|error| ConfigPersistError::Io(error.message().to_string()))?;
        if expected != after_read {
            return Err(ConfigPersistError::ConcurrentChange);
        }
        let contents = String::from_utf8(bytes)
            .map_err(|error| ConfigPersistError::Malformed(error.to_string()))?;
        let mut document: toml::Value = if contents.is_empty() {
            toml::Value::Table(toml::Table::new())
        } else {
            toml::from_str(&contents).map_err(|error: toml::de::Error| {
                ConfigPersistError::Malformed(error.to_string())
            })?
        };
        if !contents.is_empty() {
            toml::from_str::<Config>(&contents)
                .map_err(|error| ConfigPersistError::Malformed(error.to_string()))?;
        }
        let root = document.as_table_mut().ok_or_else(|| {
            ConfigPersistError::Malformed("top-level config is not a table".to_string())
        })?;
        let theme = root
            .entry("theme")
            .or_insert_with(|| toml::Value::Table(toml::Table::new()));
        let theme = theme.as_table_mut().ok_or_else(|| {
            ConfigPersistError::Malformed("theme section is not a table".to_string())
        })?;
        theme.insert(
            slot.key().to_string(),
            toml::Value::String(name.to_string()),
        );
        let updated = toml::to_string_pretty(&document)
            .map_err(|error| ConfigPersistError::Malformed(error.to_string()))?;
        let parent = parent_directory(&self.path);
        std::fs::create_dir_all(parent)
            .map_err(|error| ConfigPersistError::Io(error.to_string()))?;
        let mut temporary = tempfile::NamedTempFile::new_in(parent)
            .map_err(|error| ConfigPersistError::Io(error.to_string()))?;
        match std::fs::metadata(&self.path) {
            Ok(metadata) => temporary
                .as_file()
                .set_permissions(metadata.permissions())
                .map_err(|error| ConfigPersistError::Io(error.to_string()))?,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(ConfigPersistError::Io(error.to_string())),
        }
        temporary
            .write_all(updated.as_bytes())
            .and_then(|()| temporary.as_file().sync_all())
            .map_err(|error| ConfigPersistError::Io(error.to_string()))?;
        before_revalidation();
        let current = oom_edit_core::DiskVersion::observe(&self.path)
            .map_err(|error| ConfigPersistError::Io(error.message().to_string()))?;
        if current != expected {
            return Err(ConfigPersistError::ConcurrentChange);
        }
        temporary
            .persist(&self.path)
            .map_err(|error| ConfigPersistError::Io(error.to_string()))?;
        std::fs::File::open(parent)
            .and_then(|directory| directory.sync_all())
            .map_err(|error| ConfigPersistError::CommittedUncertain(error.to_string()))
    }
}

impl ThemePersistenceSink for FileThemeSink {
    fn persist_theme(&mut self, slot: ThemeSlot, name: &str) -> Result<(), ConfigPersistError> {
        self.persist_with_hook(slot, name, || {})
    }
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use std::io;
    use std::path::{Path, PathBuf};
    use std::process::Command;

    use super::*;

    #[derive(Debug, Clone, PartialEq, Eq)]
    enum ConfigSaveEvent {
        CreateDirectories(PathBuf),
        Write,
        FileSync,
        Rename,
        OpenParentReadOnly(PathBuf),
        DirectorySync,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Failure {
        FileSync,
        ParentOpen(usize),
        ParentSync(usize),
    }

    #[derive(Default)]
    struct RecordingConfigSave {
        events: Vec<ConfigSaveEvent>,
        failure: Option<Failure>,
        parent_open_count: usize,
        parent_sync_count: usize,
    }

    impl RecordingConfigSave {
        fn failing_at(failure: Failure) -> Self {
            Self {
                events: Vec::new(),
                failure: Some(failure),
                parent_open_count: 0,
                parent_sync_count: 0,
            }
        }

        fn fail(&self, failure: Failure) -> io::Result<()> {
            if self.failure == Some(failure) {
                Err(io::Error::other("injected config-save failure"))
            } else {
                Ok(())
            }
        }
    }

    impl ConfigSaveOperations for RecordingConfigSave {
        type ParentDirectory = ();

        fn create_dir_all(&mut self, path: &Path) -> io::Result<()> {
            self.events
                .push(ConfigSaveEvent::CreateDirectories(path.to_path_buf()));
            Ok(())
        }

        fn write(&mut self, _path: &Path, _contents: &[u8]) -> io::Result<()> {
            self.events.push(ConfigSaveEvent::Write);
            Ok(())
        }

        fn sync_file(&mut self, _path: &Path) -> io::Result<()> {
            self.events.push(ConfigSaveEvent::FileSync);
            self.fail(Failure::FileSync)
        }

        fn rename(&mut self, _source: &Path, _target: &Path) -> io::Result<()> {
            self.events.push(ConfigSaveEvent::Rename);
            Ok(())
        }

        fn open_parent_read_only(&mut self, parent: &Path) -> io::Result<Self::ParentDirectory> {
            self.events
                .push(ConfigSaveEvent::OpenParentReadOnly(parent.to_path_buf()));
            self.parent_open_count += 1;
            if self.failure == Some(Failure::ParentOpen(self.parent_open_count)) {
                Err(io::Error::other("injected config-save failure"))
            } else {
                Ok(())
            }
        }

        fn sync_parent(&mut self, _parent: &Self::ParentDirectory) -> io::Result<()> {
            self.events.push(ConfigSaveEvent::DirectorySync);
            self.parent_sync_count += 1;
            if self.failure == Some(Failure::ParentSync(self.parent_sync_count)) {
                Err(io::Error::other("injected config-save failure"))
            } else {
                Ok(())
            }
        }
    }

    fn expected_config_save_events(parent: &Path) -> Vec<ConfigSaveEvent> {
        vec![
            ConfigSaveEvent::CreateDirectories(parent.to_path_buf()),
            ConfigSaveEvent::OpenParentReadOnly(parent_directory(parent).to_path_buf()),
            ConfigSaveEvent::DirectorySync,
            ConfigSaveEvent::Write,
            ConfigSaveEvent::FileSync,
            ConfigSaveEvent::Rename,
            ConfigSaveEvent::OpenParentReadOnly(parent.to_path_buf()),
            ConfigSaveEvent::DirectorySync,
        ]
    }

    /// Default config has sensible defaults.
    #[test]
    fn config_defaults() {
        let config = Config::default();
        assert!(!config.relative_line_numbers);
        assert_eq!(config.theme.mode, None);
        assert_eq!(config.theme.dark, "default-dark");
        assert_eq!(config.theme.light, "default-light");
        assert!(config.editor.wrap);
        assert!(config.editor.cursor_shapes);
        assert_eq!(config.clipboard.copy_format, ClipboardCopyFormat::Markdown);
        let serialized = toml::to_string(&config).unwrap();
        assert!(serialized.contains("copy_format = \"markdown\""));
        assert!(config.spell.enabled);
        assert_eq!(config.spell.language, "en_US");
        assert!(config.spell.additional_dictionaries.is_empty());
    }

    #[test]
    fn theme_sink_rejects_an_intervening_write_without_touching_its_bytes() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("config.toml");
        std::fs::write(&path, "[theme]\ndark = \"original\"\n").unwrap();
        let latest = "[theme]\ndark = \"other writer\"\n[unknown]\nkeep = true\n";
        let mut sink = FileThemeSink::new(path.clone());
        let error = sink.persist_with_hook(ThemeSlot::Dark, "ours", || {
            std::fs::write(&path, latest).unwrap();
        });
        assert_eq!(error, Err(ConfigPersistError::ConcurrentChange));
        assert_eq!(std::fs::read_to_string(path).unwrap(), latest);
    }

    #[test]
    fn config_clipboard_copy_format_defaults_validates_and_roundtrips() {
        let missing: Config = toml::from_str("[editor]\nwrap = false\n").unwrap();
        assert_eq!(missing.clipboard.copy_format, ClipboardCopyFormat::Markdown);

        let markdown: Config = toml::from_str("[clipboard]\ncopy_format = \"markdown\"\n").unwrap();
        assert_eq!(
            markdown.clipboard.copy_format,
            ClipboardCopyFormat::Markdown
        );

        let plain: Config = toml::from_str("[clipboard]\ncopy_format = \"plain-text\"\n").unwrap();
        assert_eq!(plain.clipboard.copy_format, ClipboardCopyFormat::PlainText);
        let serialized = toml::to_string(&plain).unwrap();
        assert!(serialized.contains("copy_format = \"plain-text\""));
        assert_eq!(toml::from_str::<Config>(&serialized).unwrap(), plain);

        assert!(toml::from_str::<Config>("[clipboard]\ncopy_format = \"rendered\"\n").is_err());
    }

    #[test]
    fn config_spell_section_defaults_and_roundtrips() {
        let missing: Config = toml::from_str("[editor]\nwrap = false\n").unwrap();
        assert_eq!(missing.spell, SpellConfig::default());

        let source = r#"
[spell]
enabled = false
language = "en_CA"
additional_dictionaries = ["team.txt", "/opt/shared.txt"]
"#;
        let parsed: Config = toml::from_str(source).unwrap();
        assert!(!parsed.spell.enabled);
        assert_eq!(parsed.spell.language, "en_CA");
        assert_eq!(
            parsed.spell.additional_dictionaries,
            [PathBuf::from("team.txt"), PathBuf::from("/opt/shared.txt")]
        );

        let serialized = toml::to_string(&parsed).unwrap();
        assert_eq!(toml::from_str::<Config>(&serialized).unwrap(), parsed);
    }

    #[test]
    fn config_editor_wrap_defaults_true() {
        assert!(Config::default().editor.wrap);
    }

    #[test]
    fn config_editor_wrap_width_defaults_validates_and_roundtrips() {
        assert_eq!(Config::default().editor.wrap_width, 100);

        let partial: Config = toml::from_str("[editor]\nwrap = false\n").unwrap();
        assert_eq!(partial.editor.wrap_width, 100);

        let configured: Config = toml::from_str("[editor]\nwrap_width = 72\n").unwrap();
        assert_eq!(configured.editor.wrap_width, 72);
        assert_eq!(
            toml::from_str::<Config>(&toml::to_string(&configured).unwrap()).unwrap(),
            configured
        );

        let zero: Config = toml::from_str("[editor]\nwrap_width = 0\n").unwrap();
        let validation = zero.validate(Path::new("."));
        assert_eq!(validation.effective.editor.wrap_width, DEFAULT_WRAP_WIDTH);
        assert_eq!(validation.warnings, [ConfigWarning::WrapWidthZero]);
        assert!(toml::from_str::<Config>("[editor]\nwrap_width = 70000\n").is_err());
    }

    #[test]
    fn config_editor_wrap_roundtrip() {
        let config = Config {
            editor: EditorConfig {
                wrap: false,
                wrap_width: 72,
                cursor_shapes: false,
            },
            ..Config::default()
        };
        let serialized = toml::to_string(&config).unwrap();
        let parsed: Config = toml::from_str(&serialized).unwrap();
        assert!(!parsed.editor.wrap);
        assert_eq!(parsed.editor.wrap_width, 72);
        assert!(!parsed.editor.cursor_shapes);
    }

    #[test]
    fn config_missing_editor_section_defaults_wrap_true() {
        let config: Config = toml::from_str("[theme]\nmode = \"dark\"\n").unwrap();
        assert!(config.editor.wrap);
        assert!(config.editor.cursor_shapes);
    }

    #[test]
    fn config_partial_editor_section_defaults_cursor_shapes_true() {
        let config: Config = toml::from_str("[editor]\nwrap = false\n").unwrap();
        assert!(config.editor.cursor_shapes);
    }

    #[test]
    fn config_editor_cursor_shapes_explicit_false_roundtrips() {
        let config: Config = toml::from_str("[editor]\ncursor_shapes = false\n").unwrap();
        assert!(!config.editor.cursor_shapes);

        let serialized = toml::to_string(&config).unwrap();
        assert_eq!(toml::from_str::<Config>(&serialized).unwrap(), config);
    }

    #[test]
    fn removed_alternating_table_rows_field_is_ignored_and_not_serialized() {
        let config: Config =
            toml::from_str("[editor]\nwrap = false\nalternating_table_rows = true\n").unwrap();
        assert!(!config.editor.wrap);
        assert!(!toml::to_string(&config)
            .unwrap()
            .contains("alternating_table_rows"));
    }

    /// Config round-trip: save and reload produces the same config.
    #[test]
    fn config_round_trip() {
        let config = Config {
            relative_line_numbers: true,
            theme: ThemeConfig {
                mode: Some("light".to_string()),
                dark: "my-dark".to_string(),
                light: "my-light".to_string(),
                ..ThemeConfig::default()
            },
            editor: EditorConfig {
                wrap: false,
                wrap_width: 72,
                cursor_shapes: false,
            },
            clipboard: ClipboardConfig {
                copy_format: ClipboardCopyFormat::PlainText,
            },
            spell: SpellConfig {
                enabled: false,
                language: "en_AU".to_string(),
                additional_dictionaries: vec![PathBuf::from("project.words")],
            },
        };

        let temp_dir = tempfile::tempdir().unwrap();
        let config_dir = temp_dir.path().join("oom-edit");
        std::fs::create_dir_all(&config_dir).unwrap();
        let config_file = config_dir.join("config.toml");

        config.save_to_path(&config_file).unwrap();

        // Read it back.
        let contents2 = std::fs::read_to_string(&config_file).unwrap();
        let config2: Config = toml::from_str(&contents2).unwrap();

        assert_eq!(config, config2);
        assert!(config2.relative_line_numbers);
        assert_eq!(
            config2.clipboard.copy_format,
            ClipboardCopyFormat::PlainText
        );
    }

    /// Malformed TOML falls back to defaults (warns to stderr).
    #[test]
    fn config_malformed_falls_back() {
        let temp_dir = tempfile::tempdir().unwrap();
        let config_dir = temp_dir.path().join("oom-edit");
        std::fs::create_dir_all(&config_dir).unwrap();
        let config_file = config_dir.join("config.toml");

        // Write malformed TOML.
        std::fs::write(&config_file, "{{{not valid toml}}").unwrap();

        let config = Config::load_from_path(&config_file);
        assert!(!config.theme.dark_is_explicit());
        assert!(!config.theme.light_is_explicit());
        assert_eq!(config.theme.dark, "default-dark");
        assert_eq!(config.theme.light, "default-light");
    }

    #[test]
    fn invalid_clipboard_copy_format_uses_existing_config_fallback() {
        let temp_dir = tempfile::tempdir().unwrap();
        let config_file = temp_dir.path().join("config.toml");
        std::fs::write(
            &config_file,
            "[clipboard]\ncopy_format = \"rendered-html\"\n",
        )
        .unwrap();

        let config = Config::load_from_path(&config_file);
        assert!(!config.theme.dark_is_explicit());
        assert!(!config.theme.light_is_explicit());
        assert_eq!(config, Config::default());
        assert_eq!(config.clipboard.copy_format, ClipboardCopyFormat::Markdown);
    }

    /// Partial config (missing keys) uses defaults via serde.
    #[test]
    fn config_partial_uses_defaults() {
        let toml_str = r#"
[theme]
mode = "light"
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert!(!config.relative_line_numbers);
        assert_eq!(config.theme.mode, Some("light".to_string()));
        assert_eq!(config.theme.dark, "default-dark");
        assert_eq!(config.theme.light, "default-light");
    }

    #[test]
    fn partial_config_never_defaults_to_accessible() {
        for source in [
            "[theme]\n",
            "[theme]\nmode = \"dark\"\n",
            "[theme]\ndark = \"default-dark\"\n",
            "[theme]\nlight = \"default-light\"\n",
            "[editor]\nwrap = false\n",
        ] {
            let config: Config = toml::from_str(source).unwrap();
            assert_eq!(config.theme.dark, "default-dark");
            assert_eq!(config.theme.light, "default-light");
            assert_ne!(config.theme.dark, "accessible");
            assert_ne!(config.theme.light, "accessible");
        }
    }

    #[test]
    fn partial_config_tracks_only_explicit_theme_slots() {
        let temp_dir = tempfile::tempdir().unwrap();
        let config_file = temp_dir.path().join("config.toml");

        std::fs::write(&config_file, "[editor]\nwrap = false\n").unwrap();
        let config = Config::load_from_path(&config_file);
        assert!(!config.theme.dark_is_explicit());
        assert!(!config.theme.light_is_explicit());

        std::fs::write(
            &config_file,
            "[theme]\ndark = \"accessible\"\nmode = \"dark\"\n",
        )
        .unwrap();
        let config = Config::load_from_path(&config_file);
        assert!(config.theme.dark_is_explicit());
        assert!(!config.theme.light_is_explicit());
    }

    /// Empty config file uses all defaults.
    #[test]
    fn config_empty_uses_defaults() {
        let config: Config = toml::from_str("").unwrap();
        assert!(!config.relative_line_numbers);
        assert_eq!(config.theme.mode, None);
        assert_eq!(config.theme.dark, "default-dark");
        assert_eq!(config.theme.light, "default-light");
    }

    /// Save creates the config directory if needed.
    #[test]
    fn config_save_creates_directory() {
        let temp_dir = tempfile::tempdir().unwrap();
        let config_dir = temp_dir.path().join("oom-edit");
        let config_file = config_dir.join("config.toml");

        let config = Config::default();
        config.save_to_path(&config_file).unwrap();

        assert!(config_file.exists());
    }

    #[test]
    fn config_save_uses_complete_atomic_sequence() {
        let config = Config::default();
        let mut operations = RecordingConfigSave::default();

        config
            .save_to_path_using(Path::new("config.toml"), &mut operations)
            .unwrap();

        assert_eq!(
            operations.events,
            expected_config_save_events(Path::new("."))
        );
    }

    #[test]
    fn config_save_propagates_sync_and_parent_open_failures() {
        let config = Config::default();
        for (failure, expected_message) in [
            (Failure::FileSync, "failed to sync config file"),
            (Failure::ParentOpen(2), "failed to open config directory"),
            (Failure::ParentSync(2), "failed to sync config directory"),
        ] {
            let mut operations = RecordingConfigSave::failing_at(failure);
            let error = config
                .save_to_path_using(Path::new("config.toml"), &mut operations)
                .unwrap_err();
            assert!(
                error.contains(expected_message),
                "unexpected error for {failure:?}: {error}"
            );
        }
    }

    #[test]
    fn config_save_syncs_new_directory_entries_bottom_up() {
        let config = Config::default();
        let mut operations = RecordingConfigSave::default();

        config
            .save_to_path_using(Path::new("config/nested/config.toml"), &mut operations)
            .unwrap();

        assert_eq!(
            operations.events,
            vec![
                ConfigSaveEvent::CreateDirectories(PathBuf::from("config/nested")),
                ConfigSaveEvent::OpenParentReadOnly(PathBuf::from("config")),
                ConfigSaveEvent::DirectorySync,
                ConfigSaveEvent::OpenParentReadOnly(PathBuf::from(".")),
                ConfigSaveEvent::DirectorySync,
                ConfigSaveEvent::Write,
                ConfigSaveEvent::FileSync,
                ConfigSaveEvent::Rename,
                ConfigSaveEvent::OpenParentReadOnly(PathBuf::from("config/nested")),
                ConfigSaveEvent::DirectorySync,
            ]
        );
    }

    #[test]
    fn absolute_config_path_stops_ancestor_sync_at_filesystem_root() {
        let config = Config::default();
        let mut operations = RecordingConfigSave::default();

        config
            .save_to_path_using(Path::new("/config/nested/config.toml"), &mut operations)
            .unwrap();

        assert_eq!(
            &operations.events[..5],
            &[
                ConfigSaveEvent::CreateDirectories(PathBuf::from("/config/nested")),
                ConfigSaveEvent::OpenParentReadOnly(PathBuf::from("/config")),
                ConfigSaveEvent::DirectorySync,
                ConfigSaveEvent::OpenParentReadOnly(PathBuf::from("/")),
                ConfigSaveEvent::DirectorySync,
            ]
        );
        assert!(!operations
            .events
            .contains(&ConfigSaveEvent::OpenParentReadOnly(PathBuf::from("."))));
    }

    #[test]
    fn config_save_propagates_ancestor_sync_failures_before_writing() {
        let config = Config::default();
        for failure in [Failure::ParentOpen(1), Failure::ParentSync(1)] {
            let mut operations = RecordingConfigSave::failing_at(failure);

            assert!(config
                .save_to_path_using(Path::new("config/nested/config.toml"), &mut operations,)
                .is_err());
            assert!(!operations.events.contains(&ConfigSaveEvent::Write));
        }
    }

    #[test]
    fn config_save_retry_repeats_ancestor_sync_chain() {
        let config = Config::default();
        let path = Path::new("config/nested/config.toml");
        let mut first_attempt = RecordingConfigSave::failing_at(Failure::ParentSync(2));
        assert!(config.save_to_path_using(path, &mut first_attempt).is_err());
        assert!(!first_attempt.events.contains(&ConfigSaveEvent::Write));

        let mut retry = RecordingConfigSave::default();
        config.save_to_path_using(path, &mut retry).unwrap();
        assert_eq!(
            &retry.events[..5],
            &[
                ConfigSaveEvent::CreateDirectories(PathBuf::from("config/nested")),
                ConfigSaveEvent::OpenParentReadOnly(PathBuf::from("config")),
                ConfigSaveEvent::DirectorySync,
                ConfigSaveEvent::OpenParentReadOnly(PathBuf::from(".")),
                ConfigSaveEvent::DirectorySync,
            ]
        );
    }

    #[test]
    fn relative_config_path_resolves_parent_to_current_directory() {
        assert_eq!(parent_directory(Path::new("config.toml")), Path::new("."));
    }

    /// Config with all theme keys specified.
    #[test]
    fn config_full_theme_section() {
        let toml_str = r#"
[theme]
mode = "dark"
dark = "my-custom-dark"
light = "my-custom-light"
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.theme.mode, Some("dark".to_string()));
        assert_eq!(config.theme.dark, "my-custom-dark");
        assert_eq!(config.theme.light, "my-custom-light");
    }

    #[test]
    fn isolated_helper_cleans_config_after_success_failure_and_signal() {
        let script = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join("scripts/with-isolated-config.sh");

        for exit_code in [0, 7] {
            let command = format!(
                "printf '%s' \"$XDG_CONFIG_HOME\"; mkdir -p \"$XDG_CONFIG_HOME/oom-edit\"; touch \"$XDG_CONFIG_HOME/oom-edit/config.toml\"; exit {exit_code}"
            );
            let output = Command::new("bash")
                .arg(&script)
                .arg("bash")
                .arg("-c")
                .arg(command)
                .output()
                .unwrap();
            assert_eq!(output.status.success(), exit_code == 0);

            let xdg_path = PathBuf::from(String::from_utf8(output.stdout).unwrap());
            let isolation_root = xdg_path.parent().unwrap();
            assert!(
                !isolation_root.exists(),
                "isolated config root survived helper exit: {}",
                isolation_root.display()
            );
        }

        let output = Command::new("bash")
            .arg(&script)
            .arg("bash")
            .arg("-c")
            .arg("printf '%s' \"$XDG_CONFIG_HOME\"; mkdir -p \"$XDG_CONFIG_HOME/oom-edit\"; touch \"$XDG_CONFIG_HOME/oom-edit/config.toml\"; kill -HUP $$")
            .output()
            .unwrap();
        assert!(!output.status.success());
        let xdg_path = PathBuf::from(String::from_utf8(output.stdout).unwrap());
        assert!(!xdg_path.parent().unwrap().exists());
    }
}
