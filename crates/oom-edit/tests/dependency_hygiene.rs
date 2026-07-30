//! TUI dependency-hygiene regression tests.
//!
//! The reusable core has its own dependency-boundary checks. These tests cover
//! the workspace policy that belongs to the terminal crate: one crossterm
//! lineage, warning-fatal dependency-check entry points, and release builds.

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

#[test]
fn make_build_release_builds_only_the_locked_offline_binary() {
    let output = Command::new("make")
        .args(["--dry-run", "build-release"])
        .current_dir(workspace_root())
        .output()
        .expect("make --dry-run build-release should run");
    let stdout = checked_stdout(output, "make --dry-run build-release");

    assert_eq!(
        stdout.trim(),
        "cargo build --release --package oom-edit --bin oom-edit --offline --locked",
        "build-release should build only the release binary with locked offline dependencies"
    );
}

#[test]
fn spell_release_versions_and_exact_path_edges_are_reconciled() {
    let root = workspace_root();
    let tui_manifest = std::fs::read_to_string(root.join("crates/oom-edit/Cargo.toml"))
        .expect("TUI manifest should be readable");
    let core_manifest = std::fs::read_to_string(root.join("crates/oom-edit-core/Cargo.toml"))
        .expect("core manifest should be readable");
    let spell_manifest = std::fs::read_to_string(root.join("crates/oom-spell/Cargo.toml"))
        .expect("spell manifest should be readable");
    let lockfile =
        std::fs::read_to_string(root.join("Cargo.lock")).expect("lockfile should be readable");
    let changelog =
        std::fs::read_to_string(root.join("CHANGELOG.md")).expect("changelog should be readable");

    assert_eq!(env!("CARGO_PKG_VERSION"), "0.5.0");
    assert!(tui_manifest.contains("version = \"0.5.0\""));
    assert!(core_manifest.contains("version = \"0.5.0\""));
    assert!(spell_manifest.contains("version = \"0.1.0\""));
    assert!(tui_manifest
        .contains("oom-edit-core = { path = \"../oom-edit-core\", version = \"=0.5.0\" }"));
    assert_eq!(
        lockfile
            .matches("name = \"oom-edit\"\nversion = \"0.5.0\"")
            .count(),
        1
    );
    assert_eq!(
        lockfile
            .matches("name = \"oom-edit-core\"\nversion = \"0.5.0\"")
            .count(),
        1
    );
    assert!(changelog.contains("## [0.5.0] - 2026-08-15"));
    assert!(changelog.contains("`oom-spell` 0.1.0"));
}

#[test]
fn tui_never_substitutes_rendered_remap_for_a_canonical_jump() {
    let source_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut pending = vec![source_root];
    let mut offenders = Vec::new();

    while let Some(path) = pending.pop() {
        for entry in std::fs::read_dir(&path).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().and_then(|extension| extension.to_str()) == Some("rs") {
                let source = std::fs::read_to_string(&path).unwrap();
                if source.contains(".remap_rendered_cursor(") {
                    offenders.push(path);
                }
            }
        }
    }

    assert!(
        offenders.is_empty(),
        "TUI jumps must call EditorSession::jump_to_offset, not remap only: {offenders:?}"
    );
}
