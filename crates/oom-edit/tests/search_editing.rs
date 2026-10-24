//! Search editing and visible prompt cursor through the owned pane boundary.

use oom_edit::{
    AllowAllFileAccess, ClipboardError, ClipboardSink, Config, ConfigPersistError, DisplayMode,
    EditorPane, KeyCode, KeyCodeKind, KeyInput, Modifiers, OpenOptions, PaneInit, PaneInput,
    PaneOptions, PaneServices, ThemeCatalog, ThemePersistenceSink, ThemeSelection, ThemeSlot, Tier,
};
use std::path::Path;
use std::time::Instant;
struct SilentClipboard;
impl ClipboardSink for SilentClipboard {
    fn copy(&mut self, _: &str) -> Result<(), ClipboardError> {
        Ok(())
    }
}
struct SilentTheme;
impl ThemePersistenceSink for SilentTheme {
    fn persist_theme(&mut self, _: ThemeSlot, _: &str) -> Result<(), ConfigPersistError> {
        Ok(())
    }
}
fn pane(base: &Path, now: Instant) -> EditorPane {
    let mut pane = EditorPane::construct(PaneInit {
        config: Config::default(),
        theme_catalog: ThemeCatalog::builtins(),
        theme_selection: ThemeSelection::new(None, DisplayMode::Dark, Tier::Monochrome),
        services: PaneServices {
            clipboard_sink: Box::new(SilentClipboard),
            theme_sink: Box::new(SilentTheme),
            file_access_policy: Box::new(AllowAllFileAccess),
            config_base_directory: base.into(),
            personal_dictionary_path: base.join("words"),
            working_directory: base.into(),
        },
        options: PaneOptions {
            inline_hints: false,
            ..PaneOptions::default()
        },
        initial_paths: vec![],
        now,
    })
    .pane;
    pane.new_buffer(OpenOptions::default()).unwrap();
    pane.render(80, 12, now);
    pane
}
fn send(pane: &mut EditorPane, kind: KeyCodeKind, now: Instant) {
    pane.handle_input(
        PaneInput::Key(KeyInput {
            code: KeyCode { kind },
            mods: Modifiers::default(),
        }),
        now,
    );
}
fn type_text(pane: &mut EditorPane, text: &str, now: Instant) {
    for c in text.chars() {
        send(pane, KeyCodeKind::Char(c), now);
    }
}
fn status_row(pane: &mut EditorPane, width: u16, now: Instant) -> String {
    let frame = pane.render(width, 12, now);
    frame.cells[11 * usize::from(width)..12 * usize::from(width)]
        .iter()
        .map(|cell| cell.symbol.as_str())
        .collect()
}

#[test]
fn arrows_position_the_prompt_cursor_and_edits_remain_exclusive() {
    let directory = tempfile::tempdir().unwrap();
    let now = Instant::now();
    let mut pane = pane(directory.path(), now);
    type_text(&mut pane, "/alXha", now);
    for _ in 0..3 {
        send(&mut pane, KeyCodeKind::Left, now);
    }
    let frame = pane.render(80, 12, now);
    assert_eq!(
        (frame.cursor.unwrap().column, frame.cursor.unwrap().row),
        (12, 11)
    );
    send(&mut pane, KeyCodeKind::Delete, now);
    type_text(&mut pane, "p", now);
    assert_eq!(pane.status().unwrap().prompt.as_deref(), Some("/alpha"));
    assert!(status_row(&mut pane, 80, now).contains("/alpha"));
    assert_eq!(pane.status().unwrap().ruler.line_count, 1);
    assert!(!pane.status().unwrap().dirty);
    send(&mut pane, KeyCodeKind::Backspace, now);
    assert_eq!(pane.status().unwrap().prompt.as_deref(), Some("/alha"));
    send(&mut pane, KeyCodeKind::Esc, now);
    assert_eq!(pane.status().unwrap().prompt, None);
}

#[test]
fn unicode_prompt_cursor_uses_display_cells_and_combining_characters() {
    let directory = tempfile::tempdir().unwrap();
    let now = Instant::now();
    let mut pane = pane(directory.path(), now);
    type_text(&mut pane, "/a界e\u{301}b", now);
    assert_eq!(
        pane.status().unwrap().prompt.as_deref(),
        Some("/a界e\u{301}b")
    );
    assert_eq!(pane.render(80, 12, now).cursor.unwrap().column, 15);
    send(&mut pane, KeyCodeKind::Left, now);
    assert_eq!(pane.render(80, 12, now).cursor.unwrap().column, 14);
    send(&mut pane, KeyCodeKind::Left, now);
    assert_eq!(pane.render(80, 12, now).cursor.unwrap().column, 14);
    send(&mut pane, KeyCodeKind::Home, now);
    assert_eq!(pane.render(80, 12, now).cursor.unwrap().column, 10);
    send(&mut pane, KeyCodeKind::Right, now);
    send(&mut pane, KeyCodeKind::Delete, now);
    assert_eq!(
        pane.status().unwrap().prompt.as_deref(),
        Some("/ae\u{301}b")
    );
}

#[test]
fn long_prompt_scrolls_to_keep_the_edit_position_visible() {
    let directory = tempfile::tempdir().unwrap();
    let now = Instant::now();
    let mut pane = pane(directory.path(), now);
    let draft = format!("/{}tail", "x".repeat(100));
    type_text(&mut pane, &draft, now);
    assert_eq!(
        pane.status().unwrap().prompt.as_deref(),
        Some(draft.as_str())
    );
    for width in [24, 40] {
        let row = status_row(&mut pane, width, now);
        assert!(row.contains("tail"), "{width}: {row:?}");
        let cursor = pane.render(width, 12, now).cursor.unwrap();
        assert_eq!(cursor.row, 11);
        assert!((9..width - 1).contains(&cursor.column));
        send(&mut pane, KeyCodeKind::Home, now);
        let row = status_row(&mut pane, width, now);
        assert!(row.starts_with(" NORMAL  /xxx"), "{row:?}");
        assert_eq!(pane.render(width, 12, now).cursor.unwrap().column, 10);
        send(&mut pane, KeyCodeKind::End, now);
    }
    for width in [0, 1, 9, 10, 11, 12, 18] {
        let frame = pane.render(width, 12, now);
        if let Some(cursor) = frame.cursor {
            assert!(cursor.column < width);
            assert!(cursor.row < 12);
        }
    }
}
