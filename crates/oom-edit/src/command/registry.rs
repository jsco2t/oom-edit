//! The command registry — the single source of truth for every user-facing
//! operation in the app chrome.
//!
//! One [`Command`] variant per discrete action; the [`COMMANDS`] table carries
//! each command's identity (kebab-case `name`, human `desc`), the [`Contexts`]
//! in which it is available, its hint-bar `order`, and whether it is
//! `quick_bar`-eligible. Contextual UI projections consume this metadata,
//! while [`super::Keymap`] owns triggers and `App` owns dispatch.

// ── Command enum ────────────────────────────────────────────────────────────

/// One enum variant per discrete user-facing action.
///
/// A closed enum (design decision D-1): type-safe, exhaustiveness-testable,
/// and enumerable for the palette/help without a parser.
macro_rules! command_variants {
    ($($variant:ident),+ $(,)?) => {
        #[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
        pub enum Command {
            $($variant),+
        }

        #[cfg(test)]
        const ALL_COMMANDS: &'static [Command] = &[
            $(Command::$variant),+
        ];
    };
}

command_variants! {
    ToggleView,
    Help,
    Save,
    Quit,
    CycleTheme,
    NextTab,
    PrevTab,
    JumpToTab,
    TabNew,
    TabClose,
    QuitAll,
}

// ── Contexts bitset ─────────────────────────────────────────────────────────

/// The set of UI contexts in which a command is available. A hand-rolled bitset
/// (no `bitflags` dependency — supply-chain rule; ~30 lines).
///
/// A "context" is the active mode + overlay the dispatcher is in. A command is
/// offered (hint bar, palette, dispatch) only where its `contexts` intersect
/// the current context.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct Contexts(u8);

impl Contexts {
    pub const NORMAL: Contexts = Contexts(1 << 0);
    #[allow(dead_code)]
    pub const INSERT: Contexts = Contexts(1 << 1);
    #[allow(dead_code)]
    pub const VISUAL: Contexts = Contexts(1 << 2);
    #[allow(dead_code)]
    pub const COMMAND: Contexts = Contexts(1 << 3);
    pub const VIEW: Contexts = Contexts(1 << 4);
    #[allow(dead_code)]
    pub const OVERLAY: Contexts = Contexts(1 << 5);

    /// Bit count — every declared context bit above.
    const BIT_COUNT: u8 = 6;

    /// Every context — for globally-available commands.
    pub const ALL: Contexts = Contexts((1 << Self::BIT_COUNT) - 1);

    /// Union of two context sets (const so it composes in `static` initialisers).
    pub const fn or(self, other: Contexts) -> Contexts {
        Contexts(self.0 | other.0)
    }

    /// Do these two context sets intersect? Used to test "is this command
    /// available in `ctx`?" where `ctx` is a single bit.
    pub const fn contains(self, other: Contexts) -> bool {
        self.0 & other.0 != 0
    }

    /// Is this the empty set (no contexts)? An empty-context command is
    /// unreachable dead weight — the registry tests forbid it.
    #[allow(dead_code)]
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// Iterate the single-bit contexts (one per declared constant). Test-only.
    #[cfg(test)]
    #[allow(dead_code)]
    pub(crate) fn each_bit() -> impl Iterator<Item = Contexts> {
        (0..Self::BIT_COUNT).map(|i| Contexts(1 << i))
    }
}

// ── CommandSpec ─────────────────────────────────────────────────────────────

/// One row of the command registry: a command's static metadata.
#[derive(Debug)]
pub struct CommandSpec {
    /// The command this row describes.
    pub id: Command,
    /// Kebab-case identifier (`toggle-view`).
    pub name: &'static str,
    /// Human-readable description for help / hints / palette.
    pub desc: &'static str,
    /// Contexts in which the command is available.
    pub contexts: Contexts,
    /// Hint-bar priority; lower renders further left. Gaps of 10 leave room.
    #[allow(dead_code)]
    pub order: i16,
    /// Eligible for the always-visible bottom hint bar.
    #[allow(dead_code)]
    pub quick_bar: bool,
}

// ── COMMANDS table ──────────────────────────────────────────────────────────

/// The command registry. One row per [`Command`] variant, in help-display order.
///
/// App commands (initial registry; `quick_bar` marks hint-bar eligibility):
///
/// | Command id   | name         | keys          | contexts            | quick_bar |
/// |--------------|--------------|---------------|---------------------|-----------|
/// | `ToggleView` | `toggle-view`| `Space v`     | NORMAL, VIEW        | yes       |
/// | `Help`       | `help`       | `F1`, `Space h`| all non-OVERLAY     | yes       |
/// | `Save`       | `save`       | `Space w`     | NORMAL              | yes       |
/// | `Quit`       | `quit`       | `Space q`     | NORMAL, VIEW        | yes       |
/// | `CycleTheme` | `cycle-theme`| `Space t`     | NORMAL, VIEW        | no        |
pub static COMMANDS: &[CommandSpec] = &[
    CommandSpec {
        id: Command::ToggleView,
        name: "toggle-view",
        desc: "toggle view",
        contexts: Contexts::NORMAL.or(Contexts::VIEW),
        order: 0,
        quick_bar: true,
    },
    CommandSpec {
        id: Command::Help,
        name: "help",
        desc: "help / command palette",
        contexts: Contexts::NORMAL.or(Contexts::VIEW),
        order: 10,
        quick_bar: true,
    },
    CommandSpec {
        id: Command::Save,
        name: "save",
        desc: "save",
        contexts: Contexts::NORMAL,
        order: 20,
        quick_bar: true,
    },
    CommandSpec {
        id: Command::Quit,
        name: "quit",
        desc: "quit",
        contexts: Contexts::NORMAL.or(Contexts::VIEW),
        order: 30,
        quick_bar: true,
    },
    CommandSpec {
        id: Command::CycleTheme,
        name: "cycle-theme",
        desc: "cycle theme",
        contexts: Contexts::NORMAL.or(Contexts::VIEW),
        order: 40,
        quick_bar: false,
    },
    CommandSpec {
        id: Command::NextTab,
        name: "next-tab",
        desc: "next tab",
        contexts: Contexts::NORMAL.or(Contexts::VIEW),
        order: 50,
        quick_bar: false,
    },
    CommandSpec {
        id: Command::PrevTab,
        name: "prev-tab",
        desc: "previous tab",
        contexts: Contexts::NORMAL.or(Contexts::VIEW),
        order: 51,
        quick_bar: false,
    },
    CommandSpec {
        id: Command::JumpToTab,
        name: "jump-to-tab",
        desc: "jump to tab",
        contexts: Contexts::NORMAL.or(Contexts::VIEW),
        order: 52,
        quick_bar: false,
    },
    CommandSpec {
        id: Command::TabNew,
        name: "tab-new",
        desc: "new tab",
        contexts: Contexts::NORMAL.or(Contexts::VIEW),
        order: 60,
        quick_bar: false,
    },
    CommandSpec {
        id: Command::TabClose,
        name: "tab-close",
        desc: "close tab",
        contexts: Contexts::NORMAL.or(Contexts::VIEW),
        order: 61,
        quick_bar: false,
    },
    CommandSpec {
        id: Command::QuitAll,
        name: "quit-all",
        desc: "quit all tabs",
        contexts: Contexts::COMMAND,
        order: 70,
        quick_bar: false,
    },
];

/// The registry row for `id`. Every [`Command`] variant has exactly one row
/// (guaranteed by `commands_table_is_exhaustive_and_unique`).
pub fn spec_for(id: Command) -> Option<&'static CommandSpec> {
    COMMANDS.iter().find(|spec| spec.id == id)
}

/// Get quick_bar-eligible commands for a given context, sorted by order.
pub fn commands_for(ctx: Contexts) -> Vec<&'static CommandSpec> {
    let mut cmds: Vec<&CommandSpec> = COMMANDS
        .iter()
        .filter(|spec| spec.quick_bar && spec.contexts.contains(ctx))
        .collect();
    cmds.sort_by_key(|spec| spec.order);
    cmds
}

// ── Meta / drift tests ──────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Every `Command` variant appears in `COMMANDS` exactly once.
    #[test]
    fn commands_table_is_exhaustive_and_unique() {
        fn assert_registered(c: Command) {
            let count = COMMANDS.iter().filter(|spec| spec.id == c).count();
            assert_eq!(count, 1, "{c:?} must appear exactly once in COMMANDS");
        }
        for &command in ALL_COMMANDS {
            assert_registered(command);
        }
        assert_eq!(COMMANDS.len(), ALL_COMMANDS.len());
    }

    /// Every command has at least one context.
    #[test]
    fn every_command_has_context() {
        for spec in COMMANDS {
            assert!(
                !spec.contexts.is_empty(),
                "{:?} has no contexts — unreachable dead weight",
                spec.id
            );
        }
    }

    /// Command names are unique and kebab-case.
    #[test]
    fn command_names_are_unique_kebab_case() {
        let mut seen = std::collections::HashSet::new();
        for spec in COMMANDS {
            assert!(seen.insert(spec.name), "duplicate name {:?}", spec.name);
            assert!(!spec.name.is_empty(), "empty name for {:?}", spec.id);
            for ch in spec.name.chars() {
                assert!(
                    ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-',
                    "{:?} name {:?} is not kebab-case (char {ch:?})",
                    spec.id,
                    spec.name
                );
            }
            assert!(
                !spec.name.starts_with('-') && !spec.name.ends_with('-'),
                "{:?} name must not start/end with '-'",
                spec.id
            );
        }
    }
}
