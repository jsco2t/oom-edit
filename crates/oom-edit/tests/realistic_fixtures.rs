//! Fixed fixture shape and identity for the public-pane performance gate.

#[path = "../perf/realistic_fixtures.rs"]
mod fixtures;

#[test]
fn fixtures_have_exact_sizes_and_balanced_fences() {
    for class in fixtures::CLASSES {
        for bytes in [16 * 1024, 128 * 1024, 1024 * 1024] {
            let text = fixtures::generate(class, bytes);
            assert_eq!(text.len(), bytes, "{class}");
            if *class != "prose" {
                assert!(text.ends_with('\n'), "{class}");
            }
            assert_eq!(text.matches("```").count() % 2, 0, "{class}");
            assert_eq!(text, fixtures::generate(class, bytes), "{class}");
            assert_eq!(
                fixtures::identity(class, &text),
                fixtures::identity(class, &fixtures::generate(class, bytes))
            );
        }
    }
}

#[test]
fn fixture_classes_exercise_distinct_markdown_shapes() {
    let mixed = fixtures::generate("mixed", 1024 * 1024);
    assert!(mixed.starts_with("---\ntitle:"));
    assert!(mixed.matches("```rust").count() >= 50);
    assert!(mixed.matches("```go").count() >= 50);
    assert!(mixed.matches("```python").count() >= 50);
    assert!(mixed.matches("| :--- |").count() >= 100);
    assert!(mixed.matches("- A task").count() >= 700);

    let references = fixtures::generate("mixed-reference", 1024 * 1024);
    assert!(references.starts_with("---\ntitle: Performance fixture\n---\n\n[shared]:"));
    assert!(references.matches("[shared]").count() >= 100);
    assert!(references.matches("```rust").count() >= 50);

    let many = fixtures::generate("many-fences", 128 * 1024);
    assert!(many.matches("```rust").count() >= 100);
    assert!(many.matches("```python").count() >= 100);
    assert!(many.matches("```sh").count() >= 100);
    assert!(many.matches("```go").count() >= 100);

    let rust = fixtures::generate("rust-fence", 128 * 1024);
    assert_eq!(rust.matches("```rust").count(), 1);
    assert_eq!(rust.matches("```").count(), 2);

    let lists = fixtures::generate("lists", 512 * 1024);
    assert!(lists.matches("- alpha").count() >= 1000);
    let tables = fixtures::generate("tables", 448 * 1024);
    assert!(tables.matches("| :--- |").count() >= 1000);
    let prose = fixtures::generate("prose", 1024 * 1024);
    assert!(!prose.contains("```"));
}

#[test]
fn fixture_identities_are_versioned() {
    for (class, size, expected) in [
        ("prose", 1024 * 1024, "d894d5030de45b3d"),
        ("mixed", 1024 * 1024, "a2669d94455fc82d"),
        ("mixed", 144 * 1024, "bde118422ca5e1ad"),
        ("mixed-reference", 1024 * 1024, "9e6d4c35623679b3"),
        ("mixed-reference", 144 * 1024, "46803c7e01139b23"),
        ("rust-fence", 128 * 1024, "d5ef4f045322fba0"),
        ("many-fences", 128 * 1024, "a4215da162cb3947"),
        ("lists", 512 * 1024, "33b1994e35a6496c"),
        ("tables", 448 * 1024, "c5cdec08a4cb33ab"),
    ] {
        assert_eq!(
            fixtures::identity(class, &fixtures::generate(class, size)),
            format!("{}:{class}:{size}:{expected}", fixtures::VERSION)
        );
    }
}
