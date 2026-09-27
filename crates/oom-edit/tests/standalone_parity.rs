//! End-to-end public facade traces, independent of App/session internals.

use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;
use std::time::Instant;

use oom_edit::{
    AllowAllFileAccess, ClipboardError, ClipboardSink, CommandPolicy, Config, ConfigPersistError,
    DisplayMode, EditorPane, KeyCode, KeyCodeKind, KeyInput, Mode, Modifiers, PaneEvent, PaneInit,
    PaneInput, PaneMouse, PaneMouseKind, PaneOptions, PaneServices, RequestId, TabId, ThemeCatalog,
    ThemePersistenceSink, ThemeSelection, ThemeSlot, Tier,
};

const DOCUMENT: &str = "---\ntitle: Heading\n---\n\n# Heading\n\nalpha beta gamma λ e\u{301}\n\n| A | B |\n| - | - |\n| repeat | `code` |\n\n```rust\nfn main() {}\n```\n";

struct Clipboard(Rc<RefCell<Vec<String>>>);
impl ClipboardSink for Clipboard {
    fn copy(&mut self, text: &str) -> Result<(), ClipboardError> {
        self.0.borrow_mut().push(text.into());
        Ok(())
    }
}
struct NoPersistence;
impl ThemePersistenceSink for NoPersistence {
    fn persist_theme(&mut self, _: ThemeSlot, _: &str) -> Result<(), ConfigPersistError> {
        Ok(())
    }
}

struct PublicHost {
    pane: EditorPane,
    tabs: Vec<TabId>,
    requests: Vec<RequestId>,
    clipboard: Rc<RefCell<Vec<String>>>,
}

fn ordinal<T: Clone + PartialEq>(known: &mut Vec<T>, id: &T) -> usize {
    if let Some(index) = known.iter().position(|previous| previous == id) {
        index
    } else {
        known.push(id.clone());
        known.len() - 1
    }
}

impl PublicHost {
    fn new(base: &Path, policy: CommandPolicy, now: Instant) -> Self {
        let clipboard = Rc::new(RefCell::new(Vec::new()));
        let construction = EditorPane::construct(PaneInit {
            config: Config::default(),
            theme_catalog: ThemeCatalog::builtins(),
            theme_selection: ThemeSelection::new(None, DisplayMode::Dark, Tier::TrueColor),
            services: PaneServices {
                clipboard_sink: Box::new(Clipboard(Rc::clone(&clipboard))),
                theme_sink: Box::new(NoPersistence),
                file_access_policy: Box::new(AllowAllFileAccess),
                config_base_directory: base.into(),
                personal_dictionary_path: base.join("personal.txt"),
                working_directory: base.into(),
            },
            options: PaneOptions {
                command_policy: policy,
                inline_hints: true,
                ..PaneOptions::default()
            },
            initial_paths: vec![base.join("note.md")],
            now,
        });
        assert_eq!(construction.report.paths.len(), 1);
        assert!(construction.report.paths[0].is_ok());
        assert!(construction.report.warnings.is_empty());
        Self {
            pane: construction.pane,
            tabs: Vec::new(),
            requests: Vec::new(),
            clipboard,
        }
    }

    fn events(&mut self) -> Vec<String> {
        self.pane
            .drain_events()
            .into_iter()
            .map(|event| match event {
                PaneEvent::Opened { request, tab, path } => format!(
                    "opened {} {} {path:?}",
                    ordinal(&mut self.requests, &request),
                    ordinal(&mut self.tabs, &tab)
                ),
                PaneEvent::ActiveTabChanged { request, tab } => format!(
                    "active {} {}",
                    ordinal(&mut self.requests, &request),
                    ordinal(&mut self.tabs, &tab)
                ),
                PaneEvent::Completed { request } => {
                    format!("completed {}", ordinal(&mut self.requests, &request))
                }
                PaneEvent::ModeChanged { tab, mode } => {
                    format!("mode {} {mode:?}", ordinal(&mut self.tabs, &tab))
                }
                PaneEvent::ThemeChanged { theme } => format!("theme {theme:?}"),
                PaneEvent::Closed { request, tab, path } => format!(
                    "closed {} {} {path:?}",
                    ordinal(&mut self.requests, &request),
                    ordinal(&mut self.tabs, &tab)
                ),
                PaneEvent::AllClosed { request } => {
                    format!("all-closed {}", ordinal(&mut self.requests, &request))
                }
                PaneEvent::QuitAllRequested { request, force } => {
                    format!("quit {} {force}", ordinal(&mut self.requests, &request))
                }
                PaneEvent::Failed {
                    request,
                    tab,
                    error,
                } => format!(
                    "failed {} {:?} {error:?}",
                    ordinal(&mut self.requests, &request),
                    tab.map(|tab| ordinal(&mut self.tabs, &tab))
                ),
                unexpected => panic!("unexpected trace event: {unexpected:?}"),
            })
            .collect()
    }
}

fn special(kind: KeyCodeKind) -> PaneInput {
    PaneInput::Key(KeyInput {
        code: KeyCode { kind },
        mods: Modifiers::default(),
    })
}
fn keys(text: &str) -> impl Iterator<Item = PaneInput> + '_ {
    text.chars()
        .map(|character| special(KeyCodeKind::Char(character)))
}

fn apply_pair(a: &mut PublicHost, b: &mut PublicHost, input: PaneInput, now: Instant) {
    assert_eq!(
        a.pane.handle_input(input.clone(), now),
        b.pane.handle_input(input, now)
    );
    assert_eq!(a.events(), b.events());
    let a_tab = a.pane.active_tab().unwrap();
    let b_tab = b.pane.active_tab().unwrap();
    assert_eq!(a.pane.text(&a_tab).unwrap(), b.pane.text(&b_tab).unwrap());
    assert_eq!(
        a.pane.source_cursor(&a_tab).unwrap(),
        b.pane.source_cursor(&b_tab).unwrap()
    );
    assert!(a
        .pane
        .render(80, 24, now)
        .visually_equals(&b.pane.render(80, 24, now)));
}

#[test]
fn public_hosts_match_spell_idle_suggestions_trouble_and_mouse() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(
        directory.path().join("note.md"),
        "helo world\n\nsecond paragraph\n",
    )
    .unwrap();
    let initial = Instant::now();
    let now = initial + std::time::Duration::from_secs(5);
    let mut a = PublicHost::new(directory.path(), CommandPolicy::Standalone, initial);
    let mut b = PublicHost::new(directory.path(), CommandPolicy::Embedded, initial);
    assert_eq!(a.events(), b.events());
    assert!(a.pane.tick(now).idle_due);
    assert!(b.pane.tick(now).idle_due);
    let mut quiescent = false;
    for _ in 0..10_000 {
        let a_unit = a.pane.idle_unit(4096);
        assert_eq!(a_unit, b.pane.idle_unit(4096));
        if !a_unit.worked {
            quiescent = true;
            break;
        }
    }
    assert!(
        quiescent,
        "bounded public idle work must finish the real dictionary and document scan"
    );
    assert!(!a.pane.tick(now).idle_due);
    assert!(!b.pane.tick(now).idle_due);
    for input in keys("gg s") {
        apply_pair(&mut a, &mut b, input, now);
    }
    let frame = a.pane.render(80, 24, now);
    let text = frame
        .cells
        .iter()
        .map(|cell| cell.symbol.as_str())
        .collect::<String>();
    assert!(
        text.contains("Spelling: helo"),
        "a real diagnostic must open suggestions"
    );
    assert!(a.pane.input_state().modal);
    apply_pair(&mut a, &mut b, special(KeyCodeKind::Enter), now);
    let a_tab = a.pane.active_tab().unwrap();
    let corrected = a.pane.text(&a_tab).unwrap();
    assert_ne!(
        corrected, "helo world\n\nsecond paragraph\n",
        "Enter applies the selected real suggestion"
    );
    apply_pair(&mut a, &mut b, special(KeyCodeKind::Char('u')), now);
    assert_eq!(
        a.pane.text(&a_tab).unwrap(),
        "helo world\n\nsecond paragraph\n"
    );
    for input in keys(" d") {
        apply_pair(&mut a, &mut b, input, now);
    }
    let frame = a.pane.render(80, 24, now);
    assert!(frame
        .cells
        .iter()
        .map(|cell| cell.symbol.as_str())
        .collect::<String>()
        .contains("Trouble ("));
    assert!(a.pane.input_state().modal);
    apply_pair(&mut a, &mut b, special(KeyCodeKind::Esc), now);
    for (kind, column, row) in [
        (PaneMouseKind::LeftDown, 6, 0),
        (PaneMouseKind::LeftDrag, 9, 0),
        (PaneMouseKind::LeftUp, 9, 0),
        (PaneMouseKind::ScrollDown, 7, 1),
        (PaneMouseKind::ScrollUp, 7, 1),
    ] {
        apply_pair(
            &mut a,
            &mut b,
            PaneInput::Mouse(PaneMouse {
                kind,
                column,
                row,
                modifiers: Modifiers::default(),
            }),
            now,
        );
    }
    assert_eq!(
        std::fs::read_to_string(directory.path().join("note.md")).unwrap(),
        "helo world\n\nsecond paragraph\n"
    );
}

#[test]
fn fr_110_baseline_trace_replay() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(directory.path().join("note.md"), DOCUMENT).unwrap();
    let now = Instant::now();
    let mut standalone = PublicHost::new(directory.path(), CommandPolicy::Standalone, now);
    let mut embedded = PublicHost::new(directory.path(), CommandPolicy::Embedded, now);
    assert_eq!(standalone.events(), embedded.events());
    let mut inputs = keys("ggiXYZ").collect::<Vec<_>>();
    inputs.push(special(KeyCodeKind::Esc));
    inputs.extend(keys("u"));
    let undo_index = inputs.len() - 1;
    inputs.push(PaneInput::Key(KeyInput {
        code: KeyCode {
            kind: KeyCodeKind::Char('r'),
        },
        mods: Modifiers {
            ctrl: true,
            ..Modifiers::default()
        },
    }));
    inputs.extend(keys("ggvly"));
    inputs.push(special(KeyCodeKind::Esc));
    inputs.extend(keys("ggVjy"));
    inputs.push(special(KeyCodeKind::Esc));
    inputs.extend(keys("gg\"+yy/Heading"));
    inputs.push(special(KeyCodeKind::Enter));
    inputs.extend(keys("nN:set nowrap"));
    inputs.push(special(KeyCodeKind::Enter));
    inputs.extend(keys(" ?"));
    inputs.push(special(KeyCodeKind::Esc));
    inputs.extend(keys(":tabnew"));
    inputs.push(special(KeyCodeKind::Enter));
    inputs.extend(keys(":tabprev"));
    inputs.push(special(KeyCodeKind::Enter));
    inputs.extend(keys("i"));
    inputs.push(PaneInput::Paste("界 pasted λ".into()));
    inputs.push(special(KeyCodeKind::Esc));

    let mut modes = std::collections::HashSet::new();
    for (index, input) in inputs.into_iter().enumerate() {
        assert_eq!(
            standalone.pane.handle_input(input.clone(), now),
            embedded.pane.handle_input(input, now)
        );
        assert_eq!(
            standalone.events(),
            embedded.events(),
            "ordered events after step {index}"
        );
        let a = standalone.pane.active_tab().unwrap();
        let b = embedded.pane.active_tab().unwrap();
        assert_eq!(
            standalone.pane.text(&a).unwrap(),
            embedded.pane.text(&b).unwrap()
        );
        assert_eq!(
            standalone.pane.source_cursor(&a).unwrap(),
            embedded.pane.source_cursor(&b).unwrap()
        );
        let a_tabs = standalone.pane.tabs();
        let b_tabs = embedded.pane.tabs();
        assert_eq!(a_tabs.len(), b_tabs.len());
        for (a, b) in a_tabs.iter().zip(b_tabs.iter()) {
            assert_eq!(
                (&a.path, a.dirty, a.active, a.mode, a.mru_rank),
                (&b.path, b.dirty, b.active, b.mode, b.mru_rank)
            );
            modes.insert(format!("{:?}", a.mode));
        }
        let frame = standalone.pane.render(80, 24, now);
        assert!(
            frame.visually_equals(&embedded.pane.render(80, 24, now)),
            "owned frame after step {index}"
        );
        if index == undo_index {
            assert_eq!(
                standalone.pane.text(&a).unwrap(),
                DOCUMENT,
                "one Insert session undoes atomically"
            );
        }
    }
    assert_eq!(
        modes,
        [Mode::Normal, Mode::Insert, Mode::Select, Mode::Command]
            .map(|mode| format!("{mode:?}"))
            .into_iter()
            .collect()
    );
    assert_eq!(*standalone.clipboard.borrow(), *embedded.clipboard.borrow());
    assert!(
        !standalone.clipboard.borrow().is_empty(),
        "system-register copy must reach the host sink"
    );
    assert_eq!(
        std::fs::read_to_string(directory.path().join("note.md")).unwrap(),
        DOCUMENT,
        "unsaved traces never write the source"
    );
}
