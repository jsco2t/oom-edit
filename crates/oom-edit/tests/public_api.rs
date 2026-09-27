use oom_edit::{
    AllowAllFileAccess, Args, ClipboardConfig, ClipboardCopyFormat, ClipboardError, ClipboardSink,
    ColorValue, CommandPolicy, Config, ConfigApplyReport, ConfigField, ConfigPersistError,
    ConfigValidation, ConfigWarning, DisplayMode, EditorConfig, EditorPane, FileAccessPolicy,
    FileOperation, FilePolicyError, FileThemeSink, InputDisposition, KeyCode, KeyCodeKind,
    KeyInput, Mode, Modifiers, OpenCursor, OpenOptions, OpenOutcome, Osc52Clipboard, OwnedStyle,
    PaletteKind, PaneCell, PaneConstruction, PaneConstructionReport, PaneCursor, PaneCursorShape,
    PaneError, PaneErrorKind, PaneEvent, PaneFrame, PaneIdleResult, PaneInit, PaneInput,
    PaneInputState, PaneMouse, PaneMouseKind, PaneOptions, PaneServices, PaneTick, PaneWarning,
    ParseOutcome, RequestId, ResolvedTheme, SemanticStyle, SpellConfig, StyleModifiers, TabId,
    TabSnapshot, TerminalError, TerminalGuard, TerminalGuardOptions, ThemeCatalog, ThemeConfig,
    ThemeEnvironment, ThemeLoadReport, ThemeLoadWarning, ThemeLoadWarningKind,
    ThemePersistenceSink, ThemeRole, ThemeSelection, ThemeSlot, ThemeSource, Tier, UiSlot,
};
use oom_edit::{
    BindingExecution, BindingId, BindingSequence, EditorBinding, EditorRuler, EditorStatus,
    HintCell, HostReservation, KeyOwnership, SpellStatus, WhichKey, WhichKeyEntry,
};
use oom_edit::{DiskChange, ExternalChangeToken, PreparedClose, PreparedRetarget, Retarget};

type SourceCursorQuery = fn(&EditorPane, &TabId) -> Result<(usize, usize), PaneError>;

#[test]
fn oom_edit_facade_exports_only_the_curated_host_api() {
    let _ = std::any::TypeId::of::<(
        BindingExecution,
        BindingId,
        BindingSequence,
        EditorBinding,
        EditorRuler,
        EditorStatus,
        HintCell,
        HostReservation,
        KeyOwnership,
        SpellStatus,
        WhichKey,
        WhichKeyEntry,
    )>();
    let _: fn(&EditorPane) -> Vec<HintCell> = EditorPane::hints;
    let _: fn(&EditorPane) -> Option<WhichKey> = EditorPane::which_key;
    let _: fn(&EditorPane) -> Vec<EditorBinding> = EditorPane::bindings;
    let _: fn(&EditorPane, KeyInput) -> KeyOwnership = EditorPane::key_ownership;
    let _: fn(KeyInput) -> Option<HostReservation> = EditorPane::host_reservation;
    let _: fn(&EditorPane) -> Option<EditorStatus> = EditorPane::status;
    let _: fn() -> ThemeCatalog = ThemeCatalog::builtins;
    let _: fn(&std::path::Path) -> ThemeLoadReport = ThemeCatalog::load_from_base;
    let _: fn(&ThemeCatalog) -> Vec<String> = ThemeCatalog::names;
    let _: fn(&ThemeCatalog, &str, Tier, SemanticStyle) -> Option<OwnedStyle> =
        ThemeCatalog::semantic_style;
    let _: fn(&ThemeCatalog, &str, Tier, UiSlot) -> Option<OwnedStyle> = ThemeCatalog::ui_style;
    let _: fn(&ThemeCatalog, &str, Tier, ThemeRole) -> Option<OwnedStyle> =
        ThemeCatalog::role_style;
    let _: fn(&ThemeCatalog, &str, DisplayMode) -> String = ThemeCatalog::cycle;
    let _: fn(&ThemeCatalog, &str, DisplayMode, Tier) -> Option<ResolvedTheme> =
        ThemeCatalog::resolve_name;
    let _: fn(&ThemeCatalog, &ThemeConfig, &ThemeSelection) -> ResolvedTheme =
        ThemeCatalog::resolve_explicit;
    let _: fn(std::io::Sink) -> Osc52Clipboard = Osc52Clipboard::new::<std::io::Sink>;
    let _: fn(PaneInit) -> PaneConstruction = EditorPane::construct;
    let _: fn(&mut EditorPane, u16, u16, std::time::Instant) -> PaneFrame = EditorPane::render;
    let _: fn(&mut EditorPane, u16, u16, std::time::Instant) = EditorPane::resize;
    let _: fn(&mut EditorPane, PaneInput, std::time::Instant) -> InputDisposition =
        EditorPane::handle_input;
    let _: fn(&mut EditorPane, std::time::Instant) -> PaneTick = EditorPane::tick;
    let _: fn(&mut EditorPane, usize) -> PaneIdleResult = EditorPane::idle_unit;
    let _: fn(&mut EditorPane, bool) = EditorPane::set_focused;
    let _: fn(&EditorPane) -> PaneInputState = EditorPane::input_state;
    let _: fn(&EditorPane) -> Vec<TabSnapshot> = EditorPane::tabs;
    let _: fn(&EditorPane) -> Option<TabId> = EditorPane::active_tab;
    let _: fn(&EditorPane, &TabId) -> Result<String, PaneError> = EditorPane::text;
    let _: SourceCursorQuery = EditorPane::source_cursor;
    let _: fn(&mut EditorPane, &TabId) -> Result<(), PaneError> = EditorPane::focus_tab;
    let _: fn(&mut EditorPane, &std::path::Path, OpenOptions) -> Result<OpenOutcome, PaneError> =
        EditorPane::open;
    let _: fn(&mut EditorPane, &std::path::Path, OpenOptions) -> Result<OpenOutcome, PaneError> =
        EditorPane::open_existing;
    let _: fn(&mut EditorPane, OpenOptions) -> Result<TabId, PaneError> = EditorPane::new_buffer;
    let _: fn(&mut EditorPane) -> Vec<PaneEvent> = EditorPane::drain_events;
    let _: fn(&mut EditorPane, &str) -> Result<(), PaneError> = EditorPane::set_theme;
    let _: fn(&mut EditorPane, Config) -> ConfigApplyReport = EditorPane::apply_config;
    let _ = std::any::TypeId::of::<oom_edit::DiagnosticSeverity>();
    let _ = std::any::TypeId::of::<(
        DiskChange,
        ExternalChangeToken,
        PreparedClose,
        PreparedRetarget,
        Retarget,
    )>();
    let _: fn(&mut EditorPane, &[TabId]) -> Result<RequestId, PaneError> =
        EditorPane::prepare_close;
    let _: fn(&mut EditorPane, &RequestId) -> Result<(), PaneError> = EditorPane::cancel_request;
    let _: fn(&mut EditorPane, &TabId) -> Result<RequestId, PaneError> = EditorPane::close;
    let _: fn(&mut EditorPane) -> Result<RequestId, PaneError> = EditorPane::close_all;
    let _: fn(&mut EditorPane, PreparedClose) -> Result<(), PaneError> = EditorPane::commit_close;
    let _: fn(&mut EditorPane, PreparedClose) -> Result<(), PaneError> = EditorPane::abort_close;
    let _: fn(&mut EditorPane, &RequestId) -> Result<PreparedClose, PaneError> =
        EditorPane::take_prepared_close;
    let _: fn(&mut EditorPane, &RequestId, Option<&std::path::Path>) -> Result<(), PaneError> =
        EditorPane::provide_save_path;
    let _: fn(&mut EditorPane, &[Retarget]) -> Result<PreparedRetarget, PaneError> =
        EditorPane::prepare_retarget;
    let _: fn(&mut EditorPane, PreparedRetarget) -> Result<(), PaneError> =
        EditorPane::commit_retarget;
    let _: fn(&mut EditorPane, PreparedRetarget) -> Result<(), PaneError> =
        EditorPane::abort_retarget;
    let _: fn(&mut EditorPane) -> Result<ExternalChangeToken, PaneError> =
        EditorPane::begin_external_change;
    let _: fn(
        &mut EditorPane,
        ExternalChangeToken,
        &[std::path::PathBuf],
    ) -> Result<(), PaneError> = EditorPane::commit_external_change;
    let _: fn(&mut EditorPane, ExternalChangeToken) -> Result<(), PaneError> =
        EditorPane::abort_external_change;
    let _: fn(&mut EditorPane, &[std::path::PathBuf]) -> Result<(), PaneError> =
        EditorPane::notify_paths_changed;
    let _: fn(&Args) -> Result<(), Box<dyn std::error::Error>> = oom_edit::run;
    let _ = std::any::TypeId::of::<ParseOutcome>();
    let _: fn(&Config, &std::path::Path) -> ConfigValidation = Config::validate;
    let _: fn(&Config, &std::path::Path) -> Vec<std::path::PathBuf> =
        Config::additional_dictionary_paths;
    let _: fn(std::path::PathBuf) -> FileThemeSink = FileThemeSink::new;
    let _: fn(&mut FileThemeSink, ThemeSlot, &str) -> Result<(), ConfigPersistError> =
        FileThemeSink::persist_theme;
    let _ = std::any::TypeId::of::<ClipboardConfig>();
    let _ = std::any::TypeId::of::<ClipboardCopyFormat>();
    let _ = std::any::TypeId::of::<ConfigWarning>();
    let _ = std::any::TypeId::of::<ConfigApplyReport>();
    let _ = std::any::TypeId::of::<ConfigField>();
    let _ = std::any::TypeId::of::<EditorConfig>();
    let _ = std::any::TypeId::of::<SpellConfig>();
    let _ = std::any::TypeId::of::<ThemeConfig>();
    let _ = std::any::TypeId::of::<ColorValue>();
    let _ = std::any::TypeId::of::<DisplayMode>();
    let _ = std::any::TypeId::of::<OwnedStyle>();
    let _ = std::any::TypeId::of::<PaletteKind>();
    let _ = std::any::TypeId::of::<ResolvedTheme>();
    let _ = std::any::TypeId::of::<SemanticStyle>();
    let _ = std::any::TypeId::of::<StyleModifiers>();
    let _ = std::any::TypeId::of::<PaneCell>();
    let _ = std::any::TypeId::of::<PaneCursor>();
    let _ = std::any::TypeId::of::<PaneCursorShape>();
    let _ = std::any::TypeId::of::<PaneFrame>();
    let _ = std::any::TypeId::of::<ThemeCatalog>();
    let _ = std::any::TypeId::of::<ThemeEnvironment>();
    let _ = std::any::TypeId::of::<ThemeLoadReport>();
    let _ = std::any::TypeId::of::<ThemeLoadWarning>();
    let _ = std::any::TypeId::of::<ThemeLoadWarningKind>();
    let _ = std::any::TypeId::of::<ThemeRole>();
    let _ = std::any::TypeId::of::<ThemeSelection>();
    let _ = std::any::TypeId::of::<ThemeSource>();
    let _ = std::any::TypeId::of::<TerminalError>();
    let _ = std::any::TypeId::of::<TerminalGuard>();
    let _ = std::any::TypeId::of::<TerminalGuardOptions>();
    let _ = std::any::TypeId::of::<Tier>();
    let _ = std::any::TypeId::of::<UiSlot>();
    let _ = std::any::TypeId::of::<AllowAllFileAccess>();
    let _ = std::any::TypeId::of::<ClipboardError>();
    let _ = std::any::TypeId::of::<dyn ClipboardSink>();
    let _ = std::any::TypeId::of::<CommandPolicy>();
    let _ = std::any::TypeId::of::<dyn FileAccessPolicy>();
    let _ = std::any::TypeId::of::<FileOperation>();
    let _ = std::any::TypeId::of::<FilePolicyError>();
    let _ = std::any::TypeId::of::<InputDisposition>();
    let _ = std::any::TypeId::of::<KeyCode>();
    let _ = std::any::TypeId::of::<KeyCodeKind>();
    let _ = std::any::TypeId::of::<KeyInput>();
    let _ = std::any::TypeId::of::<Mode>();
    let _ = std::any::TypeId::of::<Modifiers>();
    let _ = std::any::TypeId::of::<OpenCursor>();
    let _ = std::any::TypeId::of::<OpenOptions>();
    let _ = std::any::TypeId::of::<OpenOutcome>();
    let _ = std::any::TypeId::of::<Osc52Clipboard>();
    let _ = std::any::TypeId::of::<PaneConstruction>();
    let _ = std::any::TypeId::of::<PaneConstructionReport>();
    let _ = std::any::TypeId::of::<PaneError>();
    let _ = std::any::TypeId::of::<PaneErrorKind>();
    let _ = std::any::TypeId::of::<PaneEvent>();
    let _ = std::any::TypeId::of::<PaneIdleResult>();
    let _ = std::any::TypeId::of::<PaneInit>();
    let _ = std::any::TypeId::of::<PaneInput>();
    let _ = std::any::TypeId::of::<PaneInputState>();
    let _ = std::any::TypeId::of::<PaneMouse>();
    let _ = std::any::TypeId::of::<PaneMouseKind>();
    let _ = std::any::TypeId::of::<PaneOptions>();
    let _ = std::any::TypeId::of::<PaneServices>();
    let _ = std::any::TypeId::of::<PaneTick>();
    let _ = std::any::TypeId::of::<PaneWarning>();
    let _ = std::any::TypeId::of::<RequestId>();
    let _ = std::any::TypeId::of::<TabId>();
    let _ = std::any::TypeId::of::<TabSnapshot>();
    let _: fn(PaneInit) -> PaneConstruction = EditorPane::construct;
    let _: fn(&mut EditorPane, u16, u16, std::time::Instant) -> PaneFrame = EditorPane::render;
    let _: fn(&mut EditorPane, u16, u16, std::time::Instant) = EditorPane::resize;
    let _: fn(&mut EditorPane, &str) -> Result<(), PaneError> = EditorPane::set_theme;
    let _: fn(&mut EditorPane, Config) -> ConfigApplyReport = EditorPane::apply_config;
    let _: fn(&mut EditorPane, PaneInput, std::time::Instant) -> InputDisposition =
        EditorPane::handle_input;
    let _: fn(&mut EditorPane, &std::path::Path, OpenOptions) -> Result<OpenOutcome, PaneError> =
        EditorPane::open;
    let _: fn(&mut EditorPane, &TabId) -> Result<(), PaneError> = EditorPane::focus_tab;
    let _: fn(&mut EditorPane, OpenOptions) -> Result<TabId, PaneError> = EditorPane::new_buffer;
    let _: fn() -> &'static str = oom_edit::third_party_notices;

    let lib = include_str!("../src/lib.rs");
    let facade = lib
        .lines()
        .filter(|line| line.starts_with("pub use "))
        .collect::<Vec<_>>();
    assert_eq!(
        facade,
        [
            "pub use args::{Args, ParseOutcome};",
            "pub use clipboard::Osc52Clipboard;",
            "pub use config::{",
            "pub use oom_edit_core::{",
            "pub use owned_style::{ColorValue, OwnedStyle, StyleModifiers};",
            "pub use pane::{",
            "pub use pane_frame::{PaneCell, PaneCursor, PaneCursorShape, PaneFrame};",
            "pub use pane_metadata::{",
            "pub use terminal_guard::{TerminalError, TerminalGuard, TerminalGuardOptions};",
            "pub use theme::{",
            "pub use widgets::hint_bar::HintCell;",
        ]
    );
    assert!(!lib.contains("pub use app::"));
    let declarations = lib
        .split("pub use ")
        .skip(1)
        .map(|part| {
            format!(
                "pub use {};",
                part.split(';')
                    .next()
                    .unwrap()
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ")
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(declarations, [
        "pub use args::{Args, ParseOutcome};",
        "pub use clipboard::Osc52Clipboard;",
        "pub use config::{ ClipboardConfig, ClipboardCopyFormat, Config, ConfigApplyReport, ConfigField, ConfigPersistError, ConfigValidation, ConfigWarning, EditorConfig, FileThemeSink, SpellConfig, ThemeConfig, ThemePersistenceSink, ThemeSlot, };",
        "pub use oom_edit_core::{ ClipboardError, ClipboardSink, DiagnosticSeverity, KeyCode, KeyCodeKind, KeyInput, Mode, Modifiers, SemanticStyle, };",
        "pub use owned_style::{ColorValue, OwnedStyle, StyleModifiers};",
        "pub use pane::{ AllowAllFileAccess, CommandPolicy, DiskChange, EditorPane, ExternalChangeToken, FileAccessPolicy, FileOperation, FilePolicyError, InputDisposition, OpenCursor, OpenOptions, OpenOutcome, PaneConstruction, PaneConstructionReport, PaneError, PaneErrorKind, PaneEvent, PaneIdleResult, PaneInit, PaneInput, PaneInputState, PaneMouse, PaneMouseKind, PaneOptions, PaneServices, PaneTick, PaneWarning, PreparedClose, PreparedRetarget, RequestId, Retarget, TabId, TabSnapshot, };",
        "pub use pane_frame::{PaneCell, PaneCursor, PaneCursorShape, PaneFrame};",
        "pub use pane_metadata::{ BindingExecution, BindingId, BindingSequence, EditorBinding, EditorRuler, EditorStatus, HostReservation, KeyOwnership, SpellStatus, WhichKey, WhichKeyEntry, };",
        "pub use terminal_guard::{TerminalError, TerminalGuard, TerminalGuardOptions};",
        "pub use theme::{ DisplayMode, EnvParts as ThemeEnvironment, PaletteKind, ResolvedTheme, ThemeCatalog, ThemeLoadReport, ThemeLoadWarning, ThemeLoadWarningKind, ThemeRole, ThemeSelection, ThemeSource, Tier, UiSlot, };",
        "pub use widgets::hint_bar::HintCell;",
    ]);
    assert!(!lib.lines().any(|line| line.starts_with("pub mod ")));
    for module in [
        "app",
        "pane",
        "pane_frame",
        "pane_metadata",
        "config",
        "theme",
        "owned_style",
        "standalone",
        "command",
        "terminal_guard",
    ] {
        assert!(
            lib.contains(&format!("//! use oom_edit::{module}::")),
            "missing compile-fail privacy check for {module}"
        );
    }
    assert!(lib.contains("//!     let _ = pane.session_mut();"));
}
