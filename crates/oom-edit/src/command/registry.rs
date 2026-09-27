//! Static command/binding registry used by dispatch and every UI projection.

use crate::pane_metadata::{BindingExecution, BindingSequence, EditorBinding};
use oom_edit_core::{KeyCode, KeyCodeKind, KeyInput, Mode, Modifiers};

/// Stable metadata-row identity; it is never an executable payload.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum RegistryEntryId {
    /// Enter rendered character-wise selection.
    EnterCharacterSelect,
    /// Enter rendered line-wise selection.
    EnterLineSelect,
    /// Enter rendered rectangular selection.
    EnterBlockSelect,
    /// Cancel the active rendered selection.
    CancelSelect,
    /// Choose a register for the next selection operation.
    SelectRegister,
    /// Copy the selected Markdown source.
    SelectYank,
    /// Copy the selected rendered plain text.
    SelectYankPlainText,
    /// Delete the selected source.
    SelectDelete,
    /// Replace the selected source and enter Insert.
    SelectChange,
    /// Indent the selected source lines.
    SelectIndent,
    /// Outdent the selected source lines.
    SelectOutdent,
    /// Swap the active end and anchor of a selection.
    SelectSwapAnchor,
    /// Start editor search.
    Search,
    /// Enter the ex command prompt.
    CommandMode,
    /// Show the editor command palette.
    Help,
    /// Save the active document.
    Save,
    /// Request closing the active tab.
    Quit,
    /// Cycle the active appearance's compatible themes.
    CycleTheme,
    /// Insert default front matter.
    DefaultFrontMatter,
    /// Show suggestions for the current spelling diagnostic.
    SpellSuggest,
    /// Add the current word to the personal dictionary.
    SpellAdd,
    /// Toggle spelling for this session.
    SpellToggle,
    /// Show document diagnostics.
    Trouble,
    /// Move to the next spelling diagnostic.
    SpellNext,
    /// Move to the previous spelling diagnostic.
    SpellPrevious,
    /// Set spelling through the ex grammar.
    SpellSet,
    /// Select a tab through a Space-digit chord.
    SpaceDigitTab,
    /// Activate the next tab.
    NextTab,
    /// Activate the preceding tab.
    PrevTab,
    /// Activate a tab through the counted core grammar.
    JumpToTab,
    /// Open a tab through ex input.
    TabNew,
    /// Close a tab through ex input.
    TabClose,
    /// Request host-wide quitting through ex input.
    QuitAll,
    /// Reference for rendered navigation motions.
    RenderedMotion,
    /// Reference for rendered line and document boundaries.
    RenderedEnds,
    /// Reference for rendered structural navigation.
    RenderedTargets,
    /// Reference for repeating search in either direction.
    RepeatSearch,
    /// Reference for entering source Insert mode.
    SourceInsert,
    /// Reference for leaving Insert mode.
    ExitInsert,
    /// Reference for undo and redo.
    UndoRedo,
    /// Reference for saving through ex input.
    Write,
    /// Reference for writing a copy through ex input.
    WriteCopy,
    /// Reference for guarded ex quit.
    ExQuit,
    /// Reference for explicitly forced ex quit.
    ExForceQuit,
    /// Reference for saving before ex quit.
    WriteQuit,
    /// Reference for ex exit with a save when needed.
    Exit,
    /// Reference for replacing the active document from a path.
    EditPath,
    /// Reference for explicitly reloading a document.
    Reload,
    /// Reference for reloading all tabs.
    ReloadAll,
    /// Reference for saving under a new file identity.
    SaveAs,
    /// Reference for ex source-line navigation.
    LineNumber,
    /// Reference for substitution on the current line.
    Substitute,
    /// Reference for substitution across the document.
    SubstituteAll,
    /// Reference for clearing search highlighting.
    ClearSearch,
    /// Reference for opening editor help through ex input.
    ExHelp,
}

/// Payload-free actions owned and executed by the TUI.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum AppCommand {
    Help,
    Save,
    Quit,
    CycleTheme,
    DefaultFrontMatter,
    SpellSuggest,
    SpellAdd,
    SpellToggle,
    Trouble,
}

/// Binding ownership and finite dispatch role.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum BindingRole {
    AppChord {
        continuation: char,
        command: AppCommand,
    },
    AppSpaceDigit,
    CoreKey {
        display: &'static str,
    },
    CoreEx {
        display: &'static str,
        prefill: Option<&'static str>,
    },
    ReferenceOnly,
}

/// Literal core keys or a variable grammar; never parsed from display labels.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum SequenceSpec {
    Keys(&'static [KeyInput]),
    Ex(&'static str),
    Pattern {
        prefix: &'static [KeyInput],
        notation: &'static str,
    },
}

/// A reference presentation attached to its single canonical binding row.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct ReferenceSpec {
    pub order: usize,
    pub keys: &'static str,
    pub desc: &'static str,
    pub conformance_id: &'static str,
    pub prefill: Option<&'static str>,
}

pub const fn character_key(ch: char) -> KeyInput {
    KeyInput {
        code: KeyCode {
            kind: KeyCodeKind::Char(ch),
        },
        mods: Modifiers {
            ctrl: false,
            alt: false,
            shift: false,
        },
    }
}
const fn control_key(ch: char) -> KeyInput {
    KeyInput {
        mods: Modifiers {
            ctrl: true,
            alt: false,
            shift: false,
        },
        ..character_key(ch)
    }
}
const fn special_key(kind: KeyCodeKind) -> KeyInput {
    KeyInput {
        code: KeyCode { kind },
        ..character_key(' ')
    }
}
use SequenceSpec::{Keys, Pattern};

/// An additional executable key for the same registry command row.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum BindingAlias {
    Direct {
        key: char,
        contexts: Contexts,
    },
    Space {
        continuation: char,
        contexts: Contexts,
    },
}

/// The set of UI contexts in which a registry row is visible.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct Contexts(u8);

impl Contexts {
    pub const NORMAL: Self = Self(1 << 0);
    pub const INSERT: Self = Self(1 << 1);
    pub const SELECT: Self = Self(1 << 2);
    pub const COMMAND: Self = Self(1 << 3);
    #[cfg(test)]
    pub const OVERLAY: Self = Self(1 << 4);
    #[cfg(test)]
    pub const ALL: Self = Self((1 << 5) - 1);

    pub const fn or(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 != 0
    }

    #[cfg(test)]
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    #[cfg(test)]
    pub(crate) fn each_bit() -> impl Iterator<Item = Self> {
        (0..5).map(|bit| Self(1 << bit))
    }
}

/// One immutable registry row.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct CommandSpec {
    pub id: RegistryEntryId,
    pub name: &'static str,
    pub desc: &'static str,
    pub contexts: Contexts,
    pub binding: BindingRole,
    pub aliases: &'static [BindingAlias],
    pub sequences: &'static [SequenceSpec],
    pub references: &'static [ReferenceSpec],
    /// Core conformance requirement backing a non-executable reference row.
    pub conformance_id: Option<&'static str>,
    /// Purpose-specific order for the compact hint bar only.
    pub quick_bar_order: Option<i16>,
    /// Compact, context-specific wording for the quick hint bar.
    pub quick_label: Option<&'static str>,
}

const RENDERED: Contexts = Contexts::NORMAL.or(Contexts::SELECT);

macro_rules! row {
    ($id:ident, $name:literal, $desc:literal, $contexts:expr, $binding:expr, $quick:expr, $quick_label:expr) => {
        CommandSpec {
            id: RegistryEntryId::$id,
            name: $name,
            desc: $desc,
            contexts: $contexts,
            binding: $binding,
            aliases: &[],
            sequences: &[],
            references: &[],
            conformance_id: None,
            quick_bar_order: $quick,
            quick_label: $quick_label,
        }
    };
    ($id:ident, $name:literal, $desc:literal, $contexts:expr, $binding:expr, $quick:expr) => {
        row!($id, $name, $desc, $contexts, $binding, $quick, None)
    };
}

macro_rules! conformance_row {
    ($id:ident, $name:literal, $desc:literal, $contexts:expr, $binding:expr, $conformance:literal) => {
        CommandSpec {
            id: RegistryEntryId::$id,
            name: $name,
            desc: $desc,
            contexts: $contexts,
            binding: $binding,
            aliases: &[],
            sequences: &[],
            references: &[],
            conformance_id: Some($conformance),
            quick_bar_order: None,
            quick_label: None,
        }
    };
}

macro_rules! core_row {
    ($sequences:expr, $($row:tt)*) => {
        CommandSpec { sequences: $sequences, ..row!($($row)*) }
    };
}
macro_rules! core_conformance_row {
    ($sequences:expr, $($row:tt)*) => {
        CommandSpec { sequences: $sequences, ..conformance_row!($($row)*) }
    };
}
macro_rules! reference_row {
    ($id:ident, $name:literal, $contexts:expr, $sequences:expr, $(($order:literal, $keys:literal, $desc:literal, $conformance:literal, $prefill:expr)),+ $(,)?) => {
        CommandSpec {
            id: RegistryEntryId::$id, name: $name,
            desc: &[$(ReferenceSpec { order: $order, keys: $keys, desc: $desc, conformance_id: $conformance, prefill: $prefill }),+][0].desc,
            contexts: $contexts, binding: BindingRole::ReferenceOnly,
            sequences: $sequences,
            references: &[$(ReferenceSpec { order: $order, keys: $keys, desc: $desc, conformance_id: $conformance, prefill: $prefill }),+],
            aliases: &[], conformance_id: None, quick_bar_order: None, quick_label: None,
        }
    };
}

/// Sole fixed-binding and UI-order table.
pub static COMMANDS: &[CommandSpec] = &[
    core_row!(
        &[Keys(&[character_key('v')])],
        EnterCharacterSelect,
        "select-character",
        "character-wise selection",
        Contexts::NORMAL,
        BindingRole::CoreKey { display: "v" },
        Some(0),
        Some("select")
    ),
    core_row!(
        &[Keys(&[character_key('V')])],
        EnterLineSelect,
        "select-line",
        "line-wise selection",
        Contexts::NORMAL,
        BindingRole::CoreKey { display: "V" },
        None
    ),
    core_row!(
        &[Keys(&[control_key('v')])],
        EnterBlockSelect,
        "select-block",
        "block-wise selection",
        Contexts::NORMAL,
        BindingRole::CoreKey { display: "Ctrl-V" },
        None
    ),
    core_row!(
        &[
            Keys(&[special_key(KeyCodeKind::Esc)]),
            Keys(&[control_key('c')])
        ],
        CancelSelect,
        "cancel-select",
        "cancel selection",
        Contexts::SELECT,
        BindingRole::CoreKey {
            display: "Esc / Ctrl-C"
        },
        Some(1)
    ),
    core_row!(
        &[Pattern {
            prefix: &[character_key('"')],
            notation: "{register}"
        }],
        SelectRegister,
        "select-register",
        "select register",
        Contexts::SELECT,
        BindingRole::CoreKey {
            display: "\"{register}"
        },
        None
    ),
    core_row!(
        &[Keys(&[character_key('y')])],
        SelectYank,
        "select-yank",
        "yank (Markdown by default) to clipboard",
        Contexts::SELECT,
        BindingRole::CoreKey { display: "y" },
        Some(2)
    ),
    core_row!(
        &[Keys(&[character_key('Y')])],
        SelectYankPlainText,
        "select-yank-plain-text",
        "yank and send plain text to clipboard",
        Contexts::SELECT,
        BindingRole::CoreKey { display: "Y" },
        None
    ),
    core_row!(
        &[Keys(&[character_key('d')]), Keys(&[character_key('x')])],
        SelectDelete,
        "select-delete",
        "delete selection",
        Contexts::SELECT,
        BindingRole::CoreKey { display: "d / x" },
        Some(3)
    ),
    core_row!(
        &[Keys(&[character_key('c')])],
        SelectChange,
        "select-change",
        "change selection",
        Contexts::SELECT,
        BindingRole::CoreKey { display: "c" },
        None
    ),
    core_row!(
        &[Keys(&[character_key('>')])],
        SelectIndent,
        "select-indent",
        "indent selection",
        Contexts::SELECT,
        BindingRole::CoreKey { display: ">" },
        None
    ),
    core_row!(
        &[Keys(&[character_key('<')])],
        SelectOutdent,
        "select-outdent",
        "outdent selection",
        Contexts::SELECT,
        BindingRole::CoreKey { display: "<" },
        None
    ),
    core_row!(
        &[Keys(&[character_key('o')])],
        SelectSwapAnchor,
        "select-swap-anchor",
        "swap selection endpoint",
        Contexts::SELECT,
        BindingRole::CoreKey { display: "o" },
        None
    ),
    CommandSpec {
        id: RegistryEntryId::Search,
        name: "search",
        desc: "search rendered text",
        contexts: Contexts::NORMAL,
        binding: BindingRole::CoreKey { display: "/" },
        aliases: &[],
        sequences: &[Keys(&[character_key('/')])],
        references: &[ReferenceSpec {
            order: 3,
            keys: "/pattern⏎",
            desc: "Search rendered text forward.",
            conformance_id: "R-N4",
            prefill: None,
        }],
        conformance_id: None,
        quick_bar_order: Some(5),
        quick_label: Some("search"),
    },
    CommandSpec {
        id: RegistryEntryId::CommandMode,
        name: "command",
        desc: "enter Command mode",
        contexts: Contexts::NORMAL,
        binding: BindingRole::CoreKey { display: ":" },
        aliases: &[],
        sequences: &[Keys(&[character_key(':')])],
        references: &[],
        conformance_id: None,
        quick_bar_order: Some(6),
        quick_label: Some("command"),
    },
    CommandSpec {
        id: RegistryEntryId::Help,
        name: "help",
        desc: "command palette",
        contexts: RENDERED,
        binding: BindingRole::AppChord {
            continuation: 'h',
            command: AppCommand::Help,
        },
        aliases: &[
            BindingAlias::Direct {
                key: '?',
                contexts: RENDERED,
            },
            BindingAlias::Space {
                continuation: '?',
                contexts: RENDERED,
            },
        ],
        sequences: &[],
        references: &[],
        conformance_id: None,
        quick_bar_order: Some(10),
        quick_label: Some("commands"),
    },
    row!(
        Save,
        "save",
        "save",
        RENDERED,
        BindingRole::AppChord {
            continuation: 'w',
            command: AppCommand::Save
        },
        Some(20)
    ),
    row!(
        Quit,
        "quit",
        "quit",
        RENDERED,
        BindingRole::AppChord {
            continuation: 'q',
            command: AppCommand::Quit
        },
        Some(30)
    ),
    row!(
        CycleTheme,
        "cycle-theme",
        "cycle theme",
        RENDERED,
        BindingRole::AppChord {
            continuation: 't',
            command: AppCommand::CycleTheme
        },
        None
    ),
    row!(
        DefaultFrontMatter,
        "default-front-matter",
        "insert default front matter",
        Contexts::NORMAL,
        BindingRole::AppChord {
            continuation: 'm',
            command: AppCommand::DefaultFrontMatter
        },
        None
    ),
    row!(
        SpellSuggest,
        "spell-suggest",
        "spelling suggestions",
        RENDERED,
        BindingRole::AppChord {
            continuation: 's',
            command: AppCommand::SpellSuggest
        },
        None
    ),
    row!(
        SpellAdd,
        "spell-add",
        "add word to personal dictionary",
        RENDERED,
        BindingRole::AppChord {
            continuation: 'a',
            command: AppCommand::SpellAdd
        },
        None
    ),
    row!(
        SpellToggle,
        "spell-toggle",
        "toggle spelling",
        RENDERED,
        BindingRole::AppChord {
            continuation: 'z',
            command: AppCommand::SpellToggle
        },
        None
    ),
    row!(
        Trouble,
        "trouble",
        "document diagnostics",
        RENDERED,
        BindingRole::AppChord {
            continuation: 'd',
            command: AppCommand::Trouble
        },
        None
    ),
    core_conformance_row!(
        &[Keys(&[character_key(']'), character_key('s')])],
        SpellNext,
        "spell-next",
        "next spelling diagnostic",
        RENDERED,
        BindingRole::CoreKey { display: "]s" },
        "SP-2:next-previous-wrap"
    ),
    core_conformance_row!(
        &[Keys(&[character_key('['), character_key('s')])],
        SpellPrevious,
        "spell-previous",
        "previous spelling diagnostic",
        RENDERED,
        BindingRole::CoreKey { display: "[s" },
        "SP-2:next-previous-wrap"
    ),
    core_conformance_row!(
        &[
            SequenceSpec::Ex("set spell"),
            SequenceSpec::Ex("set nospell")
        ],
        SpellSet,
        "spell-set",
        "enable or disable spelling",
        Contexts::COMMAND,
        BindingRole::CoreEx {
            display: ":set spell / :set nospell",
            prefill: None,
        },
        "SP-1:set-toggle"
    ),
    row!(
        SpaceDigitTab,
        "space-tab",
        "jump to tab",
        RENDERED,
        BindingRole::AppSpaceDigit,
        None
    ),
    core_row!(
        &[Keys(&[character_key('g'), character_key('t')])],
        NextTab,
        "next-tab",
        "next tab",
        RENDERED,
        BindingRole::CoreKey { display: "g t" },
        None
    ),
    core_row!(
        &[Keys(&[character_key('g'), character_key('T')])],
        PrevTab,
        "prev-tab",
        "previous tab",
        RENDERED,
        BindingRole::CoreKey { display: "g T" },
        None
    ),
    core_row!(
        &[Pattern {
            prefix: &[],
            notation: "{count} g t"
        }],
        JumpToTab,
        "jump-to-tab",
        "jump to numbered tab",
        RENDERED,
        BindingRole::CoreKey {
            display: "{count} g t"
        },
        None
    ),
    row!(
        TabNew,
        "tab-new",
        "open path in new tab",
        RENDERED,
        BindingRole::CoreEx {
            display: ":tabnew {path}",
            prefill: Some("tabnew "),
        },
        None
    ),
    row!(
        TabClose,
        "tab-close",
        "close tab",
        RENDERED,
        BindingRole::CoreEx {
            display: ":tabclose",
            prefill: Some("tabclose"),
        },
        None
    ),
    row!(
        QuitAll,
        "quit-all",
        "quit all tabs",
        Contexts::COMMAND,
        BindingRole::CoreEx {
            display: ":qa",
            prefill: Some("qa")
        },
        None
    ),
    reference_row!(
        RenderedMotion,
        "rendered-motion",
        RENDERED,
        &[
            Keys(&[character_key('j')]),
            Keys(&[character_key('k')]),
            Keys(&[special_key(KeyCodeKind::Up)]),
            Keys(&[special_key(KeyCodeKind::Down)])
        ],
        (0, "j/k, ↑/↓", "Move by rendered row.", "R-N1", None)
    ),
    reference_row!(
        RenderedEnds,
        "rendered-ends",
        RENDERED,
        &[
            Keys(&[character_key('g'), character_key('g')]),
            Keys(&[character_key('G')])
        ],
        (
            1,
            "gg / G",
            "Jump to the first / last rendered row.",
            "R-N2",
            None
        )
    ),
    reference_row!(
        RenderedTargets,
        "rendered-targets",
        RENDERED,
        &[
            Keys(&[special_key(KeyCodeKind::Tab)]),
            Keys(&[special_key(KeyCodeKind::BackTab)])
        ],
        (
            2,
            "Tab / S-Tab",
            "Move between rendered jump targets.",
            "R-N3",
            None
        )
    ),
    reference_row!(
        RepeatSearch,
        "repeat-search",
        RENDERED,
        &[Keys(&[character_key('n')]), Keys(&[character_key('N')])],
        (4, "n / N", "Repeat the rendered search.", "R-N4", None)
    ),
    reference_row!(
        SourceInsert,
        "source-insert",
        Contexts::NORMAL,
        &[
            Keys(&[character_key('i')]),
            Keys(&[character_key('a')]),
            Keys(&[character_key('I')]),
            Keys(&[character_key('A')]),
            Keys(&[character_key('o')]),
            Keys(&[character_key('O')])
        ],
        (5, "i/a/I/A/o/O", "Enter source Insert mode.", "R-I1", None)
    ),
    reference_row!(
        ExitInsert,
        "exit-insert",
        Contexts::INSERT,
        &[Keys(&[special_key(KeyCodeKind::Esc)])],
        (
            6,
            "Esc",
            "Return from Insert to rendered Normal.",
            "R-I2",
            None
        )
    ),
    reference_row!(
        UndoRedo,
        "undo-redo",
        Contexts::NORMAL,
        &[Keys(&[character_key('u')]), Keys(&[control_key('r')])],
        (7, "u / <C-r>", "Undo / redo.", "R-E1", None)
    ),
    reference_row!(
        Write,
        "write",
        Contexts::COMMAND,
        &[SequenceSpec::Ex("w")],
        (8, ":w", "Save (atomic).", "V-X1", Some("w"))
    ),
    reference_row!(
        WriteCopy,
        "write-copy",
        Contexts::COMMAND,
        &[SequenceSpec::Ex("w {path}")],
        (
            9,
            ":w {path}",
            "Save a copy to path without retargeting buffer.",
            "V-X1",
            Some("w ")
        )
    ),
    reference_row!(
        ExQuit,
        "ex-quit",
        Contexts::COMMAND,
        &[SequenceSpec::Ex("q")],
        (10, ":q", "Quit; refuses if dirty.", "V-X2", Some("q"))
    ),
    reference_row!(
        ExForceQuit,
        "ex-force-quit",
        Contexts::COMMAND,
        &[SequenceSpec::Ex("q!")],
        (11, ":q!", "Quit; discards changes.", "V-X2", Some("q!"))
    ),
    reference_row!(
        WriteQuit,
        "write-quit",
        Contexts::COMMAND,
        &[SequenceSpec::Ex("wq")],
        (12, ":wq", "Save then quit.", "V-X3", Some("wq"))
    ),
    reference_row!(
        Exit,
        "exit",
        Contexts::COMMAND,
        &[SequenceSpec::Ex("x")],
        (13, ":x", "Save then quit (if changed).", "V-X3", Some("x"))
    ),
    reference_row!(
        EditPath,
        "edit-path",
        Contexts::COMMAND,
        &[SequenceSpec::Ex("e {path}")],
        (
            14,
            ":e {path}",
            "Open file; refuses if dirty without !.",
            "V-X4",
            Some("e ")
        )
    ),
    reference_row!(
        Reload,
        "reload",
        Contexts::COMMAND,
        &[SequenceSpec::Ex("e!"), SequenceSpec::Ex("reload")],
        (
            15,
            ":e!",
            "Reload current file from disk.",
            "V-X4",
            Some("e!")
        ),
        (
            16,
            ":reload",
            "Reload current file from disk.",
            "V-X4",
            Some("reload")
        )
    ),
    reference_row!(
        ReloadAll,
        "reload-all",
        Contexts::COMMAND,
        &[SequenceSpec::Ex("reload-all")],
        (
            17,
            ":reload-all",
            "Reload every open tab from disk.",
            "V-X4",
            Some("reload-all")
        )
    ),
    reference_row!(
        SaveAs,
        "save-as",
        Contexts::COMMAND,
        &[SequenceSpec::Ex("saveas {path}")],
        (
            18,
            ":saveas {path}",
            "Save to path and retarget buffer.",
            "V-X5",
            Some("saveas ")
        )
    ),
    reference_row!(
        LineNumber,
        "line-number",
        Contexts::COMMAND,
        &[SequenceSpec::Ex("{number}")],
        (19, ":{number}", "Jump to line.", "V-X6", None)
    ),
    reference_row!(
        Substitute,
        "substitute",
        Contexts::COMMAND,
        &[SequenceSpec::Ex("s/pat/rep/[g]")],
        (
            20,
            ":s/pat/rep/",
            "Substitute on current line.",
            "V-X7",
            Some("s/")
        ),
        (
            21,
            ":s/pat/rep/g",
            "Substitute all on current line.",
            "V-X7",
            Some("s/")
        )
    ),
    reference_row!(
        SubstituteAll,
        "substitute-all",
        Contexts::COMMAND,
        &[SequenceSpec::Ex("%s/pat/rep/g")],
        (
            22,
            ":%s/pat/rep/g",
            "Substitute all in document.",
            "V-X7",
            Some("%s/")
        )
    ),
    reference_row!(
        ClearSearch,
        "clear-search",
        Contexts::COMMAND,
        &[SequenceSpec::Ex("noh")],
        (
            23,
            ":noh",
            "Clear search-match highlighting.",
            "V-X8",
            Some("noh")
        )
    ),
    reference_row!(
        ExHelp,
        "ex-help",
        Contexts::COMMAND,
        &[SequenceSpec::Ex("help")],
        (
            24,
            ":help",
            "Open the command palette.",
            "V-X8",
            Some("help")
        )
    ),
];

pub fn primary_commands() -> impl Iterator<Item = &'static CommandSpec> {
    COMMANDS
        .iter()
        .filter(|spec| spec.binding != BindingRole::ReferenceOnly)
}

pub fn reference_presentations() -> Vec<&'static ReferenceSpec> {
    let mut references: Vec<_> = COMMANDS.iter().flat_map(|spec| spec.references).collect();
    references.sort_by_key(|reference| reference.order);
    references
}

pub fn app_command_id(command: AppCommand) -> RegistryEntryId {
    COMMANDS
        .iter()
        .find_map(|spec| match spec.binding {
            BindingRole::AppChord { command: owner, .. } if owner == command => Some(spec.id),
            _ => None,
        })
        .expect("every App command has one registry row")
}

pub fn export_bindings() -> Vec<EditorBinding> {
    COMMANDS
        .iter()
        .map(|spec| {
            let mut sequences: Vec<_> = spec
                .sequences
                .iter()
                .map(|sequence| match sequence {
                    Keys(keys) => BindingSequence::Keys(keys.to_vec()),
                    SequenceSpec::Ex(command) => BindingSequence::Ex((*command).to_owned()),
                    Pattern { prefix, notation } => BindingSequence::Pattern {
                        prefix: prefix.to_vec(),
                        notation: (*notation).to_owned(),
                    },
                })
                .collect();
            let execution = match spec.binding {
                BindingRole::AppChord { continuation, .. } => {
                    sequences.push(BindingSequence::Keys(vec![
                        character_key(' '),
                        character_key(continuation),
                    ]));
                    for alias in spec.aliases {
                        sequences.push(BindingSequence::Keys(match alias {
                            BindingAlias::Direct { key, .. } => vec![character_key(*key)],
                            BindingAlias::Space { continuation, .. } => {
                                vec![character_key(' '), character_key(*continuation)]
                            }
                        }));
                    }
                    BindingExecution::ExecutableApp
                }
                BindingRole::AppSpaceDigit => {
                    sequences.extend(('1'..='9').map(|digit| {
                        BindingSequence::Keys(vec![character_key(' '), character_key(digit)])
                    }));
                    BindingExecution::ExecutableApp
                }
                BindingRole::CoreEx { display, .. } => {
                    if sequences.is_empty() {
                        sequences.push(BindingSequence::Ex(
                            display.strip_prefix(':').unwrap_or(display).to_owned(),
                        ));
                    }
                    BindingExecution::ExecutableCore
                }
                BindingRole::CoreKey { .. } => BindingExecution::ExecutableCore,
                BindingRole::ReferenceOnly => BindingExecution::ReferenceOnly,
            };
            EditorBinding {
                id: spec.id,
                name: spec.name.to_owned(),
                description: spec.desc.to_owned(),
                modes: if matches!(spec.binding, BindingRole::CoreEx { .. }) {
                    vec![Mode::Command]
                } else {
                    [
                        (Contexts::NORMAL, Mode::Normal),
                        (Contexts::INSERT, Mode::Insert),
                        (Contexts::SELECT, Mode::Select),
                        (Contexts::COMMAND, Mode::Command),
                    ]
                    .into_iter()
                    .filter_map(|(context, mode)| spec.contexts.contains(context).then_some(mode))
                    .collect()
                },
                sequences,
                execution,
            }
        })
        .collect()
}

pub fn rendered_binding(spec: &CommandSpec) -> String {
    rendered_binding_for(spec, spec.contexts)
}

pub fn rendered_binding_for(spec: &CommandSpec, ctx: Contexts) -> String {
    let primary = match spec.binding {
        BindingRole::AppChord { continuation, .. } => {
            let mut keys = vec![continuation.to_string()];
            keys.extend(spec.aliases.iter().filter_map(|alias| match alias {
                BindingAlias::Space {
                    continuation,
                    contexts,
                } if contexts.contains(ctx) => Some(continuation.to_string()),
                _ => None,
            }));
            format!("Space {}", keys.join("/"))
        }
        BindingRole::AppSpaceDigit => "Space 1-9".to_string(),
        BindingRole::CoreKey { display } | BindingRole::CoreEx { display, .. } => {
            display.to_string()
        }
        BindingRole::ReferenceOnly => spec.references[0].keys.to_owned(),
    };
    let mut bindings = Vec::with_capacity(spec.aliases.len() + 1);
    for alias in spec.aliases {
        if let BindingAlias::Direct { key, contexts } = alias {
            if contexts.contains(ctx) {
                bindings.push(key.to_string());
            }
        }
    }
    bindings.push(primary);
    bindings.join(" / ")
}

pub fn commands_for(ctx: Contexts) -> Vec<&'static CommandSpec> {
    let mut rows: Vec<_> = COMMANDS
        .iter()
        .filter(|spec| spec.contexts.contains(ctx) && spec.quick_bar_order.is_some())
        .collect();
    rows.sort_by_key(|spec| spec.quick_bar_order);
    rows
}

pub fn app_chord(ctx: Contexts, continuation: char) -> Option<AppCommand> {
    COMMANDS.iter().find_map(|spec| {
        if !spec.contexts.contains(ctx) {
            return None;
        }
        let primary = match spec.binding {
            BindingRole::AppChord {
                continuation: key,
                command,
            } if key == continuation => Some(command),
            _ => None,
        };
        primary.or_else(|| {
            spec.aliases
                .iter()
                .find_map(|alias| match (alias, spec.binding) {
                    (
                        BindingAlias::Space {
                            continuation: key,
                            contexts,
                        },
                        BindingRole::AppChord { command, .. },
                    ) if *key == continuation && contexts.contains(ctx) => Some(command),
                    _ => None,
                })
        })
    })
}

pub fn direct_command(ctx: Contexts, key: char) -> Option<AppCommand> {
    COMMANDS.iter().find_map(|spec| {
        spec.aliases
            .iter()
            .find_map(|alias| match (alias, spec.binding) {
                (
                    BindingAlias::Direct {
                        key: alias_key,
                        contexts,
                    },
                    BindingRole::AppChord { command, .. },
                ) if *alias_key == key && contexts.contains(ctx) => Some(command),
                _ => None,
            })
    })
}

pub fn space_continuations(ctx: Contexts) -> Vec<(char, &'static CommandSpec)> {
    let mut continuations = Vec::new();
    for spec in COMMANDS {
        if let BindingRole::AppChord { continuation, .. } = spec.binding {
            if spec.contexts.contains(ctx) {
                continuations.push((continuation, spec));
            }
            for alias in spec.aliases {
                if let BindingAlias::Space {
                    continuation,
                    contexts,
                } = alias
                {
                    if contexts.contains(ctx) {
                        continuations.push((*continuation, spec));
                    }
                }
            }
        }
    }
    continuations
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn command_registry_is_exhaustive_unique_and_contextual() {
        let mut ids = HashSet::new();
        let mut names = HashSet::new();
        for spec in COMMANDS {
            use RegistryEntryId::*;
            match spec.id {
                EnterCharacterSelect | EnterLineSelect | EnterBlockSelect | CancelSelect
                | SelectRegister | SelectYank | SelectYankPlainText | SelectDelete
                | SelectChange | SelectIndent | SelectOutdent | SelectSwapAnchor | Search
                | CommandMode | Help | Save | Quit | CycleTheme | DefaultFrontMatter
                | SpellSuggest | SpellAdd | SpellToggle | Trouble | SpellNext | SpellPrevious
                | SpellSet | SpaceDigitTab | NextTab | PrevTab | JumpToTab | TabNew | TabClose
                | QuitAll | RenderedMotion | RenderedEnds | RenderedTargets | RepeatSearch
                | SourceInsert | ExitInsert | UndoRedo | Write | WriteCopy | ExQuit
                | ExForceQuit | WriteQuit | Exit | EditPath | Reload | ReloadAll | SaveAs
                | LineNumber | Substitute | SubstituteAll | ClearSearch | ExHelp => {}
            }
            assert!(ids.insert(spec.id));
            assert!(names.insert(spec.name));
            assert!(!spec.contexts.is_empty());
            assert!(!rendered_binding(spec).is_empty());
        }
        assert_eq!(COMMANDS.len(), 55);
        assert_eq!(primary_commands().count(), 33);
    }

    #[test]
    fn structured_registry_sequences_modes_roles_and_reference_order_are_unique() {
        let bindings = export_bindings();
        let mut concrete = HashSet::new();
        for (spec, binding) in COMMANDS.iter().zip(&bindings) {
            for alias in spec.aliases {
                let contexts = match alias {
                    BindingAlias::Direct { contexts, .. }
                    | BindingAlias::Space { contexts, .. } => contexts,
                };
                assert_eq!(
                    *contexts, spec.contexts,
                    "exported sequence modes must cover every alias exactly"
                );
            }
            assert!(!binding.sequences.is_empty(), "{:?}", spec.id);
            assert!(!binding.modes.is_empty());
            for (index, sequence) in binding.sequences.iter().enumerate() {
                assert!(
                    !binding.sequences[..index].contains(sequence),
                    "duplicate {:?} sequence",
                    spec.id
                );
                if let BindingSequence::Keys(keys) = sequence {
                    assert!(!keys.is_empty());
                    for mode in &binding.modes {
                        assert!(
                            concrete.insert((format!("{mode:?}"), keys.clone())),
                            "duplicate concrete {mode:?} {keys:?}"
                        );
                    }
                }
            }
            let expected_role = match spec.binding {
                BindingRole::AppChord { .. } | BindingRole::AppSpaceDigit => {
                    BindingExecution::ExecutableApp
                }
                BindingRole::CoreKey { .. } | BindingRole::CoreEx { .. } => {
                    BindingExecution::ExecutableCore
                }
                BindingRole::ReferenceOnly => {
                    assert!(spec.quick_bar_order.is_none());
                    assert!(spec.aliases.is_empty());
                    BindingExecution::ReferenceOnly
                }
            };
            assert_eq!(binding.execution, expected_role);
        }
        let refs = reference_presentations();
        assert_eq!(
            refs.iter()
                .map(|reference| reference.order)
                .collect::<Vec<_>>(),
            (0..25).collect::<Vec<_>>()
        );
        assert_eq!(
            COMMANDS.iter().filter(|spec| spec.name == "search").count(),
            1
        );
        assert_eq!(
            COMMANDS
                .iter()
                .find(|spec| spec.id == RegistryEntryId::Reload)
                .unwrap()
                .references
                .len(),
            2
        );
        assert_eq!(
            COMMANDS
                .iter()
                .find(|spec| spec.id == RegistryEntryId::Substitute)
                .unwrap()
                .references
                .len(),
            2
        );
        let palette = crate::overlay::PaletteState::new(Contexts::ALL);
        assert_eq!(palette.build_rows(Contexts::ALL).len(), 58);
    }

    #[test]
    fn app_aliases_are_unique_contextual_and_keep_one_command_row() {
        let mut direct = HashSet::new();
        let mut space = HashSet::new();
        for spec in COMMANDS {
            for ctx in Contexts::each_bit() {
                if let BindingRole::AppChord { continuation, .. } = spec.binding {
                    if spec.contexts.contains(ctx) {
                        assert!(space.insert((ctx.0, continuation)));
                    }
                }
                for alias in spec.aliases {
                    match alias {
                        BindingAlias::Direct { key, contexts } if contexts.contains(ctx) => {
                            assert!(spec.contexts.contains(ctx));
                            assert!(matches!(spec.binding, BindingRole::AppChord { .. }));
                            assert!(direct.insert((ctx.0, *key)));
                        }
                        BindingAlias::Space {
                            continuation,
                            contexts,
                        } if contexts.contains(ctx) => {
                            assert!(spec.contexts.contains(ctx));
                            assert!(matches!(spec.binding, BindingRole::AppChord { .. }));
                            assert!(space.insert((ctx.0, *continuation)));
                        }
                        _ => {}
                    }
                }
            }
        }
        let help = COMMANDS
            .iter()
            .find(|spec| spec.id == RegistryEntryId::Help)
            .unwrap();
        assert_eq!(
            help.aliases,
            &[
                BindingAlias::Direct {
                    key: '?',
                    contexts: RENDERED
                },
                BindingAlias::Space {
                    continuation: '?',
                    contexts: RENDERED
                },
            ]
        );
        assert_eq!(rendered_binding(help), "? / Space h/?");
        assert_eq!(
            rendered_binding_for(help, Contexts::SELECT),
            "? / Space h/?"
        );
        assert_eq!(
            direct_command(Contexts::NORMAL, '?'),
            Some(AppCommand::Help)
        );
        assert_eq!(
            direct_command(Contexts::SELECT, '?'),
            Some(AppCommand::Help)
        );
        assert_eq!(app_chord(Contexts::NORMAL, '?'), Some(AppCommand::Help));
        assert_eq!(app_chord(Contexts::SELECT, '?'), Some(AppCommand::Help));
    }

    #[test]
    fn phase_eight_trouble_row_is_complete_and_app_owned() {
        let trouble = COMMANDS
            .iter()
            .find(|spec| spec.name == "trouble")
            .expect("Space d Trouble row must land with its executable behavior");

        assert_eq!(trouble.desc, "document diagnostics");
        assert_eq!(trouble.contexts, RENDERED);
        assert_eq!(rendered_binding(trouble), "Space d");
        assert_eq!(
            trouble.binding,
            BindingRole::AppChord {
                continuation: 'd',
                command: AppCommand::Trouble,
            }
        );
        assert_eq!(trouble.conformance_id, None);
        assert_eq!(trouble.quick_bar_order, None);
    }

    #[test]
    fn phase_seven_spell_rows_have_exact_identity_and_ownership() {
        let expected = [
            (
                RegistryEntryId::SpellSuggest,
                "spell-suggest",
                BindingRole::AppChord {
                    continuation: 's',
                    command: AppCommand::SpellSuggest,
                },
                None,
            ),
            (
                RegistryEntryId::SpellAdd,
                "spell-add",
                BindingRole::AppChord {
                    continuation: 'a',
                    command: AppCommand::SpellAdd,
                },
                None,
            ),
            (
                RegistryEntryId::SpellToggle,
                "spell-toggle",
                BindingRole::AppChord {
                    continuation: 'z',
                    command: AppCommand::SpellToggle,
                },
                None,
            ),
            (
                RegistryEntryId::SpellNext,
                "spell-next",
                BindingRole::CoreKey { display: "]s" },
                Some("SP-2:next-previous-wrap"),
            ),
            (
                RegistryEntryId::SpellPrevious,
                "spell-previous",
                BindingRole::CoreKey { display: "[s" },
                Some("SP-2:next-previous-wrap"),
            ),
            (
                RegistryEntryId::SpellSet,
                "spell-set",
                BindingRole::CoreEx {
                    display: ":set spell / :set nospell",
                    prefill: None,
                },
                Some("SP-1:set-toggle"),
            ),
        ];

        let actual = COMMANDS
            .iter()
            .filter(|spec| {
                matches!(
                    spec.id,
                    RegistryEntryId::SpellSuggest
                        | RegistryEntryId::SpellAdd
                        | RegistryEntryId::SpellToggle
                        | RegistryEntryId::SpellNext
                        | RegistryEntryId::SpellPrevious
                        | RegistryEntryId::SpellSet
                )
            })
            .map(|spec| (spec.id, spec.name, spec.binding, spec.conformance_id))
            .collect::<Vec<_>>();

        assert_eq!(actual, expected);
        assert!(actual.iter().all(|(_, _, binding, conformance_id)| {
            matches!(binding, BindingRole::AppChord { .. })
                || conformance_id.is_some_and(|id| id.starts_with("SP-"))
        }));
    }

    #[test]
    fn registry_exactly_matches_fixed_binding_contract() {
        let expected = [
            (
                RegistryEntryId::EnterCharacterSelect,
                "select-character",
                "character-wise selection",
                Contexts::NORMAL,
                BindingRole::CoreKey { display: "v" },
                Some(0),
            ),
            (
                RegistryEntryId::EnterLineSelect,
                "select-line",
                "line-wise selection",
                Contexts::NORMAL,
                BindingRole::CoreKey { display: "V" },
                None,
            ),
            (
                RegistryEntryId::EnterBlockSelect,
                "select-block",
                "block-wise selection",
                Contexts::NORMAL,
                BindingRole::CoreKey { display: "Ctrl-V" },
                None,
            ),
            (
                RegistryEntryId::CancelSelect,
                "cancel-select",
                "cancel selection",
                Contexts::SELECT,
                BindingRole::CoreKey {
                    display: "Esc / Ctrl-C",
                },
                Some(1),
            ),
            (
                RegistryEntryId::SelectRegister,
                "select-register",
                "select register",
                Contexts::SELECT,
                BindingRole::CoreKey {
                    display: "\"{register}",
                },
                None,
            ),
            (
                RegistryEntryId::SelectYank,
                "select-yank",
                "yank (Markdown by default) to clipboard",
                Contexts::SELECT,
                BindingRole::CoreKey { display: "y" },
                Some(2),
            ),
            (
                RegistryEntryId::SelectYankPlainText,
                "select-yank-plain-text",
                "yank and send plain text to clipboard",
                Contexts::SELECT,
                BindingRole::CoreKey { display: "Y" },
                None,
            ),
            (
                RegistryEntryId::SelectDelete,
                "select-delete",
                "delete selection",
                Contexts::SELECT,
                BindingRole::CoreKey { display: "d / x" },
                Some(3),
            ),
            (
                RegistryEntryId::SelectChange,
                "select-change",
                "change selection",
                Contexts::SELECT,
                BindingRole::CoreKey { display: "c" },
                None,
            ),
            (
                RegistryEntryId::SelectIndent,
                "select-indent",
                "indent selection",
                Contexts::SELECT,
                BindingRole::CoreKey { display: ">" },
                None,
            ),
            (
                RegistryEntryId::SelectOutdent,
                "select-outdent",
                "outdent selection",
                Contexts::SELECT,
                BindingRole::CoreKey { display: "<" },
                None,
            ),
            (
                RegistryEntryId::SelectSwapAnchor,
                "select-swap-anchor",
                "swap selection endpoint",
                Contexts::SELECT,
                BindingRole::CoreKey { display: "o" },
                None,
            ),
            (
                RegistryEntryId::Search,
                "search",
                "search rendered text",
                Contexts::NORMAL,
                BindingRole::CoreKey { display: "/" },
                Some(5),
            ),
            (
                RegistryEntryId::CommandMode,
                "command",
                "enter Command mode",
                Contexts::NORMAL,
                BindingRole::CoreKey { display: ":" },
                Some(6),
            ),
            (
                RegistryEntryId::Help,
                "help",
                "command palette",
                RENDERED,
                BindingRole::AppChord {
                    continuation: 'h',
                    command: AppCommand::Help,
                },
                Some(10),
            ),
            (
                RegistryEntryId::Save,
                "save",
                "save",
                RENDERED,
                BindingRole::AppChord {
                    continuation: 'w',
                    command: AppCommand::Save,
                },
                Some(20),
            ),
            (
                RegistryEntryId::Quit,
                "quit",
                "quit",
                RENDERED,
                BindingRole::AppChord {
                    continuation: 'q',
                    command: AppCommand::Quit,
                },
                Some(30),
            ),
            (
                RegistryEntryId::CycleTheme,
                "cycle-theme",
                "cycle theme",
                RENDERED,
                BindingRole::AppChord {
                    continuation: 't',
                    command: AppCommand::CycleTheme,
                },
                None,
            ),
            (
                RegistryEntryId::DefaultFrontMatter,
                "default-front-matter",
                "insert default front matter",
                Contexts::NORMAL,
                BindingRole::AppChord {
                    continuation: 'm',
                    command: AppCommand::DefaultFrontMatter,
                },
                None,
            ),
            (
                RegistryEntryId::SpellSuggest,
                "spell-suggest",
                "spelling suggestions",
                RENDERED,
                BindingRole::AppChord {
                    continuation: 's',
                    command: AppCommand::SpellSuggest,
                },
                None,
            ),
            (
                RegistryEntryId::SpellAdd,
                "spell-add",
                "add word to personal dictionary",
                RENDERED,
                BindingRole::AppChord {
                    continuation: 'a',
                    command: AppCommand::SpellAdd,
                },
                None,
            ),
            (
                RegistryEntryId::SpellToggle,
                "spell-toggle",
                "toggle spelling",
                RENDERED,
                BindingRole::AppChord {
                    continuation: 'z',
                    command: AppCommand::SpellToggle,
                },
                None,
            ),
            (
                RegistryEntryId::Trouble,
                "trouble",
                "document diagnostics",
                RENDERED,
                BindingRole::AppChord {
                    continuation: 'd',
                    command: AppCommand::Trouble,
                },
                None,
            ),
            (
                RegistryEntryId::SpellNext,
                "spell-next",
                "next spelling diagnostic",
                RENDERED,
                BindingRole::CoreKey { display: "]s" },
                None,
            ),
            (
                RegistryEntryId::SpellPrevious,
                "spell-previous",
                "previous spelling diagnostic",
                RENDERED,
                BindingRole::CoreKey { display: "[s" },
                None,
            ),
            (
                RegistryEntryId::SpellSet,
                "spell-set",
                "enable or disable spelling",
                Contexts::COMMAND,
                BindingRole::CoreEx {
                    display: ":set spell / :set nospell",
                    prefill: None,
                },
                None,
            ),
            (
                RegistryEntryId::SpaceDigitTab,
                "space-tab",
                "jump to tab",
                RENDERED,
                BindingRole::AppSpaceDigit,
                None,
            ),
            (
                RegistryEntryId::NextTab,
                "next-tab",
                "next tab",
                RENDERED,
                BindingRole::CoreKey { display: "g t" },
                None,
            ),
            (
                RegistryEntryId::PrevTab,
                "prev-tab",
                "previous tab",
                RENDERED,
                BindingRole::CoreKey { display: "g T" },
                None,
            ),
            (
                RegistryEntryId::JumpToTab,
                "jump-to-tab",
                "jump to numbered tab",
                RENDERED,
                BindingRole::CoreKey {
                    display: "{count} g t",
                },
                None,
            ),
            (
                RegistryEntryId::TabNew,
                "tab-new",
                "open path in new tab",
                RENDERED,
                BindingRole::CoreEx {
                    display: ":tabnew {path}",
                    prefill: Some("tabnew "),
                },
                None,
            ),
            (
                RegistryEntryId::TabClose,
                "tab-close",
                "close tab",
                RENDERED,
                BindingRole::CoreEx {
                    display: ":tabclose",
                    prefill: Some("tabclose"),
                },
                None,
            ),
            (
                RegistryEntryId::QuitAll,
                "quit-all",
                "quit all tabs",
                Contexts::COMMAND,
                BindingRole::CoreEx {
                    display: ":qa",
                    prefill: Some("qa"),
                },
                None,
            ),
        ];
        let actual = primary_commands()
            .map(|spec| {
                (
                    spec.id,
                    spec.name,
                    spec.desc,
                    spec.contexts,
                    spec.binding,
                    spec.quick_bar_order,
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(actual, expected);

        for (index, left) in COMMANDS.iter().enumerate() {
            for right in &COMMANDS[index + 1..] {
                let contexts_overlap = left.contexts.0 & right.contexts.0 != 0;
                if !contexts_overlap {
                    continue;
                }
                if let (
                    BindingRole::AppChord {
                        continuation: left_key,
                        ..
                    },
                    BindingRole::AppChord {
                        continuation: right_key,
                        ..
                    },
                ) = (left.binding, right.binding)
                {
                    assert_ne!(
                        left_key, right_key,
                        "overlapping App chord for {} and {}",
                        left.name, right.name
                    );
                }
                if let (Some(left_order), Some(right_order)) =
                    (left.quick_bar_order, right.quick_bar_order)
                {
                    assert_ne!(
                        left_order, right_order,
                        "overlapping quick-bar order for {} and {}",
                        left.name, right.name
                    );
                }
            }
        }
    }

    #[test]
    fn core_binding_descriptors_never_dispatch_app_commands() {
        for spec in COMMANDS {
            if matches!(
                spec.binding,
                BindingRole::CoreKey { .. } | BindingRole::CoreEx { .. }
            ) {
                assert!(!matches!(spec.binding, BindingRole::AppChord { .. }));
            }
        }
    }

    #[test]
    fn core_ex_prefills_are_safe_single_command_inputs() {
        for spec in COMMANDS {
            if let BindingRole::CoreEx { display, prefill } = spec.binding {
                assert!(display.starts_with(':'));
                if let Some(text) = prefill {
                    assert!(!text.is_empty());
                    assert!(!text.starts_with(':'));
                    assert!(!text.chars().any(char::is_control));
                    assert!(!text.contains('{') && !text.contains('}'));
                    assert!(
                        !display.contains(" / :"),
                        "combined commands need one choice"
                    );
                }
            }
        }
    }

    #[test]
    fn registry_binding_rendering_is_total() {
        for spec in COMMANDS {
            let rendered = rendered_binding(spec);
            assert!(
                !rendered.trim().is_empty(),
                "missing binding for {:?}",
                spec.id
            );
            assert!(!rendered.contains("no binding"));
        }
    }
}
