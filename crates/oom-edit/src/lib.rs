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
pub(crate) mod command;
pub(crate) mod event;
pub(crate) mod overlay;
pub(crate) mod screens;
pub(crate) mod terminal_guard;
pub(crate) mod theme;
pub(crate) mod widgets;

pub use args::{Args, ParseOutcome};

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

    let app = App::new(session);

    // Enter raw mode + alternate screen + install panic/signal hooks.
    let _guard = TerminalGuard::new()?;

    let terminal = Terminal::new(CrosstermBackend::new(stdout()))?;

    // Run the event loop.
    event::run_event_loop(app, terminal)
}
