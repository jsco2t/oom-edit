use std::path::Path;

fn root() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
}

#[test]
fn nfr_007_public_documentation_guard() {
    for source in [
        include_str!("../src/lib.rs"),
        include_str!("../../oom-edit-core/src/lib.rs"),
    ] {
        assert!(source.contains("#![deny(missing_docs)]"));
        assert!(!source.contains("allow(missing_docs)"));
    }
}

#[test]
fn owned_style_boundary_has_no_public_renderer_conversion_traits() {
    let styles = include_str!("../src/owned_style.rs");
    let themes = include_str!("../src/theme.rs");
    assert!(!styles.contains("impl From<Modifier> for StyleModifiers"));
    assert!(!themes.contains("impl From<Color> for ColorValue"));
}

#[test]
fn canonical_repository_metadata_matches_consumption_origin() {
    let source = std::fs::read_to_string(root().join("Cargo.toml")).unwrap();
    let manifest: toml::Value = toml::from_str(&source).unwrap();
    assert_eq!(
        manifest["workspace"]["package"]["repository"].as_str(),
        Some("https://github.com/jsco2t/oom-edit")
    );
}

#[test]
fn developer_guide_uses_the_exercised_public_consumer() {
    let guide = std::fs::read_to_string(root().join("DEVELOPER.md"))
        .expect("the integration guide must exist");
    let sample = std::fs::read_to_string(root().join("fixtures/downstream/src/lib.rs")).unwrap();
    assert!(
        guide.contains(&format!("```rust\n{sample}```")),
        "the runnable consumer snippet must match the independently compiled fixture exactly"
    );
    for section in [
        "## Construction and configuration",
        "## Input, focus and time",
        "## Owned frames and host chrome",
        "## Prepared lifecycle and disk coordination",
        "## Notices and terminal ownership",
    ] {
        assert!(
            guide.contains(section),
            "missing integration section {section}"
        );
    }
}

#[test]
fn integrated_ci_owns_coverage_docs_examples_and_performance() {
    let makefile = include_str!("../../../Makefile");
    let recipes = makefile
        .split("\nci:")
        .nth(1)
        .unwrap()
        .split("\n\n")
        .next()
        .unwrap();
    for target in [
        "build-release",
        "check",
        "coverage-check",
        "build-examples",
        "doc",
        "bench-check",
        "bench",
    ] {
        assert!(
            recipes
                .lines()
                .any(|line| line == format!("\t$(MAKE) {target}")),
            "CI must invoke make-owned {target}"
        );
    }
    assert!(
        !recipes.contains("downstream"),
        "committed-revision checks are separate CI steps, not pre-commit requirements"
    );
}

fn rust_sources(directory: &Path, files: &mut Vec<std::path::PathBuf>) {
    for entry in std::fs::read_dir(directory).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            rust_sources(&path, files);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            files.push(path);
        }
    }
}

#[test]
fn production_sources_confine_hjkl_and_unsafe_to_the_audited_wrappers() {
    for crate_name in ["oom-edit-core", "oom-edit"] {
        let mut files = Vec::new();
        rust_sources(
            &root().join("crates").join(crate_name).join("src"),
            &mut files,
        );
        assert!(!files.is_empty());
        for path in files {
            let source = std::fs::read_to_string(&path).unwrap();
            if path.file_name().unwrap() != "vim.rs" {
                assert!(
                    !source.contains("hjkl_buffer") && !source.contains("hjkl_engine"),
                    "engine types escaped into {}",
                    path.display()
                );
            }
            if path.file_name().unwrap() != "terminal_guard.rs" {
                for line in source
                    .lines()
                    .map(str::trim)
                    .filter(|line| !line.starts_with("//"))
                {
                    assert!(
                        !line.starts_with("#[allow(unsafe_code)]")
                            && !line.contains("unsafe {")
                            && !line.contains("unsafe fn ")
                            && !line.contains("unsafe impl "),
                        "unaudited unsafe in {}: {line}",
                        path.display()
                    );
                }
            }
        }
    }
    let terminal = include_str!("../src/terminal_guard.rs");
    assert_eq!(terminal.matches("unsafe {").count(), 2);
    assert_eq!(
        terminal
            .lines()
            .filter(|line| line.trim() == "#[allow(unsafe_code)]")
            .count(),
        1
    );
    assert!(include_str!("../../oom-edit-core/src/lib.rs").contains("#![forbid(unsafe_code)]"));
}

#[test]
fn integration_reachability_inventory_names_each_editor_dependent_prd_clause() {
    let guide = std::fs::read_to_string(root().join("docs/host-api-reachability.md")).unwrap();
    for number in [
        4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 47, 63, 64, 65, 66, 67, 68, 69, 82, 83, 84, 125, 147,
        162, 164, 165, 166,
    ] {
        let prefix = format!("| FR-{number:03} |");
        assert_eq!(
            guide
                .lines()
                .filter(|line| line.starts_with(&prefix))
                .count(),
            1,
            "missing or duplicated {prefix}"
        );
    }
    assert!(guide.contains("Host-owned behavior is not implemented by the editor"));
    assert!(guide.contains("EditorPane::prepare_close"));
    assert!(guide.contains("EditorPane::prepare_retarget"));
    assert!(guide.contains("EditorPane::begin_external_change"));
}

#[path = "../../../fixtures/downstream/src/lib.rs"]
mod exercised_documentation_consumer;
