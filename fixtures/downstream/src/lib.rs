//! Independently compiled public consumer, with no private or terminal imports.

#[cfg(test)]
mod tests {
    use oom_edit::{
        AllowAllFileAccess, ClipboardError, ClipboardSink, Config, ConfigPersistError, DisplayMode,
        EditorPane, KeyCode, KeyCodeKind, KeyInput, Mode, Modifiers, OpenOptions, PaneEvent,
        PaneInit, PaneInput, PaneOptions, PaneServices, ThemeCatalog, ThemePersistenceSink,
        ThemeSelection, ThemeSlot, Tier,
    };
    use std::time::Instant;

    struct DisabledPersistence;
    impl ClipboardSink for DisabledPersistence {
        fn copy(&mut self, _: &str) -> Result<(), ClipboardError> {
            Ok(())
        }
    }
    impl ThemePersistenceSink for DisabledPersistence {
        fn persist_theme(&mut self, _: ThemeSlot, _: &str) -> Result<(), ConfigPersistError> {
            Ok(())
        }
    }
    fn key(kind: KeyCodeKind) -> PaneInput {
        PaneInput::Key(KeyInput {
            code: KeyCode { kind },
            mods: Modifiers::default(),
        })
    }

    #[test]
    fn fr_112_independent_consumer() {
        let base = std::env::current_dir().unwrap();
        let now = Instant::now();
        let construction = EditorPane::construct(PaneInit {
            config: Config::default(),
            theme_catalog: ThemeCatalog::builtins(),
            theme_selection: ThemeSelection::new(None, DisplayMode::Dark, Tier::Monochrome),
            services: PaneServices {
                clipboard_sink: Box::new(DisabledPersistence),
                theme_sink: Box::new(DisabledPersistence),
                file_access_policy: Box::new(AllowAllFileAccess),
                config_base_directory: base.clone(),
                personal_dictionary_path: base.join("personal.txt"),
                working_directory: base,
            },
            options: PaneOptions::default(),
            initial_paths: Vec::new(),
            now,
        });
        assert!(construction.report.paths.is_empty());
        let mut pane = construction.pane;
        assert!(pane.tabs().is_empty());
        let id = pane.new_buffer(OpenOptions::default()).unwrap();
        let first = pane.render(80, 24, now);
        assert_eq!(first.cells.len(), 80 * 24);
        assert!(!pane.hints().is_empty());
        assert_eq!(pane.bindings().len(), 55);
        pane.handle_input(key(KeyCodeKind::Char('i')), now);
        pane.handle_input(PaneInput::Paste("# Independent λ\n".into()), now);
        assert_eq!(pane.status().unwrap().mode, Mode::Insert);
        assert_eq!(pane.text(&id).unwrap(), "# Independent λ\n");
        let inserted = pane.render(80, 24, now);
        assert_eq!(inserted.cells.len(), 80 * 24);
        assert!(inserted.cursor.is_some());
        assert!(!inserted.visually_equals(&first));
        pane.handle_input(key(KeyCodeKind::Esc), now);
        pane.handle_input(key(KeyCodeKind::Char('u')), now);
        assert_eq!(pane.text(&id).unwrap(), "");
        assert!(!pane.tabs()[0].dirty);
        pane.drain_events();
        let request = pane.close_all().unwrap();
        let events = pane.drain_events();
        assert!(
            matches!(events.as_slice(), [PaneEvent::Closed { request: closed, .. }, PaneEvent::AllClosed { request: all }, PaneEvent::Completed { request: done }] if [closed, all, done].iter().all(|id| **id == request))
        );
        assert!(pane.tabs().is_empty());
        assert_eq!(pane.render(80, 24, now).cells.len(), 80 * 24);
    }
}
