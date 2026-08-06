//! Dependency-hygiene test (FR-8.1): asserts that `oom-edit-core` has no
//! terminal, async, or networking dependencies.
//!
//! This test runs `cargo tree` (full dependency tree, not just direct deps)
//! and checks that the output contains none of the banned crate names.
//! It must run in `make test` (not `#[ignore]`).

use std::process::Command;

/// Run `cargo tree` for oom-edit-core and return stdout as a string.
fn cargo_tree_output() -> String {
    let output = Command::new("cargo")
        .args([
            "tree",
            "-p",
            "oom-edit-core",
            "--edges",
            "normal",
            "--offline",
        ])
        .output()
        .expect("cargo tree should run");

    assert!(
        output.status.success(),
        "cargo tree failed with status {}: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    assert!(
        !stdout.is_empty(),
        "cargo tree produced no output — dependency check is vacuous"
    );
    assert!(
        stdout.contains("oom-edit-core"),
        "cargo tree output does not contain the requested root package"
    );
    stdout
}

/// Check that the cargo tree output does not contain any of the banned patterns.
fn assert_no_banned_deps(banned: &[&str], label: &str) {
    let stdout = cargo_tree_output();
    for pattern in banned {
        assert!(
            !stdout.contains(pattern),
            "oom-edit-core must not have {} dependency '{}'. Found in:\n{}",
            label,
            pattern,
            stdout
        );
    }
}

#[test]
fn dependency_hygiene_no_terminal_deps() {
    assert_no_banned_deps(
        &["ratatui", "crossterm", "tokio", "async-std"],
        "terminal/async",
    );
}

#[test]
fn dependency_hygiene_no_network_deps() {
    assert_no_banned_deps(&["reqwest", "hyper", "curl", "libnghttp2"], "network");
}

#[test]
fn hjkl_types_do_not_leak_public_api() {
    // Assert that `lib.rs`'s `pub use` list contains no `vim::` re-export.
    // The `hjkl` types are confined to `vim.rs` and must not appear in the
    // public API surface (T03 / FR-8.2).
    let lib_src = include_str!("../src/lib.rs");
    assert!(
        !lib_src.contains("pub use vim::"),
        "lib.rs must not re-export any vim:: types in its public API"
    );
}
