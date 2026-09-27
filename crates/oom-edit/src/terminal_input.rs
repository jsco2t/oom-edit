//! Translate terminal reports once at the standalone host boundary.

use crate::pane::{PaneMouse, PaneMouseKind};
use crossterm::event::{
    KeyCode as CrosstermKeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use oom_edit_core::{KeyCode, KeyCodeKind, KeyInput, Modifiers};

/// Translate a crossterm mouse event once at the standalone boundary.
pub(crate) fn crossterm_mouse_to_pane(mouse: MouseEvent) -> PaneMouse {
    let kind = match mouse.kind {
        MouseEventKind::Down(MouseButton::Left) => PaneMouseKind::LeftDown,
        MouseEventKind::Drag(MouseButton::Left) => PaneMouseKind::LeftDrag,
        MouseEventKind::Up(MouseButton::Left) => PaneMouseKind::LeftUp,
        MouseEventKind::ScrollUp => PaneMouseKind::ScrollUp,
        MouseEventKind::ScrollDown => PaneMouseKind::ScrollDown,
        MouseEventKind::Moved => PaneMouseKind::Moved,
        _ => PaneMouseKind::Other,
    };
    PaneMouse {
        kind,
        column: mouse.column,
        row: mouse.row,
        modifiers: Modifiers {
            ctrl: mouse.modifiers.contains(KeyModifiers::CONTROL),
            alt: mouse.modifiers.contains(KeyModifiers::ALT),
            shift: mouse.modifiers.contains(KeyModifiers::SHIFT),
        },
    }
}

/// Translate a crossterm [`KeyEvent`] to a core [`KeyInput`].
pub(crate) fn crossterm_key_to_core(key: &KeyEvent) -> KeyInput {
    let code = match key.code {
        CrosstermKeyCode::Char(c) => KeyCode {
            kind: KeyCodeKind::Char(c),
        },
        CrosstermKeyCode::Backspace => KeyCode {
            kind: KeyCodeKind::Backspace,
        },
        CrosstermKeyCode::Enter => KeyCode {
            kind: KeyCodeKind::Enter,
        },
        CrosstermKeyCode::Left => KeyCode {
            kind: KeyCodeKind::Left,
        },
        CrosstermKeyCode::Right => KeyCode {
            kind: KeyCodeKind::Right,
        },
        CrosstermKeyCode::Up => KeyCode {
            kind: KeyCodeKind::Up,
        },
        CrosstermKeyCode::Down => KeyCode {
            kind: KeyCodeKind::Down,
        },
        CrosstermKeyCode::Tab => KeyCode {
            kind: KeyCodeKind::Tab,
        },
        CrosstermKeyCode::BackTab => KeyCode {
            kind: KeyCodeKind::BackTab,
        },
        CrosstermKeyCode::Home => KeyCode {
            kind: KeyCodeKind::Home,
        },
        CrosstermKeyCode::End => KeyCode {
            kind: KeyCodeKind::End,
        },
        CrosstermKeyCode::PageUp => KeyCode {
            kind: KeyCodeKind::PageUp,
        },
        CrosstermKeyCode::PageDown => KeyCode {
            kind: KeyCodeKind::PageDown,
        },
        CrosstermKeyCode::Delete => KeyCode {
            kind: KeyCodeKind::Delete,
        },
        CrosstermKeyCode::Insert => KeyCode {
            kind: KeyCodeKind::Noop,
        },
        CrosstermKeyCode::F(n) => KeyCode {
            kind: KeyCodeKind::F(n),
        },
        CrosstermKeyCode::Null => KeyCode {
            kind: KeyCodeKind::Noop,
        },
        CrosstermKeyCode::Esc => KeyCode {
            kind: KeyCodeKind::Esc,
        },
        CrosstermKeyCode::CapsLock => KeyCode {
            kind: KeyCodeKind::Noop,
        },
        CrosstermKeyCode::Menu => KeyCode {
            kind: KeyCodeKind::Noop,
        },
        CrosstermKeyCode::ScrollLock => KeyCode {
            kind: KeyCodeKind::Noop,
        },
        CrosstermKeyCode::Pause => KeyCode {
            kind: KeyCodeKind::Noop,
        },
        CrosstermKeyCode::NumLock => KeyCode {
            kind: KeyCodeKind::Noop,
        },
        CrosstermKeyCode::PrintScreen => KeyCode {
            kind: KeyCodeKind::Noop,
        },
        CrosstermKeyCode::KeypadBegin => KeyCode {
            kind: KeyCodeKind::Noop,
        },
        CrosstermKeyCode::Media(_) => KeyCode {
            kind: KeyCodeKind::Noop,
        },
        CrosstermKeyCode::Modifier(_) => KeyCode {
            kind: KeyCodeKind::Noop,
        },
    };

    let mut mods = Modifiers::default();
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        mods.ctrl = true;
    }
    if key.modifiers.contains(KeyModifiers::ALT) {
        mods.alt = true;
    }
    if key.modifiers.contains(KeyModifiers::SHIFT) {
        mods.shift = true;
    }

    // Legacy terminals cannot distinguish these control chords from the
    // corresponding special keys. Keep enhanced terminals behaviorally equal.
    if mods.ctrl && !mods.alt && !mods.shift {
        let alias = match code.kind {
            KeyCodeKind::Char('[') => Some(KeyCodeKind::Esc),
            KeyCodeKind::Char('i') => Some(KeyCodeKind::Tab),
            KeyCodeKind::Char('m') => Some(KeyCodeKind::Enter),
            KeyCodeKind::Char('h') => Some(KeyCodeKind::Backspace),
            _ => None,
        };
        if let Some(kind) = alias {
            return KeyInput {
                code: KeyCode { kind },
                mods: Modifiers::default(),
            };
        }
    }
    if code.kind == KeyCodeKind::Tab && mods.shift && !mods.ctrl && !mods.alt {
        return KeyInput {
            code: KeyCode {
                kind: KeyCodeKind::BackTab,
            },
            mods: Modifiers::default(),
        };
    }
    KeyInput { code, mods }
}
