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
        Ok(ParseOutcome::Run(args)) => match oom_edit::run(&args) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("oom-edit: {e}");
                ExitCode::FAILURE
            }
        },
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
