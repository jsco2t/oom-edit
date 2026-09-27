//! Headless consumer assertions for metadata and actual input ownership.

use std::path::Path;
use std::time::{Duration, Instant};

use oom_edit::{
    AllowAllFileAccess, BindingExecution, BindingId, BindingSequence, ClipboardError,
    ClipboardSink, Config, ConfigPersistError, DisplayMode, EditorPane, HostReservation,
    InputDisposition, KeyCode, KeyCodeKind, KeyInput, KeyOwnership, Mode, Modifiers, OpenOptions,
    PaneInit, PaneInput, PaneOptions, PaneServices, ThemeCatalog, ThemePersistenceSink,
    ThemeSelection, ThemeSlot, Tier,
};

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

fn pane(base: &Path, now: Instant, inline_hints: bool) -> EditorPane {
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
            inline_hints,
            ..PaneOptions::default()
        },
        initial_paths: vec![],
        now,
    })
    .pane;
    pane.new_buffer(OpenOptions::default()).unwrap();
    pane.render(200, 30, now);
    pane.drain_events();
    pane
}

fn special(kind: KeyCodeKind) -> KeyInput {
    KeyInput {
        code: KeyCode { kind },
        mods: Modifiers::default(),
    }
}
fn key(ch: char) -> KeyInput {
    special(KeyCodeKind::Char(ch))
}
fn feed(pane: &mut EditorPane, text: &str, now: Instant) {
    for ch in text.chars() {
        pane.handle_input(PaneInput::Key(key(ch)), now);
    }
}
fn row(pane: &mut EditorPane, now: Instant) -> String {
    let frame = pane.render(200, 30, now);
    frame.cells[29 * 200..30 * 200]
        .iter()
        .map(|cell| cell.symbol.as_str())
        .collect()
}

#[test]
fn fr_080_hint_parity() {
    let directory = tempfile::tempdir().unwrap();
    let now = Instant::now();
    let mut pane = pane(directory.path(), now, true);
    assert_eq!(
        pane.hints()
            .iter()
            .map(|c| c.text.as_str())
            .collect::<Vec<_>>(),
        [
            "v=select",
            "/=search",
            ":=command",
            "?=commands",
            "Space [h=commands, ?=commands, w=save, q=quit]",
        ]
    );
    assert_eq!(
        pane.hints()[4].compact_text.as_deref(),
        Some("Space+h/?=help")
    );
    assert!(pane.hints().iter().all(|c| !c.disabled));
    for cell in pane.hints() {
        assert!(row(&mut pane, now).contains(&cell.text));
    }
    feed(&mut pane, "v", now);
    assert_eq!(pane.hints()[0].text, "Esc / Ctrl-C=cancel selection");
    pane.handle_input(PaneInput::Key(special(KeyCodeKind::Esc)), now);
    feed(&mut pane, "i", now);
    assert!(pane.hints().is_empty());
    pane.handle_input(PaneInput::Key(special(KeyCodeKind::Esc)), now);
    feed(&mut pane, ":", now);
    assert!(pane.hints().is_empty());
    pane.handle_input(PaneInput::Key(special(KeyCodeKind::Esc)), now);
    feed(&mut pane, "?", now);
    assert_eq!(pane.hints().len(), 1);
    assert!(row(&mut pane, now).contains(&pane.hints()[0].text));
}

#[test]
fn fr_081_which_key_delay() {
    let directory = tempfile::tempdir().unwrap();
    let now = Instant::now();
    let mut pane = pane(directory.path(), now, false);
    assert!(pane.which_key().is_none());
    feed(&mut pane, " ", now);
    pane.tick(now + Duration::from_millis(149));
    assert!(pane.which_key().is_none());
    pane.tick(now + Duration::from_millis(150));
    let which = pane.which_key().unwrap();
    assert_eq!(which.prefix, "Space");
    assert_eq!(
        which
            .entries
            .iter()
            .map(|entry| entry.key)
            .collect::<Vec<_>>(),
        "h?wqtmsazd".chars().map(key).collect::<Vec<_>>()
    );
    pane.set_focused(false);
    assert!(pane.which_key().is_none());
    pane.set_focused(true);
    assert!(pane.which_key().is_none());
}

#[test]
fn fr_082_binding_roles() {
    let directory = tempfile::tempdir().unwrap();
    let now = Instant::now();
    let pane = pane(directory.path(), now, false);
    let bindings = pane.bindings();
    assert_binding_contract(&bindings);
    assert!(!bindings.is_empty());
    let mut ids = std::collections::HashSet::new();
    let mut names = std::collections::HashSet::new();
    for binding in &bindings {
        assert!(ids.insert(binding.id));
        assert!(names.insert(&binding.name));
        assert!(!binding.modes.is_empty());
        assert!(!binding.sequences.is_empty());
    }
    assert_eq!(
        bindings
            .iter()
            .filter(|b| b.execution == BindingExecution::ExecutableApp)
            .count(),
        10
    );
    assert_eq!(
        bindings
            .iter()
            .find(|b| b.name == "select-character")
            .unwrap()
            .execution,
        BindingExecution::ExecutableCore
    );
    assert!(bindings
        .iter()
        .any(|b| b.execution == BindingExecution::ReferenceOnly));
}

fn assert_binding_contract(bindings: &[oom_edit::EditorBinding]) {
    assert_eq!(bindings.len(), 55);
    assert_eq!(
        bindings
            .iter()
            .filter(|row| row.execution == BindingExecution::ExecutableApp)
            .count(),
        10
    );
    assert_eq!(
        bindings
            .iter()
            .filter(|row| row.execution == BindingExecution::ExecutableCore)
            .count(),
        23
    );
    assert_eq!(
        bindings
            .iter()
            .filter(|row| row.execution == BindingExecution::ReferenceOnly)
            .count(),
        22
    );
    let select = bindings
        .iter()
        .find(|row| row.id == BindingId::EnterCharacterSelect)
        .unwrap();
    assert_eq!(select.modes, [Mode::Normal]);
    assert_eq!(select.sequences, [BindingSequence::Keys(vec![key('v')])]);
    let help = bindings
        .iter()
        .find(|row| row.id == BindingId::Help)
        .unwrap();
    assert_eq!(help.modes, [Mode::Normal, Mode::Select]);
    assert_eq!(
        help.sequences,
        [
            BindingSequence::Keys(vec![key(' '), key('h')]),
            BindingSequence::Keys(vec![key('?')]),
            BindingSequence::Keys(vec![key(' '), key('?')])
        ]
    );
}

#[test]
fn registry_contract_rejects_missing_rows_changed_keys_and_executable_references() {
    let directory = tempfile::tempdir().unwrap();
    let now = Instant::now();
    let bindings = pane(directory.path(), now, false).bindings();
    assert_binding_contract(&bindings);
    let mut missing = bindings.clone();
    missing.pop();
    assert!(std::panic::catch_unwind(|| assert_binding_contract(&missing)).is_err());
    let mut changed = bindings.clone();
    changed[0].sequences = vec![BindingSequence::Keys(vec![key('q')])];
    assert!(std::panic::catch_unwind(|| assert_binding_contract(&changed)).is_err());
    let mut executable = bindings;
    executable
        .iter_mut()
        .find(|row| row.execution == BindingExecution::ReferenceOnly)
        .unwrap()
        .execution = BindingExecution::ExecutableApp;
    assert!(std::panic::catch_unwind(|| assert_binding_contract(&executable)).is_err());
}

#[test]
fn pending_native_grammars_keep_app_shortcut_precedence_and_unknown_keys_unchanged() {
    let directory = tempfile::tempdir().unwrap();
    let now = Instant::now();
    for prefix in ["g", "2", "g~", "v\""] {
        let mut pane = pane(directory.path(), now, false);
        feed(&mut pane, prefix, now);
        assert_eq!(
            pane.key_ownership(special(KeyCodeKind::F(1))),
            KeyOwnership::Pending
        );
        assert_eq!(
            pane.key_ownership(key('?')),
            KeyOwnership::AppCommand(BindingId::Help)
        );
        pane.handle_input(PaneInput::Key(key('?')), now);
        assert!(pane.input_state().modal);
    }
    let mut pane = pane(directory.path(), now, false);
    feed(&mut pane, " ", now);
    assert_eq!(pane.key_ownership(key('i')), KeyOwnership::Pending);
    pane.handle_input(PaneInput::Key(key('i')), now);
    assert_eq!(pane.status().unwrap().mode, Mode::Insert);
    let alt = KeyInput {
        mods: Modifiers {
            alt: true,
            ..Modifiers::default()
        },
        ..key('x')
    };
    pane.handle_input(PaneInput::Key(alt), now);
    assert_eq!(pane.text(&pane.active_tab().unwrap()).unwrap(), "x");
    assert_eq!(
        EditorPane::host_reservation(special(KeyCodeKind::F(2))),
        None
    );
    assert_eq!(
        EditorPane::host_reservation(KeyInput {
            mods: Modifiers {
                ctrl: true,
                shift: true,
                alt: false
            },
            ..key('g')
        }),
        None
    );
}

#[test]
fn fr_083_modified_key_contexts() {
    let directory = tempfile::tempdir().unwrap();
    let now = Instant::now();
    let mut pane = pane(directory.path(), now, false);
    let alt = KeyInput {
        mods: Modifiers {
            alt: true,
            ..Modifiers::default()
        },
        ..key('x')
    };
    feed(&mut pane, "i", now);
    assert_eq!(pane.key_ownership(alt), KeyOwnership::TextEntry);
    assert_eq!(
        EditorPane::host_reservation(alt),
        Some(HostReservation::AltChord)
    );
    assert_eq!(
        pane.handle_input(PaneInput::Key(alt), now),
        InputDisposition::Consumed
    );
    assert_eq!(pane.text(&pane.active_tab().unwrap()).unwrap(), "x");
}

#[test]
fn fr_083_reserved_key_dispatch() {
    let directory = tempfile::tempdir().unwrap();
    let now = Instant::now();
    let mut pane = pane(directory.path(), now, false);
    let control_g = KeyInput {
        mods: Modifiers {
            ctrl: true,
            ..Modifiers::default()
        },
        ..key('g')
    };
    assert_eq!(
        EditorPane::host_reservation(control_g),
        Some(HostReservation::ControlG)
    );
    assert_eq!(pane.key_ownership(control_g), KeyOwnership::CoreGrammar);
    assert_eq!(
        pane.key_ownership(special(KeyCodeKind::F(1))),
        KeyOwnership::Unclaimed
    );
    assert_eq!(
        EditorPane::host_reservation(special(KeyCodeKind::F(1))),
        Some(HostReservation::HelpFunction)
    );
    feed(&mut pane, "g", now);
    assert_eq!(
        pane.key_ownership(special(KeyCodeKind::F(1))),
        KeyOwnership::Pending
    );
    assert_eq!(
        pane.handle_input(PaneInput::Key(special(KeyCodeKind::F(1))), now),
        InputDisposition::Consumed
    );
}

#[test]
fn fr_084_palette_which_key_parity() {
    let directory = tempfile::tempdir().unwrap();
    let now = Instant::now();
    let mut pane = pane(directory.path(), now, true);
    feed(&mut pane, " ", now);
    pane.tick(now + Duration::from_millis(150));
    let which = pane.which_key().unwrap();
    let text = format!(
        "{}: {}",
        which.prefix,
        which
            .entries
            .iter()
            .map(|entry| {
                let KeyCodeKind::Char(ch) = entry.key.code.kind else {
                    panic!("not a character continuation")
                };
                format!("{ch}={}", entry.label)
            })
            .collect::<Vec<_>>()
            .join("  ")
    );
    let frame = pane.render(400, 30, now + Duration::from_millis(150));
    let status_row = frame.cells[29 * 400..30 * 400]
        .iter()
        .map(|cell| cell.symbol.as_str())
        .collect::<String>();
    assert!(status_row.contains(&text), "{status_row}");
}

#[test]
fn fr_085_status_data() {
    let directory = tempfile::tempdir().unwrap();
    let now = Instant::now();
    let mut pane = pane(directory.path(), now, false);
    feed(&mut pane, "i", now);
    pane.handle_input(PaneInput::Paste("one\ntwo\nthree".into()), now);
    let status = pane.status().unwrap();
    assert_eq!(status.mode, Mode::Insert);
    assert!(status.path.is_none());
    assert!(status.dirty);
    assert!(
        !status.is_new,
        "unnamed buffers do not display the named-new-file badge"
    );
    assert_eq!(status.ruler.line, 3);
    assert_eq!(status.ruler.column, 6);
    assert_eq!(status.ruler.line_count, 3);
    assert_eq!(status.ruler.text, "3:6  100% Bot");
    assert!(row(&mut pane, now).contains(&status.ruler.text));
}

#[test]
fn real_dispatch_ownership_matrix_includes_text_pending_and_modal_noops() {
    let directory = tempfile::tempdir().unwrap();
    let now = Instant::now();
    let contexts = [
        ("", KeyOwnership::CoreGrammar),
        ("i", KeyOwnership::TextEntry),
        ("v", KeyOwnership::CoreGrammar),
        (":", KeyOwnership::TextEntry),
        ("/", KeyOwnership::TextEntry),
        ("2", KeyOwnership::Pending),
        ("g", KeyOwnership::Pending),
        ("g~", KeyOwnership::Pending),
        ("v\"", KeyOwnership::Pending),
        (" ", KeyOwnership::Pending),
        ("?", KeyOwnership::Modal),
        (" d", KeyOwnership::Modal),
    ];
    let inputs = [
        KeyInput {
            mods: Modifiers {
                alt: true,
                ..Modifiers::default()
            },
            ..key('x')
        },
        KeyInput {
            mods: Modifiers {
                ctrl: true,
                ..Modifiers::default()
            },
            ..key('g')
        },
        special(KeyCodeKind::F(1)),
        special(KeyCodeKind::F(12)),
        special(KeyCodeKind::Noop),
        KeyInput {
            mods: Modifiers {
                ctrl: true,
                ..Modifiers::default()
            },
            ..special(KeyCodeKind::Right)
        },
        KeyInput {
            mods: Modifiers {
                alt: true,
                shift: true,
                ..Modifiers::default()
            },
            ..key('X')
        },
    ];
    let mut checked = 0;
    for (prefix, owner) in contexts {
        for input in inputs {
            let mut pane = pane(directory.path(), now, false);
            feed(&mut pane, prefix, now);
            assert_eq!(
                pane.input_state().modal,
                owner == KeyOwnership::Modal,
                "{prefix:?}"
            );
            assert_eq!(
                pane.input_state().text_entry,
                owner == KeyOwnership::TextEntry || prefix == "?",
                "{prefix:?}"
            );
            if owner == KeyOwnership::Pending {
                assert_eq!(
                    pane.key_ownership(special(KeyCodeKind::F(1))),
                    KeyOwnership::Pending,
                    "pending precondition {prefix:?}"
                );
            }
            let unsupported = matches!(input.code.kind, KeyCodeKind::Noop | KeyCodeKind::F(_));
            let expected = if unsupported && owner == KeyOwnership::CoreGrammar {
                KeyOwnership::Unclaimed
            } else {
                owner
            };
            let tab = pane.active_tab().unwrap();
            let before = pane.text(&tab).unwrap();
            let cursor = pane.source_cursor(&tab).unwrap();
            let status = pane.status();
            let hints = pane.hints();
            pane.drain_events();
            assert_eq!(pane.key_ownership(input), expected, "{prefix:?} {input:?}");
            assert_eq!(
                pane.handle_input(PaneInput::Key(input), now),
                if expected == KeyOwnership::Unclaimed {
                    InputDisposition::NotConsumed
                } else {
                    InputDisposition::Consumed
                },
                "{prefix:?} {input:?}"
            );
            if input.code.kind == KeyCodeKind::Noop || expected == KeyOwnership::Unclaimed {
                assert_eq!(pane.text(&tab).unwrap(), before);
                assert_eq!(pane.source_cursor(&tab).unwrap(), cursor);
                assert_eq!(pane.status(), status);
                assert_eq!(pane.hints(), hints);
                assert!(pane.drain_events().is_empty());
            }
            if owner == KeyOwnership::Modal {
                assert_eq!(pane.text(&tab).unwrap(), before);
            }
            checked += 1;
        }
    }
    assert_eq!(checked, 84);
}

#[test]
fn app_sequences_and_context_overrides_drive_the_existing_dispatcher() {
    let directory = tempfile::tempdir().unwrap();
    let now = Instant::now();
    let inventory = pane(directory.path(), now, false).bindings();
    let mut checked = 0;
    for binding in inventory
        .iter()
        .filter(|row| row.execution == BindingExecution::ExecutableApp)
    {
        for mode in &binding.modes {
            for sequence in &binding.sequences {
                let BindingSequence::Keys(keys) = sequence else {
                    panic!("App bindings must be concrete keys")
                };
                let mut pane = pane(directory.path(), now, false);
                if *mode == Mode::Select {
                    feed(&mut pane, "v", now);
                }
                for (index, input) in keys.iter().enumerate() {
                    let expected = if index + 1 == keys.len() {
                        KeyOwnership::AppCommand(binding.id)
                    } else {
                        KeyOwnership::Pending
                    };
                    assert_eq!(
                        pane.key_ownership(*input),
                        expected,
                        "{:?} {mode:?} {input:?}",
                        binding.id
                    );
                    assert_eq!(
                        pane.handle_input(PaneInput::Key(*input), now),
                        InputDisposition::Consumed
                    );
                }
                match binding.id {
                    BindingId::Help | BindingId::Trouble => assert!(pane.input_state().modal),
                    BindingId::SpellToggle => assert!(pane.status().unwrap().spell.is_none()),
                    BindingId::DefaultFrontMatter => assert!(pane
                        .text(&pane.active_tab().unwrap())
                        .unwrap()
                        .starts_with("---\n")),
                    BindingId::Quit => assert!(pane.tabs().is_empty()),
                    BindingId::CycleTheme => assert!(pane
                        .drain_events()
                        .iter()
                        .any(|event| matches!(event, oom_edit::PaneEvent::ThemeChanged { .. }))),
                    BindingId::Save
                    | BindingId::SpellSuggest
                    | BindingId::SpellAdd
                    | BindingId::SpaceDigitTab => {}
                    _ => panic!("unclassified App behavior: {:?}", binding.id),
                }
                checked += 1;
            }
        }
    }
    assert_eq!(checked, 39);
    for prefix in ["i", ":", "/"] {
        let mut pane = pane(directory.path(), now, false);
        feed(&mut pane, prefix, now);
        assert_eq!(pane.key_ownership(key('?')), KeyOwnership::TextEntry);
        feed(&mut pane, " ?", now);
        assert!(!pane.input_state().modal);
        if prefix == "i" {
            assert_eq!(pane.text(&pane.active_tab().unwrap()).unwrap(), " ?");
        }
    }
}

#[test]
fn empty_unfocused_and_focus_suspended_grammars_have_exact_ownership() {
    let directory = tempfile::tempdir().unwrap();
    let now = Instant::now();
    let mut pane = pane(directory.path(), now, false);
    feed(&mut pane, "g~", now);
    assert_eq!(pane.key_ownership(key('x')), KeyOwnership::Pending);
    pane.set_focused(false);
    for input in [
        key('x'),
        special(KeyCodeKind::F(1)),
        special(KeyCodeKind::Noop),
    ] {
        assert_eq!(pane.key_ownership(input), KeyOwnership::Unclaimed);
        assert_eq!(
            pane.handle_input(PaneInput::Key(input), now),
            InputDisposition::NotConsumed
        );
    }
    pane.set_focused(true);
    assert_eq!(pane.key_ownership(key('x')), KeyOwnership::Pending);
    pane.handle_input(PaneInput::Key(special(KeyCodeKind::Esc)), now);
    feed(&mut pane, " ", now);
    pane.set_focused(false);
    pane.set_focused(true);
    assert_eq!(pane.key_ownership(key('h')), KeyOwnership::CoreGrammar);
    pane.close(&pane.active_tab().unwrap()).unwrap();
    assert!(pane.status().is_none());
    assert!(pane.hints().is_empty());
    assert!(pane.which_key().is_none());
    assert_eq!(pane.key_ownership(key('i')), KeyOwnership::Unclaimed);
}

#[test]
fn core_key_metadata_drives_real_modes_selections_and_tab_effects() {
    let directory = tempfile::tempdir().unwrap();
    let now = Instant::now();
    let inventory = pane(directory.path(), now, false).bindings();
    let mut checked = 0;
    for binding in inventory
        .iter()
        .filter(|row| row.execution == BindingExecution::ExecutableCore)
    {
        for mode in &binding.modes {
            for sequence in &binding.sequences {
                let BindingSequence::Keys(keys) = sequence else {
                    continue;
                };
                let mut pane = pane(directory.path(), now, false);
                feed(&mut pane, "i", now);
                pane.handle_input(PaneInput::Paste("    alpha beta\nsecond".into()), now);
                pane.handle_input(PaneInput::Key(special(KeyCodeKind::Esc)), now);
                pane.render(200, 30, now);
                feed(&mut pane, "gg", now);
                pane.render(200, 30, now);
                assert_eq!(
                    pane.source_cursor(&pane.active_tab().unwrap()).unwrap().0,
                    0
                );
                if matches!(binding.id, BindingId::NextTab | BindingId::PrevTab) {
                    let first = pane.active_tab().unwrap();
                    pane.new_buffer(OpenOptions::default()).unwrap();
                    pane.focus_tab(&first).unwrap();
                }
                if *mode == Mode::Select {
                    feed(&mut pane, "v", now);
                }
                assert_eq!(pane.status().unwrap().mode, *mode);
                if matches!(
                    binding.id,
                    BindingId::SelectIndent | BindingId::SelectOutdent
                ) {
                    pane.handle_input(PaneInput::Key(special(KeyCodeKind::Esc)), now);
                    feed(&mut pane, "Vj", now);
                }
                let before_tab = pane.active_tab();
                let before_text = pane.text(&before_tab.clone().unwrap()).unwrap();
                for input in keys {
                    assert!(matches!(
                        pane.key_ownership(*input),
                        KeyOwnership::CoreGrammar | KeyOwnership::Pending
                    ));
                    assert_eq!(
                        pane.handle_input(PaneInput::Key(*input), now),
                        InputDisposition::Consumed
                    );
                }
                match binding.id {
                    BindingId::EnterCharacterSelect
                    | BindingId::EnterLineSelect
                    | BindingId::EnterBlockSelect => {
                        assert_eq!(pane.status().unwrap().mode, Mode::Select)
                    }
                    BindingId::CancelSelect
                    | BindingId::SelectYank
                    | BindingId::SelectYankPlainText => {
                        assert_eq!(pane.status().unwrap().mode, Mode::Normal)
                    }
                    BindingId::SelectChange => {
                        assert_eq!(pane.status().unwrap().mode, Mode::Insert)
                    }
                    BindingId::SelectDelete
                    | BindingId::SelectIndent
                    | BindingId::SelectOutdent => {
                        assert_ne!(
                            pane.text(&pane.active_tab().unwrap()).unwrap(),
                            before_text,
                            "{:?} {mode:?} {sequence:?}",
                            binding.id
                        )
                    }
                    BindingId::SelectSwapAnchor => {
                        assert_eq!(pane.status().unwrap().mode, Mode::Select)
                    }
                    BindingId::Search => {
                        assert_eq!(pane.status().unwrap().prompt.as_deref(), Some("/"));
                        assert!(pane.input_state().text_entry);
                    }
                    BindingId::CommandMode => {
                        assert_eq!(pane.status().unwrap().mode, Mode::Command);
                        assert_eq!(pane.status().unwrap().prompt.as_deref(), Some(":"));
                    }
                    BindingId::NextTab | BindingId::PrevTab => {
                        assert_ne!(pane.active_tab(), before_tab)
                    }
                    BindingId::SpellNext | BindingId::SpellPrevious => {
                        assert_eq!(pane.text(&pane.active_tab().unwrap()).unwrap(), before_text)
                    }
                    _ => panic!("unclassified core sequence {:?}", binding.id),
                }
                checked += 1;
            }
        }
    }
    assert_eq!(checked, 23);
    let mut pane = pane(directory.path(), now, false);
    let first = pane.active_tab().unwrap();
    let second = pane.new_buffer(OpenOptions::default()).unwrap();
    pane.focus_tab(&first).unwrap();
    feed(&mut pane, "2gt", now);
    assert_eq!(pane.active_tab(), Some(second));
    feed(&mut pane, "v\"a", now);
    assert_eq!(pane.status().unwrap().mode, Mode::Select);
    assert_eq!(
        pane.key_ownership(special(KeyCodeKind::F(1))),
        KeyOwnership::Pending
    );
    pane.handle_input(PaneInput::Key(special(KeyCodeKind::Esc)), now);
    assert_eq!(
        pane.key_ownership(special(KeyCodeKind::F(1))),
        KeyOwnership::Unclaimed
    );
}

#[test]
fn ex_metadata_has_a_core_dispatcher_without_executable_palette_actions() {
    let directory = tempfile::tempdir_in(std::env::temp_dir().canonicalize().unwrap()).unwrap();
    let now = Instant::now();
    let path = directory.path().join("note.md");
    std::fs::write(&path, "note\n").unwrap();
    let mut pane = pane(directory.path(), now, false);
    let bindings = pane.bindings();
    for id in [
        BindingId::SpellSet,
        BindingId::TabNew,
        BindingId::TabClose,
        BindingId::QuitAll,
    ] {
        let row = bindings.iter().find(|row| row.id == id).unwrap();
        assert_eq!(row.execution, BindingExecution::ExecutableCore);
        assert_eq!(row.modes, [Mode::Command]);
        assert!(row
            .sequences
            .iter()
            .all(|sequence| matches!(sequence, BindingSequence::Ex(_))));
    }
    for command in ["set nospell", "set spell", "tabnew note.md", "tabclose"] {
        feed(&mut pane, ":", now);
        feed(&mut pane, command, now);
        assert_eq!(
            pane.key_ownership(special(KeyCodeKind::Enter)),
            KeyOwnership::TextEntry
        );
        pane.handle_input(PaneInput::Key(special(KeyCodeKind::Enter)), now);
        match command {
            "set nospell" => assert!(pane.status().unwrap().spell.is_none()),
            "set spell" => assert!(pane.status().unwrap().spell.is_some()),
            "tabnew note.md" => {
                assert_eq!(pane.tabs().len(), 2);
                assert_eq!(pane.status().unwrap().path.as_deref(), Some(path.as_path()));
            }
            "tabclose" => assert_eq!(pane.tabs().len(), 1),
            _ => unreachable!(),
        }
    }
    feed(&mut pane, ":qa", now);
    pane.handle_input(PaneInput::Key(special(KeyCodeKind::Enter)), now);
    assert!(pane
        .drain_events()
        .iter()
        .any(|event| matches!(event, oom_edit::PaneEvent::QuitAllRequested { .. })));
    assert_eq!(std::fs::read(&path).unwrap(), b"note\n");
}

#[test]
fn status_tracks_named_paths_active_tabs_prompts_spell_and_disk_markers() {
    let directory = tempfile::tempdir_in(std::env::temp_dir().canonicalize().unwrap()).unwrap();
    let now = Instant::now();
    let path = directory.path().join("界 note.md");
    std::fs::write(&path, "first\nsecond\n").unwrap();
    let mut pane = pane(directory.path(), now, false);
    let unnamed = pane.active_tab().unwrap();
    pane.open_existing(&path, OpenOptions::default()).unwrap();
    let named = pane.active_tab().unwrap();
    let status = pane.status().unwrap();
    assert_eq!(status.path.as_deref(), Some(path.as_path()));
    assert!(!status.dirty);
    assert!(!status.is_new);
    assert_eq!(status.ruler.line, 1);
    assert_eq!(status.ruler.column, 1);
    assert_eq!(status.ruler.line_count, 3);
    assert_eq!(status.ruler.text, "1:1  33% Top");
    assert_eq!(status.spell.unwrap().count, 0);
    assert!(status.spell.unwrap().highest_severity.is_none());
    feed(&mut pane, " z", now);
    assert!(pane.status().unwrap().spell.is_none());
    assert!(row(&mut pane, now).contains("[spell off]"));
    feed(&mut pane, ":set wrap", now);
    assert_eq!(pane.status().unwrap().prompt.as_deref(), Some(":set wrap"));
    pane.handle_input(PaneInput::Key(special(KeyCodeKind::Esc)), now);
    pane.render(200, 30, now);
    feed(&mut pane, "/second", now);
    assert_eq!(pane.status().unwrap().prompt.as_deref(), Some("/second"));
    pane.handle_input(PaneInput::Key(special(KeyCodeKind::Esc)), now);
    pane.set_focused(false);
    std::fs::write(&path, "later\n").unwrap();
    pane.notify_paths_changed(std::slice::from_ref(&path))
        .unwrap();
    assert_eq!(
        pane.status().unwrap().disk_marker.as_deref(),
        Some("[disk changed]")
    );
    assert_eq!(pane.text(&named).unwrap(), "first\nsecond\n");
    pane.focus_tab(&unnamed).unwrap();
    assert!(pane.status().unwrap().path.is_none());
    pane.set_focused(true);
    let new_path = directory.path().join("never-created.md");
    pane.open(&new_path, OpenOptions::default()).unwrap();
    assert!(pane.status().unwrap().is_new);
    assert!(!new_path.exists());
    assert!(row(&mut pane, now).contains("[new file]"));
    assert_eq!(std::fs::read(&path).unwrap(), b"later\n");
}

#[test]
fn prepared_close_freezes_ownership_but_external_change_keeps_text_entry() {
    let directory = tempfile::tempdir().unwrap();
    let now = Instant::now();
    let mut pane = pane(directory.path(), now, false);
    feed(&mut pane, "i", now);
    let tab = pane.active_tab().unwrap();
    let request = pane.prepare_close(std::slice::from_ref(&tab)).unwrap();
    let token = pane.take_prepared_close(&request).unwrap();
    assert_eq!(pane.key_ownership(key('x')), KeyOwnership::Lifecycle);
    assert_eq!(
        pane.handle_input(PaneInput::Key(key('x')), now),
        InputDisposition::Consumed
    );
    assert_eq!(pane.text(&tab).unwrap(), "");
    pane.abort_close(token).unwrap();
    assert_eq!(pane.key_ownership(key('x')), KeyOwnership::TextEntry);
    let token = pane.begin_external_change().unwrap();
    assert_eq!(pane.key_ownership(key('x')), KeyOwnership::TextEntry);
    pane.handle_input(PaneInput::Key(key('x')), now);
    assert_eq!(pane.text(&tab).unwrap(), "x");
    pane.abort_external_change(token).unwrap();
}
