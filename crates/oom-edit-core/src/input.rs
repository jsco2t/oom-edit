//! Terminal-neutral input values shared by every core subsystem.

/// One key event and its modifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct KeyInput {
    /// Key identity.
    pub code: KeyCode,
    /// Modifier bits.
    pub mods: Modifiers,
}

/// A terminal-neutral key code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct KeyCode {
    /// Key identity.
    pub kind: KeyCodeKind,
}

/// Supported character and special keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KeyCodeKind {
    /// Intentionally ignored input.
    Noop,
    /// Printable character.
    Char(char),
    /// Enter / Return.
    Enter,
    /// Escape.
    Esc,
    /// Backspace.
    Backspace,
    /// Tab.
    Tab,
    /// Shift-Tab.
    BackTab,
    /// Up arrow.
    Up,
    /// Down arrow.
    Down,
    /// Left arrow.
    Left,
    /// Right arrow.
    Right,
    /// Home.
    Home,
    /// End.
    End,
    /// Page Up.
    PageUp,
    /// Page Down.
    PageDown,
    /// Delete.
    Delete,
    /// Function key F1-F24.
    F(u8),
}

/// Keyboard modifier bits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Modifiers {
    /// Ctrl is pressed.
    pub ctrl: bool,
    /// Alt/Option is pressed.
    pub alt: bool,
    /// Shift is pressed.
    pub shift: bool,
}
