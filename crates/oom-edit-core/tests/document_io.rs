//! Supported-facade document I/O coverage.

use std::fs;
use std::path::Path;

use oom_edit_core::{
    EditorSession, FrontMatter, KeyCode, KeyCodeKind, KeyInput, LineEnding, Modifiers, OpenError,
    SaveError, Value,
};

fn key(c: char) -> KeyInput {
    KeyInput {
        code: KeyCode {
            kind: KeyCodeKind::Char(c),
        },
        mods: Modifiers::default(),
    }
}

fn esc() -> KeyInput {
    KeyInput {
        code: KeyCode {
            kind: KeyCodeKind::Esc,
        },
        mods: Modifiers::default(),
    }
}

fn write(path: &Path, bytes: impl AsRef<[u8]>) {
    fs::write(path, bytes).unwrap();
}

#[test]
fn session_opens_existing_and_new_files() {
    let dir = tempfile::tempdir().unwrap();
    let existing = dir.path().join("existing.md");
    write(&existing, "Hello\n");
    let session = EditorSession::open(&existing).unwrap();
    assert_eq!(session.document(), "Hello\n");
    assert_eq!(session.path(), Some(existing.as_path()));
    assert!(!session.is_new());

    let new = dir.path().join("new.md");
    let session = EditorSession::open(&new).unwrap();
    assert_eq!(session.document(), "");
    assert_eq!(session.line_count(), 1);
    assert!(session.is_new());
}

#[test]
fn invalid_utf8_reports_the_exact_first_bad_byte() {
    let dir = tempfile::tempdir().unwrap();
    for (index, bytes, expected) in [
        (0, b"\xff".as_slice(), 0),
        (1, b"hello\xff".as_slice(), 5),
        (2, b"caf\xc3\xa9\xff".as_slice(), 5),
        (3, b"abc\xe2\x82\xac\xff".as_slice(), 6),
    ] {
        let path = dir.path().join(format!("bad-{index}.md"));
        write(&path, bytes);
        assert!(
            matches!(EditorSession::open(&path), Err(OpenError::NotUtf8(offset)) if offset == expected)
        );
    }
}

#[test]
fn open_normalizes_text_and_retains_serialization_policy() {
    let dir = tempfile::tempdir().unwrap();
    for (name, disk, normalized, ending, final_newline) in [
        (
            "lf-final.md",
            "one\ntwo\n",
            "one\ntwo\n",
            LineEnding::Lf,
            true,
        ),
        ("lf-none.md", "one\ntwo", "one\ntwo", LineEnding::Lf, false),
        (
            "crlf-final.md",
            "one\r\ntwo\r\n",
            "one\ntwo\n",
            LineEnding::CrLf,
            true,
        ),
        (
            "crlf-none.md",
            "one\r\ntwo",
            "one\ntwo",
            LineEnding::CrLf,
            false,
        ),
    ] {
        let path = dir.path().join(name);
        write(&path, disk);
        let mut session = EditorSession::open(&path).unwrap();
        assert_eq!(session.document(), normalized);
        assert_eq!(session.line_ending(), ending);
        assert_eq!(session.has_final_newline(), final_newline);
        session.save(None, false).unwrap();
        assert_eq!(fs::read_to_string(path).unwrap(), disk);
    }
}

#[test]
fn edit_save_reload_preserves_exact_lf_and_crlf_bytes() {
    let dir = tempfile::tempdir().unwrap();
    for (name, original, inserted, expected) in [
        ("lf.md", "old\n", "new ", "new old\n"),
        ("crlf.md", "old\r\n", "new ", "new old\r\n"),
        ("lf-no-final.md", "old", "new ", "new old"),
        (
            "crlf-no-final.md",
            "old\r\nsecond",
            "new ",
            "new old\r\nsecond",
        ),
    ] {
        let path = dir.path().join(name);
        write(&path, original);
        let mut session = EditorSession::open(&path).unwrap();
        session.render_layout(40);
        session.handle_key(key('i'));
        session.insert_paste(inserted);
        session.handle_key(esc());
        session.save(None, false).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), expected);
        assert_eq!(
            EditorSession::open(&path).unwrap().document().replace(
                '\n',
                if expected.contains("\r\n") {
                    "\r\n"
                } else {
                    "\n"
                }
            ),
            expected
        );
    }
}

#[test]
fn session_front_matter_is_live_and_parser_neutral() {
    let session = EditorSession::from_text("---\ntitle: Hello\nauthor: World\n---\n\nBody\n");
    match session.front_matter() {
        FrontMatter::Yaml(Ok(value)) => {
            assert_eq!(value.get("title"), Some(&Value::str("Hello".into())));
            assert_eq!(value.get("author"), Some(&Value::str("World".into())));
        }
        other => panic!("expected parsed YAML, got {other:?}"),
    }
}

#[test]
fn save_retargets_while_save_copy_does_not() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("target.md");
    let copy = dir.path().join("copy.md");
    let mut session = EditorSession::from_text("contents\n");
    session.save(Some(&target), false).unwrap();
    assert_eq!(session.path(), Some(target.as_path()));
    session.save_copy(&copy).unwrap();
    assert_eq!(session.path(), Some(target.as_path()));
    assert_eq!(fs::read_to_string(copy).unwrap(), "contents\n");
}

#[test]
fn external_modification_requires_force() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("external.md");
    write(&path, "original\n");
    let mut session = EditorSession::open(&path).unwrap();
    write(&path, "externally changed and longer\n");
    assert!(
        matches!(session.save(None, false), Err(SaveError::ExternallyModified(changed)) if changed == path)
    );
    session.save(None, true).unwrap();
    assert_eq!(fs::read_to_string(path).unwrap(), "original\n");
}

#[test]
fn dirty_state_tracks_save_edit_undo_and_redo() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("dirty.md");
    write(&path, "text\n");
    let mut session = EditorSession::open(&path).unwrap();
    assert!(!session.is_dirty());
    session.render_layout(20);
    session.handle_key(key('i'));
    session.handle_key(key('x'));
    session.handle_key(esc());
    assert!(session.is_dirty());
    session.handle_key(key('u'));
    assert!(!session.is_dirty());
}

#[test]
fn save_without_any_path_returns_io_error() {
    let mut session = EditorSession::from_text("text");
    assert!(matches!(session.save(None, false), Err(SaveError::Io(_))));
}

#[cfg(unix)]
#[test]
fn save_masks_existing_permissions_to_safe_file_mode() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("permissions.md");
    write(&path, "text\n");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o777)).unwrap();
    let mut session = EditorSession::open(&path).unwrap();
    session.save(None, false).unwrap();
    assert_eq!(
        fs::metadata(path).unwrap().permissions().mode() & 0o777,
        0o644
    );
}
