//! Owned read-only projections for host chrome and shortcut routing.

use std::path::PathBuf;

use oom_edit_core::{DiagnosticSeverity, EditorSession, KeyCodeKind, KeyInput, Mode};

/// Stable registry identity. It is metadata, never a dispatch payload.
pub use crate::command::registry::RegistryEntryId as BindingId;

/// Whether a registry entry has a real owner or is documentation only.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BindingExecution {
    /// Dispatched by App's existing Space grammar or tab action.
    ExecutableApp,
    /// Dispatched by EditorSession, not directly executable from the palette.
    ExecutableCore,
    /// Reference metadata, never an additional dispatcher.
    ReferenceOnly,
}

/// A concrete sequence or a parameterized grammar using the core key model.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BindingSequence {
    /// Literal core-key sequence, in dispatch order.
    Keys(Vec<KeyInput>),
    /// Ex grammar, with placeholders retained as metadata (not a dispatch API).
    Ex(String),
    /// Literal prefix followed by the displayed variable grammar.
    Pattern {
        /// Literal grammar prefix, without a host key namespace.
        prefix: Vec<KeyInput>,
        /// Display notation for the variable continuation; metadata is not a dispatch API.
        notation: String,
    },
}

/// One owned row in registry order. Ex rows use Command mode; other modes
/// describe the key grammar's entry state. Reference roles are never actions.
///
/// The implementation registry and App dispatch payload remain private:
///
/// ```compile_fail
/// use oom_edit::command::AppCommand;
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EditorBinding {
    /// Stable canonical binding identity, never an executable payload.
    pub id: BindingId,
    /// Stable human-readable registry name.
    pub name: String,
    /// Owned description of the binding.
    pub description: String,
    /// Public modes in which the sequence grammar can begin.
    pub modes: Vec<Mode>,
    /// Concrete or parameterized input grammars for this row.
    pub sequences: Vec<BindingSequence>,
    /// Actual dispatch owner or reference-only role.
    pub execution: BindingExecution,
}

/// Ownership before dispatch, including input intentionally consumed as a no-op.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyOwnership {
    /// Unfocused/empty pane, or an unsupported key outside an active grammar.
    Unclaimed,
    /// An editor overlay exclusively owns the input.
    Modal,
    /// Insert, ex or search text entry owns the input.
    TextEntry,
    /// An incomplete editor grammar owns the continuation.
    Pending,
    /// The existing App grammar owns this concrete binding.
    AppCommand(BindingId),
    /// The core owns this key, whether it executes a command or consumes a no-op.
    CoreGrammar,
    /// A prepared close has frozen this target until its token is resolved.
    Lifecycle,
}

/// Optional outer-host overrides. Reservations do not change editor dispatch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostReservation {
    /// Alt-character family available for an intentional outer-host override.
    AltChord,
    /// Exact Ctrl-g chord available for an outer-host prefix.
    ControlG,
    /// Plain F1 available for outer-host help.
    HelpFunction,
}

/// One continuation in registry order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WhichKeyEntry {
    /// Terminal-neutral key for this continuation.
    pub key: KeyInput,
    /// Owned continuation label in registry order.
    pub label: String,
}

/// Visible delayed Space-prefix content, independent of terminal geometry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WhichKey {
    /// Literal grammar prefix, without a host key namespace.
    pub prefix: String,
    /// Continuations in canonical registry order.
    pub entries: Vec<WhichKeyEntry>,
}

/// Enabled spell state. Absence in EditorStatus means spelling is disabled.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpellStatus {
    /// Number of currently displayed diagnostics.
    pub count: usize,
    /// Highest severity among displayed diagnostics, or none if there are none.
    pub highest_severity: Option<DiagnosticSeverity>,
}

/// Source ruler, with one-based display coordinates and exact displayed text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EditorRuler {
    /// One-based source line.
    pub line: usize,
    /// One-based source column.
    pub column: usize,
    /// Total source-line count.
    pub line_count: usize,
    /// Exact rendered ruler text.
    pub text: String,
}

/// Active editor-line data; no terminal styles or rendering state are exposed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EditorStatus {
    /// Current public editing mode.
    pub mode: Mode,
    /// Active absolute file path, or none for an unnamed buffer.
    pub path: Option<PathBuf>,
    /// Whether live text has unsaved changes.
    pub dirty: bool,
    /// Whether the active named path has never been created.
    pub is_new: bool,
    /// Spelling resource and runtime configuration.
    pub spell: Option<SpellStatus>,
    /// Current source position and document size.
    pub ruler: EditorRuler,
    /// Active ex or search prompt text, when present.
    pub prompt: Option<String>,
    /// Non-color disk-change marker, when a conflict remains deferred.
    pub disk_marker: Option<String>,
}

impl KeyOwnership {
    pub(crate) fn disposition(self) -> crate::InputDisposition {
        if self == Self::Unclaimed {
            crate::InputDisposition::NotConsumed
        } else {
            crate::InputDisposition::Consumed
        }
    }
}

impl WhichKey {
    pub(crate) fn display_text(&self) -> String {
        format!(
            "{}: {}",
            self.prefix,
            self.entries
                .iter()
                .map(|entry| {
                    let KeyCodeKind::Char(key) = entry.key.code.kind else {
                        unreachable!("the Space registry contains character continuations")
                    };
                    format!("{key}={}", entry.label)
                })
                .collect::<Vec<_>>()
                .join("  ")
        )
    }
}

pub(crate) fn editor_status(session: &EditorSession, disk_marker: Option<&str>) -> EditorStatus {
    let (line, column) = session.cursor();
    let ruler = EditorRuler {
        line: line + 1,
        column: column + 1,
        line_count: session.line_count(),
        text: crate::widgets::status_bar::ruler_text(line + 1, column + 1, session.line_count()),
    };
    EditorStatus {
        mode: session.mode(),
        path: session.path().map(PathBuf::from),
        dirty: session.is_dirty(),
        is_new: session.is_new(),
        spell: session.spell_enabled().then(|| SpellStatus {
            count: session.diagnostics().len(),
            highest_severity: session
                .diagnostics()
                .iter()
                .map(|d| d.severity)
                .max_by_key(|severity| crate::gutter::severity_priority(*severity)),
        }),
        ruler,
        prompt: session
            .command_line()
            .map(|text| format!(":{text}"))
            .or_else(|| session.rendered_search_prompt()),
        disk_marker: disk_marker.map(str::to_owned),
    }
}
