use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};

pub const MARKDOWN: &str = "---\ntitle: Same\n---\n\n# Heading\n\nSame λ e\u{301} 👩\u{200d}💻 repeated same words.\n\n| A | B |\n| - | - |\n| same | `code` |\n\n```rust\nfn main() {}\n```\n\nTail words.\n";

#[derive(Clone, Debug)]
pub enum Step {
    Event(Event),
    PaneSize(u16, u16),
}

fn key(code: KeyCode) -> Step {
    Step::Event(Event::Key(KeyEvent::new(code, KeyModifiers::NONE)))
}

/// The same retained-layout edits and mode transitions run through each host.
pub fn steps() -> Vec<Step> {
    vec![
        key(KeyCode::Char('i')),
        key(KeyCode::Char('X')),
        key(KeyCode::Esc),
        key(KeyCode::Char('u')),
        Step::Event(Event::Key(KeyEvent::new(
            KeyCode::Char('r'),
            KeyModifiers::CONTROL,
        ))),
        Step::PaneSize(44, 12),
        key(KeyCode::Char('G')),
        key(KeyCode::Char('i')),
        key(KeyCode::Char('Z')),
        key(KeyCode::Esc),
        key(KeyCode::Char('u')),
        Step::PaneSize(96, 24),
        Step::Event(Event::Key(KeyEvent::new(
            KeyCode::Char('r'),
            KeyModifiers::CONTROL,
        ))),
        key(KeyCode::Char('V')),
        key(KeyCode::Char('j')),
        key(KeyCode::Esc),
        key(KeyCode::Char(':')),
        key(KeyCode::Esc),
    ]
}
