//! TUI dependency-hygiene regression tests.
//!
//! The reusable core has its own dependency-boundary checks. These tests cover
//! the workspace policy that belongs to the terminal crate: one crossterm
//! lineage and warning-fatal dependency-check entry points.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root should resolve")
}

fn checked_stdout(output: Output, command: &str) -> String {
    assert!(
        output.status.success(),
        "{command} failed with status {}:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    assert!(
        !stdout.trim().is_empty(),
        "{command} produced no output — dependency check is vacuous"
    );
    stdout
}

#[test]
fn tui_resolves_single_crossterm_version() {
    let output = Command::new("cargo")
        .args([
            "tree",
            "-p",
            "oom-edit",
            "--edges",
            "normal",
            "--offline",
            "--invert",
            "crossterm",
        ])
        .current_dir(workspace_root())
        .output()
        .expect("cargo tree should run");
    let stdout = checked_stdout(
        output,
        "cargo tree -p oom-edit --edges normal --offline --invert crossterm",
    );

    assert_eq!(
        stdout.lines().next(),
        Some("crossterm v0.29.0"),
        "inverse tree should be rooted at the reviewed crossterm version:\n{stdout}"
    );
    assert!(
        stdout.contains("oom-edit v"),
        "inverse tree should contain the TUI consumer:\n{stdout}"
    );
    assert!(
        stdout.contains("ratatui-crossterm v"),
        "inverse tree should contain Ratatui's Crossterm backend:\n{stdout}"
    );
}

#[test]
fn make_deny_and_check_promote_warnings() {
    let root = workspace_root();

    for target in ["deny", "check"] {
        let output = Command::new("make")
            .args(["--dry-run", target])
            .current_dir(&root)
            .output()
            .unwrap_or_else(|error| panic!("make --dry-run {target} should run: {error}"));
        let stdout = checked_stdout(output, &format!("make --dry-run {target}"));

        assert!(
            stdout.contains("cargo deny check -D warnings"),
            "make --dry-run {target} should expand the shared warning-fatal policy:\n{stdout}"
        );

        let output = Command::new("make")
            .args(["--dry-run", target, "DENY_FLAGS=__DENY_FLAGS_SENTINEL__"])
            .current_dir(&root)
            .output()
            .unwrap_or_else(|error| {
                panic!("make --dry-run {target} with a DENY_FLAGS override should run: {error}")
            });
        let stdout = checked_stdout(
            output,
            &format!("make --dry-run {target} DENY_FLAGS=__DENY_FLAGS_SENTINEL__"),
        );

        assert!(
            stdout.contains("cargo deny __DENY_FLAGS_SENTINEL__"),
            "make --dry-run {target} should consume the shared DENY_FLAGS variable:\n{stdout}"
        );
    }
}

#[test]
fn make_audit_and_check_promote_warnings() {
    let root = workspace_root();

    for target in ["audit", "check"] {
        let output = Command::new("make")
            .args(["--dry-run", target])
            .current_dir(&root)
            .output()
            .unwrap_or_else(|error| panic!("make --dry-run {target} should run: {error}"));
        let stdout = checked_stdout(output, &format!("make --dry-run {target}"));

        assert!(
            stdout.contains("cargo audit -D warnings"),
            "make --dry-run {target} should expand the shared warning-fatal policy:\n{stdout}"
        );

        let output = Command::new("make")
            .args(["--dry-run", target, "AUDIT_FLAGS=__AUDIT_FLAGS_SENTINEL__"])
            .current_dir(&root)
            .output()
            .unwrap_or_else(|error| {
                panic!("make --dry-run {target} with an AUDIT_FLAGS override should run: {error}")
            });
        let stdout = checked_stdout(
            output,
            &format!("make --dry-run {target} AUDIT_FLAGS=__AUDIT_FLAGS_SENTINEL__"),
        );

        assert!(
            stdout.contains("cargo audit __AUDIT_FLAGS_SENTINEL__"),
            "make --dry-run {target} should consume the shared AUDIT_FLAGS variable:\n{stdout}"
        );
    }
}
