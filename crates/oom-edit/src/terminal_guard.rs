//! Exclusive RAII terminal setup, with acquisition-aware panic and signal restore.
//!
//! See the "Signals" block at the bottom for why this module carries exactly
//! one audited `#[allow(unsafe_code)]` block.

use std::io::{stdout, Write};
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::Once;

use crossterm::cursor::SetCursorStyle;
use crossterm::event::{
    DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
    KeyboardEnhancementFlags, PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, supports_keyboard_enhancement, EndSynchronizedUpdate,
    EnterAlternateScreen, LeaveAlternateScreen,
};

const RAW: u8 = 1 << 0;
const ALTERNATE: u8 = 1 << 1;
const MOUSE: u8 = 1 << 2;
const PASTE: u8 = 1 << 3;
const KEYBOARD: u8 = 1 << 4;

static ACTIVE: AtomicBool = AtomicBool::new(false);
/// Bits are published before each terminal command, so a signal during setup
/// can conservatively undo an operation that may have partially reached the TTY.
static ACQUIRED: AtomicU8 = AtomicU8::new(0);

/// Terminal features owned by this guard. Standalone retains its legacy mouse
/// and bracketed-paste defaults; keyboard disambiguation is opt-in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TerminalGuardOptions {
    /// Acquire mouse capture on guard creation.
    pub mouse_capture: bool,
    /// Acquire bracketed-paste reporting on guard creation.
    pub bracketed_paste: bool,
    /// Acquire supported keyboard disambiguation; standalone defaults to false.
    pub keyboard_enhancement: bool,
}

impl Default for TerminalGuardOptions {
    fn default() -> Self {
        Self {
            mouse_capture: true,
            bracketed_paste: true,
            keyboard_enhancement: false,
        }
    }
}

/// Owns one process-wide raw-mode and alternate-screen session.
pub struct TerminalGuard {
    _private: (),
}

/// Errors that can occur during terminal setup.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminalError {
    /// Another live guard owns the process terminal.
    Busy,
    /// Crossterm failed to enable raw mode.
    RawMode(std::io::ErrorKind),
    /// Crossterm failed to enter the alternate screen.
    AlternateScreen(std::io::ErrorKind),
    /// Mouse capture could not be toggled.
    MouseCapture(std::io::ErrorKind),
    /// Bracketed paste could not be enabled.
    BracketedPaste(std::io::ErrorKind),
    /// Supported keyboard enhancement could not be pushed.
    KeyboardEnhancement(std::io::ErrorKind),
    /// A panic already restored the terminal owned by this guard.
    Inactive,
}

impl std::fmt::Display for TerminalError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TerminalError::Busy => f.write_str("a terminal guard is already active"),
            TerminalError::RawMode(kind) => write!(f, "failed to enable raw mode: {kind}"),
            TerminalError::AlternateScreen(kind) => {
                write!(f, "failed to enter alternate screen: {kind}")
            }
            TerminalError::MouseCapture(kind) => {
                write!(f, "failed to enable mouse capture: {kind}")
            }
            TerminalError::BracketedPaste(kind) => {
                write!(f, "failed to enable bracketed paste: {kind}")
            }
            TerminalError::KeyboardEnhancement(kind) => {
                write!(f, "failed to enable keyboard enhancement: {kind}")
            }
            TerminalError::Inactive => f.write_str("terminal guard is no longer active"),
        }
    }
}

impl std::error::Error for TerminalError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SetupStep {
    Raw,
    Alternate,
    Mouse,
    Paste,
    Keyboard,
}

impl SetupStep {
    const fn bit(self) -> u8 {
        match self {
            Self::Raw => RAW,
            Self::Alternate => ALTERNATE,
            Self::Mouse => MOUSE,
            Self::Paste => PASTE,
            Self::Keyboard => KEYBOARD,
        }
    }

    const fn error(self, kind: std::io::ErrorKind) -> TerminalError {
        match self {
            Self::Raw => TerminalError::RawMode(kind),
            Self::Alternate => TerminalError::AlternateScreen(kind),
            Self::Mouse => TerminalError::MouseCapture(kind),
            Self::Paste => TerminalError::BracketedPaste(kind),
            Self::Keyboard => TerminalError::KeyboardEnhancement(kind),
        }
    }
}

/// The same acquisition order drives the real terminal and fault-injection
/// tests. A bit is published before each operation for signal-time rollback.
fn setup_steps(
    options: TerminalGuardOptions,
    mut keyboard_supported: impl FnMut() -> bool,
    mut perform: impl FnMut(SetupStep) -> std::io::Result<()>,
    mut update: impl FnMut(u8, bool),
) -> Result<(), TerminalError> {
    for step in [
        SetupStep::Raw,
        SetupStep::Alternate,
        SetupStep::Mouse,
        SetupStep::Paste,
        SetupStep::Keyboard,
    ] {
        let requested = match step {
            SetupStep::Raw | SetupStep::Alternate => true,
            SetupStep::Mouse => options.mouse_capture,
            SetupStep::Paste => options.bracketed_paste,
            SetupStep::Keyboard => options.keyboard_enhancement && keyboard_supported(),
        };
        if !requested {
            continue;
        }
        update(step.bit(), true);
        if let Err(error) = perform(step) {
            update(step.bit(), false);
            return Err(step.error(error.kind()));
        }
    }
    Ok(())
}

impl TerminalGuard {
    /// Enter raw mode and alternate screen with standalone-compatible defaults.
    ///
    /// # Errors
    /// Returns a typed setup error, restoring acquired features first.
    pub fn new() -> Result<Self, TerminalError> {
        Self::with_options(TerminalGuardOptions::default())
    }

    /// Acquire the requested terminal features, exclusively for this process.
    ///
    /// # Errors
    /// Returns `Busy` if another guard is active or a typed setup failure.
    pub fn with_options(options: TerminalGuardOptions) -> Result<Self, TerminalError> {
        if ACTIVE
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return Err(TerminalError::Busy);
        }
        install_panic_hook();
        signals::install();
        let result = setup_steps(
            options,
            || supports_keyboard_enhancement().unwrap_or(false),
            |step| match step {
                SetupStep::Raw => enable_raw_mode(),
                SetupStep::Alternate => execute!(stdout(), EnterAlternateScreen),
                SetupStep::Mouse => execute!(stdout(), EnableMouseCapture),
                SetupStep::Paste => execute!(stdout(), EnableBracketedPaste),
                SetupStep::Keyboard => execute!(
                    stdout(),
                    PushKeyboardEnhancementFlags(
                        KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
                            | KeyboardEnhancementFlags::REPORT_EVENT_TYPES
                    )
                ),
            },
            |bit, acquired| {
                if acquired {
                    ACQUIRED.fetch_or(bit, Ordering::AcqRel);
                } else {
                    ACQUIRED.fetch_and(!bit, Ordering::AcqRel);
                }
            },
        );
        if let Err(error) = result {
            return setup_failed(error);
        }

        Ok(Self { _private: () })
    }

    /// Toggle mouse capture without recreating the guard.
    ///
    /// # Errors
    /// Returns a typed error if the active terminal cannot accept the command.
    pub fn set_mouse_capture(&mut self, enabled: bool) -> Result<(), TerminalError> {
        if !ACTIVE.load(Ordering::Acquire) || ACQUIRED.load(Ordering::Acquire) == 0 {
            return Err(TerminalError::Inactive);
        }
        let currently_enabled = ACQUIRED.load(Ordering::Acquire) & MOUSE != 0;
        if currently_enabled == enabled {
            return Ok(());
        }
        if enabled {
            ACQUIRED.fetch_or(MOUSE, Ordering::AcqRel);
            if let Err(error) = execute!(stdout(), EnableMouseCapture) {
                ACQUIRED.fetch_and(!MOUSE, Ordering::AcqRel);
                return Err(TerminalError::MouseCapture(error.kind()));
            }
        } else {
            ACQUIRED.fetch_and(!MOUSE, Ordering::AcqRel);
            if let Err(error) = execute!(stdout(), DisableMouseCapture) {
                ACQUIRED.fetch_or(MOUSE, Ordering::AcqRel);
                return Err(TerminalError::MouseCapture(error.kind()));
            }
        }
        Ok(())
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        restore_active();
        ACTIVE.store(false, Ordering::Release);
    }
}

fn setup_failed<T>(error: TerminalError) -> Result<T, TerminalError> {
    restore_active();
    ACTIVE.store(false, Ordering::Release);
    Err(error)
}

fn restore_active() {
    let acquired = ACQUIRED.swap(0, Ordering::AcqRel);
    restore_terminal(&mut stdout(), acquired);
}

/// Restore only features this guard acquired, in reverse setup order.
fn restore_terminal(out: &mut impl Write, acquired: u8) {
    if acquired & KEYBOARD != 0 {
        let _ = execute!(out, PopKeyboardEnhancementFlags);
    }
    if acquired & PASTE != 0 {
        let _ = execute!(out, DisableBracketedPaste);
    }
    if acquired & MOUSE != 0 {
        let _ = execute!(out, DisableMouseCapture);
    }
    if acquired & ALTERNATE != 0 {
        let _ = execute!(out, EndSynchronizedUpdate);
        let _ = execute!(out, SetCursorStyle::DefaultUserShape);
        let _ = execute!(out, LeaveAlternateScreen);
    }
    if acquired & RAW != 0 {
        let _ = disable_raw_mode();
    }
}

/// Global panic hook that restores the terminal before the default hook prints.
///
/// `Once`-gated: tests construct several `TerminalGuard`s across a run, and
/// installing the hook repeatedly would chain (and progressively slow) it.
pub fn install_panic_hook() {
    static ONCE: Once = Once::new();
    install_hook_once(&ONCE, || {
        let default = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            restore_then_default(restore_active, || default(info));
        }));
    });
}

/// Install a hook body exactly once through the same seam used in production.
fn install_hook_once(once: &Once, install: impl FnOnce()) {
    once.call_once(install);
}

/// Run terminal restoration before delegating to the previous panic hook.
fn restore_then_default(restore: impl FnOnce(), default: impl FnOnce()) {
    restore();
    default();
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
    use super::{ACQUIRED, ALTERNATE, KEYBOARD, MOUSE, PASTE, RAW};
    use std::sync::atomic::{AtomicBool, Ordering};

    /// Cooked-mode termios captured before raw mode, restored by the handler.
    static mut SAVED_TERMIOS: Option<libc::termios> = None;
    static TERMIOS_SAVED: AtomicBool = AtomicBool::new(false);
    static INSTALLED: AtomicBool = AtomicBool::new(false);

    pub(super) const POP_KEYBOARD: &[u8] = b"\x1b[<1u";
    pub(super) const DISABLE_PASTE: &[u8] = b"\x1b[?2004l";
    pub(super) const DISABLE_MOUSE: &[u8] =
        b"\x1b[?1006l\x1b[?1015l\x1b[?1003l\x1b[?1002l\x1b[?1000l";
    pub(super) const LEAVE_ALTERNATE: &[u8] = b"\x1b[?2026l\x1b[0 q\x1b[?1049l";

    /// Refresh cooked termios for this guard and install handlers once.
    /// An inactive handler emits no terminal bytes.
    pub fn install() {
        // SAFETY: the exclusive guard slot is held and ACQUIRED is zero, so
        // the handler cannot read this snapshot until publication of RAW.
        unsafe {
            TERMIOS_SAVED.store(false, Ordering::SeqCst);
            let mut termios = std::mem::zeroed::<libc::termios>();
            if libc::tcgetattr(libc::STDIN_FILENO, std::ptr::addr_of_mut!(termios)) == 0 {
                SAVED_TERMIOS = Some(termios);
                TERMIOS_SAVED.store(true, Ordering::SeqCst);
            }
            if !INSTALLED.swap(true, Ordering::SeqCst) {
                let handler_ptr = handler as *const () as libc::sighandler_t;
                libc::signal(libc::SIGHUP, handler_ptr);
                libc::signal(libc::SIGTERM, handler_ptr);
            }
        }
    }

    /// Async-signal-safe handler: claim the acquired bits once, restore only
    /// those features, then re-raise the signal's default disposition.
    extern "C" fn handler(sig: libc::c_int) {
        // SAFETY: only async-signal-safe syscalls (`tcsetattr`, `write`,
        // `signal`, `raise`) and reads of statics written before the handler
        // could fire. No allocation, no locks, no Rust runtime services.
        unsafe {
            let acquired = ACQUIRED.swap(0, Ordering::AcqRel);
            if acquired & RAW != 0 && TERMIOS_SAVED.load(Ordering::SeqCst) {
                if let Some(termios) = std::ptr::addr_of!(SAVED_TERMIOS).read() {
                    libc::tcsetattr(
                        libc::STDIN_FILENO,
                        libc::TCSANOW,
                        std::ptr::addr_of!(termios),
                    );
                }
            }
            if acquired & KEYBOARD != 0 {
                libc::write(
                    libc::STDOUT_FILENO,
                    POP_KEYBOARD.as_ptr().cast(),
                    POP_KEYBOARD.len(),
                );
            }
            if acquired & PASTE != 0 {
                libc::write(
                    libc::STDOUT_FILENO,
                    DISABLE_PASTE.as_ptr().cast(),
                    DISABLE_PASTE.len(),
                );
            }
            if acquired & MOUSE != 0 {
                libc::write(
                    libc::STDOUT_FILENO,
                    DISABLE_MOUSE.as_ptr().cast(),
                    DISABLE_MOUSE.len(),
                );
            }
            if acquired & ALTERNATE != 0 {
                libc::write(
                    libc::STDOUT_FILENO,
                    LEAVE_ALTERNATE.as_ptr().cast(),
                    LEAVE_ALTERNATE.len(),
                );
            }
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
    //! Terminal guard wiring tests.
    //!
    //! `enable_raw_mode()` errors with no TTY, so `TerminalGuard::new` cannot
    //! be constructed in CI. These are deterministic in-process tests of the
    //! restore wiring instead.

    use std::cell::{Cell, RefCell};

    use super::*;

    /// The leave-alternate-screen escape emitted by `restore_terminal`.
    const LEAVE_ALT_SCREEN: &[u8] = b"\x1b[?1049l";
    /// The complete disable-mouse-capture sequence emitted by crossterm.
    const DISABLE_MOUSE: &[u8] = b"\x1b[?1006l\x1b[?1015l\x1b[?1003l\x1b[?1002l\x1b[?1000l";
    /// The user-default cursor-shape escape emitted by every restore path.
    const DEFAULT_CURSOR_SHAPE: &[u8] = b"\x1b[0 q";
    /// The end-synchronized-update escape emitted defensively during cleanup.
    const END_SYNCHRONIZED_UPDATE: &[u8] = b"\x1b[?2026l";
    /// The bracketed-paste disable escape emitted by every restore path.
    const DISABLE_BRACKETED_PASTE: &[u8] = b"\x1b[?2004l";

    /// Every optional feature is restored exactly when it was acquired.
    #[test]
    fn restore_terminal_writes_only_acquired_feature_escapes() {
        for bits in 0..=(RAW | ALTERNATE | MOUSE | PASTE | KEYBOARD) {
            let mut sink = Vec::new();
            restore_terminal(&mut sink, bits);
            let mut expected = Vec::new();
            if bits & KEYBOARD != 0 {
                expected.extend_from_slice(b"\x1b[<1u");
            }
            if bits & PASTE != 0 {
                expected.extend_from_slice(DISABLE_BRACKETED_PASTE);
            }
            if bits & MOUSE != 0 {
                expected.extend_from_slice(DISABLE_MOUSE);
            }
            if bits & ALTERNATE != 0 {
                expected.extend_from_slice(END_SYNCHRONIZED_UPDATE);
                expected.extend_from_slice(DEFAULT_CURSOR_SHAPE);
                expected.extend_from_slice(LEAVE_ALT_SCREEN);
            }
            assert_eq!(sink, expected, "acquired bits {bits:#07b}");
        }
    }

    #[test]
    fn setup_failure_rolls_back_every_published_feature() {
        let options = TerminalGuardOptions {
            mouse_capture: true,
            bracketed_paste: true,
            keyboard_enhancement: true,
        };
        for failed in [
            SetupStep::Raw,
            SetupStep::Alternate,
            SetupStep::Mouse,
            SetupStep::Paste,
            SetupStep::Keyboard,
        ] {
            let published = Cell::new(0_u8);
            let reached = RefCell::new(Vec::new());
            let result = setup_steps(
                options,
                || true,
                |step| {
                    reached.borrow_mut().push(step);
                    if step == failed {
                        Err(std::io::Error::from(std::io::ErrorKind::BrokenPipe))
                    } else {
                        Ok(())
                    }
                },
                |bit, acquired| {
                    published.set(if acquired {
                        published.get() | bit
                    } else {
                        published.get() & !bit
                    });
                },
            );
            assert_eq!(result, Err(failed.error(std::io::ErrorKind::BrokenPipe)));
            assert_eq!(reached.borrow().last(), Some(&failed));
            assert_eq!(published.get() & failed.bit(), 0);
            let mut rollback = Vec::new();
            restore_terminal(&mut rollback, published.get());
            assert_eq!(
                rollback
                    .windows(b"\x1b[<1u".len())
                    .filter(|window| *window == b"\x1b[<1u")
                    .count(),
                usize::from(published.get() & KEYBOARD != 0)
            );
            assert_eq!(
                rollback
                    .windows(LEAVE_ALT_SCREEN.len())
                    .filter(|window| *window == LEAVE_ALT_SCREEN)
                    .count(),
                usize::from(published.get() & ALTERNATE != 0)
            );
            assert_eq!(
                rollback
                    .windows(DISABLE_MOUSE.len())
                    .filter(|window| *window == DISABLE_MOUSE)
                    .count(),
                usize::from(published.get() & MOUSE != 0)
            );
            assert_eq!(
                rollback
                    .windows(DISABLE_BRACKETED_PASTE.len())
                    .filter(|window| *window == DISABLE_BRACKETED_PASTE)
                    .count(),
                usize::from(published.get() & PASTE != 0)
            );
        }
    }

    #[test]
    fn optional_setup_skips_unrequested_and_unsupported_features() {
        for options in [
            TerminalGuardOptions {
                mouse_capture: false,
                bracketed_paste: false,
                keyboard_enhancement: false,
            },
            TerminalGuardOptions {
                mouse_capture: true,
                bracketed_paste: false,
                keyboard_enhancement: true,
            },
            TerminalGuardOptions {
                mouse_capture: false,
                bracketed_paste: true,
                keyboard_enhancement: true,
            },
        ] {
            let published = Cell::new(0_u8);
            setup_steps(
                options,
                || false,
                |_| Ok(()),
                |bit, acquired| {
                    published.set(if acquired {
                        published.get() | bit
                    } else {
                        published.get() & !bit
                    });
                },
            )
            .unwrap();
            assert_eq!(published.get() & MOUSE != 0, options.mouse_capture);
            assert_eq!(published.get() & PASTE != 0, options.bracketed_paste);
            assert_eq!(published.get() & KEYBOARD, 0);
        }
    }

    /// The production panic-hook composition runs the restore body *before*
    /// delegating to the previous hook. Deterministic, in-process, no TTY.
    #[test]
    fn panic_hook_wiring_runs_restore_before_default() {
        let events = RefCell::new(Vec::new());

        restore_then_default(
            || events.borrow_mut().push("restore"),
            || events.borrow_mut().push("default"),
        );

        assert_eq!(*events.borrow(), ["restore", "default"]);
    }

    /// The production installation seam invokes its installer exactly once.
    #[test]
    fn panic_hook_install_is_idempotent() {
        let once = Once::new();
        let calls = Cell::new(0);

        for _ in 0..3 {
            install_hook_once(&once, || calls.set(calls.get() + 1));
        }

        assert_eq!(calls.get(), 1);
    }

    /// The signal handler uses the same escape bytes and restoration order.
    #[cfg(unix)]
    #[test]
    fn signal_restore_uses_the_exact_acquired_feature_bytes() {
        let bytes = [
            signals::POP_KEYBOARD,
            signals::DISABLE_PASTE,
            signals::DISABLE_MOUSE,
            signals::LEAVE_ALTERNATE,
        ]
        .concat();
        assert_eq!(signals::POP_KEYBOARD, b"\x1b[<1u");
        assert_eq!(signals::DISABLE_PASTE, DISABLE_BRACKETED_PASTE);
        assert_eq!(signals::DISABLE_MOUSE, DISABLE_MOUSE);
        let cursor = bytes
            .windows(DEFAULT_CURSOR_SHAPE.len())
            .position(|w| w == DEFAULT_CURSOR_SHAPE)
            .expect("signal restore resets the cursor shape");
        let synchronized = bytes
            .windows(END_SYNCHRONIZED_UPDATE.len())
            .position(|w| w == END_SYNCHRONIZED_UPDATE)
            .expect("signal restore ends synchronized updates");
        let paste = bytes
            .windows(DISABLE_BRACKETED_PASTE.len())
            .position(|w| w == DISABLE_BRACKETED_PASTE)
            .expect("signal restore disables bracketed paste");
        let mouse = bytes
            .windows(DISABLE_MOUSE.len())
            .position(|w| w == DISABLE_MOUSE)
            .expect("signal restore disables mouse");
        let alt = bytes
            .windows(LEAVE_ALT_SCREEN.len())
            .position(|w| w == LEAVE_ALT_SCREEN)
            .expect("signal restore leaves alternate screen");
        assert!(
            paste < mouse && mouse < synchronized && synchronized < cursor && cursor < alt,
            "optional features must be restored before alternate-screen cleanup"
        );
    }
}
