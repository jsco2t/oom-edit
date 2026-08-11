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
pub(crate) mod lifecycle;
pub(crate) mod overlay;
pub(crate) mod screens;
#[cfg(test)]
pub(crate) mod snapshot_tests;
pub(crate) mod terminal_guard;
pub(crate) mod theme;
pub(crate) mod widgets;

pub use args::{Args, ParseOutcome};

use std::io::stdout;

use oom_edit_core::EditorSession;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use crate::app::App;
use crate::config::{Config, ConfigPresence};
use crate::terminal_guard::TerminalGuard;
use crate::theme::{EnvParts, ResolvedTheme};

fn resolve_startup_theme(
    cli_theme: Option<&str>,
    config: &Config,
    presence: ConfigPresence,
    env: &EnvParts,
) -> ResolvedTheme {
    theme::resolve_theme(
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

    let env = EnvParts::from_current_process();

    // Resolve theme through the selection ladder.
    let resolved_theme =
        resolve_startup_theme(args.theme.as_deref(), &config, config_presence, &env);

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

    let app = App::new(
        session,
        resolved_theme,
        config.editor.wrap,
        config.relative_line_numbers,
        Box::new(crate::clipboard::Osc52Clipboard::stdout()),
        Box::new(crate::config::FileConfigStore::production()),
        std::time::Instant::now(),
    );

    // Enter raw mode + alternate screen + install panic/signal hooks.
    let _guard = TerminalGuard::new()?;

    let terminal = Terminal::new(CrosstermBackend::new(stdout()))?;

    // NFR-5 verification: deliberate panic while the terminal is in raw mode.
    // The panic hook should restore the terminal before the default handler.
    if args.panic_test {
        panic!("--panic-test: deliberate panic for NFR-5 verification");
    }

    // Run the event loop.
    event::run_event_loop(app, terminal)
}

#[cfg(test)]
mod startup_tests {
    use super::*;
    use crate::config::EditorConfig;
    use crate::theme::ThemeSource;

    #[test]
    fn production_resolver_reports_partial_config_slot_as_fallback() {
        let config = Config {
            editor: EditorConfig { wrap: false },
            ..Config::default()
        };
        let env = EnvParts {
            term: Some("xterm-256color".to_string()),
            colorterm: Some("truecolor".to_string()),
            ..EnvParts::default()
        };
        let fallback = resolve_startup_theme(None, &config, ConfigPresence::default(), &env);
        assert_eq!(fallback.name, "default-dark");
        assert_eq!(fallback.source, ThemeSource::Fallback);

        let configured = resolve_startup_theme(
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
}
