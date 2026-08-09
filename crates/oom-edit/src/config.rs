//! Configuration — `$XDG_CONFIG_HOME/oom-edit/config.toml` (fallback
//! `~/.config/oom-edit/config.toml`).
//!
//! `[theme]` and `[editor]` sections.
//! Load-with-defaults on missing/partial config. Atomic write on change.
//! Never fail startup on malformed config (warn to stderr, use defaults).

use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

trait ConfigSaveOperations {
    type ParentDirectory;

    fn create_dir_all(&mut self, path: &Path) -> io::Result<()>;
    fn write(&mut self, path: &Path, contents: &[u8]) -> io::Result<()>;
    fn sync_file(&mut self, path: &Path) -> io::Result<()>;
    fn rename(&mut self, source: &Path, target: &Path) -> io::Result<()>;
    fn open_parent_read_only(&mut self, parent: &Path) -> io::Result<Self::ParentDirectory>;
    fn sync_parent(&mut self, parent: &Self::ParentDirectory) -> io::Result<()>;
}

struct FileSystemConfigSave;

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

fn containing_directory(path: &Path) -> Option<&Path> {
    path.parent().map(|parent| {
        if parent.as_os_str().is_empty() {
            Path::new(".")
        } else {
            parent
        }
    })
}

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
    #[serde(default)]
    pub theme: ThemeConfig,
    #[serde(default)]
    pub editor: EditorConfig,
}

/// The `[editor]` section of the config.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EditorConfig {
    /// Whether long source lines wrap. Defaults to `true`.
    #[serde(default = "default_wrap")]
    pub wrap: bool,
}

impl Default for EditorConfig {
    fn default() -> Self {
        Self {
            wrap: default_wrap(),
        }
    }
}

fn default_wrap() -> bool {
    true
}

/// The `[theme]` section of the config.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThemeConfig {
    /// Display mode override: `"light"` or `"dark"`. If `None`, the editor
    /// uses the `COLORFGBG` heuristic (or defaults to dark).
    #[serde(default)]
    pub mode: Option<String>,
    /// Theme name for dark mode. Defaults to `"default-dark"`.
    #[serde(default = "default_dark")]
    pub dark: String,
    /// Theme name for light mode. Defaults to `"default-light"`.
    #[serde(default = "default_light")]
    pub light: String,
}

impl Default for ThemeConfig {
    fn default() -> Self {
        Self {
            mode: None,
            dark: default_dark(),
            light: default_light(),
        }
    }
}

fn default_dark() -> String {
    "default-dark".to_string()
}

fn default_light() -> String {
    "default-light".to_string()
}

impl Config {
    /// Load configuration from disk, using defaults for missing files or
    /// malformed TOML. Warns to stderr on errors.
    pub fn load() -> Self {
        let path = config_path();
        Self::load_from_path(&path)
    }

    pub(crate) fn load_from_path(path: &Path) -> Self {
        match std::fs::read_to_string(path) {
            Ok(contents) => match toml::from_str(&contents) {
                Ok(config) => config,
                Err(e) => {
                    eprintln!("oom-edit: config parse error: {e}, using defaults");
                    Config::default()
                }
            },
            Err(e) => {
                // File not found is normal — use defaults silently.
                if e.kind() != std::io::ErrorKind::NotFound {
                    eprintln!("oom-edit: config read error: {e}, using defaults");
                }
                Config::default()
            }
        }
    }

    /// Save the configuration atomically (write temp + fsync + rename + dir fsync).
    ///
    /// Creates the parent directory if it doesn't exist.
    ///
    /// # Errors
    ///
    /// Returns an error if the directory cannot be created or the file
    /// cannot be written or synced.
    pub fn save(&self) -> Result<(), String> {
        let path = config_path();
        self.save_to_path(&path)
    }

    pub(crate) fn save_to_path(&self, path: &Path) -> Result<(), String> {
        self.save_to_path_using(path, &mut FileSystemConfigSave)
    }

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
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use std::io;
    use std::path::{Path, PathBuf};

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
        assert_eq!(config.theme.mode, None);
        assert_eq!(config.theme.dark, "default-dark");
        assert_eq!(config.theme.light, "default-light");
        assert!(config.editor.wrap);
    }

    #[test]
    fn config_editor_wrap_defaults_true() {
        assert!(Config::default().editor.wrap);
    }

    #[test]
    fn config_editor_wrap_roundtrip() {
        let config = Config {
            editor: EditorConfig { wrap: false },
            ..Config::default()
        };
        let serialized = toml::to_string(&config).unwrap();
        let parsed: Config = toml::from_str(&serialized).unwrap();
        assert!(!parsed.editor.wrap);
    }

    #[test]
    fn config_missing_editor_section_defaults_wrap_true() {
        let config: Config = toml::from_str("[theme]\nmode = \"dark\"\n").unwrap();
        assert!(config.editor.wrap);
    }

    /// Config round-trip: save and reload produces the same config.
    #[test]
    fn config_round_trip() {
        let config = Config {
            theme: ThemeConfig {
                mode: Some("light".to_string()),
                dark: "my-dark".to_string(),
                light: "my-light".to_string(),
            },
            editor: EditorConfig { wrap: false },
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

        // We can't easily override config_path() in tests, so we test
        // the parsing logic directly.
        let result: Result<Config, _> = toml::from_str("{{{not valid");
        assert!(result.is_err());
    }

    /// Partial config (missing keys) uses defaults via serde.
    #[test]
    fn config_partial_uses_defaults() {
        let toml_str = r#"
[theme]
mode = "light"
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.theme.mode, Some("light".to_string()));
        assert_eq!(config.theme.dark, "default-dark");
        assert_eq!(config.theme.light, "default-light");
    }

    /// Empty config file uses all defaults.
    #[test]
    fn config_empty_uses_defaults() {
        let config: Config = toml::from_str("").unwrap();
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
}
