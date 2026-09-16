//! Static command/binding registry used by dispatch and every UI projection.

/// Stable metadata-row identity; it is never an executable payload.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum RegistryEntryId {
    EnterCharacterSelect,
    EnterLineSelect,
    EnterBlockSelect,
    CancelSelect,
    SelectRegister,
    SelectYank,
    SelectYankPlainText,
    SelectDelete,
    SelectChange,
    SelectIndent,
    SelectOutdent,
    SelectSwapAnchor,
    Search,
    CommandMode,
    Help,
    Save,
    Quit,
    CycleTheme,
    SpellSuggest,
    SpellAdd,
    SpellToggle,
    Trouble,
    SpellNext,
    SpellPrevious,
    SpellSet,
    SpaceDigitTab,
    NextTab,
    PrevTab,
    JumpToTab,
    TabNew,
    TabClose,
    QuitAll,
}

/// Payload-free actions owned and executed by the TUI.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum AppCommand {
    Help,
    Save,
    Quit,
    CycleTheme,
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
    },
}

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
            conformance_id: Some($conformance),
            quick_bar_order: None,
            quick_label: None,
        }
    };
}

/// Sole fixed-binding and UI-order table.
pub static COMMANDS: &[CommandSpec] = &[
    row!(
        EnterCharacterSelect,
        "select-character",
        "character-wise selection",
        Contexts::NORMAL,
        BindingRole::CoreKey { display: "v" },
        Some(0),
        Some("select")
    ),
    row!(
        EnterLineSelect,
        "select-line",
        "line-wise selection",
        Contexts::NORMAL,
        BindingRole::CoreKey { display: "V" },
        None
    ),
    row!(
        EnterBlockSelect,
        "select-block",
        "block-wise selection",
        Contexts::NORMAL,
        BindingRole::CoreKey { display: "Ctrl-V" },
        None
    ),
    row!(
        CancelSelect,
        "cancel-select",
        "cancel selection",
        Contexts::SELECT,
        BindingRole::CoreKey {
            display: "Esc / Ctrl-C"
        },
        Some(1)
    ),
    row!(
        SelectRegister,
        "select-register",
        "select register",
        Contexts::SELECT,
        BindingRole::CoreKey {
            display: "\"{register}"
        },
        None
    ),
    row!(
        SelectYank,
        "select-yank",
        "yank (Markdown by default) to clipboard",
        Contexts::SELECT,
        BindingRole::CoreKey { display: "y" },
        Some(2)
    ),
    row!(
        SelectYankPlainText,
        "select-yank-plain-text",
        "yank and send plain text to clipboard",
        Contexts::SELECT,
        BindingRole::CoreKey { display: "Y" },
        None
    ),
    row!(
        SelectDelete,
        "select-delete",
        "delete selection",
        Contexts::SELECT,
        BindingRole::CoreKey { display: "d / x" },
        Some(3)
    ),
    row!(
        SelectChange,
        "select-change",
        "change selection",
        Contexts::SELECT,
        BindingRole::CoreKey { display: "c" },
        None
    ),
    row!(
        SelectIndent,
        "select-indent",
        "indent selection",
        Contexts::SELECT,
        BindingRole::CoreKey { display: ">" },
        None
    ),
    row!(
        SelectOutdent,
        "select-outdent",
        "outdent selection",
        Contexts::SELECT,
        BindingRole::CoreKey { display: "<" },
        None
    ),
    row!(
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
                contexts: Contexts::NORMAL,
            },
            BindingAlias::Space {
                continuation: '?',
                contexts: RENDERED,
            },
        ],
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
    conformance_row!(
        SpellNext,
        "spell-next",
        "next spelling diagnostic",
        RENDERED,
        BindingRole::CoreKey { display: "]s" },
        "SP-2:next-previous-wrap"
    ),
    conformance_row!(
        SpellPrevious,
        "spell-previous",
        "previous spelling diagnostic",
        RENDERED,
        BindingRole::CoreKey { display: "[s" },
        "SP-2:next-previous-wrap"
    ),
    conformance_row!(
        SpellSet,
        "spell-set",
        "enable or disable spelling",
        Contexts::COMMAND,
        BindingRole::CoreEx {
            display: ":set spell / :set nospell"
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
    row!(
        NextTab,
        "next-tab",
        "next tab",
        RENDERED,
        BindingRole::CoreKey { display: "g t" },
        None
    ),
    row!(
        PrevTab,
        "prev-tab",
        "previous tab",
        RENDERED,
        BindingRole::CoreKey { display: "g T" },
        None
    ),
    row!(
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
            display: ":tabnew {path}"
        },
        None
    ),
    row!(
        TabClose,
        "tab-close",
        "close tab",
        RENDERED,
        BindingRole::CoreEx {
            display: ":tabclose"
        },
        None
    ),
    row!(
        QuitAll,
        "quit-all",
        "quit all tabs",
        Contexts::COMMAND,
        BindingRole::CoreEx { display: ":qa" },
        None
    ),
];

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
        BindingRole::CoreKey { display } | BindingRole::CoreEx { display } => display.to_string(),
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
            assert!(ids.insert(spec.id));
            assert!(names.insert(spec.name));
            assert!(!spec.contexts.is_empty());
            assert!(!rendered_binding(spec).is_empty());
        }
        assert_eq!(COMMANDS.len(), 32);
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
                    contexts: Contexts::NORMAL
                },
                BindingAlias::Space {
                    continuation: '?',
                    contexts: RENDERED
                },
            ]
        );
        assert_eq!(rendered_binding(help), "? / Space h/?");
        assert_eq!(rendered_binding_for(help, Contexts::SELECT), "Space h/?");
        assert_eq!(
            direct_command(Contexts::NORMAL, '?'),
            Some(AppCommand::Help)
        );
        assert_eq!(direct_command(Contexts::SELECT, '?'), None);
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
                },
                None,
            ),
            (
                RegistryEntryId::QuitAll,
                "quit-all",
                "quit all tabs",
                Contexts::COMMAND,
                BindingRole::CoreEx { display: ":qa" },
                None,
            ),
        ];
        let actual = COMMANDS
            .iter()
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
    fn core_binding_descriptors_are_visibility_only() {
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
