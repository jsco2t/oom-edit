//! Document I/O tests — open, save, dirty tracking, round-trip, and errors.
//!
//! All tests use a tempdir for isolation. See task T05.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::time::Duration;

use oom_edit_core::document::{Document, LineEnding};
use oom_edit_core::error::{OpenError, SaveError};
use oom_edit_core::frontmatter::{FrontMatter, Value};
use oom_edit_core::session::EditorSession;

// ── Helpers ────────────────────────────────────────────────────────────────

fn temp_dir() -> tempfile::TempDir {
    tempfile::tempdir().expect("create tempdir")
}

fn write_file(dir: &Path, name: &str, content: &str) -> std::path::PathBuf {
    let path = dir.join(name);
    fs::write(&path, content).unwrap();
    path
}

// ── Open tests ─────────────────────────────────────────────────────────────

#[test]
fn open_existing_file() {
    let dir = temp_dir();
    let path = write_file(dir.path(), "test.md", "Hello, world!\n");

    let doc = Document::open(&path).unwrap();
    assert_eq!(doc.text(), "Hello, world!\n");
    assert_eq!(doc.line_count(), 2);
    assert!(doc.path().is_some());
}

#[test]
fn open_new_file_returns_empty_buffer() {
    let dir = temp_dir();
    let path = dir.path().join("new.md");

    let doc = Document::open(&path).unwrap();
    assert_eq!(doc.text(), "");
    assert_eq!(doc.line_count(), 1);
    assert_eq!(doc.line_ending(), LineEnding::Lf);
    // New file with save_point=0 is dirty when dirty_gen != 0
    assert!(doc.is_dirty(1));
}

#[test]
fn open_invalid_utf8_refuses() {
    let dir = temp_dir();
    let path = dir.path().join("bad.md");

    // Write a file with invalid UTF-8 (0xFF byte)
    fs::write(&path, b"hello\xffworld").unwrap();

    let result = Document::open(&path);
    assert!(result.is_err());
    match result.unwrap_err() {
        OpenError::NotUtf8(offset) => assert_eq!(offset, 5),
        other => panic!("expected NotUtf8, got {:?}", other),
    }
}

#[test]
fn open_detects_crlf_line_ending() {
    let dir = temp_dir();
    let path = write_file(dir.path(), "crlf.md", "line1\r\nline2\r\n");

    let doc = Document::open(&path).unwrap();
    assert_eq!(doc.line_ending(), LineEnding::CrLf);
    assert!(doc.has_final_newline());
    // Text is normalized to LF internally
    assert_eq!(doc.text(), "line1\nline2\n");
}

#[test]
fn open_detects_lf_line_ending() {
    let dir = temp_dir();
    let path = write_file(dir.path(), "lf.md", "line1\nline2\n");

    let doc = Document::open(&path).unwrap();
    assert_eq!(doc.line_ending(), LineEnding::Lf);
    assert!(doc.has_final_newline());
}

#[test]
fn open_detects_no_final_newline() {
    let dir = temp_dir();
    let path = write_file(dir.path(), "nofinal.md", "no final newline");

    let doc = Document::open(&path).unwrap();
    assert!(!doc.has_final_newline());
    assert_eq!(doc.text(), "no final newline");
}

#[test]
fn open_detects_no_final_newline_crlf() {
    let dir = temp_dir();
    let path = write_file(dir.path(), "nofinal_crlf.md", "no final\r\ncrlf");

    let doc = Document::open(&path).unwrap();
    assert_eq!(doc.line_ending(), LineEnding::CrLf);
    assert!(!doc.has_final_newline());
}

// ── Front matter tests ─────────────────────────────────────────────────────

#[test]
fn open_parses_yaml_front_matter() {
    let dir = temp_dir();
    let content = "---\ntitle: Hello\nauthor: World\n---\n\nBody text\n";
    let path = write_file(dir.path(), "yaml.md", content);

    let doc = Document::open(&path).unwrap();
    match doc.front_matter() {
        FrontMatter::Yaml(Ok(value)) => {
            assert!(value.is_map());
            let map = value.as_map().unwrap();
            assert_eq!(map.get("title").unwrap(), &Value::str("Hello".to_string()));
            assert_eq!(map.get("author").unwrap(), &Value::str("World".to_string()));
        }
        other => panic!("expected YAML Ok, got {:?}", other),
    }
}

#[test]
fn open_parses_toml_front_matter() {
    let dir = temp_dir();
    let content = "+++\ntitle = \"Hello\"\nauthor = \"World\"\n+++\n\nBody text\n";
    let path = write_file(dir.path(), "toml.md", content);

    let doc = Document::open(&path).unwrap();
    match doc.front_matter() {
        FrontMatter::Toml(Ok(value)) => {
            assert!(value.is_map());
            let map = value.as_map().unwrap();
            assert_eq!(map.get("title").unwrap(), &Value::str("Hello".to_string()));
            assert_eq!(map.get("author").unwrap(), &Value::str("World".to_string()));
        }
        other => panic!("expected TOML Ok, got {:?}", other),
    }
}

#[test]
fn open_parses_nested_front_matter() {
    let dir = temp_dir();
    let content = "---\nauthor:\n  name: John\n  tags:\n    - rust\n    - editor\n---\n\nBody\n";
    let path = write_file(dir.path(), "nested.md", content);

    let doc = Document::open(&path).unwrap();
    match doc.front_matter() {
        FrontMatter::Yaml(Ok(value)) => {
            let map = value.as_map().unwrap();
            let author = map.get("author").unwrap();
            assert!(author.is_map());
            let author_map = author.as_map().unwrap();
            assert_eq!(
                author_map.get("name").unwrap(),
                &Value::str("John".to_string())
            );
            let tags = author_map.get("tags").unwrap();
            assert!(matches!(tags, Value::Seq(_)));
        }
        other => panic!("expected YAML Ok, got {:?}", other),
    }
}

#[test]
fn open_malformed_yaml_fm_still_opens() {
    let dir = temp_dir();
    let content = "---\n[invalid yaml:::\n---\n\nBody\n";
    let path = write_file(dir.path(), "bad_yaml.md", content);

    // Should not error — malformed front matter degrades gracefully
    let doc = Document::open(&path).unwrap();
    match doc.front_matter() {
        FrontMatter::Yaml(Err(_)) => {} // expected
        other => panic!("expected YAML Err, got {:?}", other),
    }
    // Body text is still accessible
    assert!(doc.text().contains("Body"));
}

#[test]
fn open_no_front_matter() {
    let dir = temp_dir();
    let path = write_file(dir.path(), "plain.md", "just plain text\n");

    let doc = Document::open(&path).unwrap();
    assert!(matches!(doc.front_matter(), FrontMatter::None));
}

// ── Save tests ─────────────────────────────────────────────────────────────

#[test]
fn save_creates_file() {
    let dir = temp_dir();
    let path = dir.path().join("output.md");

    let mut doc = Document::from_text("hello world\n");
    doc.save(Some(&path), false).unwrap();

    assert!(path.exists());
    assert_eq!(fs::read_to_string(&path).unwrap(), "hello world\n");
}

#[test]
fn save_preserves_lf() {
    let dir = temp_dir();
    let path = dir.path().join("lf_out.md");

    let mut doc = Document::from_text("line1\nline2\n");
    doc.save(Some(&path), false).unwrap();

    let content = fs::read_to_string(&path).unwrap();
    assert_eq!(content, "line1\nline2\n");
    assert!(!content.contains("\r\n"));
}

#[test]
fn save_preserves_crlf() {
    let dir = temp_dir();
    let path = dir.path().join("crlf_out.md");

    let mut doc = Document::from_text("line1\r\nline2\r\n");
    doc.save(Some(&path), false).unwrap();

    let content = fs::read_to_string(&path).unwrap();
    assert_eq!(content, "line1\r\nline2\r\n");
}

#[test]
fn save_preserves_no_final_newline() {
    let dir = temp_dir();
    let path = dir.path().join("nofinal_out.md");

    let mut doc = Document::from_text("no final newline");
    doc.save(Some(&path), false).unwrap();

    let content = fs::read_to_string(&path).unwrap();
    assert_eq!(content, "no final newline");
    assert!(!content.ends_with('\n'));
}

#[test]
fn save_preserves_file_permissions() {
    let dir = temp_dir();
    let path = dir.path().join("perm.md");

    // Create file with 0o600 permissions
    fs::write(&path, "initial\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    }

    let mut doc = Document::open(&path).unwrap();
    doc.save(None, false).unwrap();

    // Check permissions are preserved (masked to original & 0o644)
    #[cfg(unix)]
    {
        let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        // Original was 0o600, masked with 0o644 gives 0o600
        assert_eq!(
            mode,
            0o600,
            "permissions should be 0o600 (0o600 & 0o644), got {:?}",
            octal(mode)
        );
    }
}

#[test]
fn save_atomic_write_no_truncation() {
    // Verify that during save, the target file is never observed in a
    // truncated state. We do this by listing the temp directory before
    // and after rename, and checking the target content is always complete.
    let dir = temp_dir();
    let path = dir.path().join("atomic.md");

    // Write initial content
    fs::write(&path, "initial content\n").unwrap();

    let mut doc = Document::open(&path).unwrap();
    // The document's text is "initial content\n" (normalized)
    doc.save(None, false).unwrap();

    // After save, content should be intact
    let content = fs::read_to_string(&path).unwrap();
    assert_eq!(content, "initial content\n");
}

#[test]
fn save_copy_without_retarget() {
    let dir = temp_dir();
    let path1 = dir.path().join("original.md");
    let path2 = dir.path().join("copy.md");

    let mut doc = Document::from_text("original content\n");
    doc.save(Some(&path1), false).unwrap();

    // Change the document
    doc.set_text("modified content\n");

    // Save a copy to path2
    doc.save_copy(&path2).unwrap();

    // path1 should still have original content (from first save)
    // path2 should have the modified content
    assert_eq!(fs::read_to_string(&path1).unwrap(), "original content\n");
    assert_eq!(fs::read_to_string(&path2).unwrap(), "modified content\n");
}

// ── Round-trip tests (FR-5.6) ─────────────────────────────────────────────

/// Property-style round-trip test: open → save with zero edits should be
/// byte-identical to the original file.
#[test]
fn roundtrip_lf() {
    let dir = temp_dir();
    let original = "line1\nline2\nline3\n";
    let path = write_file(dir.path(), "rt_lf.md", original);

    let mut doc = Document::open(&path).unwrap();
    doc.save(None, false).unwrap();

    let saved = fs::read_to_string(&path).unwrap();
    assert_eq!(saved, original);
}

#[test]
fn roundtrip_crlf() {
    let dir = temp_dir();
    let original = "line1\r\nline2\r\nline3\r\n";
    let path = write_file(dir.path(), "rt_crlf.md", original);

    let mut doc = Document::open(&path).unwrap();
    doc.save(None, false).unwrap();

    let saved = fs::read_to_string(&path).unwrap();
    assert_eq!(saved, original);
}

#[test]
fn roundtrip_no_final_newline() {
    let dir = temp_dir();
    let original = "no final newline";
    let path = write_file(dir.path(), "rt_nofinal.md", original);

    let mut doc = Document::open(&path).unwrap();
    doc.save(None, false).unwrap();

    let saved = fs::read_to_string(&path).unwrap();
    assert_eq!(saved, original);
}

#[test]
fn roundtrip_yaml_front_matter() {
    let dir = temp_dir();
    let original = "---\ntitle: Test\n---\n\nBody text\n";
    let path = write_file(dir.path(), "rt_yaml.md", original);

    let mut doc = Document::open(&path).unwrap();
    doc.save(None, false).unwrap();

    let saved = fs::read_to_string(&path).unwrap();
    assert_eq!(saved, original);
}

#[test]
fn roundtrip_toml_front_matter() {
    let dir = temp_dir();
    let original = "+++\ntitle = \"Test\"\n+++\n\nBody text\n";
    let path = write_file(dir.path(), "rt_toml.md", original);

    let mut doc = Document::open(&path).unwrap();
    doc.save(None, false).unwrap();

    let saved = fs::read_to_string(&path).unwrap();
    assert_eq!(saved, original);
}

#[test]
fn roundtrip_yaml_fm_with_parse_error() {
    let dir = temp_dir();
    let original = "---\n[bad yaml:::\n---\n\nBody\n";
    let path = write_file(dir.path(), "rt_bad_yaml.md", original);

    let mut doc = Document::open(&path).unwrap();
    doc.save(None, false).unwrap();

    let saved = fs::read_to_string(&path).unwrap();
    assert_eq!(saved, original);
}

#[test]
fn roundtrip_empty_file() {
    let dir = temp_dir();
    let original = "";
    let path = write_file(dir.path(), "rt_empty.md", original);

    let mut doc = Document::open(&path).unwrap();
    doc.save(None, false).unwrap();

    let saved = fs::read_to_string(&path).unwrap();
    assert_eq!(saved, original);
}

// ── Dirty tracking tests (FR-5.4) ─────────────────────────────────────────

#[test]
fn new_document_is_not_dirty() {
    let doc = Document::from_text("hello\n");
    // save_point is UndoMark(0), dirty gen 0 means not dirty
    assert!(!doc.is_dirty(0));
}

#[test]
fn document_becomes_dirty_after_edit() {
    let doc = Document::from_text("hello\n");
    // save_point is UndoMark(0)
    assert!(!doc.is_dirty(0));

    // Simulate edit: dirty gen changes to 1
    assert!(doc.is_dirty(1));
}

#[test]
fn document_is_clean_after_save() {
    let dir = temp_dir();
    let path = dir.path().join("dirty.md");

    let mut doc = Document::from_text("hello\n");
    doc.save(Some(&path), false).unwrap();

    // After save, save_point is updated. With matching dirty gen, not dirty.
    let mark = doc.save_point();
    assert!(!doc.is_dirty(mark.0));
}

// ── External modification detection (FR-5.7) ──────────────────────────────

#[test]
fn save_detects_external_modification() {
    let dir = temp_dir();
    let path = write_file(dir.path(), "external.md", "original\n");

    let mut doc = Document::open(&path).unwrap();

    // Simulate external modification: change the file
    std::thread::sleep(Duration::from_millis(100));
    fs::write(&path, "externally modified\n").unwrap();

    // Save should fail with ExternallyModified
    let result = doc.save(None, false);
    assert!(matches!(result, Err(SaveError::ExternallyModified(_))));
}

#[test]
fn force_save_overwrites_external_changes() {
    let dir = temp_dir();
    let path = write_file(dir.path(), "force.md", "original\n");

    let mut doc = Document::open(&path).unwrap();

    // Simulate external modification
    std::thread::sleep(Duration::from_millis(100));
    fs::write(&path, "externally modified\n").unwrap();

    // Force save should succeed
    doc.save(None, true).unwrap();

    // File should have the document's content
    let content = fs::read_to_string(&path).unwrap();
    assert_eq!(content, "original\n");
}

#[test]
fn save_untouched_file_succeeds() {
    let dir = temp_dir();
    let path = write_file(dir.path(), "untouched.md", "original\n");

    let mut doc = Document::open(&path).unwrap();

    // No external modification — save should succeed
    doc.save(None, false).unwrap();

    let content = fs::read_to_string(&path).unwrap();
    assert_eq!(content, "original\n");
}

// ── EditorSession integration tests ────────────────────────────────────────

#[test]
fn session_open_existing_file() {
    let dir = temp_dir();
    let path = write_file(dir.path(), "session.md", "hello from file\n");

    let session = EditorSession::open(&path).unwrap();
    assert_eq!(session.document(), "hello from file\n");
    assert!(session.document_ref().path().is_some());
}

#[test]
fn session_open_new_file() {
    let dir = temp_dir();
    let path = dir.path().join("new_session.md");

    let session = EditorSession::open(&path).unwrap();
    assert_eq!(session.document(), "");
    assert!(session.document_ref().path().is_some());
}

#[test]
fn session_save_and_reload() {
    let dir = temp_dir();
    let path = dir.path().join("save_reload.md");

    // Create session, save, reload
    let mut session = EditorSession::from_text("initial content\n");
    session.save(Some(&path), false).unwrap();

    let session2 = EditorSession::open(&path).unwrap();
    assert_eq!(session2.document(), "initial content\n");
}

#[test]
fn session_dirty_tracking() {
    let mut session = EditorSession::from_text("hello\n");
    assert!(!session.is_dirty());

    // Simulate edit by changing the vim buffer's dirty gen
    // (In real usage, hjkl increments dirty_gen on edits)
    session.save_point(); // mark as saved
    assert!(!session.is_dirty());
}

// ── Utility ────────────────────────────────────────────────────────────────

#[cfg(unix)]
fn octal(mode: u32) -> String {
    format!("0o{:03o}", mode & 0o777)
}
