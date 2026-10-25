//! Total source-line counts in pane metadata and rendered status rows.

use oom_edit::{
    AllowAllFileAccess, ClipboardError, ClipboardSink, Config, ConfigPersistError, DisplayMode,
    EditorPane, KeyCode, KeyCodeKind, KeyInput, Mode, Modifiers, OpenOptions, PaneInit, PaneInput,
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
fn pane(base: &Path, now: Instant, spell_enabled: bool) -> EditorPane {
    let mut config = Config::default();
    config.spell.enabled = spell_enabled;
    let mut pane = EditorPane::construct(PaneInit {
        config,
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
fn ruler_shows_total_lines_after_percentage_in_metadata_and_frame() {
    let directory = tempfile::tempdir().unwrap();
    let now = Instant::now();
    let mut pane = pane(directory.path(), now, true);
    type_text(&mut pane, "i", now);
    let source = (1..=71)
        .map(|line| format!("line {line}"))
        .collect::<Vec<_>>()
        .join("\n");
    pane.handle_input(PaneInput::Paste(source), now);
    send(&mut pane, KeyCodeKind::Esc, now);
    pane.render(100, 12, now);
    type_text(&mut pane, ":61", now);
    send(&mut pane, KeyCodeKind::Enter, now);
    type_text(&mut pane, "ll", now);
    let status = pane.status().unwrap();
    assert_eq!(status.ruler.line_count, 71);
    assert_eq!(status.ruler.text, "61:3  85% [71]");
    assert!(status_row(&mut pane, 100, now).contains(&status.ruler.text));
}

#[test]
fn empty_and_single_line_rulers_include_the_total() {
    let directory = tempfile::tempdir().unwrap();
    let now = Instant::now();
    let mut pane = pane(directory.path(), now, false);
    assert_eq!(pane.status().unwrap().ruler.text, "1:1  [1] Top");
    assert!(status_row(&mut pane, 80, now).contains("1:1  [1] Top"));
    type_text(&mut pane, "ihello", now);
    let status = pane.status().unwrap();
    assert_eq!(status.ruler.line_count, 1);
    assert_eq!(status.ruler.text, "1:6  [1] Top");
    assert!(status_row(&mut pane, 80, now).contains(&status.ruler.text));
}

#[test]
fn line_count_updates_after_edits_and_undo_in_every_editing_mode() {
    let directory = tempfile::tempdir().unwrap();
    let now = Instant::now();
    let mut pane = pane(directory.path(), now, true);
    type_text(&mut pane, "i", now);
    pane.handle_input(PaneInput::Paste("one\ntwo\nthree".into()), now);
    assert_eq!(pane.status().unwrap().mode, Mode::Insert);
    assert_eq!(pane.status().unwrap().ruler.line_count, 3);
    assert!(status_row(&mut pane, 80, now).contains("[3]"));
    send(&mut pane, KeyCodeKind::Esc, now);
    assert_eq!(pane.status().unwrap().mode, Mode::Normal);
    assert!(status_row(&mut pane, 80, now).contains("[3]"));
    type_text(&mut pane, "v", now);
    assert_eq!(pane.status().unwrap().mode, Mode::Select);
    assert!(status_row(&mut pane, 80, now).contains("[3]"));
    send(&mut pane, KeyCodeKind::Esc, now);
    type_text(&mut pane, "o", now);
    assert_eq!(pane.status().unwrap().ruler.line_count, 4);
    assert!(status_row(&mut pane, 80, now).contains("[4]"));
    send(&mut pane, KeyCodeKind::Esc, now);
    type_text(&mut pane, "u", now);
    assert_eq!(pane.status().unwrap().ruler.line_count, 3);
    assert!(status_row(&mut pane, 80, now).contains("[3]"));
}

#[test]
fn counts_are_source_lines_including_final_empty_line_not_wrapped_rows() {
    let directory = tempfile::tempdir().unwrap();
    let now = Instant::now();
    let mut pane = pane(directory.path(), now, true);
    type_text(&mut pane, "i", now);
    pane.handle_input(
        PaneInput::Paste(format!("{}\n", "many wrapped words ".repeat(20))),
        now,
    );
    send(&mut pane, KeyCodeKind::Esc, now);
    assert_eq!(pane.status().unwrap().ruler.line_count, 2);
    for width in [24, 40, 100] {
        assert!(status_row(&mut pane, width, now).contains("[2]"));
        assert_eq!(pane.status().unwrap().ruler.line_count, 2);
    }
}

#[test]
fn large_rulers_and_spelling_indicator_fit_without_losing_total_lines() {
    let directory = tempfile::tempdir().unwrap();
    let now = Instant::now();
    for spell in [false, true] {
        let mut pane = pane(directory.path(), now, spell);
        type_text(&mut pane, "i", now);
        pane.handle_input(
            PaneInput::Paste(format!("{}{}", "x\n".repeat(9_999), "x".repeat(400))),
            now,
        );
        let status = pane.status().unwrap();
        assert_eq!(status.ruler.line_count, 10_000);
        assert_eq!(status.ruler.text, "10000:401  100% [10000] Bot");
        let row = status_row(&mut pane, 100, now);
        assert!(row.ends_with(&format!("{} ", status.ruler.text)), "{row:?}");
        assert_eq!(row.contains('🅂'), spell);
        for width in [24, 40] {
            let row = status_row(&mut pane, width, now);
            assert!(row.starts_with(" INSERT "));
            assert!(row.ends_with(' '));
        }
    }
}
