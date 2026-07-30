#![deny(unsafe_code)]

use std::process::ExitCode;

fn main() -> ExitCode {
    println!("oom-edit v{}", env!("CARGO_PKG_VERSION"));
    ExitCode::SUCCESS
}
