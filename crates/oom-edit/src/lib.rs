//! `oom-edit` — the TUI binary crate.
//!
//! A `ratatui` + `crossterm` keyboard-driven presentation layer over the
//! `oom-edit-core` editing engine. All business logic, document model,
//! syntax highlighting, and rendered Markdown live in `oom-edit-core`;
//! this crate renders and dispatches keys.
//!
//! ## Startup ordering (hidlins pattern)
//!
//! 1. Parse CLI args **before** any terminal setup (`main`).
//! 2. Build the `App` (open file via `EditorSession::open`) — startup errors
//!    print on a normal terminal, not inside raw mode.
//! 3. Install `TerminalGuard` (raw mode + alternate screen + panic/signal restore).
//! 4. Create the `ratatui::Terminal`.
//! 5. Run the event loop.
//!
//! The guard restores the terminal on any return path, panic, or fatal signal.
//!
//! ## Crate posture
//!
//! `deny(unsafe_code)` (the workspace default) with exactly **one** audited
//! `#[allow(unsafe_code)]` block — the async-signal-safe `SIGHUP`/`SIGTERM`
//! handler in `terminal_guard`. No other `unsafe` is permitted in this crate.

pub(crate) mod app;
pub(crate) mod args;
pub(crate) mod clipboard;
pub(crate) mod command;
pub(crate) mod config;
pub(crate) mod event;
pub(crate) mod gutter;
pub(crate) mod lifecycle;
pub(crate) mod overlay;
#[cfg(test)]
mod perf_tests;
pub(crate) mod screens;
#[cfg(test)]
pub(crate) mod snapshot_tests;
pub(crate) mod spell_host;
pub(crate) mod terminal_guard;
pub(crate) mod theme;
pub(crate) mod widgets;

pub use args::{Args, ParseOutcome};

use std::io::stdout;

use oom_edit_core::EditorSession;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use crate::app::{App, AppServices};
use crate::config::{Config, ConfigPresence};
use crate::spell_host::{resolve_wordlist_source, SpellHost};
use crate::terminal_guard::TerminalGuard;
use crate::theme::{EnvParts, ResolvedTheme, ThemeCatalog};

fn resolve_startup_theme(
    catalog: &ThemeCatalog,
    cli_theme: Option<&str>,
    config: &Config,
    presence: ConfigPresence,
    env: &EnvParts,
) -> ResolvedTheme {
    catalog.resolve_theme(
        cli_theme,
        config.theme.mode.as_deref(),
        presence.dark.then_some(config.theme.dark.as_str()),
        presence.light.then_some(config.theme.light.as_str()),
        env,
    )
}

/// Top-level entry point invoked by `main.rs`.
///
/// Builds the `App` (file I/O) *before* touching the terminal, so startup
/// errors print on a normal terminal. Then installs the terminal guard
/// (raw mode + alternate screen + panic/signal restore) and runs the event
/// loop; the guard restores the terminal on any return path, panic, or
/// fatal signal.
///
/// # Errors
///
/// Returns an error if terminal setup fails. File-open errors are reported
/// as status messages (the session opens with a new buffer for missing paths).
pub fn run(args: &Args) -> Result<(), Box<dyn std::error::Error>> {
    // Load config (never fails — warns to stderr on malformed config).
    let (config, config_presence) = Config::load_with_presence();
    let config_path = crate::config::config_path();

    let env = EnvParts::from_current_process();
    let theme_report = ThemeCatalog::load_from_config_path(&config_path);
    for warning in &theme_report.warnings {
        eprintln!("oom-edit: warning: {warning}");
    }
    let theme_catalog = theme_report.catalog;

    // Resolve theme through the selection ladder.
    let resolved_theme = resolve_startup_theme(
        &theme_catalog,
        args.theme.as_deref(),
        &config,
        config_presence,
        &env,
    );

    let spell_resolution = resolve_wordlist_source(&config.spell, &config_path);
    if let Some(warning) = &spell_resolution.warning {
        eprintln!("oom-edit: warning: {warning}");
    }
    let personal_dictionary_path = config_path.with_file_name("dictionary.txt");
    let spell_host = SpellHost::production(spell_resolution.source, personal_dictionary_path);

    // Announce theme to stderr before entering alternate screen (hidlins pattern).
    eprintln!("oom-edit: {resolved_theme}");

    // Build the App (open file) BEFORE touching the terminal.
    let session = match &args.path {
        Some(path) => match EditorSession::open(path) {
            Ok(session) => session,
            Err(e) => {
                eprintln!("oom-edit: open error: {e}");
                return Err(format!("open error: {e}").into());
            }
        },
        None => EditorSession::from_text(""),
    };

    let launch_dir = std::env::current_dir()?;
    let app = App::new_with_spell(
        session,
        theme_catalog,
        resolved_theme,
        app::AppStartupOptions::new(
            config.editor.wrap,
            config.relative_line_numbers,
            config.clipboard.copy_format,
            config.spell.enabled,
        )
        .with_wrap_width(config.editor.wrap_width),
        AppServices::new(
            Box::new(crate::clipboard::Osc52Clipboard::stdout()),
            Box::new(crate::config::FileConfigStore::production()),
            spell_host,
            launch_dir,
        ),
        std::time::Instant::now(),
    );

    // Enter raw mode + alternate screen + install panic/signal hooks.
    let _guard = TerminalGuard::new()?;

    let terminal = Terminal::new(CrosstermBackend::new(stdout()))?;

    // Deliberately panic in raw mode so restoration can be verified.
    if args.panic_test {
        panic!("--panic-test: deliberate panic for terminal-restoration verification");
    }

    // Run the event loop.
    event::run_event_loop(app, terminal, config.editor.cursor_shapes)
}

#[cfg(test)]
mod startup_tests {
    use super::*;
    use crate::config::EditorConfig;
    use crate::theme::{ThemeLoadWarningKind, ThemeSource};
    use std::fs;

    #[test]
    fn production_resolver_reports_partial_config_slot_as_fallback() {
        let config = Config {
            editor: EditorConfig {
                wrap: false,
                ..EditorConfig::default()
            },
            ..Config::default()
        };
        let env = EnvParts {
            term: Some("xterm-256color".to_string()),
            colorterm: Some("truecolor".to_string()),
            ..EnvParts::default()
        };
        let catalog = ThemeCatalog::builtins();
        let fallback =
            resolve_startup_theme(&catalog, None, &config, ConfigPresence::default(), &env);
        assert_eq!(fallback.name, "default-dark");
        assert_eq!(fallback.source, ThemeSource::Fallback);

        let configured = resolve_startup_theme(
            &catalog,
            None,
            &config,
            ConfigPresence {
                dark: true,
                light: false,
            },
            &env,
        );
        assert_eq!(configured.name, "default-dark");
        assert_eq!(configured.source, ThemeSource::ConfigDark);
    }

    #[test]
    fn rejected_selected_theme_and_unrelated_failure_warn_before_matching_fallback() {
        let directory = tempfile::tempdir().unwrap();
        let config_path = directory.path().join("config.toml");
        let themes = directory.path().join("themes");
        fs::create_dir(&themes).unwrap();
        fs::write(themes.join("broken-selected.toml"), "appearance = [").unwrap();
        fs::write(themes.join("unrelated-invalid.toml"), [0xff]).unwrap();

        let report = ThemeCatalog::load_from_config_path(&config_path);
        let warnings = report
            .warnings
            .iter()
            .map(|warning| {
                (
                    warning.path.file_name().unwrap().to_string_lossy(),
                    warning.kind,
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(
            warnings,
            vec![
                (
                    "broken-selected.toml".into(),
                    ThemeLoadWarningKind::InvalidToml,
                ),
                (
                    "unrelated-invalid.toml".into(),
                    ThemeLoadWarningKind::InvalidUtf8,
                ),
            ]
        );

        let mut config = Config::default();
        config.theme.mode = Some("light".to_string());
        config.theme.light = "broken-selected".to_string();
        let resolved = resolve_startup_theme(
            &report.catalog,
            None,
            &config,
            ConfigPresence {
                dark: false,
                light: true,
            },
            &EnvParts::default(),
        );
        assert_eq!(resolved.name, "default-light");
        assert_eq!(resolved.source, ThemeSource::Fallback);

        let startup_source = include_str!("lib.rs");
        let load = startup_source
            .find("ThemeCatalog::load_from_config_path")
            .unwrap();
        let warnings = startup_source
            .find("for warning in &theme_report.warnings")
            .unwrap();
        let terminal = startup_source.find("TerminalGuard::new()").unwrap();
        assert!(load < warnings && warnings < terminal);
    }
}
