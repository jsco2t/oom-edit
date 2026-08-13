//! Downstream-style guards for the curated `oom-spell` facade and dependency graph.

use oom_spell::{
    normalize_dictionary_entry, AddWordOutcome, BuildIncomplete, BuildProgress,
    DictionaryEntryError, SpellEngine, SpellEngineBuilder, MAX_CHECKED_WORD_BYTES,
};
use std::path::{Path, PathBuf};
use std::process::Command;

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root should resolve")
}

fn public_use_declarations(source: &str) -> Vec<String> {
    let mut declarations = Vec::new();
    let mut current = String::new();
    for line in source.lines() {
        let trimmed = line.trim();
        if current.is_empty() {
            if !trimmed.starts_with("pub use ") {
                continue;
            }
            current.push_str(trimmed);
        } else {
            current.push(' ');
            current.push_str(trimmed);
        }
        if trimmed.ends_with(';') {
            declarations.push(std::mem::take(&mut current));
        }
    }
    assert!(current.is_empty(), "unterminated pub use declaration");
    declarations
}

fn assert_no_exported_macros(path: &Path) {
    for entry in std::fs::read_dir(path).expect("spell source directory should be readable") {
        let path = entry.expect("spell source entry should be readable").path();
        if path.is_dir() {
            assert_no_exported_macros(&path);
        } else if path.extension().and_then(|extension| extension.to_str()) == Some("rs") {
            let source =
                std::fs::read_to_string(&path).expect("spell Rust source should be readable");
            let compact_source: String = source.split_whitespace().collect();
            assert!(
                !compact_source.contains("#[macro_export"),
                "exported macro bypasses the curated facade: {}",
                path.display()
            );
        }
    }
}

#[test]
fn curated_facade_is_nameable_from_the_crate_root() {
    let _: usize = MAX_CHECKED_WORD_BYTES;
    let _: fn(&str) -> Result<Option<String>, DictionaryEntryError> = normalize_dictionary_entry;
    let _ = std::any::TypeId::of::<(
        AddWordOutcome,
        BuildIncomplete,
        BuildProgress,
        DictionaryEntryError,
        SpellEngine,
        SpellEngineBuilder,
    )>();
}

#[test]
fn crate_has_no_dependencies_of_any_kind() {
    let manifest = include_str!("../Cargo.toml");
    let dependency_tables: Vec<_> = manifest
        .lines()
        .map(str::trim)
        .filter(|line| {
            (line.starts_with('[') && line.contains("dependencies"))
                || line.starts_with("dependencies")
                || line.starts_with("dev-dependencies")
                || line.starts_with("build-dependencies")
        })
        .collect();
    assert!(
        dependency_tables.is_empty(),
        "oom-spell manifest must not declare active, optional, development, build, or target-specific dependencies: {dependency_tables:?}"
    );

    let output = Command::new("cargo")
        .args([
            "tree",
            "-p",
            "oom-spell",
            "--all-features",
            "--target",
            "all",
            "--edges",
            "all",
            "--offline",
        ])
        .current_dir(workspace_root())
        .output()
        .expect("cargo tree should run");
    assert!(
        output.status.success(),
        "cargo tree failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(
        stdout.lines().count(),
        1,
        "oom-spell must have no dependency edges:\n{stdout}"
    );
    assert!(stdout.starts_with("oom-spell v0.1.0"));
}

#[test]
fn facade_is_curated_and_modules_remain_private() {
    let source = include_str!("../src/lib.rs");
    assert!(!source.contains("pub mod "));
    assert!(!source.contains("extern crate"));
    assert_eq!(
        public_use_declarations(source),
        ["pub use engine::{ normalize_dictionary_entry, AddWordOutcome, BuildIncomplete, BuildProgress, DictionaryEntryError, SpellEngine, SpellEngineBuilder, MAX_CHECKED_WORD_BYTES, };"],
        "the crate-root facade changed; update the architecture contract and guard together"
    );
    let unexpected_public_items: Vec<_> = source
        .lines()
        .filter(|line| line.starts_with("pub ") && !line.starts_with("pub use "))
        .collect();
    assert!(
        unexpected_public_items.is_empty(),
        "facade must consist only of curated re-exports: {unexpected_public_items:?}"
    );
    assert_no_exported_macros(Path::new(env!("CARGO_MANIFEST_DIR")).join("src").as_path());
}

#[test]
fn core_does_not_create_a_partial_engine_facade() {
    let core = std::fs::read_to_string(workspace_root().join("crates/oom-edit-core/src/lib.rs"))
        .expect("core lib.rs should be readable");
    assert!(!core.contains("pub use oom_spell"));
    assert!(!core.contains("pub use oom_spell::"));
}
