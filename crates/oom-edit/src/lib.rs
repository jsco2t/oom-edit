//! `oom-edit` — the TUI binary crate.
//!
//! A `ratatui` + `crossterm` keyboard-driven presentation layer over the
//! `oom-edit-core` editing engine. All business logic, document model,
//! syntax highlighting, and View rendering live in `oom-edit-core`;
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
pub(crate) mod overlay;
pub(crate) mod screens;
#[cfg(test)]
pub(crate) mod snapshot_tests;
pub(crate) mod terminal_guard;
pub(crate) mod theme;
pub(crate) mod widgets;

pub use args::{Args, ParseOutcome};
pub use config::Config;
pub use theme::{EnvParts, Tier};

use std::io::stdout;

use oom_edit_core::session::EditorSession;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use crate::app::App;
use crate::terminal_guard::TerminalGuard;

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
    let config = Config::load();

    // Build EnvParts from environment for the selection ladder.
    let env = EnvParts {
        oom_edit_theme: std::env::var("OOM_EDIT_THEME")
            .ok()
            .map(|s| Box::leak(s.into_boxed_str()) as &'static str),
        no_color: std::env::var("NO_COLOR").is_ok(),
        term: std::env::var("TERM")
            .ok()
            .map(|s| Box::leak(s.into_boxed_str()) as &'static str),
        colorterm: std::env::var("COLORTERM")
            .ok()
            .map(|s| Box::leak(s.into_boxed_str()) as &'static str),
        colorfgbg: std::env::var("COLORFGBG")
            .ok()
            .map(|s| Box::leak(s.into_boxed_str()) as &'static str),
    };

    // Resolve theme through the selection ladder.
    let (theme_name, is_light) = theme::resolve_theme(
        args.theme.as_deref(),
        config.theme.mode.as_deref(),
        Some(&config.theme.dark),
        Some(&config.theme.light),
        &env,
    );

    // Determine tier.
    let tier = env.effective_tier();

    // Announce theme to stderr before entering alternate screen (hidlins pattern).
    eprintln!(
        "oom-edit: theme={theme_name} tier={tier:?}{}",
        if is_light { " light" } else { " dark" }
    );

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
        theme_name,
        is_light,
        tier,
        Box::new(crate::clipboard::Osc52Clipboard::stdout()),
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
