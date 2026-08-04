//! `TerminalGuard` — RAII terminal setup/teardown + a panic hook + a
//! best-effort fatal-signal handler, so the terminal is always restored.
//!
//! Order matters: the panic hook **and** the signal handler are installed
//! *first*, before any terminal state is touched, so (a) a panic during
//! construction still restores the terminal and (b) the hook is installable
//! /testable without a TTY. Restore is factored into [`restore_terminal`]
//! so the wiring test can inject a buffer, and the alternate-screen error
//! path explicitly undoes raw mode (a bare `?` would leak it).
//!
//! See the "Signals" block at the bottom for why this module carries exactly
//! one audited `#[allow(unsafe_code)]` block.

use std::io::{stdout, Write};
use std::sync::Once;

use crossterm::event::{DisableMouseCapture, EnableMouseCapture};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};

/// Owns the raw-mode + alternate-screen state for the lifetime of the TUI.
///
/// Constructed only via [`TerminalGuard::new`] (`_private` blocks struct-literal
/// construction), which guarantees the panic hook + signal handler are armed.
pub struct TerminalGuard {
    /// Private field: only constructible via `TerminalGuard::new`.
    _private: (),
}

/// Errors that can occur during terminal setup.
#[derive(Debug)]
pub enum TerminalError {
    /// Crossterm failed to enable raw mode.
    RawMode(std::io::ErrorKind),
    /// Crossterm failed to enter the alternate screen.
    AlternateScreen(std::io::ErrorKind),
}

impl std::fmt::Display for TerminalError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TerminalError::RawMode(kind) => write!(f, "failed to enable raw mode: {kind}"),
            TerminalError::AlternateScreen(kind) => {
                write!(f, "failed to enter alternate screen: {kind}")
            }
        }
    }
}

impl std::error::Error for TerminalError {}

impl TerminalGuard {
    /// Enter raw mode + alternate screen.
    ///
    /// Hooks (panic + signal) are installed *before* any terminal mutation,
    /// so a panic during this call still restores the terminal.
    ///
    /// # Errors
    ///
    /// Returns `TerminalError` if raw mode or alternate screen cannot be
    /// entered. Raw mode is undone on error (no guard is constructed).
    pub fn new() -> Result<Self, TerminalError> {
        // Hooks FIRST: a panic or fatal signal during construction must still
        // restore the terminal.
        install_panic_hook();
        signals::install();

        if enable_raw_mode().is_err() {
            return Err(TerminalError::RawMode(std::io::ErrorKind::Other));
        }

        if let Err(e) = execute!(stdout(), EnterAlternateScreen) {
            // Raw mode already succeeded but no guard is constructed on this
            // path, so Drop will never run to undo it — undo it here.
            let _ = disable_raw_mode();
            return Err(TerminalError::AlternateScreen(e.kind()));
        }

        // Best-effort mouse capture (FR-6.11 is Should-have; proceed keyboard-only).
        let _ = execute!(stdout(), EnableMouseCapture);

        Ok(Self { _private: () })
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        restore_terminal(&mut stdout());
    }
}

/// Best-effort terminal restore, shared by `Drop` and the panic hook so there
/// is exactly one restore path.
///
/// `disable_raw_mode()` is a `tcsetattr` termios syscall and writes no bytes
/// through `out`, so a `Vec<u8>` sink observes the alt-screen escape but NOT
/// the raw-mode restoration. The termios half is covered by manual
/// verification, not the in-process test.
pub fn restore_terminal(out: &mut impl Write) {
    // Disable mouse capture unconditionally (harmless if it was never enabled)
    // so a panic mid-session never leaves the terminal emitting mouse escapes.
    let _ = execute!(out, DisableMouseCapture);
    let _ = execute!(out, LeaveAlternateScreen);
    let _ = disable_raw_mode();
}

/// Global panic hook that restores the terminal before the default hook prints.
///
/// `Once`-gated: tests construct several `TerminalGuard`s across a run, and
/// installing the hook repeatedly would chain (and progressively slow) it.
pub fn install_panic_hook() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let default = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            restore_terminal(&mut stdout());
            default(info);
        }));
    });
}

/// Best-effort `SIGHUP`/`SIGTERM` handling.
///
/// `SIGINT` is moot — in raw mode crossterm delivers Ctrl+C as a key event, not
/// a signal. The exposure is `SIGHUP` (terminal closed) and `SIGTERM` (`kill`),
/// which terminate the process *without* running `Drop` or the panic hook,
/// leaving the terminal in raw mode. We restore the terminal and re-raise the
/// signal's default disposition.
///
/// This is the one place the crate needs `unsafe`: an async-signal-safe handler
/// cannot call crossterm's `disable_raw_mode` (it takes a mutex), so it issues a
/// saved-`termios` `tcsetattr` + a `write(2)` of the leave-alt-screen escape
/// directly. The crate is `deny(unsafe_code)` (workspace default) with exactly
/// one audited `#[allow(unsafe_code)]` module.
#[cfg(unix)]
#[allow(unsafe_code)]
mod signals {
    use std::sync::atomic::{AtomicBool, Ordering};

    /// Cooked-mode termios captured before raw mode, restored by the handler.
    static mut SAVED_TERMIOS: Option<libc::termios> = None;
    static TERMIOS_SAVED: AtomicBool = AtomicBool::new(false);
    static INSTALLED: AtomicBool = AtomicBool::new(false);

    /// Disable all mouse-reporting modes, then leave the alternate screen.
    /// Written directly because `execute!`/`Stdout` are not async-signal-safe.
    pub(super) const RESTORE_TERMINAL: &[u8] =
        b"\x1b[?1000l\x1b[?1002l\x1b[?1003l\x1b[?1015l\x1b[?1006l\x1b[?1049l";

    /// Capture the current (cooked) termios and install the handlers.
    /// Idempotent and called before `enable_raw_mode`.
    pub fn install() {
        if INSTALLED.swap(true, Ordering::SeqCst) {
            return;
        }
        // SAFETY: single-threaded construction path; we capture the terminal's
        // current termios into a process-global before any signal handler is
        // armed, and only read it (never write) from the handler thereafter.
        unsafe {
            let mut termios = std::mem::zeroed::<libc::termios>();
            if libc::tcgetattr(libc::STDIN_FILENO, std::ptr::addr_of_mut!(termios)) == 0 {
                SAVED_TERMIOS = Some(termios);
                TERMIOS_SAVED.store(true, Ordering::SeqCst);
            }
            let handler_ptr = handler as *const () as libc::sighandler_t;
            libc::signal(libc::SIGHUP, handler_ptr);
            libc::signal(libc::SIGTERM, handler_ptr);
        }
    }

    /// Async-signal-safe handler: restore termios, leave the alternate screen,
    /// then re-raise the signal's default disposition.
    extern "C" fn handler(sig: libc::c_int) {
        // SAFETY: only async-signal-safe syscalls (`tcsetattr`, `write`,
        // `signal`, `raise`) and reads of statics written before the handler
        // could fire. No allocation, no locks, no Rust runtime services.
        unsafe {
            if TERMIOS_SAVED.load(Ordering::SeqCst) {
                if let Some(termios) = std::ptr::addr_of!(SAVED_TERMIOS).read() {
                    libc::tcsetattr(
                        libc::STDIN_FILENO,
                        libc::TCSANOW,
                        std::ptr::addr_of!(termios),
                    );
                }
            }
            libc::write(
                libc::STDOUT_FILENO,
                RESTORE_TERMINAL.as_ptr().cast::<libc::c_void>(),
                RESTORE_TERMINAL.len(),
            );
            libc::signal(sig, libc::SIG_DFL);
            libc::raise(sig);
        }
    }
}

/// On non-Unix targets there is no signal handler.
#[cfg(not(unix))]
mod signals {
    pub fn install() {}
}

#[cfg(test)]
mod tests {
    //! Terminal guard wiring tests (NFR-5 / FR-6.8).
    //!
    //! `enable_raw_mode()` errors with no TTY, so `TerminalGuard::new` cannot
    //! be constructed in CI. These are deterministic in-process tests of the
    //! restore wiring instead.

    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};

    use super::*;

    /// The leave-alternate-screen escape emitted by `restore_terminal`.
    const LEAVE_ALT_SCREEN: &[u8] = b"\x1b[?1049l";
    /// The disable-mouse-capture escape emitted by `restore_terminal`.
    const DISABLE_MOUSE: &[u8] = b"\x1b[?1000l";

    fn contains(bytes: &[u8], needle: &[u8]) -> bool {
        bytes.windows(needle.len()).any(|w| w == needle)
    }

    /// The restore body writes the leave-alt-screen escape AND disables mouse
    /// capture into an injected sink (the observable half of the cleanup).
    #[test]
    fn restore_terminal_writes_escapes() {
        let mut sink: Vec<u8> = Vec::new();
        restore_terminal(&mut sink);
        assert!(
            contains(&sink, LEAVE_ALT_SCREEN),
            "restore_terminal must emit the leave-alt-screen escape; got {sink:?}"
        );
        assert!(
            contains(&sink, DISABLE_MOUSE),
            "restore_terminal must disable mouse capture; got {sink:?}"
        );
    }

    /// The panic hook wiring runs the restore body *before* the previous hook.
    /// Deterministic, in-process, no TTY.
    #[test]
    fn panic_hook_wiring_runs_restore_before_default() {
        let restored = Arc::new(AtomicBool::new(false));
        let default_ran_after_restore = Arc::new(AtomicBool::new(false));
        let sink = Arc::new(Mutex::new(Vec::<u8>::new()));

        let prev = std::panic::take_hook();
        {
            let restored = Arc::clone(&restored);
            let default_ran_after_restore = Arc::clone(&default_ran_after_restore);
            let sink = Arc::clone(&sink);
            std::panic::set_hook(Box::new(move |_info| {
                restore_terminal(&mut *sink.lock().expect("sink poisoned"));
                restored.store(true, Ordering::SeqCst);
                default_ran_after_restore.store(restored.load(Ordering::SeqCst), Ordering::SeqCst);
            }));
        }

        let result = std::panic::catch_unwind(|| panic!("deliberate test panic"));
        std::panic::set_hook(prev);

        assert!(result.is_err(), "the closure should have panicked");
        assert!(
            restored.load(Ordering::SeqCst),
            "the restore body must run on panic"
        );
        assert!(
            default_ran_after_restore.load(Ordering::SeqCst),
            "the default hook must run after the restore body"
        );
        let bytes = sink.lock().expect("sink poisoned");
        assert!(
            contains(&bytes, LEAVE_ALT_SCREEN),
            "the restore body must emit the leave-alt-screen escape during a panic"
        );
    }

    /// `install_panic_hook` is `Once`-gated, so calling it repeatedly is safe.
    #[test]
    fn panic_hook_install_is_idempotent() {
        install_panic_hook();
        install_panic_hook();
        install_panic_hook();
    }

    /// The signal handler's escape sequence disables mouse before leaving
    /// the alternate screen (NFR-5 panic-safety extension).
    #[cfg(unix)]
    #[test]
    fn signal_restore_disables_mouse_before_leaving_alt_screen() {
        let bytes = signals::RESTORE_TERMINAL;
        let mouse = bytes
            .windows(DISABLE_MOUSE.len())
            .position(|w| w == DISABLE_MOUSE)
            .expect("signal restore disables mouse");
        let alt = bytes
            .windows(LEAVE_ALT_SCREEN.len())
            .position(|w| w == LEAVE_ALT_SCREEN)
            .expect("signal restore leaves alternate screen");
        assert!(
            mouse < alt,
            "mouse must be disabled before alt-screen leave"
        );
    }
}
