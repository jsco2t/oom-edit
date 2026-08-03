//! Integration test that runs the headless example binary and verifies it
//! exits with status 0.
//!
//! The test captures stdout and asserts on key outputs (mode, line count,
//! source frame lines) to verify meaningful behavior rather than just
//! checking that the binary doesn't crash.

use std::process::Command;

#[test]
fn headless_example_runs_successfully() {
    // Run the example (cargo run builds if needed)
    let output = Command::new("cargo")
        .args([
            "run",
            "--offline",
            "--locked",
            "--example",
            "headless",
            "-p",
            "oom-edit-core",
        ])
        .output()
        .expect("cargo run --example headless should run");

    assert!(
        output.status.success(),
        "headless example should exit 0: {}\n{}",
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout)
    );

    // Verify meaningful output (not just exit 0)
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Mode: Normal"),
        "headless example should report Normal mode"
    );
    assert!(
        stdout.contains("Lines:"),
        "headless example should report line count"
    );
    assert!(
        stdout.contains("Source frame"),
        "headless example should render source frame"
    );
    assert!(
        stdout.contains("View layout"),
        "headless example should render view layout"
    );
}
