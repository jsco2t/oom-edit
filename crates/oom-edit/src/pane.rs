//! Public, terminal-neutral host boundary over the private App.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Weak};
use std::time::Instant;

use oom_edit_core::{analyze_markdown, EditorSession, Mode};

use crate::app::{App, AppRenderOptions, AppServices, AppStartupOptions, ThemeSetOutcome};
use crate::config::{Config, ConfigApplyReport, ConfigWarning, ThemePersistenceSink};
use crate::lifecycle::{LifecycleAction, LifecycleOutcome};
use crate::pane_frame::PaneFrame;
use crate::spell_host::{resolve_wordlist_source, SpellHost};
use crate::theme::{DisplayMode, ResolvedTheme, ThemeCatalog, ThemeSelection};

/// Opaque identity of a tab within one live pane generation.
#[derive(Clone, Debug)]
pub struct TabId {
    pub(crate) pane: Weak<()>,
    pub(crate) serial: u64,
}

impl PartialEq for TabId {
    fn eq(&self, other: &Self) -> bool {
        self.serial == other.serial && self.pane.ptr_eq(&other.pane)
    }
}

impl Eq for TabId {}

impl std::hash::Hash for TabId {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        std::hash::Hash::hash(&self.serial, state);
        std::hash::Hash::hash(&self.pane.as_ptr(), state);
    }
}

/// Opaque correlation identity for one accepted host request.
#[derive(Clone, Debug)]
pub struct RequestId {
    pub(crate) pane: Weak<()>,
    pub(crate) serial: u64,
}

impl PartialEq for RequestId {
    fn eq(&self, other: &Self) -> bool {
        self.serial == other.serial && self.pane.ptr_eq(&other.pane)
    }
}

impl Eq for RequestId {}

impl std::hash::Hash for RequestId {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        std::hash::Hash::hash(&self.serial, state);
        std::hash::Hash::hash(&self.pane.as_ptr(), state);
    }
}

/// Which App command policy the host requests.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommandPolicy {
    /// Preserve the standalone command semantics, including duplicate `:tabnew`.
    Standalone,
    /// Focus an already-open canonical path instead of opening a duplicate.
    Embedded,
}

/// Presentation and command behavior selected by the host.
#[derive(Clone, Debug)]
pub struct PaneOptions {
    /// Standalone compatibility or embedded open-or-focus command semantics.
    pub command_policy: CommandPolicy,
    /// Render hints and which-key inside the editor, instead of host-owned chrome.
    pub inline_hints: bool,
    /// Show only the mode badge and right-side indicators outside active prompts.
    /// This suppresses inline hints, file details, and transient notices.
    pub minimal_status_bar: bool,
    /// Show the tab bar even when only one tab is open.
    pub always_tab_bar: bool,
    /// Host-supplied lines shown while no tabs are open.
    pub empty_state_lines: Vec<String>,
}

impl Default for PaneOptions {
    fn default() -> Self {
        Self {
            command_policy: CommandPolicy::Embedded,
            inline_hints: false,
            minimal_status_bar: false,
            always_tab_bar: false,
            empty_state_lines: Vec::new(),
        }
    }
}

/// File operation submitted to the host's access policy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileOperation {
    /// Open or focus a document, allowing a new path.
    Open,
    /// Open an existing file only.
    OpenExisting,
    /// Save the active document.
    Save,
    /// Change file identity by saving to a new path.
    SaveAs,
    /// Write a copy without changing the current identity.
    SaveCopy,
    /// Reference for explicitly reloading a document.
    Reload,
    /// Change a document binding without writing its text.
    Retarget,
    /// Observe a bound file's metadata or content.
    Inspect,
}

/// Owned denial from a host file-access policy.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FilePolicyError {
    /// Owned explanation of the policy denial.
    pub reason: String,
}

/// Host-defined access checks for every pane file request.
pub trait FileAccessPolicy {
    /// Authorize this typed file operation at its resolved path.
    fn authorize(&self, operation: FileOperation, path: &Path) -> Result<(), FilePolicyError>;
}

/// Unrestricted policy for the standalone host and simple integrations.
#[derive(Clone, Copy, Debug, Default)]
pub struct AllowAllFileAccess;

impl FileAccessPolicy for AllowAllFileAccess {
    fn authorize(&self, _operation: FileOperation, _path: &Path) -> Result<(), FilePolicyError> {
        Ok(())
    }
}

/// Explicit services and paths supplied by the host, without process discovery.
pub struct PaneServices {
    /// Host-owned destination for clipboard copies.
    pub clipboard_sink: Box<dyn oom_edit_core::ClipboardSink>,
    /// Host-owned appearance/name persistence boundary.
    pub theme_sink: Box<dyn ThemePersistenceSink>,
    /// Host policy checked at every pane file-I/O boundary.
    pub file_access_policy: Box<dyn FileAccessPolicy>,
    /// Base directory for user themes and relative dictionary paths.
    pub config_base_directory: PathBuf,
    /// Independent file used for personal dictionary loading and additions.
    pub personal_dictionary_path: PathBuf,
    /// Explicit base for relative document paths; no process directory discovery.
    pub working_directory: PathBuf,
}

/// Complete host-owned pane construction input.
pub struct PaneInit {
    /// Complete owned editor configuration.
    pub config: Config,
    /// Explicit built-in and user theme catalog.
    pub theme_catalog: ThemeCatalog,
    /// Explicit override, appearance and capability inputs.
    pub theme_selection: ThemeSelection,
    /// Host-injected I/O services and path roots.
    pub services: PaneServices,
    /// Host-selected presentation and command policy.
    pub options: PaneOptions,
    /// Paths to attempt opening, in input order; empty means zero initial tabs.
    pub initial_paths: Vec<PathBuf>,
    /// Host-sampled monotonic construction instant.
    pub now: Instant,
}

/// A nonfatal construction warning.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PaneWarning {
    /// Configuration validation warning.
    Config(ConfigWarning),
    /// Spelling initialization warning.
    Spell(String),
}

/// Per-path startup outcomes preserve partial-open errors in request order.
#[derive(Debug)]
pub struct PaneConstructionReport {
    /// Ordered per-path results or affected paths for this operation.
    pub paths: Vec<Result<TabId, PaneError>>,
    /// Every nonfatal construction warning.
    pub warnings: Vec<PaneWarning>,
}

/// Constructed pane plus every startup result.
pub struct PaneConstruction {
    /// The constructed pane.
    pub pane: EditorPane,
    /// Startup outcomes and warnings; partial-open errors do not discard successful tabs.
    pub report: PaneConstructionReport,
}

/// Owned failure kind for a host request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaneErrorKind {
    /// The tab identity does not belong to this live pane.
    UnknownTab,
    /// Another exclusive lifecycle or mutation flow owns the pane.
    Busy,
    /// A token, request or observed version is no longer valid.
    Stale,
    /// Unsaved edits prevent this operation.
    Dirty,
    /// An unnamed buffer requires a host-supplied filename.
    SavePathRequired,
    /// The host's file-access policy rejected the operation.
    Denied,
    /// The requested path or mapping is invalid.
    InvalidPath,
    /// A required backed file has disappeared.
    Missing,
    /// File bytes are not valid UTF-8.
    NotUtf8,
    /// A filesystem operation failed.
    Io,
    /// The requested theme cannot be selected.
    InvalidTheme,
}

/// Error with an optional path and an owned diagnostic.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PaneError {
    /// Typed classification of this outcome or input.
    pub kind: PaneErrorKind,
    /// File path associated with this value or operation.
    pub path: Option<PathBuf>,
    /// Owned diagnostic explaining this result.
    pub detail: String,
}

impl std::fmt::Display for PaneError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.detail)
    }
}

impl std::error::Error for PaneError {}

/// Initial cursor placement for a newly opened tab.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum OpenCursor {
    #[default]
    /// Start at the beginning of the document.
    Start,
    /// Zero-based source line, clamped to the document.
    SourceLine(usize),
    /// First body position following valid front matter.
    AfterFrontMatter,
}

/// Options applied only when a new tab is opened, never when focusing an existing one.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct OpenOptions {
    /// Visible pane-local cursor, or none when unfocused/hidden.
    pub cursor: OpenCursor,
    /// Enter Insert only when a new tab is opened.
    pub enter_insert: bool,
}

/// Result of a successful open request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OpenOutcome {
    /// A document was successfully opened.
    Opened(TabId),
    /// An already-open document was activated.
    FocusedExisting(TabId),
}

/// Read-only tab metadata in tab order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TabSnapshot {
    /// Stable identity used by the public host protocol.
    pub id: TabId,
    /// File path associated with this value or operation.
    pub path: Option<PathBuf>,
    /// Display title, independent of the canonical file path.
    pub title: String,
    /// Whether live text has unsaved changes.
    pub dirty: bool,
    /// Whether this tab is currently active.
    pub active: bool,
    /// A named path that has never been created; unnamed buffers are unbacked.
    pub is_new: bool,
    /// A non-color disk-change marker remains pending for this tab.
    pub changed_on_disk: bool,
    /// Zero is the most recently activated tab.
    pub mru_rank: usize,
    /// Current public editing mode.
    pub mode: Mode,
}

/// An ordered cross-boundary notification.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PaneEvent {
    /// The pane's effective theme changed.
    ThemeChanged {
        /// Theme appearance and name configuration.
        theme: ResolvedTheme,
    },
    /// The single terminal success for a lifecycle request. All other success
    /// events describe its ordered progress; Failed/Cancelled are terminal failures.
    Completed {
        /// Correlation identity of the originating accepted request.
        request: RequestId,
    },
    /// A document was successfully opened.
    Opened {
        /// Correlation identity of the originating accepted request.
        request: RequestId,
        /// Stable identity of the tab affected by this event.
        tab: TabId,
        /// File path associated with this value or operation.
        path: Option<PathBuf>,
    },
    /// The active tab actually changed.
    ActiveTabChanged {
        /// Correlation identity of the originating accepted request.
        request: RequestId,
        /// Stable identity of the tab affected by this event.
        tab: TabId,
    },
    /// The tab's public editing mode changed.
    ModeChanged {
        /// Stable identity of the tab affected by this event.
        tab: TabId,
        /// Current public editing mode.
        mode: Mode,
    },
    /// The document was durably saved.
    Saved {
        /// Correlation identity of the originating accepted request.
        request: RequestId,
        /// Stable identity of the tab affected by this event.
        tab: TabId,
        /// File path associated with this value or operation.
        path: PathBuf,
    },
    /// A copy was durably saved without retargeting the tab.
    SavedCopy {
        /// Correlation identity of the originating accepted request.
        request: RequestId,
        /// Stable identity of the tab affected by this event.
        tab: TabId,
        /// File path associated with this value or operation.
        path: PathBuf,
    },
    /// The captured tab was closed.
    Closed {
        /// Correlation identity of the originating accepted request.
        request: RequestId,
        /// Stable identity of the tab affected by this event.
        tab: TabId,
        /// File path associated with this value or operation.
        path: Option<PathBuf>,
    },
    /// Every tab captured by the close-all request was closed.
    AllClosed {
        /// Correlation identity of the originating accepted request.
        request: RequestId,
    },
    /// The accepted request was cancelled without closing retained tabs.
    Cancelled {
        /// Correlation identity of the originating accepted request.
        request: RequestId,
    },
    /// The accepted request terminated with an owned failure.
    Failed {
        /// Correlation identity of the originating accepted request.
        request: RequestId,
        /// Stable identity of the tab affected by this event.
        tab: Option<TabId>,
        /// Owned typed failure diagnostic.
        error: PaneError,
    },
    /// Replacement committed, but durability confirmation failed; the buffer remains dirty.
    DiskWriteCommitted {
        /// Correlation identity of the originating accepted request.
        request: RequestId,
        /// Stable identity of the tab affected by this event.
        tab: TabId,
        /// File path associated with this value or operation.
        path: PathBuf,
        /// Owned diagnostic explaining this result.
        detail: String,
    },
    /// The editor requests host quit orchestration; the pane never exits the process.
    QuitAllRequested {
        /// Correlation identity of the originating accepted request.
        request: RequestId,
        /// Whether the editor command explicitly requested force; host path policy still applies.
        force: bool,
    },
    /// An explicit reload succeeded.
    Reloaded {
        /// Correlation identity of the originating accepted request.
        request: RequestId,
        /// Stable identity of the tab affected by this event.
        tab: TabId,
        /// File path associated with this value or operation.
        path: PathBuf,
    },
    /// A host-requested dirty flow needs this tab presented to the user.
    AttentionRequired {
        /// Correlation identity of the originating accepted request.
        request: RequestId,
        /// Stable identity of the tab affected by this event.
        tab: TabId,
    },
    /// The host must supply a filename or cancel this unnamed-buffer save.
    SavePathRequested {
        /// Correlation identity of the originating accepted request.
        request: RequestId,
        /// Stable identity of the tab affected by this event.
        tab: TabId,
    },
    /// Dirty decisions are resolved; the host may obtain the prepared close token.
    PreparedClose {
        /// Correlation identity of the originating accepted request.
        request: RequestId,
        /// Captured tab identities in stable tab order.
        tabs: Vec<TabId>,
    },
    /// All proposed bindings were validated before the host filesystem operation.
    PreparedRetarget {
        /// Correlation identity of the originating accepted request.
        request: RequestId,
        /// Captured validated retarget mappings.
        targets: Vec<Retarget>,
    },
    /// A tab's file binding changed without replacing its live text.
    Retargeted {
        /// Correlation identity of the originating accepted request.
        request: RequestId,
        /// Stable identity of the tab affected by this event.
        tab: TabId,
        /// Previous file binding.
        from: PathBuf,
        /// New file binding.
        to: PathBuf,
    },
    /// An explicit prepared transaction was committed.
    PreparedCommitted {
        /// Correlation identity of the originating accepted request.
        request: RequestId,
    },
    /// The host's external mutation finished and the listed paths were reconciled.
    ExternalChangeCommitted {
        /// Correlation identity of the originating accepted request.
        request: RequestId,
        /// Ordered per-path results or affected paths for this operation.
        paths: Vec<PathBuf>,
    },
    /// A new observed disk state awaits a safe point; it does not request focus.
    DiskChangePending {
        /// Stable identity of the tab affected by this event.
        tab: TabId,
        /// Content-validated disk state awaiting safe reconciliation.
        change: DiskChange,
    },
    /// A safe-point disk watcher reload succeeded.
    ReloadedFromDisk {
        /// Correlation identity of the originating accepted request.
        request: RequestId,
        /// Stable identity of the tab affected by this event.
        tab: TabId,
        /// File path associated with this value or operation.
        path: PathBuf,
    },
}

/// Content-validated external state retained until a safe decision.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DiskChange {
    /// The bound file's content differs from the observed baseline.
    Modified(oom_edit_core::DiskVersion),
    /// A required backed file has disappeared.
    Missing(oom_edit_core::DiskVersion),
    /// Disk observation failed; retained text is unchanged.
    IoError(PaneError),
}

/// Single-use suspension of pane file I/O while the host changes its files.
/// Dropping the token aborts on the pane's next input, tick or event drain.
///
/// ```compile_fail
/// fn duplicate(token: oom_edit::ExternalChangeToken) { let _ = token.clone(); }
/// ```
#[derive(Debug)]
pub struct ExternalChangeToken {
    pub(crate) request: RequestId,
    pub(crate) _lease: Arc<()>,
}

/// A prepared close retains discarded buffers until explicit commit.
#[derive(Debug)]
pub struct PreparedClose {
    pub(crate) token: ExternalChangeToken,
}

/// One captured tab and its final path after a host-managed move.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Retarget {
    /// Stable identity of the tab affected by this event.
    pub tab: TabId,
    /// File path associated with this value or operation.
    pub path: PathBuf,
}

/// Single-use batch identity update; the host owns the actual filesystem move.
///
/// ```compile_fail
/// fn reuse(pane: &mut oom_edit::EditorPane, token: oom_edit::PreparedRetarget) {
///     pane.commit_retarget(token).unwrap();
///     pane.commit_retarget(token).unwrap();
/// }
/// ```
#[derive(Debug)]
pub struct PreparedRetarget {
    pub(crate) token: ExternalChangeToken,
}

impl ExternalChangeToken {
    /// Correlates events emitted by this owned transaction.
    pub fn request_id(&self) -> &RequestId {
        &self.request
    }
}

impl PreparedRetarget {
    /// Correlates preparation, retarget and terminal events.
    pub fn request_id(&self) -> &RequestId {
        self.token.request_id()
    }
}

impl PreparedClose {
    /// Correlates preparation, close and terminal events.
    pub fn request_id(&self) -> &RequestId {
        self.token.request_id()
    }
}

/// Whether the active pane grammar owns one input.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputDisposition {
    /// The pane owns this input, including grammar-specific no-ops.
    Consumed,
    /// The host may route this input elsewhere.
    NotConsumed,
}

/// Pane-local mouse action; screen origins are never included.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaneMouseKind {
    /// Left-button press.
    LeftDown,
    /// Left-button drag.
    LeftDrag,
    /// Left-button release.
    LeftUp,
    /// Scroll wheel up.
    ScrollUp,
    /// Scroll wheel down.
    ScrollDown,
    /// Pointer movement without a handled button.
    Moved,
    /// An otherwise unsupported mouse report.
    Other,
}

/// One pane-local mouse input.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PaneMouse {
    /// Typed classification of this outcome or input.
    pub kind: PaneMouseKind,
    /// Zero-based pane-local display column.
    pub column: u16,
    /// Zero-based pane-local display row.
    pub row: u16,
    /// Text effects to apply.
    pub modifiers: oom_edit_core::Modifiers,
}

/// Input already translated by the host. Key events must be presses only.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PaneInput {
    /// One translated terminal-neutral key press.
    Key(oom_edit_core::KeyInput),
    /// Owned bracketed-paste text.
    Paste(String),
    /// One pane-local mouse report.
    Mouse(PaneMouse),
}

/// Whether the pane is accepting text or owns an exclusive modal interaction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PaneInputState {
    /// Whether Insert, ex or search text entry owns editor input.
    pub text_entry: bool,
    /// Whether an exclusive editor interaction owns input.
    pub modal: bool,
}

/// Result of advancing pane-owned timers at the host's monotonic instant.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PaneTick {
    /// Earliest pane-owned timer deadline, when any timer is pending.
    pub next_deadline: Option<Instant>,
    /// Whether the host should request another owned frame.
    pub redraw: bool,
    /// Whether spell work is eligible after five seconds without input.
    pub idle_due: bool,
}

/// One bounded cooperative background-work result.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PaneIdleResult {
    /// Whether one bounded unit of idle work was completed.
    pub worked: bool,
    /// Whether the host should request another owned frame.
    pub redraw: bool,
}

/// Embeddable editor facade. App remains the sole UI and editing-state owner.
///
/// Implementation modules and mutable sessions are deliberately private:
///
/// ```compile_fail
/// use oom_edit::app::App;
/// ```
///
/// ```compile_fail
/// fn bypass_policy(pane: &mut oom_edit::EditorPane) {
///     let _ = pane.session_mut();
/// }
/// ```
pub struct EditorPane {
    app: App,
    configuration: Config,
    theme_selection: ThemeSelection,
    options: PaneOptions,
    previous_frame: Option<CachedFrame>,
    events: Vec<PaneEvent>,
    idle_complete: bool,
}

struct CachedFrame {
    key: (u64, bool),
    frame: PaneFrame,
}

impl EditorPane {
    /// Current complete hint cells, including modal-specific hints.
    pub fn hints(&self) -> Vec<crate::HintCell> {
        self.app.current_hints()
    }

    /// Delayed focused Space-prefix content at the most recent supplied time.
    /// Call `tick(now)` even when hidden; inline-hint options do not affect this data.
    pub fn which_key(&self) -> Option<crate::WhichKey> {
        self.app.current_which_key()
    }

    /// Structured registry metadata in canonical order, not a dispatch API.
    /// Core and reference rows cannot be executed directly through the palette.
    pub fn bindings(&self) -> Vec<crate::EditorBinding> {
        crate::command::registry::export_bindings()
    }

    /// Predict grammar ownership without dispatching or changing state. A core
    /// no-op is still owned input; absence of a named command is not collision freedom.
    pub fn key_ownership(&self, key: oom_edit_core::KeyInput) -> crate::KeyOwnership {
        self.app.key_ownership(key)
    }

    /// Optional host reservation family; the host chooses its own global shortcuts
    /// and must explicitly intercept those keys before `handle_input`. In particular,
    /// Alt-character insertion, Ctrl-g and pending/modal F1 behavior remain unchanged.
    pub fn host_reservation(key: oom_edit_core::KeyInput) -> Option<crate::HostReservation> {
        use oom_edit_core::KeyCodeKind;
        if key.mods.alt && matches!(key.code.kind, KeyCodeKind::Char(_)) {
            Some(crate::HostReservation::AltChord)
        } else if key.mods
            == (oom_edit_core::Modifiers {
                ctrl: true,
                alt: false,
                shift: false,
            })
            && key.code.kind == KeyCodeKind::Char('g')
        {
            Some(crate::HostReservation::ControlG)
        } else if key.mods == oom_edit_core::Modifiers::default()
            && key.code.kind == KeyCodeKind::F(1)
        {
            Some(crate::HostReservation::HelpFunction)
        } else {
            None
        }
    }

    /// Active editor-line data; an empty pane has no editor status.
    pub fn status(&self) -> Option<crate::EditorStatus> {
        self.app.current_status()
    }

    #[cfg(test)]
    pub(crate) fn from_app_for_test(app: App) -> Self {
        Self {
            app,
            configuration: Config::default(),
            theme_selection: ThemeSelection::new(None, DisplayMode::Dark, crate::Tier::TrueColor),
            options: PaneOptions {
                command_policy: CommandPolicy::Standalone,
                inline_hints: true,
                ..PaneOptions::default()
            },
            previous_frame: None,
            events: Vec::new(),
            idle_complete: false,
        }
    }

    #[cfg(test)]
    pub(crate) fn app_for_test(&self) -> &App {
        &self.app
    }

    #[cfg(test)]
    pub(crate) fn app_mut_for_test(&mut self) -> &mut App {
        &mut self.app
    }

    /// Construct without reading argv, environment or config files, and without terminal setup.
    pub fn construct(init: PaneInit) -> PaneConstruction {
        let PaneInit {
            config,
            theme_catalog,
            theme_selection,
            services,
            options,
            initial_paths,
            now,
        } = init;
        let validation = config.validate(&services.config_base_directory);
        let config = validation.effective;
        let mut warnings = validation
            .warnings
            .into_iter()
            .map(PaneWarning::Config)
            .collect::<Vec<_>>();
        let resolved_theme = theme_catalog.resolve_explicit(&config.theme, &theme_selection);
        let source = resolve_wordlist_source(&config.spell, &services.config_base_directory);
        if let Some(warning) = source.warning {
            warnings.push(PaneWarning::Spell(warning));
        }
        let spell_host = SpellHost::production(source.source, services.personal_dictionary_path);
        let file_access_policy: Arc<dyn FileAccessPolicy> = services.file_access_policy.into();
        let app = App::new_empty(
            theme_catalog,
            resolved_theme,
            AppStartupOptions::from_config(&config),
            AppServices::new(
                services.clipboard_sink,
                services.theme_sink,
                spell_host,
                services.working_directory.clone(),
            )
            .with_file_access_policy(Arc::clone(&file_access_policy)),
            now,
            options.command_policy,
        );
        let mut pane = Self {
            app,
            configuration: config,
            theme_selection,
            options,
            previous_frame: None,
            events: Vec::new(),
            idle_complete: false,
        };
        let paths = initial_paths
            .iter()
            .map(|path| match pane.open(path, OpenOptions::default()) {
                Ok(OpenOutcome::Opened(id) | OpenOutcome::FocusedExisting(id)) => Ok(id),
                Err(error) => Err(error),
            })
            .collect();
        PaneConstruction {
            pane,
            report: PaneConstructionReport { paths, warnings },
        }
    }

    /// Render to an owned, pane-local grid at the requested size and time.
    pub fn render(&mut self, width: u16, height: u16, now: Instant) -> PaneFrame {
        let key = self.app.presentation_key(now);
        if let Some(previous) = self.previous_frame.as_ref().filter(|cached| {
            cached.key == key && cached.frame.width == width && cached.frame.height == height
        }) {
            let mut frame = previous.frame.clone();
            frame.changed = false;
            return frame;
        }
        let frame = self.app.render_owned(
            width,
            height,
            now,
            AppRenderOptions {
                inline_hints: self.options.inline_hints,
                minimal_status_bar: self.options.minimal_status_bar,
                always_tab_bar: self.options.always_tab_bar,
                empty_state_lines: &self.options.empty_state_lines,
            },
            self.previous_frame.as_ref().map(|cached| &cached.frame),
        );
        self.previous_frame = Some(CachedFrame {
            key: self.app.presentation_key(now),
            frame: frame.clone(),
        });
        frame
    }

    /// Apply final coalesced dimensions before the next input without painting.
    /// Hosts may instead supply dimensions directly on their next `render` call.
    pub fn resize(&mut self, width: u16, height: u16, now: Instant) {
        self.app.resize(
            width,
            height,
            now,
            self.options.always_tab_bar || self.app.tab_count() > 1,
        );
        self.idle_complete = false;
    }

    /// Restyle without persisting host-owned settings. Unknown or incompatible
    /// themes leave the pane unchanged and return a typed error.
    pub fn set_theme(&mut self, name: &str) -> Result<(), PaneError> {
        match self.app.set_theme(name) {
            ThemeSetOutcome::Unavailable => Err(PaneError {
                kind: PaneErrorKind::InvalidTheme,
                path: None,
                detail: format!("theme {name:?} is unavailable for the current appearance"),
            }),
            ThemeSetOutcome::Unchanged | ThemeSetOutcome::Changed(_) => {
                self.theme_selection = ThemeSelection::new(
                    Some(name.to_string()),
                    self.theme_selection.display_mode(),
                    self.theme_selection.capability(),
                );
                self.flush_lifecycle_events();
                Ok(())
            }
        }
    }

    /// Apply runtime-safe settings without persistence or resource reloads.
    /// Validate host input with `Config::validate` first. Changes to wrap width,
    /// spell language or extra dictionaries remain unapplied and are reported
    /// on every call until the host constructs a new pane.
    pub fn apply_config(&mut self, mut config: Config) -> ConfigApplyReport {
        let report = ConfigApplyReport::between(&self.configuration, &config);
        if self.configuration.theme != config.theme {
            let mode = match config.theme.mode.as_deref() {
                Some("light") => DisplayMode::Light,
                Some("dark") => DisplayMode::Dark,
                _ => self.theme_selection.display_mode(),
            };
            self.theme_selection =
                ThemeSelection::new(None, mode, self.theme_selection.capability());
        }
        self.app
            .apply_runtime_config(&config, &self.theme_selection);
        config.editor.wrap_width = self.configuration.editor.wrap_width;
        config
            .spell
            .language
            .clone_from(&self.configuration.spell.language);
        config
            .spell
            .additional_dictionaries
            .clone_from(&self.configuration.spell.additional_dictionaries);
        self.configuration = config;
        self.idle_complete = false;
        self.flush_lifecycle_events();
        report
    }

    /// Return tab metadata without exposing mutable editor sessions.
    pub fn tabs(&self) -> Vec<TabSnapshot> {
        self.app.tab_snapshots()
    }

    /// Return the active tab identity, if any.
    pub fn active_tab(&self) -> Option<TabId> {
        self.app.active_tab_id()
    }

    /// Read authoritative text without granting a mutable session escape.
    pub fn text(&self, id: &TabId) -> Result<String, PaneError> {
        self.app.tab_text(id).ok_or_else(PaneError::unknown_tab)
    }

    /// Read the authoritative zero-based source cursor of one tab.
    pub fn source_cursor(&self, id: &TabId) -> Result<(usize, usize), PaneError> {
        self.app.tab_cursor(id).ok_or_else(PaneError::unknown_tab)
    }

    /// Focus a tab; repeated focus does not change MRU or emit an event.
    pub fn focus_tab(&mut self, id: &TabId) -> Result<(), PaneError> {
        if !self.app.contains_tab_id(id) {
            return Err(PaneError::unknown_tab());
        }
        if self.app.focus_tab_id(id) {
            self.idle_complete = false;
        }
        Ok(())
    }

    /// Open a path or focus its existing canonical tab, regardless of command policy.
    pub fn open(&mut self, path: &Path, options: OpenOptions) -> Result<OpenOutcome, PaneError> {
        self.open_inner(path, options, false)
    }

    /// Open only an existing path; a missing file is a typed error.
    pub fn open_existing(
        &mut self,
        path: &Path,
        options: OpenOptions,
    ) -> Result<OpenOutcome, PaneError> {
        self.open_inner(path, options, true)
    }

    /// Open an empty unnamed buffer without filesystem access.
    pub fn new_buffer(&mut self, options: OpenOptions) -> Result<TabId, PaneError> {
        match self
            .app
            .execute_lifecycle(LifecycleAction::NewBuffer { options })
        {
            LifecycleOutcome::Open(OpenOutcome::Opened(id)) => {
                self.idle_complete = false;
                Ok(id)
            }
            LifecycleOutcome::Failed(error) => Err(error),
            _ => unreachable!("new unnamed buffer always returns an open outcome"),
        }
    }

    /// Validate the complete move mapping and suspend pane file IO.
    pub fn prepare_retarget(
        &mut self,
        targets: &[Retarget],
    ) -> Result<PreparedRetarget, PaneError> {
        match self
            .app
            .execute_lifecycle(LifecycleAction::PrepareRetarget {
                targets: targets.to_vec(),
            }) {
            LifecycleOutcome::Retarget(token) => Ok(token),
            LifecycleOutcome::Failed(error) => Err(error),
            _ => unreachable!("retarget preparation returns its owned token"),
        }
    }

    /// After the host move, validate every destination and update all bindings together.
    pub fn commit_retarget(&mut self, prepared: PreparedRetarget) -> Result<(), PaneError> {
        Self::unit_outcome(self.app.execute_lifecycle(LifecycleAction::CommitRetarget {
            request: prepared.token.request.clone(),
        }))
    }

    /// Release preparation without changing any tab binding.
    pub fn abort_retarget(&mut self, prepared: PreparedRetarget) -> Result<(), PaneError> {
        Self::unit_outcome(self.app.execute_lifecycle(LifecycleAction::AbortToken {
            request: prepared.token.request.clone(),
        }))
    }

    /// Resolve dirty decisions and retain the captured tabs until commit.
    pub fn prepare_close(&mut self, tabs: &[TabId]) -> Result<RequestId, PaneError> {
        self.close_request(tabs.to_vec(), false, false)
    }

    /// Cancel a pending confirmation or untaken prepared close, even while
    /// unfocused. Issued tokens must instead be consumed by their abort method.
    pub fn cancel_request(&mut self, request: &RequestId) -> Result<(), PaneError> {
        Self::unit_outcome(self.app.execute_lifecycle(LifecycleAction::CancelRequest {
            request: request.clone(),
        }))
    }

    /// Close one captured tab after its save/discard/cancel decision.
    pub fn close(&mut self, tab: &TabId) -> Result<RequestId, PaneError> {
        self.close_request(vec![tab.clone()], true, false)
    }

    /// Close all current tabs through the same ordered preparation protocol.
    pub fn close_all(&mut self) -> Result<RequestId, PaneError> {
        self.close_request(
            self.tabs().into_iter().map(|tab| tab.id).collect(),
            true,
            true,
        )
    }

    fn close_request(
        &mut self,
        tabs: Vec<TabId>,
        auto_commit: bool,
        all: bool,
    ) -> Result<RequestId, PaneError> {
        match self.app.execute_lifecycle(LifecycleAction::PrepareClose {
            targets: tabs,
            auto_commit,
            all,
        }) {
            LifecycleOutcome::Request(request) => Ok(request),
            LifecycleOutcome::Failed(error) => Err(error),
            _ => unreachable!("close request returns its correlation identity"),
        }
    }

    /// Take the prepared token once after receiving PreparedClose.
    pub fn take_prepared_close(&mut self, request: &RequestId) -> Result<PreparedClose, PaneError> {
        self.app.take_prepared_close(request)
    }

    /// Supply an explicit filename for the pending unnamed-buffer save.
    /// `None` cancels the whole close preparation.
    pub fn provide_save_path(
        &mut self,
        request: &RequestId,
        path: Option<&Path>,
    ) -> Result<(), PaneError> {
        Self::unit_outcome(
            self.app
                .execute_lifecycle(LifecycleAction::ProvideSavePath {
                    request: request.clone(),
                    path: path.map(PathBuf::from),
                }),
        )
    }

    /// Commit the captured close exactly once, in tab order.
    pub fn commit_close(&mut self, token: PreparedClose) -> Result<(), PaneError> {
        Self::unit_outcome(self.app.execute_lifecycle(LifecycleAction::CommitClose {
            request: token.token.request.clone(),
        }))
    }

    /// Abort a prepared close without closing any retained buffer.
    pub fn abort_close(&mut self, token: PreparedClose) -> Result<(), PaneError> {
        Self::unit_outcome(self.app.execute_lifecycle(LifecycleAction::AbortToken {
            request: token.token.request.clone(),
        }))
    }

    /// Suspend file I/O, leaving ordinary in-memory editing available.
    pub fn begin_external_change(&mut self) -> Result<ExternalChangeToken, PaneError> {
        match self
            .app
            .execute_lifecycle(LifecycleAction::BeginExternalChange)
        {
            LifecycleOutcome::External(token) => Ok(token),
            LifecycleOutcome::Failed(error) => Err(error),
            _ => unreachable!("external change returns its owned token"),
        }
    }

    /// Release suspension after the host operation and reconcile affected paths.
    pub fn commit_external_change(
        &mut self,
        token: ExternalChangeToken,
        paths: &[PathBuf],
    ) -> Result<(), PaneError> {
        Self::unit_outcome(
            self.app
                .execute_lifecycle(LifecycleAction::CommitExternalChange {
                    request: token.request.clone(),
                    paths: paths.to_vec(),
                }),
        )
    }

    /// Release suspension without acknowledging a host filesystem change.
    pub fn abort_external_change(&mut self, token: ExternalChangeToken) -> Result<(), PaneError> {
        Self::unit_outcome(self.app.execute_lifecycle(LifecycleAction::AbortToken {
            request: token.request.clone(),
        }))
    }

    fn unit_outcome(outcome: LifecycleOutcome) -> Result<(), PaneError> {
        match outcome {
            LifecycleOutcome::Applied | LifecycleOutcome::Request(_) => Ok(()),
            LifecycleOutcome::Failed(error) => Err(error),
            _ => unreachable!("lifecycle completion returns a unit outcome"),
        }
    }

    fn open_inner(
        &mut self,
        path: &Path,
        options: OpenOptions,
        existing_only: bool,
    ) -> Result<OpenOutcome, PaneError> {
        self.flush_lifecycle_events();
        match self.app.execute_lifecycle(LifecycleAction::HostOpen {
            path: path.to_path_buf(),
            options,
            existing_only,
        }) {
            LifecycleOutcome::Open(outcome) => {
                self.idle_complete = false;
                Ok(outcome)
            }
            LifecycleOutcome::Failed(error) => Err(error),
            _ => unreachable!("host open always returns an open outcome"),
        }
    }

    pub(crate) fn apply_open_options(session: &mut EditorSession, options: OpenOptions) {
        let offset = match options.cursor {
            OpenCursor::Start => 0,
            OpenCursor::SourceLine(line) => {
                let text = session.document();
                text.split_inclusive('\n')
                    .take(line)
                    .map(str::len)
                    .sum::<usize>()
                    .min(text.len())
            }
            OpenCursor::AfterFrontMatter => {
                let text = session.document();
                analyze_markdown(&text).body_span.start
            }
        };
        if offset != 0 {
            let _ = session.jump_to_offset(offset);
        }
        if options.enter_insert {
            let _ = session.handle_key(oom_edit_core::KeyInput {
                code: oom_edit_core::KeyCode {
                    kind: oom_edit_core::KeyCodeKind::Char('i'),
                },
                mods: oom_edit_core::Modifiers::default(),
            });
        }
    }

    fn flush_lifecycle_events(&mut self) {
        for event in self.app.drain_lifecycle_events() {
            if let PaneEvent::ThemeChanged { theme } = &event {
                self.theme_selection = ThemeSelection::new(
                    Some(theme.name.clone()),
                    theme.display_mode,
                    theme.capability,
                );
            }
            self.events.push(event);
        }
    }

    /// Feed one already-translated input at the host's post-read timestamp.
    pub fn handle_input(&mut self, input: PaneInput, now: Instant) -> InputDisposition {
        if !self.app.is_focused() || self.app.tab_count() == 0 {
            return InputDisposition::NotConsumed;
        }
        if let PaneInput::Key(key) = &input {
            if self.key_ownership(*key).disposition() == InputDisposition::NotConsumed {
                return InputDisposition::NotConsumed;
            }
        }
        self.flush_lifecycle_events();
        let before_active = self.app.active_tab_id();
        let before_mode = self.app.mode();
        match input {
            PaneInput::Key(key) => self.app.handle_key_input(key, now),
            PaneInput::Paste(text) => self.app.handle_paste(&text, now),
            PaneInput::Mouse(mouse) => self.app.handle_pane_mouse(mouse, now),
        }
        self.idle_complete = false;
        let after_active = self.app.active_tab_id();
        self.flush_lifecycle_events();
        if let Some(tab) = before_active {
            if after_active.as_ref() == Some(&tab) && self.app.mode() != before_mode {
                self.events.push(PaneEvent::ModeChanged {
                    tab,
                    mode: self.app.mode(),
                });
            }
        }
        InputDisposition::Consumed
    }

    /// Set the one focus authority for input, cursor, hints and safe reload.
    /// Call `tick(now)` before a focus transition to supply its timestamp.
    pub fn set_focused(&mut self, focused: bool) {
        self.app.set_focused(focused);
    }

    /// Content-validate tabs matching these paths or directories, even when
    /// metadata is unchanged. An empty list checks all backed tabs. Notifications
    /// never steal attention; application waits for a focused Normal safe point.
    /// Returns Busy without any file I/O while a host mutation token is held.
    pub fn notify_paths_changed(&mut self, paths: &[PathBuf]) -> Result<(), PaneError> {
        let result = self.app.notify_disk_paths(paths);
        if result.is_ok() {
            self.idle_complete = false;
        }
        self.flush_lifecycle_events();
        result
    }

    /// Report text-entry and modal ownership for host key routing.
    pub fn input_state(&self) -> PaneInputState {
        self.app.input_state()
    }

    /// Advance timers even when the host does not draw this pane.
    pub fn tick(&mut self, now: Instant) -> PaneTick {
        let result = self.app.tick(now);
        if result.redraw {
            self.idle_complete = false;
        }
        let idle_start = self
            .app
            .idle_start_deadline()
            .filter(|_| !self.idle_complete);
        let idle_due = idle_start.is_some_and(|deadline| now >= deadline);
        let next_deadline = result
            .deadline
            .into_iter()
            .chain(idle_start.filter(|deadline| *deadline > now))
            .min();
        PaneTick {
            next_deadline,
            redraw: result.redraw,
            idle_due,
        }
    }

    /// Perform one bounded spell/diagnostic work unit; the host enforces its 8 ms slice.
    pub fn idle_unit(&mut self, max_bytes: usize) -> PaneIdleResult {
        let worked = self.app.on_idle_unit(max_bytes);
        if max_bytes > 0 {
            self.idle_complete = !worked;
        }
        PaneIdleResult {
            worked,
            redraw: worked,
        }
    }

    /// Drain ordered events exactly once.
    pub fn drain_events(&mut self) -> Vec<PaneEvent> {
        self.flush_lifecycle_events();
        std::mem::take(&mut self.events)
    }
}

/// Resolve an existing target or the nearest existing parent before policy checks.
pub(crate) fn resolve_host_path(
    working_directory: &Path,
    path: &Path,
) -> Result<PathBuf, PaneError> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        working_directory.join(path)
    };
    let mut tail = Vec::new();
    let mut ancestor = absolute.as_path();
    // Resolve directly rather than adding a metadata-only existence probe at
    // each boundary. Permission and non-file errors must not look like absence.
    let mut canonical = loop {
        match ancestor.canonicalize() {
            Ok(canonical) => break canonical,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let name = ancestor.file_name().ok_or_else(|| PaneError {
                    kind: PaneErrorKind::InvalidPath,
                    path: Some(absolute.clone()),
                    detail: "path has no existing ancestor".to_string(),
                })?;
                tail.push(name.to_os_string());
                ancestor = ancestor.parent().ok_or_else(|| PaneError {
                    kind: PaneErrorKind::InvalidPath,
                    path: Some(absolute.clone()),
                    detail: "path has no parent".to_string(),
                })?;
            }
            Err(error) => {
                return Err(PaneError {
                    kind: PaneErrorKind::Io,
                    path: Some(absolute.clone()),
                    detail: error.to_string(),
                })
            }
        }
    };
    for part in tail.into_iter().rev() {
        canonical.push(part);
    }
    Ok(canonical)
}

impl PaneError {
    fn unknown_tab() -> Self {
        Self {
            kind: PaneErrorKind::UnknownTab,
            path: None,
            detail: "unknown or stale tab identity".to_string(),
        }
    }

    pub(crate) fn from_open_error(path: PathBuf, error: oom_edit_core::OpenError) -> Self {
        let kind = match &error {
            oom_edit_core::OpenError::NotUtf8(_) => PaneErrorKind::NotUtf8,
            oom_edit_core::OpenError::Io(source)
                if source.kind() == std::io::ErrorKind::NotFound =>
            {
                PaneErrorKind::Missing
            }
            oom_edit_core::OpenError::Io(_) => PaneErrorKind::Io,
        };
        Self {
            kind,
            path: Some(path),
            detail: error.to_string(),
        }
    }
}
