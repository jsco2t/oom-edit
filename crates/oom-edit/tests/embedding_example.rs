#[path = "../examples/support/embedded_host.rs"]
mod embedded_host;
#[path = "support/incremental_host_vector.rs"]
mod incremental_host_vector;

use std::time::{Duration, Instant};

use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use embedded_host::{translate_key, EmbeddedHost};
use oom_edit::{KeyCodeKind, Modifiers, OpenOptions, PaneEvent, PaneInput};
use oom_edit_core::EditorSession;
use ratatui::{backend::TestBackend, layout::Rect, Terminal};

fn key(code: KeyCode, modifiers: KeyModifiers) -> Event {
    Event::Key(KeyEvent::new(code, modifiers))
}

#[test]
fn fr_111_split_host_example() {
    let directory = tempfile::tempdir().unwrap();
    let now = Instant::now();
    let mut host = EmbeddedHost::new(directory.path(), Vec::new(), now);
    let id = host.pane.new_buffer(OpenOptions::default()).unwrap();
    let mut terminal = Terminal::new(TestBackend::new(120, 20)).unwrap();
    terminal.draw(|frame| host.draw(frame, now)).unwrap();
    let area = Rect::new(24, 0, 96, 19);
    let before = host.pane.text(&id).unwrap();
    host.handle_event(key(KeyCode::F(1), KeyModifiers::NONE), area, now);
    assert_eq!(host.global_count, 1);
    assert_eq!(host.pane.text(&id).unwrap(), before);
    for hint in host.pane.hints() {
        assert!(host.status_text().contains(&hint.text));
    }
    host.handle_event(key(KeyCode::Char(' '), KeyModifiers::NONE), area, now);
    host.tick(now + Duration::from_millis(150));
    let which_key = host.pane.which_key().unwrap();
    for entry in which_key.entries {
        let KeyCodeKind::Char(key) = entry.key.code.kind else {
            panic!("unexpected Space continuation");
        };
        assert!(host
            .status_text()
            .contains(&format!("{key}={}", entry.label)));
    }
    terminal
        .draw(|frame| host.draw(frame, now + Duration::from_millis(150)))
        .unwrap();
    let screen = terminal.backend().buffer();
    assert_eq!(screen[(0, 0)].symbol(), "H");
    let owned = host.pane.render(area.width, area.height, now);
    for row in 0..owned.height {
        for column in 0..owned.width {
            let source =
                &owned.cells[usize::from(row) * usize::from(owned.width) + usize::from(column)];
            assert_eq!(
                screen[(area.x + column, area.y + row)].symbol(),
                source.symbol
            );
        }
    }
    host.handle_event(key(KeyCode::Esc, KeyModifiers::NONE), area, now);
    host.handle_event(key(KeyCode::Char('q'), KeyModifiers::ALT), area, now);
    assert!(host.quit);
    assert!(host.pane.tabs().is_empty());
    let all_closed = host
        .events
        .iter()
        .position(|event| matches!(event, PaneEvent::AllClosed { .. }))
        .unwrap();
    assert!(matches!(
        host.events[all_closed + 1],
        PaneEvent::Completed { .. }
    ));
}

#[test]
fn fr_111_disk_notification_and_global_interception() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("note.md");
    std::fs::write(&path, "before\n").unwrap();
    let now = Instant::now();
    let mut host = EmbeddedHost::new(directory.path(), vec![path.clone()], now);
    let id = host.pane.active_tab().unwrap();
    let area = Rect::new(24, 0, 56, 23);
    for input in ['i', ' ', ':'] {
        host.handle_event(key(KeyCode::Char(input), KeyModifiers::NONE), area, now);
        let before = host.pane.text(&id).unwrap();
        let state = host.pane.input_state();
        host.handle_event(key(KeyCode::F(1), KeyModifiers::NONE), area, now);
        assert_eq!(host.pane.text(&id).unwrap(), before);
        assert_eq!(host.pane.input_state(), state);
        host.handle_event(key(KeyCode::Esc, KeyModifiers::NONE), area, now);
    }
    assert_eq!(host.global_count, 3);
    std::fs::write(&path, "after\n").unwrap();
    host.pane
        .notify_paths_changed(std::slice::from_ref(&path))
        .unwrap();
    host.handle_event(Event::FocusGained, area, now);
    assert_eq!(host.pane.text(&id).unwrap(), "after\n");
    assert!(host
        .events
        .iter()
        .any(|event| matches!(event, PaneEvent::ReloadedFromDisk { tab, .. } if tab == &id)));
}

#[test]
fn fr_094_example_shared_translation_vectors() {
    fn code(value: &str) -> KeyCode {
        match value {
            "Esc" => KeyCode::Esc,
            "Tab" => KeyCode::Tab,
            "Enter" => KeyCode::Enter,
            "Backspace" => KeyCode::Backspace,
            "BackTab" => KeyCode::BackTab,
            value if value.starts_with("Char:") => {
                KeyCode::Char(value[5..].chars().next().unwrap())
            }
            _ => panic!("unknown code {value}"),
        }
    }
    fn modifiers(value: &str) -> KeyModifiers {
        match value {
            "none" => KeyModifiers::NONE,
            "ctrl" => KeyModifiers::CONTROL,
            "shift" => KeyModifiers::SHIFT,
            "ctrl_alt" => KeyModifiers::CONTROL | KeyModifiers::ALT,
            _ => panic!("unknown modifiers {value}"),
        }
    }
    for enhanced in [false, true] {
        let mut count = 0;
        for line in include_str!("data/terminal_keys.tsv")
            .lines()
            .filter(|line| !line.starts_with('#'))
        {
            let fields = line.split_whitespace().collect::<Vec<_>>();
            let kind = match fields[2] {
                "Press" => KeyEventKind::Press,
                "Repeat" => KeyEventKind::Repeat,
                "Release" => KeyEventKind::Release,
                _ => panic!("unknown kind"),
            };
            let event = KeyEvent::new_with_kind(code(fields[0]), modifiers(fields[1]), kind);
            let translated = translate_key(&event);
            assert_eq!(
                translated.is_some(),
                fields[5] == "yes",
                "enhanced={enhanced}: {line}"
            );
            if let Some(key) = translated {
                let expected = match code(fields[3]) {
                    KeyCode::Esc => KeyCodeKind::Esc,
                    KeyCode::Tab => KeyCodeKind::Tab,
                    KeyCode::Enter => KeyCodeKind::Enter,
                    KeyCode::Backspace => KeyCodeKind::Backspace,
                    KeyCode::BackTab => KeyCodeKind::BackTab,
                    KeyCode::Char(c) => KeyCodeKind::Char(c),
                    _ => unreachable!(),
                };
                assert_eq!(key.code.kind, expected, "{line}");
                let expected = modifiers(fields[4]);
                assert_eq!(
                    key.mods,
                    Modifiers {
                        ctrl: expected.contains(KeyModifiers::CONTROL),
                        alt: expected.contains(KeyModifiers::ALT),
                        shift: expected.contains(KeyModifiers::SHIFT)
                    }
                );
            }
            count += 1;
        }
        assert_eq!(count, 17);
    }
}

#[test]
fn shift_v_public_hosts_select_switch_cancel_and_delete_exact_line() {
    use oom_edit::{CommandPolicy, Mode, PaneOptions};

    let text = "# alpha λ\n\nsecond\n";
    for policy in [CommandPolicy::Standalone, CommandPolicy::Embedded] {
        for modifiers in [KeyModifiers::NONE, KeyModifiers::SHIFT] {
            let directory = tempfile::tempdir().unwrap();
            let path = directory.path().join("note.md");
            std::fs::write(&path, text).unwrap();
            let now = Instant::now();
            let mut host = EmbeddedHost::with_options(
                directory.path(),
                vec![path.clone()],
                PaneOptions {
                    command_policy: policy,
                    ..PaneOptions::default()
                },
                now,
            );
            let id = host.pane.active_tab().unwrap();
            let area = Rect::new(24, 0, 56, 23);
            host.pane.render(area.width, area.height, now);
            let line = key(KeyCode::Char('V'), modifiers);
            host.handle_event(line.clone(), area, now);
            assert_eq!(host.pane.status().unwrap().mode, Mode::Select);
            host.handle_event(line.clone(), area, now);
            assert_eq!(host.pane.status().unwrap().mode, Mode::Normal);
            host.handle_event(key(KeyCode::Char('v'), KeyModifiers::NONE), area, now);
            host.handle_event(line, area, now);
            assert_eq!(host.pane.status().unwrap().mode, Mode::Select);
            host.handle_event(key(KeyCode::Char('d'), KeyModifiers::NONE), area, now);
            assert_eq!(host.pane.status().unwrap().mode, Mode::Normal);
            assert_eq!(host.pane.text(&id).unwrap(), "\nsecond\n");
            host.handle_event(key(KeyCode::Char('u'), KeyModifiers::NONE), area, now);
            assert_eq!(host.pane.text(&id).unwrap(), text);
            assert_eq!(std::fs::read_to_string(&path).unwrap(), text);
        }
    }
}

#[test]
fn fr_117_two_public_hosts() {
    let directory = tempfile::tempdir().unwrap();
    let now = Instant::now();
    let mut embedded = EmbeddedHost::new(directory.path(), Vec::new(), now);
    let mut standalone = EmbeddedHost::with_options(
        directory.path(),
        Vec::new(),
        oom_edit::PaneOptions {
            command_policy: oom_edit::CommandPolicy::Standalone,
            ..oom_edit::PaneOptions::default()
        },
        now,
    )
    .pane;
    let embedded_id = embedded.pane.new_buffer(OpenOptions::default()).unwrap();
    let standalone_id = standalone.new_buffer(OpenOptions::default()).unwrap();
    let area = Rect::new(24, 0, 56, 23);
    let mut script = vec![
        key(KeyCode::Char('i'), KeyModifiers::NONE),
        Event::Paste("same λ text\n".into()),
        key(KeyCode::Esc, KeyModifiers::NONE),
        key(KeyCode::Char('u'), KeyModifiers::NONE),
        key(KeyCode::Char('r'), KeyModifiers::CONTROL),
    ];
    for sequence in [
        "ggvll",
        "\u{1b}",
        "Vj",
        "\u{1b}",
        "/same\n",
        ":set nowrap\n",
        " h",
        "\u{1b}",
    ] {
        script.extend(sequence.chars().map(|character| {
            key(
                match character {
                    '\u{1b}' => KeyCode::Esc,
                    '\n' => KeyCode::Enter,
                    character => KeyCode::Char(character),
                },
                KeyModifiers::NONE,
            )
        }));
    }
    embedded.tick(now);
    embedded.events.clear();
    standalone.drain_events();
    for event in script {
        match &event {
            Event::Key(key) => {
                standalone.handle_input(PaneInput::Key(translate_key(key).unwrap()), now);
            }
            Event::Paste(text) => {
                standalone.handle_input(PaneInput::Paste(text.clone()), now);
            }
            _ => unreachable!(),
        }
        embedded.handle_event(event, area, now);
        assert_eq!(
            embedded
                .events
                .iter()
                .map(std::mem::discriminant)
                .collect::<Vec<_>>(),
            standalone
                .drain_events()
                .iter()
                .map(std::mem::discriminant)
                .collect::<Vec<_>>()
        );
        embedded.events.clear();
        assert_eq!(
            embedded.pane.text(&embedded_id).unwrap(),
            standalone.text(&standalone_id).unwrap()
        );
        assert_eq!(embedded.pane.status(), standalone.status());
        assert_eq!(
            embedded.pane.source_cursor(&embedded_id).unwrap(),
            standalone.source_cursor(&standalone_id).unwrap()
        );
        assert_eq!(
            embedded.pane.render(56, 23, now).cells,
            standalone.render(56, 23, now).cells
        );
    }
    assert_eq!(embedded.pane.text(&embedded_id).unwrap(), "same λ text\n");
}

#[test]
fn shared_incremental_vector_matches_core_and_both_public_host_policies() {
    let directory = tempfile::tempdir().unwrap();
    let embedded_dir = directory.path().join("embedded");
    let standalone_dir = directory.path().join("standalone");
    let core_dir = directory.path().join("core");
    std::fs::create_dir(&embedded_dir).unwrap();
    std::fs::create_dir(&standalone_dir).unwrap();
    std::fs::create_dir(&core_dir).unwrap();
    let embedded_path = embedded_dir.join("note.md");
    let standalone_path = standalone_dir.join("note.md");
    let core_path = core_dir.join("note.md");
    std::fs::write(&embedded_path, incremental_host_vector::MARKDOWN).unwrap();
    std::fs::write(&standalone_path, incremental_host_vector::MARKDOWN).unwrap();
    std::fs::write(&core_path, incremental_host_vector::MARKDOWN).unwrap();
    let initial = Instant::now();
    let mut embedded = EmbeddedHost::new(directory.path(), vec![embedded_path.clone()], initial);
    let mut standalone = EmbeddedHost::with_options(
        directory.path(),
        vec![standalone_path.clone()],
        oom_edit::PaneOptions {
            command_policy: oom_edit::CommandPolicy::Standalone,
            ..oom_edit::PaneOptions::default()
        },
        initial,
    )
    .pane;
    let embedded_id = embedded.pane.active_tab().unwrap();
    let standalone_id = standalone.active_tab().unwrap();
    let mut core = EditorSession::open_existing(&core_path).unwrap();
    let (mut width, mut height) = (96, 24);
    let _ = core.render_layout(width - 4);
    embedded.pane.render(width, height, initial);
    standalone.render(width, height, initial);

    for (index, step) in incremental_host_vector::steps().into_iter().enumerate() {
        let now = initial + Duration::from_millis(index as u64 * 10);
        match step {
            incremental_host_vector::Step::Event(event) => {
                let Event::Key(key) = &event else {
                    panic!("shared edit vector must contain key events");
                };
                let translated = translate_key(key).unwrap();
                core.handle_key(translated);
                standalone.handle_input(PaneInput::Key(translated), now);
                embedded.handle_event(event, Rect::new(24, 0, width, height), now);
            }
            incremental_host_vector::Step::PaneSize(next_width, next_height) => {
                (width, height) = (next_width, next_height);
                let _ = core.render_layout(width - 4);
                standalone.resize(width, height, now);
                embedded.handle_event(
                    Event::Resize(width + 24, height + 1),
                    Rect::new(24, 0, width, height),
                    now,
                );
            }
        }
        assert_eq!(
            core.document(),
            standalone.text(&standalone_id).unwrap(),
            "core text step {index}"
        );
        assert_eq!(
            core.document(),
            embedded.pane.text(&embedded_id).unwrap(),
            "embedded text step {index}"
        );
        assert_eq!(
            core.mode(),
            standalone.status().unwrap().mode,
            "standalone mode step {index}"
        );
        assert_eq!(
            core.mode(),
            embedded.pane.status().unwrap().mode,
            "embedded mode step {index}"
        );
        assert_eq!(
            core.cursor(),
            standalone.source_cursor(&standalone_id).unwrap(),
            "standalone cursor step {index}"
        );
        assert_eq!(
            core.cursor(),
            embedded.pane.source_cursor(&embedded_id).unwrap(),
            "embedded cursor step {index}"
        );
        match index {
            1 | 4 => assert!(core.document().starts_with("X---")),
            3 => assert_eq!(core.document(), incremental_host_vector::MARKDOWN),
            8 | 12 => assert!(core.document().contains('Z')),
            10 => assert!(!core.document().contains('Z')),
            13 => assert_eq!(core.mode(), oom_edit_core::Mode::Select),
            16 => assert_eq!(core.mode(), oom_edit_core::Mode::Command),
            17 => assert_eq!(core.mode(), oom_edit_core::Mode::Normal),
            _ => {}
        }
        let current = core.render_layout(width - 4).clone();
        let mut fresh = EditorSession::from_text(&core.document());
        assert_eq!(
            current,
            *fresh.render_layout(width - 4),
            "semantic rows and atoms step {index}"
        );
        let standalone_frame = standalone.render(width, height, now);
        let embedded_frame = embedded.pane.render(width, height, now);
        if let Some((cell_index, (standalone_cell, embedded_cell))) = standalone_frame
            .cells
            .iter()
            .zip(embedded_frame.cells.iter())
            .enumerate()
            .find(|(_, (left, right))| left != right)
        {
            panic!(
                "owned cell step {index} at row {} column {}: {standalone_cell:?} != {embedded_cell:?}",
                cell_index / usize::from(width),
                cell_index % usize::from(width)
            );
        }
        assert_eq!(
            standalone_frame.cursor, embedded_frame.cursor,
            "owned cursor step {index}"
        );
    }

    for character in [':', 'w', '\n'] {
        let event = key(
            if character == '\n' {
                KeyCode::Enter
            } else {
                KeyCode::Char(character)
            },
            KeyModifiers::NONE,
        );
        let Event::Key(terminal_key) = &event else {
            unreachable!()
        };
        let translated = translate_key(terminal_key).unwrap();
        core.handle_key(translated);
        standalone.handle_input(PaneInput::Key(translated), initial);
        embedded.handle_event(event, Rect::new(24, 0, width, height), initial);
    }
    core.save(None, false).unwrap();
    for path in [&core_path, &standalone_path, &embedded_path] {
        assert_eq!(std::fs::read_to_string(path).unwrap(), core.document());
    }
    assert_eq!(standalone.tabs()[0].dirty, embedded.pane.tabs()[0].dirty);
    assert!(!standalone.tabs()[0].dirty);

    let replacement = "# Reloaded λ\n\nA [link][ref].\n\n[ref]: /fresh\n";
    for path in [&core_path, &standalone_path, &embedded_path] {
        std::fs::write(path, replacement).unwrap();
    }
    let oom_edit_core::DiskState::Modified { version } = core.disk_state() else {
        panic!("core must observe the external version");
    };
    core.reload_from_disk(&version).unwrap();
    standalone
        .notify_paths_changed(std::slice::from_ref(&standalone_path))
        .unwrap();
    let _ = standalone.tick(initial);
    embedded
        .pane
        .notify_paths_changed(std::slice::from_ref(&embedded_path))
        .unwrap();
    embedded.tick(initial);
    assert_eq!(core.document(), replacement);
    assert_eq!(standalone.text(&standalone_id).unwrap(), replacement);
    assert_eq!(embedded.pane.text(&embedded_id).unwrap(), replacement);
    assert_eq!(
        core.cursor(),
        standalone.source_cursor(&standalone_id).unwrap()
    );
    assert_eq!(
        core.cursor(),
        embedded.pane.source_cursor(&embedded_id).unwrap()
    );
    assert_eq!(
        core.render_layout(width - 4).clone(),
        *EditorSession::from_text(replacement).render_layout(width - 4)
    );
    assert_eq!(
        standalone.render(width, height, initial).cells,
        embedded.pane.render(width, height, initial).cells
    );

    let standalone_other = standalone.new_buffer(OpenOptions::default()).unwrap();
    let embedded_other = embedded.pane.new_buffer(OpenOptions::default()).unwrap();
    assert_eq!(standalone.tabs().len(), embedded.pane.tabs().len());
    standalone.focus_tab(&standalone_id).unwrap();
    embedded.pane.focus_tab(&embedded_id).unwrap();
    assert_eq!(standalone.active_tab(), Some(standalone_id));
    assert_eq!(embedded.pane.active_tab(), Some(embedded_id));
    assert_eq!(
        standalone.render(width, height, initial).cells,
        embedded.pane.render(width, height, initial).cells
    );
    standalone.focus_tab(&standalone_other).unwrap();
    embedded.pane.focus_tab(&embedded_other).unwrap();
    assert_eq!(
        standalone.render(width, height, initial).cells,
        embedded.pane.render(width, height, initial).cells
    );
}

#[test]
fn fr_111_unnamed_close_save_path_and_cancel() {
    for save in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let now = Instant::now();
        let mut host = EmbeddedHost::new(directory.path(), Vec::new(), now);
        let id = host.pane.new_buffer(OpenOptions::default()).unwrap();
        let area = Rect::new(24, 0, 96, 19);
        host.pane.render(area.width, area.height, now);
        for event in [
            key(KeyCode::Char('i'), KeyModifiers::NONE),
            Event::Paste("retained λ".into()),
            key(KeyCode::Esc, KeyModifiers::NONE),
            key(KeyCode::Char('q'), KeyModifiers::ALT),
        ] {
            host.handle_event(event, area, now);
        }
        assert!(host.pane.input_state().modal);
        host.handle_event(key(KeyCode::Char('y'), KeyModifiers::NONE), area, now);
        assert!(host.status_text().starts_with("Save path:"));
        assert!(host
            .events
            .iter()
            .any(|event| matches!(event, PaneEvent::SavePathRequested { tab, .. } if tab == &id)));
        assert!(std::fs::read_dir(directory.path())
            .unwrap()
            .next()
            .is_none());
        if save {
            let path = directory.path().join("chosen.md");
            host.handle_event(Event::Paste(path.to_str().unwrap().into()), area, now);
            host.handle_event(key(KeyCode::Enter, KeyModifiers::NONE), area, now);
            assert_eq!(std::fs::read_to_string(&path).unwrap(), "retained λ");
            assert!(host.quit);
            assert!(host.pane.tabs().is_empty());
        } else {
            host.handle_event(key(KeyCode::Esc, KeyModifiers::NONE), area, now);
            assert!(!host.quit);
            assert_eq!(host.pane.text(&id).unwrap(), "retained λ");
            assert!(host.pane.tabs()[0].dirty);
            assert!(host.pane.tabs()[0].path.is_none());
            assert!(std::fs::read_dir(directory.path())
                .unwrap()
                .next()
                .is_none());
            assert!(host
                .events
                .iter()
                .any(|event| matches!(event, PaneEvent::Cancelled { .. })));
        }
    }
}

#[test]
fn fr_111_split_mouse_origin_and_compact_frames() {
    use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
    let directory = tempfile::tempdir().unwrap();
    let now = Instant::now();
    let mut host = EmbeddedHost::new(directory.path(), Vec::new(), now);
    let id = host.pane.new_buffer(OpenOptions::default()).unwrap();
    for dimensions in [(120, 20), (20, 5), (1, 1), (0, 0)] {
        let mut terminal = Terminal::new(TestBackend::new(dimensions.0, dimensions.1)).unwrap();
        terminal.draw(|frame| host.draw(frame, now)).unwrap();
    }
    let mut terminal = Terminal::new(TestBackend::new(120, 20)).unwrap();
    host.handle_event(
        key(KeyCode::Char('i'), KeyModifiers::NONE),
        Rect::new(24, 0, 96, 19),
        now,
    );
    host.handle_event(
        Event::Paste("# Wide 界 e\u{301}\nsecond\n".into()),
        Rect::new(24, 0, 96, 19),
        now,
    );
    terminal.draw(|frame| host.draw(frame, now)).unwrap();
    let mouse = |column, row| {
        Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column,
            row,
            modifiers: KeyModifiers::NONE,
        })
    };
    host.handle_event(mouse(1, 1), Rect::new(24, 0, 96, 19), now);
    let text = host.pane.text(&id).unwrap();
    host.handle_event(
        key(KeyCode::Char('x'), KeyModifiers::NONE),
        Rect::new(24, 0, 96, 19),
        now,
    );
    assert_eq!(host.pane.text(&id).unwrap(), text);
    assert!(host.pane.render(96, 19, now).cursor.is_none());
    host.handle_event(mouse(29, 1), Rect::new(24, 0, 96, 19), now);
    assert!(host.pane.render(96, 19, now).cursor.is_some());
    terminal.draw(|frame| host.draw(frame, now)).unwrap();
    let local = host.pane.render(96, 19, now).cursor.unwrap();
    let cursor = terminal.get_cursor_position().unwrap();
    assert_eq!(cursor.x, 24 + local.column);
    assert_eq!(cursor.y, local.row);
}

#[test]
fn fr_111_failed_save_path_releases_host_prompt() {
    let directory = tempfile::tempdir().unwrap();
    let now = Instant::now();
    let area = Rect::new(24, 0, 96, 19);
    let mut host = EmbeddedHost::new(directory.path(), Vec::new(), now);
    let id = host.pane.new_buffer(OpenOptions::default()).unwrap();
    host.pane.render(area.width, area.height, now);
    for event in [
        key(KeyCode::Char('i'), KeyModifiers::NONE),
        Event::Paste("retained".into()),
        key(KeyCode::Esc, KeyModifiers::NONE),
        key(KeyCode::Char('q'), KeyModifiers::ALT),
        key(KeyCode::Char('y'), KeyModifiers::NONE),
        Event::Paste(directory.path().to_str().unwrap().into()),
        key(KeyCode::Enter, KeyModifiers::NONE),
    ] {
        host.handle_event(event, area, now);
    }
    assert!(host
        .events
        .iter()
        .any(|event| matches!(event, PaneEvent::Failed { .. })));
    assert!(!host.status_text().starts_with("Save path:"));
    assert!(!host.quit);
    assert_eq!(host.pane.text(&id).unwrap(), "retained");
    host.handle_event(key(KeyCode::Char('i'), KeyModifiers::NONE), area, now);
    host.handle_event(Event::Paste("still editable ".into()), area, now);
    assert!(host.pane.text(&id).unwrap().contains("still editable "));
    assert!(std::fs::read_dir(directory.path())
        .unwrap()
        .next()
        .is_none());
}
