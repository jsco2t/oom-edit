//! Search counts and clear-search behavior through the owned pane boundary.
use oom_edit::{
    AllowAllFileAccess, ClipboardError, ClipboardSink, Config, ConfigPersistError, DisplayMode,
    EditorPane, KeyCode, KeyCodeKind, KeyInput, Modifiers, OpenOptions, PaneInit, PaneInput,
    PaneOptions, PaneServices, ThemeCatalog, ThemePersistenceSink, ThemeSelection, ThemeSlot, Tier,
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
fn submit(pane: &mut EditorPane, text: &str, now: Instant) {
    type_keys(pane, text, now);
    send(pane, KeyCodeKind::Enter, now);
}
fn row(pane: &mut EditorPane, width: u16, now: Instant) -> String {
    let frame = pane.render(width, 20, now);
    frame.cells[19 * usize::from(width)..]
        .iter()
        .map(|cell| cell.symbol.as_str())
        .collect()
}
fn assert_indicator(pane: &mut EditorPane, width: u16, expected: &str, now: Instant) {
    let status = row(pane, width, now);
    assert!(
        status.contains(expected),
        "expected {expected:?} in {status:?}"
    );
}

#[test]
fn supplied_example_reports_the_last_occurrence_on_each_logical_source_line() {
    for minimal in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let now = Instant::now();
        let mut pane = pane(
            directory.path(),
            "This is a search term how do you like searching for how,\nhow about now?\n",
            minimal,
            now,
        );
        assert!(!row(&mut pane, 100, now).contains('🄵'));
        submit(&mut pane, "/how", now);
        submit(&mut pane, ":1", now);
        assert_indicator(&mut pane, 100, "🄵 2:3", now);
        let status = row(&mut pane, 100, now);
        assert!(status.find('🄵').unwrap() < status.find('🅂').unwrap());
        submit(&mut pane, ":2", now);
        assert_indicator(&mut pane, 100, "🄵 3:3", now);
        assert!(!pane.status().unwrap().dirty);
    }
}

#[test]
fn nonmatching_lines_zero_matches_and_empty_search_have_explicit_counts() {
    let directory = tempfile::tempdir().unwrap();
    let now = Instant::now();
    let mut pane = pane(directory.path(), "before\n\nhow\n\nafter\n", false, now);
    submit(&mut pane, "/how", now);
    submit(&mut pane, ":1", now);
    assert_indicator(&mut pane, 100, "🄵 0:1", now);
    submit(&mut pane, ":5", now);
    assert_indicator(&mut pane, 100, "🄵 1:1", now);
    submit(&mut pane, "/missing", now);
    assert_indicator(&mut pane, 100, "🄵 0:0", now);
    type_keys(&mut pane, "/", now);
    send(&mut pane, KeyCodeKind::Esc, now);
    assert_indicator(&mut pane, 100, "🄵 0:0", now);
    type_keys(&mut pane, " c", now);
    assert!(!row(&mut pane, 100, now).contains('🄵'));
}

#[test]
fn clear_chord_clears_highlights_history_and_indicator_without_edits() {
    let directory = tempfile::tempdir().unwrap();
    let now = Instant::now();
    let mut pane = pane(directory.path(), "how\n\nhow\n", false, now);
    submit(&mut pane, "/how", now);
    assert_indicator(&mut pane, 100, ":2", now);
    let id = pane.active_tab().unwrap();
    let original = pane.text(&id).unwrap();
    type_keys(&mut pane, " c", now);
    assert!(!row(&mut pane, 100, now).contains('🄵'));
    let cursor = pane.source_cursor(&id).unwrap();
    type_keys(&mut pane, "nN", now);
    assert_eq!(pane.source_cursor(&id).unwrap(), cursor);
    assert_eq!(pane.text(&id).unwrap(), original);
    assert!(!pane.status().unwrap().dirty);
    submit(&mut pane, "/how", now);
    submit(&mut pane, ":noh", now);
    assert!(!row(&mut pane, 100, now).contains('🄵'));
}

#[test]
fn cancel_restores_the_submitted_search_and_prompt_keys_are_exclusive() {
    let directory = tempfile::tempdir().unwrap();
    let now = Instant::now();
    let mut pane = pane(directory.path(), "how how\n\nnow\n", false, now);
    submit(&mut pane, "/how", now);
    type_keys(&mut pane, "/now c", now);
    assert_eq!(pane.status().unwrap().prompt.as_deref(), Some("/now c"));
    send(&mut pane, KeyCodeKind::Esc, now);
    assert_indicator(&mut pane, 100, ":2", now);
    type_keys(&mut pane, " h", now);
    type_keys(&mut pane, " c", now);
    send(&mut pane, KeyCodeKind::Esc, now);
    assert_indicator(&mut pane, 100, ":2", now);
}

#[test]
fn unicode_rendered_matches_exclude_hidden_syntax_and_synthetic_footer() {
    let directory = tempfile::tempdir().unwrap();
    let now = Instant::now();
    let mut pane = pane(
        directory.path(),
        "# café\n\n[café](https://café.invalid)\n",
        false,
        now,
    );
    submit(&mut pane, "/café", now);
    submit(&mut pane, ":1", now);
    assert_indicator(&mut pane, 100, "🄵 1:2", now);
    submit(&mut pane, ":3", now);
    assert_indicator(&mut pane, 100, "🄵 2:2", now);
}

#[test]
fn reflow_spell_off_and_narrow_frames_preserve_counts_and_bounds() {
    let directory = tempfile::tempdir().unwrap();
    let now = Instant::now();
    let mut pane = pane(directory.path(), "how how how how how how\n", false, now);
    submit(&mut pane, "/how", now);
    submit(&mut pane, ":set nospell", now);
    for width in [100, 80, 60] {
        assert_indicator(&mut pane, width, "🄵 6:6", now);
        assert!(!row(&mut pane, width, now).contains('🅂'));
    }
    for width in [0, 1, 9, 12, 24, 40] {
        let frame = pane.render(width, 20, now);
        assert_eq!(frame.cells.len(), usize::from(width) * 20);
        if let Some(cursor) = frame.cursor {
            assert!(cursor.column < width);
            assert!(cursor.row < 20);
        }
    }
    assert_indicator(&mut pane, 100, "🄵 6:6", now);
}

#[test]
fn edits_undo_reload_and_tabs_never_reuse_stale_search_totals() {
    let directory = tempfile::tempdir().unwrap();
    let now = Instant::now();
    let mut pane = pane(directory.path(), "how\n\nhow\n", false, now);
    submit(&mut pane, "/how", now);
    submit(&mut pane, ":1", now);
    type_keys(&mut pane, "ix", now);
    send(&mut pane, KeyCodeKind::Esc, now);
    // Prefixing does not remove the literal match.
    assert_indicator(&mut pane, 100, ":2", now);
    type_keys(&mut pane, "u", now);
    assert_indicator(&mut pane, 100, ":2", now);
    submit(&mut pane, ":%s/how/now/g", now);
    assert_indicator(&mut pane, 100, "🄵 0:0", now);
    type_keys(&mut pane, "u", now);
    assert_indicator(&mut pane, 100, ":2", now);
    let first = pane.active_tab().unwrap();
    pane.new_buffer(OpenOptions::default()).unwrap();
    assert!(!row(&mut pane, 100, now).contains('🄵'));
    pane.focus_tab(&first).unwrap();
    assert_indicator(&mut pane, 100, ":2", now);
    std::fs::write(directory.path().join("search.md"), "now\n").unwrap();
    submit(&mut pane, ":e!", now);
    assert!(!row(&mut pane, 100, now).contains('🄵'));
}

#[test]
fn help_rows_use_meaningful_descriptions_and_never_print_internal_test_ids() {
    let directory = tempfile::tempdir().unwrap();
    let now = Instant::now();
    let mut pane = pane(directory.path(), "how\n", false, now);
    type_keys(&mut pane, " h", now);
    type_keys(&mut pane, "n / N", now);
    let frame = pane.render(160, 30, now);
    let text: String = frame
        .cells
        .iter()
        .map(|cell| cell.symbol.as_str())
        .collect();
    assert!(
        text.to_lowercase().contains("next/previous search matches"),
        "{text}"
    );
    for internal in ["R-N4", "R-N1", "R-I1", "V-X1"] {
        assert!(!text.contains(internal));
    }
    send(&mut pane, KeyCodeKind::Esc, now);
    type_keys(&mut pane, " h", now);
    type_keys(&mut pane, "rendered row", now);
    let frame = pane.render(160, 30, now);
    let text: String = frame
        .cells
        .iter()
        .map(|cell| cell.symbol.as_str())
        .collect();
    assert!(!text.contains("R-N1"));
}
