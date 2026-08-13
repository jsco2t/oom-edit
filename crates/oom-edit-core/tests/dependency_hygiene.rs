//! Dependency-hygiene test (FR-8.1): asserts that `oom-edit-core` has no
//! terminal, async, or networking dependencies.
//!
//! This test runs `cargo tree` (full dependency tree, not just direct deps)
//! and checks that the output contains none of the banned crate names.
//! It must run in `make test` (not `#[ignore]`).

use std::path::{Path, PathBuf};
use std::process::Command;

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root should resolve")
}

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

#[test]
fn workspace_is_exactly_three_crates_with_the_spell_dependency_diamond() {
    let root = workspace_root();
    let workspace = std::fs::read_to_string(root.join("Cargo.toml"))
        .expect("workspace manifest should be readable");
    let core = std::fs::read_to_string(root.join("crates/oom-edit-core/Cargo.toml"))
        .expect("core manifest should be readable");
    let tui = std::fs::read_to_string(root.join("crates/oom-edit/Cargo.toml"))
        .expect("TUI manifest should be readable");

    assert!(
        workspace.contains(
            "members = [\"crates/oom-spell\", \"crates/oom-edit-core\", \"crates/oom-edit\"]"
        ),
        "workspace must contain exactly oom-spell, oom-edit-core, and oom-edit"
    );
    assert_eq!(
        workspace.matches("members =").count(),
        1,
        "workspace membership must have one source of truth"
    );
    assert!(
        tui.contains("oom-edit-core = { path = \"../oom-edit-core\", version = \"=0.4.0\" }"),
        "TUI must depend directly on the exact-pinned core crate"
    );
    assert!(
        core.contains("oom-spell = { path = \"../oom-spell\", version = \"=0.1.0\" }"),
        "core must depend directly on the exact-pinned spell crate"
    );
    assert!(
        tui.contains("oom-spell = { path = \"../oom-spell\", version = \"=0.1.0\" }"),
        "TUI must depend directly on the exact-pinned spell crate"
    );
    assert!(
        !core.contains("oom-edit =") && !core.contains("../oom-edit\""),
        "core must never depend on the TUI crate"
    );
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

#[test]
fn renderer_implementation_module_is_not_public_api() {
    let lib_src = include_str!("../src/lib.rs");
    assert!(!lib_src.contains("pub mod rendered;"));
    assert!(!lib_src.contains("pub use rendered::"));
}

#[test]
fn current_surfaces_have_no_legacy_public_modes_or_bindings() {
    let banned = [
        concat!("Mode::", "View"),
        concat!("Mode::", "Visual"),
        concat!("Contexts::", "VIEW"),
        concat!("Contexts::", "VISUAL"),
        concat!("Badge", "View"),
        concat!("Badge", "Visual"),
        concat!("toggle", "_view"),
        concat!("toggle", "-view"),
        concat!("render", "_view"),
        concat!("view", "_cursor_line"),
        concat!(":", "view"),
        concat!("Space", " v"),
    ];
    let mut pending = vec![workspace_root()];
    let mut violations = Vec::new();

    while let Some(path) = pending.pop() {
        if path.is_dir() {
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("");
            if [".git", "target", "vendor", "patches"].contains(&name) {
                continue;
            }
            for entry in std::fs::read_dir(&path).expect("repository directory should be readable")
            {
                pending.push(entry.expect("repository entry should be readable").path());
            }
            continue;
        }

        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("");
        if name == "vim.rs" || name == "CHANGELOG.md" {
            continue;
        }
        let extension = path.extension().and_then(|extension| extension.to_str());
        if !matches!(extension, Some("rs" | "md" | "txt" | "toml")) {
            continue;
        }
        let source = std::fs::read_to_string(&path).expect("audited text file should be UTF-8");
        for token in banned {
            if source.contains(token) {
                violations.push(format!("{} contains {token:?}", path.display()));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "legacy public mode names or bindings remain:\n{}",
        violations.join("\n")
    );
}

#[test]
fn vim_adapter_does_not_depend_on_rendered_selection_dtos() {
    let vim_src = include_str!("../src/vim.rs");
    for banned in [
        concat!("Rendered", "Selection"),
        concat!("Rendered", "SelectionRow"),
        concat!("Rendered", "Point"),
        concat!("Selection", "Shape"),
        concat!("crate::", "session"),
    ] {
        assert!(
            !vim_src.contains(banned),
            "vim.rs must own its projected request model and not depend on {banned}"
        );
    }
}
