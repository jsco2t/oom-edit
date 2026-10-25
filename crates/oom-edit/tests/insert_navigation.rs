//! Insert whitespace and wrapped navigation through the owned pane.
use oom_edit::{
    AllowAllFileAccess, ClipboardError, ClipboardSink, Config, ConfigPersistError, DisplayMode,
    EditorPane, KeyCode, KeyCodeKind, KeyInput, Modifiers, PaneInit, PaneInput, PaneOptions,
    PaneServices, ThemeCatalog, ThemePersistenceSink, ThemeSelection, ThemeSlot, Tier,
};
use std::{path::Path, time::Instant};

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
fn pane(base: &Path, text: &str, minimal: bool, now: Instant) -> EditorPane {
    let path = base.join("search.md");
    std::fs::write(&path, text).unwrap();
    let mut pane = EditorPane::construct(PaneInit {
        config: {
            let mut config = Config::default();
            config.editor.wrap_width = 5;
            config
        },
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
            minimal_status_bar: minimal,
            ..PaneOptions::default()
        },
        initial_paths: vec![path],
        now,
    })
    .pane;
    pane.render(100, 20, now);
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
fn type_keys(pane: &mut EditorPane, text: &str, now: Instant) {
    for c in text.chars() {
        send(pane, KeyCodeKind::Char(c), now);
    }
}

#[test]
fn repeated_enter_has_no_source_or_visual_padding_after_the_cursor() {
    let directory = tempfile::tempdir().unwrap();
    let now = Instant::now();
    let mut pane = pane(
        directory.path(),
        "This is the first line of text",
        false,
        now,
    );
    type_keys(&mut pane, "A", now);
    let id = pane.active_tab().unwrap();
    let mut first_column = None;
    for count in 1..=3 {
        send(&mut pane, KeyCodeKind::Enter, now);
        let frame = pane.render(80, 20, now);
        assert_eq!(
            pane.text(&id).unwrap(),
            format!("This is the first line of text{}", "\n".repeat(count))
        );
        assert_eq!(pane.source_cursor(&id).unwrap(), (count, 0));
        let column = frame.cursor.unwrap().column;
        if let Some(first) = first_column {
            assert_eq!(column, first);
        }
        first_column = Some(column);
    }
    type_keys(&mut pane, "X", now);
    assert!(pane.text(&id).unwrap().ends_with("\nX"));
    send(&mut pane, KeyCodeKind::Esc, now);
    type_keys(&mut pane, "u", now);
    assert_eq!(pane.text(&id).unwrap(), "This is the first line of text");
}

#[test]
fn pane_insert_arrows_use_configured_wrapped_rows_and_scroll_the_destination() {
    let directory = tempfile::tempdir().unwrap();
    let now = Instant::now();
    let source = "abcdefghijklmnop\nxy\nABCDEFGHIJKLMNOP";
    let mut pane = pane(directory.path(), source, false, now);
    type_keys(&mut pane, "i", now);
    for _ in 0..7 {
        send(&mut pane, KeyCodeKind::Right, now);
    }
    pane.render(80, 20, now);
    let id = pane.active_tab().unwrap();
    for (kind, expected) in [
        (KeyCodeKind::Up, (0, 2)),
        (KeyCodeKind::Down, (0, 7)),
        (KeyCodeKind::Down, (0, 12)),
        (KeyCodeKind::Down, (0, 16)),
        (KeyCodeKind::Down, (1, 2)),
        (KeyCodeKind::Down, (2, 2)),
    ] {
        send(&mut pane, kind, now);
        let frame = pane.render(80, 5, now);
        assert_eq!(pane.source_cursor(&id).unwrap(), expected, "{kind:?}");
        assert!(frame
            .cursor
            .is_some_and(|cursor| cursor.row < 4 && cursor.column < 80));
        assert_eq!(pane.text(&id).unwrap(), source);
    }
}

#[test]
fn resize_and_unpainted_input_keep_source_navigation_valid() {
    let directory = tempfile::tempdir().unwrap();
    let now = Instant::now();
    let mut pane = pane(directory.path(), "abcdefghijklmnopqrstuvwx", false, now);
    type_keys(&mut pane, "i", now);
    for _ in 0..12 {
        send(&mut pane, KeyCodeKind::Right, now);
    }
    let id = pane.active_tab().unwrap();
    send(&mut pane, KeyCodeKind::Up, now);
    assert_eq!(pane.source_cursor(&id).unwrap(), (0, 7));
    for width in [8, 12, 40, 80] {
        pane.resize(width, 8, now);
        send(&mut pane, KeyCodeKind::Up, now);
        send(&mut pane, KeyCodeKind::Down, now);
        let frame = pane.render(width, 8, now);
        if width < 20 {
            assert!(frame.cursor.is_none());
        } else {
            assert!(frame
                .cursor
                .is_some_and(|cursor| cursor.column < width && cursor.row < 7));
        }
        assert_eq!(pane.source_cursor(&id).unwrap().0, 0);
    }
    assert_eq!(pane.text(&id).unwrap(), "abcdefghijklmnopqrstuvwx");
}
