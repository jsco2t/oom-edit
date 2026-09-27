use oom_edit::{TerminalError, TerminalGuard, TerminalGuardOptions};

#[cfg(unix)]
use std::process::{Command, Stdio};

#[test]
fn fr_091_guard_options_keep_standalone_keyboard_enhancement_off() {
    assert_eq!(
        TerminalGuardOptions::default(),
        TerminalGuardOptions {
            mouse_capture: true,
            bracketed_paste: true,
            keyboard_enhancement: false,
        }
    );
    let _: fn() -> Result<TerminalGuard, TerminalError> = TerminalGuard::new;
    let _: fn(TerminalGuardOptions) -> Result<TerminalGuard, TerminalError> =
        TerminalGuard::with_options;
    let _: fn(&mut TerminalGuard, bool) -> Result<(), TerminalError> =
        TerminalGuard::set_mouse_capture;
}

#[test]
fn fr_093_unsafe_scope_is_confined_to_the_existing_signal_module() {
    let source = include_str!("../src/terminal_guard.rs");
    assert_eq!(
        source
            .lines()
            .filter(|line| line.trim() == "#[allow(unsafe_code)]")
            .count(),
        1
    );
    assert_eq!(source.matches("unsafe {").count(), 2);
    assert_eq!(source.matches("mod signals {").count(), 2);
    let audited = source.find("\n#[allow(unsafe_code)]\n").unwrap();
    assert!(!source[..audited].contains("unsafe {"));
}

#[cfg(unix)]
#[test]
fn terminal_guard_probe_child() {
    let Ok(scenario) = std::env::var("OOM_GUARD_PROBE") else {
        return;
    };
    let before = stty_state();
    let options = TerminalGuardOptions::default();
    match scenario.as_str() {
        "normal" => {
            let guard = TerminalGuard::with_options(options).unwrap();
            assert_ne!(stty_state(), before, "raw mode must change termios");
            assert!(matches!(TerminalGuard::new(), Err(TerminalError::Busy)));
            drop(guard);
            assert_eq!(stty_state(), before);
            let second = TerminalGuard::with_options(options).unwrap();
            drop(second);
            assert_eq!(stty_state(), before);
            assert!(std::panic::catch_unwind(|| panic!("after guard drop")).is_err());
            println!("GUARD_NORMAL_OK");
        }
        "minimal" => {
            let guard = TerminalGuard::with_options(TerminalGuardOptions {
                mouse_capture: false,
                bracketed_paste: false,
                keyboard_enhancement: false,
            })
            .unwrap();
            drop(guard);
            assert_eq!(stty_state(), before);
            println!("GUARD_MINIMAL_OK");
        }
        "mouse" => {
            let mut guard = TerminalGuard::with_options(TerminalGuardOptions {
                mouse_capture: false,
                bracketed_paste: false,
                keyboard_enhancement: false,
            })
            .unwrap();
            guard.set_mouse_capture(true).unwrap();
            guard.set_mouse_capture(true).unwrap();
            guard.set_mouse_capture(false).unwrap();
            guard.set_mouse_capture(false).unwrap();
            guard.set_mouse_capture(true).unwrap();
            drop(guard);
            assert_eq!(stty_state(), before);
            println!("GUARD_MOUSE_OK");
        }
        "enhanced" | "unsupported" | "enhanced-panic" | "enhanced-signal" => {
            let enhanced_options = TerminalGuardOptions {
                mouse_capture: false,
                bracketed_paste: false,
                keyboard_enhancement: true,
            };
            if scenario == "enhanced-panic" {
                assert!(std::panic::catch_unwind(|| {
                    let _guard = TerminalGuard::with_options(enhanced_options).unwrap();
                    panic!("intentional enhanced panic");
                })
                .is_err());
                assert_eq!(stty_state(), before);
            } else {
                let guard = TerminalGuard::with_options(enhanced_options).unwrap();
                if scenario == "enhanced-signal" {
                    println!("GUARD_ENHANCED_SIGNAL_READY");
                    use std::io::Write;
                    std::io::stdout().flush().unwrap();
                    Command::new("kill")
                        .arg("-TERM")
                        .arg(std::process::id().to_string())
                        .status()
                        .unwrap();
                    unreachable!("signal handler must terminate the process");
                }
                drop(guard);
                assert_eq!(stty_state(), before);
            }
            println!("GUARD_ENHANCED_OK");
        }
        "panic" => {
            let result = std::panic::catch_unwind(|| {
                let _guard = TerminalGuard::with_options(options).unwrap();
                panic!("intentional guard panic");
            });
            assert!(result.is_err());
            assert_eq!(stty_state(), before);
            println!("GUARD_PANIC_OK");
        }
        "signal" | "hup" => {
            let _guard = TerminalGuard::with_options(options).unwrap();
            println!("GUARD_SIGNAL_READY");
            use std::io::Write;
            std::io::stdout().flush().unwrap();
            let status = Command::new("kill")
                .arg(if scenario == "hup" { "-HUP" } else { "-TERM" })
                .arg(std::process::id().to_string())
                .status()
                .unwrap();
            assert!(status.success());
            unreachable!("signal handler must terminate the process");
        }
        "post-drop-signal" => {
            let guard = TerminalGuard::with_options(options).unwrap();
            drop(guard);
            assert_eq!(stty_state(), before);
            println!("GUARD_DROPPED_BEFORE_SIGNAL");
            use std::io::Write;
            std::io::stdout().flush().unwrap();
            Command::new("kill")
                .arg("-TERM")
                .arg(std::process::id().to_string())
                .status()
                .unwrap();
            unreachable!("signal handler must terminate the process");
        }
        _ => panic!("unknown probe scenario"),
    }
}

#[cfg(unix)]
fn stty_state() -> Vec<u8> {
    let output = Command::new("stty")
        .arg("-g")
        .stdin(Stdio::inherit())
        .output()
        .unwrap();
    assert!(output.status.success(), "stty failed: {output:?}");
    output.stdout
}

#[cfg(unix)]
fn probe(scenario: &str) -> Vec<u8> {
    let executable = std::env::current_exe().unwrap();
    let output = Command::new("python3")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../scripts/terminal_guard_pty.py"
        ))
        .arg(executable)
        .arg("--exact")
        .arg("terminal_guard_probe_child")
        .arg("--nocapture")
        .env("OOM_GUARD_PROBE", scenario)
        .env(
            "OOM_GUARD_EMULATE_KEYBOARD",
            if scenario.starts_with("enhanced") {
                "supported"
            } else {
                "unsupported"
            },
        )
        .output()
        .unwrap();
    assert!(output.status.success(), "probe failed: {output:?}");
    assert!(
        output
            .stdout
            .windows(b"GUARD_TERMIOS_RESTORED=True".len())
            .any(|part| part == b"GUARD_TERMIOS_RESTORED=True"),
        "termios not restored: {output:?}"
    );
    output.stdout
}

#[cfg(unix)]
fn count(bytes: &[u8], needle: &[u8]) -> usize {
    bytes
        .windows(needle.len())
        .filter(|window| *window == needle)
        .count()
}

#[cfg(unix)]
#[test]
fn fr_090_exclusive_guard_and_sequential_reuse_restore_termios() {
    let output = probe("normal");
    assert!(output
        .windows(b"GUARD_NORMAL_OK".len())
        .any(|part| part == b"GUARD_NORMAL_OK"));
    assert_eq!(count(&output, b"\x1b[?1049h"), 2);
    assert_eq!(count(&output, b"\x1b[?1049l"), 2);
    assert_eq!(count(&output, b"\x1b[?1000h"), 2);
    assert_eq!(count(&output, b"\x1b[?1000l"), 2);
    assert_eq!(count(&output, b"\x1b[?2004h"), 2);
    assert_eq!(count(&output, b"\x1b[?2004l"), 2);
    assert_eq!(count(&output, b"\x1b[<1u"), 0);
}

#[cfg(unix)]
#[test]
fn fr_090_panic_and_signal_restore_once_without_stale_hook_writes() {
    let panic_output = probe("panic");
    assert!(panic_output
        .windows(b"GUARD_PANIC_OK".len())
        .any(|part| part == b"GUARD_PANIC_OK"));
    assert_eq!(count(&panic_output, b"\x1b[?1049l"), 1);
    assert_eq!(count(&panic_output, b"\x1b[?1000l"), 1);
    assert_eq!(count(&panic_output, b"\x1b[?2004l"), 1);

    for signal in ["signal", "hup"] {
        let signal_output = probe(signal);
        assert!(signal_output
            .windows(b"GUARD_SIGNAL_READY".len())
            .any(|part| part == b"GUARD_SIGNAL_READY"));
        assert_eq!(count(&signal_output, b"\x1b[?1049l"), 1);
        assert_eq!(count(&signal_output, b"\x1b[?1000l"), 1);
        assert_eq!(count(&signal_output, b"\x1b[?2004l"), 1);
    }
    let dropped = probe("post-drop-signal");
    assert_eq!(count(&dropped, b"\x1b[?1049l"), 1);
    assert_eq!(count(&dropped, b"\x1b[?1000l"), 1);
    assert_eq!(count(&dropped, b"\x1b[?2004l"), 1);
}

#[cfg(unix)]
#[test]
fn fr_092_runtime_mouse_and_minimal_options_emit_exact_captures() {
    let minimal = probe("minimal");
    assert_eq!(count(&minimal, b"\x1b[?1000h"), 0);
    assert_eq!(count(&minimal, b"\x1b[?1000l"), 0);
    assert_eq!(count(&minimal, b"\x1b[?2004h"), 0);
    assert_eq!(count(&minimal, b"\x1b[?2004l"), 0);

    let mouse = probe("mouse");
    assert_eq!(count(&mouse, b"\x1b[?1000h"), 2);
    assert_eq!(count(&mouse, b"\x1b[?1000l"), 2);
    assert_eq!(count(&mouse, b"\x1b[?2004h"), 0);
}

#[cfg(unix)]
#[test]
fn fr_091_enhancement_pop_once_only_after_supported_push() {
    let minimal = probe("minimal");
    assert_eq!(count(&minimal, b"\x1b[>3u"), 0);
    assert_eq!(count(&minimal, b"\x1b[<1u"), 0);

    let enhanced = probe("enhanced");
    assert_eq!(count(&enhanced, b"\x1b[>3u"), 1);
    assert_eq!(count(&enhanced, b"\x1b[<1u"), 1);

    let unsupported = probe("unsupported");
    assert_eq!(count(&unsupported, b"\x1b[>3u"), 0);
    assert_eq!(count(&unsupported, b"\x1b[<1u"), 0);

    for scenario in ["enhanced-panic", "enhanced-signal"] {
        let output = probe(scenario);
        assert_eq!(count(&output, b"\x1b[>3u"), 1, "{scenario}");
        assert_eq!(count(&output, b"\x1b[<1u"), 1, "{scenario}");
    }
}
