use super::*;
use crate::{EditorPane, KeyOwnership, PaneInput};

#[test]
fn every_overlay_exports_exact_hints_and_owns_modified_and_noop_input() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("note.md");
    std::fs::write(&path, "teh\n").unwrap();
    let version = oom_edit_core::DiskVersion::observe(&path).unwrap();
    let diagnostic = oom_edit_core::Diagnostic {
        provider: oom_edit_core::DiagnosticProvider::Spell,
        severity: oom_edit_core::DiagnosticSeverity::Warning,
        range: 0..3,
        source_text: "teh".into(),
        message: "Unknown word: teh".into(),
    };
    let overlays = [
        Overlay::None,
        Overlay::open_palette(crate::command::Contexts::NORMAL),
        Overlay::open_spell_suggest(diagnostic, vec!["the".into()]),
        Overlay::open_trouble(vec![], TroubleProgress::Complete),
        Overlay::open_confirm_quit(CloseTabRequest {
            target: 0,
            force: false,
            dirty_policy: DirtyClosePolicy::Confirm,
        }),
        Overlay::open_confirm_overwrite(
            SaveRequest {
                target: 0,
                path: None,
                force: false,
                retarget: true,
                continuation: SaveContinuation::StayOpen,
            },
            path.clone(),
            version.clone(),
        ),
        Overlay::open_disk_change(0, path.clone(), version),
    ];
    assert_eq!(overlays.len(), 7);
    let now = Instant::now();
    for overlay in overlays {
        match &overlay {
            Overlay::None
            | Overlay::Palette(_)
            | Overlay::SpellSuggest(_)
            | Overlay::Trouble(_)
            | Overlay::ConfirmQuit(_)
            | Overlay::ConfirmOverwrite(_)
            | Overlay::DiskChange(_) => {}
        }
        let mut app = App::new(
            EditorSession::open_existing(&path).unwrap(),
            ResolvedTheme::injected("accessible", false, Tier::Monochrome),
            true,
            false,
            Box::new(oom_edit_core::RecordingClipboardSink::default()),
            Box::new(crate::config::DisabledConfigStore),
            now,
        );
        let expected_hints = overlay.hints().to_owned();
        let modal = overlay.is_some();
        app.overlay = overlay;
        let mut pane = EditorPane::from_app_for_test(app);
        if modal {
            assert_eq!(
                pane.hints(),
                vec![crate::HintCell {
                    text: expected_hints.clone(),
                    compact_text: None,
                    disabled: false
                }]
            );
            let frame = pane.render(200, 30, now);
            let row = frame.cells[29 * 200..30 * 200]
                .iter()
                .map(|cell| cell.symbol.as_str())
                .collect::<String>();
            assert!(row.contains(&expected_hints), "{row}");
        } else {
            assert_eq!(pane.hints().len(), 5);
        }
        let tab = pane.active_tab().unwrap();
        let text = pane.text(&tab).unwrap();
        for key in [
            KeyInput {
                mods: Modifiers {
                    alt: true,
                    ..Modifiers::default()
                },
                code: KeyCode {
                    kind: KeyCodeKind::Char('x'),
                },
            },
            KeyInput {
                mods: Modifiers {
                    ctrl: true,
                    ..Modifiers::default()
                },
                code: KeyCode {
                    kind: KeyCodeKind::Char('g'),
                },
            },
            KeyInput {
                mods: Modifiers::default(),
                code: KeyCode {
                    kind: KeyCodeKind::F(1),
                },
            },
            KeyInput {
                mods: Modifiers::default(),
                code: KeyCode {
                    kind: KeyCodeKind::Noop,
                },
            },
        ] {
            if modal {
                assert_eq!(pane.key_ownership(key), KeyOwnership::Modal);
                assert_eq!(
                    pane.handle_input(PaneInput::Key(key), now),
                    crate::InputDisposition::Consumed
                );
                assert!(pane.input_state().modal);
                assert!(pane.which_key().is_none());
                assert_eq!(pane.text(&tab).unwrap(), text);
            }
        }
        let previous_hints = pane.hints();
        pane.set_focused(false);
        assert_eq!(pane.hints(), previous_hints);
        for (width, height) in [(0, 0), (19, 4), (20, 5), (200, 30)] {
            let frame = pane.render(width, height, now);
            assert_eq!(frame.cells.len(), usize::from(width) * usize::from(height));
            assert!(frame.cursor.is_none());
            assert_eq!(pane.input_state().modal, modal);
            assert_eq!(pane.text(&tab).unwrap(), text);
        }
        pane.set_focused(true);
        let compact = pane.render(20, 5, now);
        if let Some(cursor) = compact.cursor {
            assert!(cursor.column < compact.width && cursor.row < compact.height);
        }
        pane.set_focused(false);
        assert_eq!(
            pane.key_ownership(KeyInput {
                code: KeyCode {
                    kind: KeyCodeKind::Char('x')
                },
                mods: Modifiers::default()
            }),
            KeyOwnership::Unclaimed
        );
    }
}
