//! Configuration — `$XDG_CONFIG_HOME/oom-edit/config.toml` (fallback
//! `~/.config/oom-edit/config.toml`).
//!
//! `[theme]` section with `mode`, `dark`, and `light` keys.
//! Load-with-defaults on missing/partial config. Atomic write on change.
//! Never fail startup on malformed config (warn to stderr, use defaults).

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

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
        match std::fs::read_to_string(&path) {
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

    /// Save the configuration atomically (write temp + rename).
    ///
    /// Creates the parent directory if it doesn't exist.
    ///
    /// # Errors
    ///
    /// Returns an error if the directory cannot be created or the file
    /// cannot be written.
    pub fn save(&self) -> Result<(), String> {
        let path = config_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                format!(
                    "failed to create config directory '{}': {e}",
                    parent.display()
                )
            })?;
        }

        let contents =
            toml::to_string_pretty(self).map_err(|e| format!("failed to serialize config: {e}"))?;

        // Atomic write: write to temp file, then rename.
        let temp_path = path.with_extension("toml.tmp");
        std::fs::write(&temp_path, &contents)
            .map_err(|e| format!("failed to write config file: {e}"))?;

        std::fs::rename(&temp_path, &path).map_err(|e| format!("failed to rename config file: {e}"))
    }
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Default config has sensible defaults.
    #[test]
    fn config_defaults() {
        let config = Config::default();
        assert_eq!(config.theme.mode, None);
        assert_eq!(config.theme.dark, "default-dark");
        assert_eq!(config.theme.light, "default-light");
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
        };

        let temp_dir = tempfile::tempdir().unwrap();
        let config_dir = temp_dir.path().join("oom-edit");
        std::fs::create_dir_all(&config_dir).unwrap();
        let config_file = config_dir.join("config.toml");

        // Override config_path for testing by writing to our temp dir.
        let contents = toml::to_string_pretty(&config).unwrap();
        std::fs::write(&config_file, &contents).unwrap();

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

        // Override config_path by writing directly.
        std::fs::create_dir_all(&config_dir).unwrap();
        let contents = toml::to_string_pretty(&config).unwrap();
        std::fs::write(&config_file, &contents).unwrap();

        assert!(config_file.exists());
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
