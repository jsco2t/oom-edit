//! `oom-edit` — an embeddable terminal pane and its standalone host.
//!
//! A `ratatui` + `crossterm` keyboard-driven presentation layer over the
//! `oom-edit-core` editing engine. All business logic, document model,
//! syntax highlighting, and rendered Markdown live in `oom-edit-core`;
//! this crate renders and dispatches keys.
//!
//! ## Startup ordering (hidlins pattern)
//!
//! 1. Parse CLI args **before** any terminal setup (`main`).
//! 2. Construct the public pane and open initial files — startup errors
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
//!
//! ## Private implementation boundaries
//!
//! Hosts use crate-root exports, not implementation modules or mutable sessions.
//!
//! ```compile_fail
//! use oom_edit::app::App;
//! ```
//! ```compile_fail
//! use oom_edit::pane::EditorPane;
//! ```
//! ```compile_fail
//! use oom_edit::pane_frame::PaneFrame;
//! ```
//! ```compile_fail
//! use oom_edit::pane_metadata::EditorStatus;
//! ```
//! ```compile_fail
//! use oom_edit::config::Config;
//! ```
//! ```compile_fail
//! use oom_edit::theme::ThemeCatalog;
//! ```
//! ```compile_fail
//! use oom_edit::owned_style::OwnedStyle;
//! ```
//! ```compile_fail
//! use oom_edit::standalone::StandaloneHost;
//! ```
//! ```compile_fail
//! use oom_edit::command::AppCommand;
//! ```
//! ```compile_fail
//! use oom_edit::terminal_guard::TerminalGuard;
//! ```
//! ```compile_fail
//! fn bypass(pane: &mut oom_edit::EditorPane) {
//!     let _ = pane.session_mut();
//! }
//! ```

#![deny(missing_docs)]

pub(crate) mod app;
pub(crate) mod args;
pub(crate) mod clipboard;
pub(crate) mod command;
pub(crate) mod config;
pub(crate) mod event;
pub(crate) mod gutter;
pub(crate) mod lifecycle;
pub(crate) mod overlay;
pub(crate) mod owned_style;
pub(crate) mod pane;
pub(crate) mod pane_frame;
pub(crate) mod pane_metadata;
#[cfg(test)]
mod perf_tests;
pub(crate) mod screens;
#[cfg(test)]
pub(crate) mod snapshot_tests;
pub(crate) mod spell_host;
pub(crate) mod standalone;
pub(crate) mod terminal_guard;
pub(crate) mod terminal_input;
pub(crate) mod theme;
pub(crate) mod widgets;

pub use args::{Args, ParseOutcome};
pub use clipboard::Osc52Clipboard;
pub use config::{
    ClipboardConfig, ClipboardCopyFormat, Config, ConfigApplyReport, ConfigField,
    ConfigPersistError, ConfigValidation, ConfigWarning, EditorConfig, FileThemeSink, SpellConfig,
    ThemeConfig, ThemePersistenceSink, ThemeSlot,
};
pub use oom_edit_core::{
    ClipboardError, ClipboardSink, DiagnosticSeverity, KeyCode, KeyCodeKind, KeyInput, Mode,
    Modifiers, SemanticStyle,
};
pub use owned_style::{ColorValue, OwnedStyle, StyleModifiers};
pub use pane::{
    AllowAllFileAccess, CommandPolicy, DiskChange, EditorPane, ExternalChangeToken,
    FileAccessPolicy, FileOperation, FilePolicyError, InputDisposition, OpenCursor, OpenOptions,
    OpenOutcome, PaneConstruction, PaneConstructionReport, PaneError, PaneErrorKind, PaneEvent,
    PaneIdleResult, PaneInit, PaneInput, PaneInputState, PaneMouse, PaneMouseKind, PaneOptions,
    PaneServices, PaneTick, PaneWarning, PreparedClose, PreparedRetarget, RequestId, Retarget,
    TabId, TabSnapshot,
};
pub use pane_frame::{PaneCell, PaneCursor, PaneCursorShape, PaneFrame};
pub use pane_metadata::{
    BindingExecution, BindingId, BindingSequence, EditorBinding, EditorRuler, EditorStatus,
    HostReservation, KeyOwnership, SpellStatus, WhichKey, WhichKeyEntry,
};
pub use terminal_guard::{TerminalError, TerminalGuard, TerminalGuardOptions};
pub use theme::{
    DisplayMode, EnvParts as ThemeEnvironment, PaletteKind, ResolvedTheme, ThemeCatalog,
    ThemeLoadReport, ThemeLoadWarning, ThemeLoadWarningKind, ThemeRole, ThemeSelection,
    ThemeSource, Tier, UiSlot,
};
pub use widgets::hint_bar::HintCell;

/// Complete bundled-data and theme notices, identical to `--licenses` output.
pub fn third_party_notices() -> &'static str {
    include_str!("../assets/THIRD-PARTY-NOTICES.md")
}

use std::io::stdout;

use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use crate::standalone::StandaloneHost;
use crate::theme::EnvParts;

fn resolve_startup_theme(
    catalog: &ThemeCatalog,
    cli_theme: Option<&str>,
    config: &Config,
    env: &EnvParts,
) -> ResolvedTheme {
    catalog.resolve_theme(
        cli_theme,
        config.theme.mode.as_deref(),
        config
            .theme
            .dark_is_explicit()
            .then_some(config.theme.dark.as_str()),
        config
            .theme
            .light_is_explicit()
            .then_some(config.theme.light.as_str()),
        env,
    )
}

/// Top-level entry point invoked by `main.rs`.
///
/// Constructs the public pane (file I/O) *before* touching the terminal, so startup
/// errors print on a normal terminal. Then installs the terminal guard
/// (raw mode + alternate screen + panic/signal restore) and runs the event
/// loop; the guard restores the terminal on any return path, panic, or
/// fatal signal.
///
/// # Errors
///
/// Returns an error if terminal setup or opening an initial file fails. Missing
/// named paths open as unsaved buffers; other file errors precede terminal setup.
pub fn run(args: &Args) -> Result<(), Box<dyn std::error::Error>> {
    // Load config (never fails — warns to stderr on malformed config).
    let config = Config::load_production();
    let config_path = crate::config::config_path();
    let config_base = config_path
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."));
    let validated = config.validate(config_base);
    for warning in &validated.warnings {
        eprintln!("oom-edit: warning: {warning:?}");
    }
    let config = validated.effective;

    let env = EnvParts::from_current_process();
    let theme_report = ThemeCatalog::load_from_config_path(&config_path);
    for warning in &theme_report.warnings {
        eprintln!("oom-edit: warning: {warning}");
    }
    let theme_catalog = theme_report.catalog;

    // Resolve theme through the selection ladder.
    let resolved_theme =
        resolve_startup_theme(&theme_catalog, args.theme.as_deref(), &config, &env);

    // Announce theme to stderr before entering alternate screen (hidlins pattern).
    eprintln!("oom-edit: {resolved_theme}");
    let selection = env.selection(config.theme.mode.as_deref(), args.theme.as_deref());
    let cursor_shapes = config.editor.cursor_shapes;
    let PaneConstruction { mut pane, report } = EditorPane::construct(PaneInit {
        config,
        theme_catalog,
        theme_selection: selection,
        services: PaneServices {
            clipboard_sink: Box::new(Osc52Clipboard::stdout()),
            theme_sink: Box::new(FileThemeSink::new(config_path.clone())),
            file_access_policy: Box::new(AllowAllFileAccess),
            config_base_directory: config_base.to_path_buf(),
            personal_dictionary_path: config_path.with_file_name("dictionary.txt"),
            working_directory: std::env::current_dir()?,
        },
        options: PaneOptions {
            command_policy: CommandPolicy::Standalone,
            inline_hints: true,
            ..PaneOptions::default()
        },
        initial_paths: args.path.iter().cloned().collect(),
        now: std::time::Instant::now(),
    });
    for warning in report.warnings {
        if let PaneWarning::Spell(warning) = warning {
            eprintln!("oom-edit: warning: {warning}");
        }
    }
    for result in report.paths {
        if let Err(error) = result {
            eprintln!("oom-edit: open error: {error}");
            return Err(format!("open error: {error}").into());
        }
    }
    if args.path.is_none() {
        pane.new_buffer(OpenOptions::default())?;
    }
    let host = StandaloneHost::new(pane, cursor_shapes, std::time::Instant::now());

    // Enter raw mode + alternate screen + install panic/signal hooks.
    let _guard = TerminalGuard::new()?;

    let terminal = Terminal::new(CrosstermBackend::new(stdout()))?;

    // Deliberately panic in raw mode so restoration can be verified.
    if args.panic_test {
        panic!("--panic-test: deliberate panic for terminal-restoration verification");
    }

    // Run the event loop.
    event::run_event_loop(host, terminal)
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
        let fallback = resolve_startup_theme(&catalog, None, &config, &env);
        assert_eq!(fallback.name, "default-dark");
        assert_eq!(fallback.source, ThemeSource::Fallback);

        let mut config = config;
        config.theme.set_dark("default-dark".to_string());
        let configured = resolve_startup_theme(&catalog, None, &config, &env);
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
        let resolved = resolve_startup_theme(&report.catalog, None, &config, &EnvParts::default());
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
