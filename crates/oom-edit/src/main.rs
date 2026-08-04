//! Binary entry point for `oom-edit`.
//!
//! A thin shell: parse CLI arguments (before any terminal setup), handle the
//! print-and-exit forms (`--help`/`--version`) and usage errors here, then
//! hand the parsed [`Args`] to the library's `run`. All real logic lives
//! in the library crate; `main` only adapts errors into a clean process exit.

use std::process::ExitCode;

use oom_edit::ParseOutcome;

fn main() -> ExitCode {
    match oom_edit::Args::parse(std::env::args()) {
        Ok(ParseOutcome::Run(args)) => {
            // Hidden debug flag: trigger a panic for NFR-5 verification.
            // This flag is intentionally undocumented in --help.
            if args.panic_test {
                // Terminal guard hooks are installed during run(), so a panic
                // here will trigger the panic hook and restore the terminal.
                // We call run() first to ensure the hooks are armed, then panic.
                let result = oom_edit::run(&args);
                if result.is_err() {
                    // If run() failed (e.g., terminal setup error), the hooks
                    // may not be installed yet. Panic anyway to test the hook.
                }
                panic!("--panic-test: deliberate panic for NFR-5 verification");
            }

            match oom_edit::run(&args) {
                Ok(()) => ExitCode::SUCCESS,
                Err(e) => {
                    eprintln!("oom-edit: {e}");
                    ExitCode::FAILURE
                }
            }
        }
        // `--help` / `--version`: print to stdout and exit 0.
        Ok(ParseOutcome::Message(msg)) => {
            print!("{msg}");
            ExitCode::SUCCESS
        }
        // Unknown flag / missing value: usage already formatted, print to stderr
        // and exit 1 — before any terminal state is touched.
        Err(usage) => {
            eprint!("{usage}");
            ExitCode::FAILURE
        }
    }
}
