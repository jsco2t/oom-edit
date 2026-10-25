use std::fs;

use oom_edit::{
    third_party_notices, ClipboardCopyFormat, ColorValue, Config, ConfigApplyReport, ConfigField,
    DisplayMode, SemanticStyle, ThemeCatalog, ThemeEnvironment, ThemeLoadWarningKind, ThemeRole,
    ThemeSelection, Tier, UiSlot,
};

#[test]
fn public_catalog_preserves_builtin_order_and_reports_user_theme_failures() {
    let builtins = ThemeCatalog::builtins();
    assert_eq!(
        builtins.names(),
        [
            "default-dark",
            "catppuccin-mocha",
            "dracula",
            "nord",
            "solarized-dark",
            "tokyo-night",
            "default-light",
            "accessible",
        ]
    );

    let directory = tempfile::tempdir().unwrap();
    let themes = directory.path().join("themes");
    fs::create_dir(&themes).unwrap();
    fs::write(themes.join("broken.toml"), "appearance = [").unwrap();
    fs::write(themes.join("custom-dark.toml"), valid_user_theme()).unwrap();
    let report = ThemeCatalog::load_from_base(directory.path());
    assert_eq!(report.warnings.len(), 1);
    assert_eq!(report.warnings[0].kind, ThemeLoadWarningKind::InvalidToml);
    assert_eq!(report.warnings[0].path, themes.join("broken.toml"));
    assert_eq!(report.catalog.names().last().unwrap(), "custom-dark");
    assert_eq!(
        report.catalog.cycle("accessible", DisplayMode::Dark),
        "custom-dark"
    );
}

#[test]
fn explicit_theme_inputs_and_environment_helper_keep_standalone_precedence() {
    let catalog = ThemeCatalog::builtins();
    let mut config = Config::default();
    config.theme.set_dark("nord".to_string());
    let explicit = ThemeSelection::new(None, DisplayMode::Dark, Tier::TrueColor);
    let resolved = catalog.resolve_explicit(&config.theme, &explicit);
    assert_eq!(resolved.name, "nord");
    assert_eq!(resolved.capability, Tier::TrueColor);

    let override_input = ThemeSelection::new(
        Some("dracula".to_string()),
        DisplayMode::Dark,
        Tier::Color16,
    );
    assert_eq!(
        catalog
            .resolve_explicit(&config.theme, &override_input)
            .name,
        "dracula"
    );

    let env = ThemeEnvironment {
        oom_edit_theme: Some("accessible".to_string()),
        no_color: false,
        term: Some("xterm-256color".to_string()),
        colorterm: Some("truecolor".to_string()),
        colorfgbg: Some("0;7".to_string()),
    };
    let selected = env.selection(config.theme.mode.as_deref(), None);
    assert_eq!(selected.override_name(), Some("accessible"));
    assert_eq!(selected.display_mode(), DisplayMode::Light);
    assert_eq!(selected.capability(), Tier::TrueColor);
    let resolved = catalog.resolve_explicit(&config.theme, &selected);
    assert_eq!(resolved.name, "accessible");
    assert_eq!(resolved.display_mode, DisplayMode::Light);
    assert_eq!(resolved.capability, Tier::TrueColor);
    assert_eq!(resolved.palette_kind.to_string(), "monochrome");
    assert_eq!(
        catalog
            .resolve_explicit(&config.theme, &env.selection(Some("dark"), Some("dracula")),)
            .name,
        "dracula"
    );
}

#[test]
fn owned_styles_keep_rgb_indexed_default_and_non_color_signals() {
    let catalog = ThemeCatalog::builtins();
    let rgb = catalog
        .semantic_style("default-dark", Tier::TrueColor, SemanticStyle::Heading1)
        .unwrap();
    assert!(matches!(rgb.foreground, Some(ColorValue::Rgb { .. })));
    assert!(rgb.modifiers.bold);
    let indexed = catalog
        .ui_style("default-dark", Tier::Color16, UiSlot::BadgeNormal)
        .unwrap();
    assert!(matches!(indexed.foreground, Some(ColorValue::Indexed(_))));
    assert!(indexed.modifiers.bold);

    let mono = catalog
        .semantic_style("default-dark", Tier::Monochrome, SemanticStyle::Heading1)
        .unwrap();
    assert!(matches!(mono.foreground, None | Some(ColorValue::Default)));
    assert!(matches!(mono.background, None | Some(ColorValue::Default)));
    assert!(matches!(
        mono.underline_color,
        None | Some(ColorValue::Default)
    ));
    assert!(mono.modifiers.bold);
    assert!(catalog
        .ui_style("missing", Tier::Color16, UiSlot::StatusBar)
        .is_none());
}

#[test]
fn every_public_host_role_is_available_at_each_builtin_tier() {
    let catalog = ThemeCatalog::builtins();
    let roles = [
        ThemeRole::Background,
        ThemeRole::BackgroundAlt,
        ThemeRole::GutterBackground,
        ThemeRole::GutterText,
        ThemeRole::GutterTextActive,
        ThemeRole::Surface,
        ThemeRole::SurfaceActive,
        ThemeRole::Border,
        ThemeRole::Text,
        ThemeRole::TextMuted,
        ThemeRole::TextEmphasis,
        ThemeRole::Primary,
        ThemeRole::Secondary,
        ThemeRole::Info,
        ThemeRole::Success,
        ThemeRole::Warning,
        ThemeRole::Error,
        ThemeRole::Attention,
    ];
    for name in catalog.names() {
        for tier in [Tier::TrueColor, Tier::Color16, Tier::Monochrome] {
            for role in roles {
                let style = catalog.role_style(&name, tier, role).unwrap();
                if tier == Tier::Monochrome {
                    assert!(matches!(style.foreground, None | Some(ColorValue::Default)));
                    assert!(matches!(style.background, None | Some(ColorValue::Default)));
                    assert!(matches!(
                        style.underline_color,
                        None | Some(ColorValue::Default)
                    ));
                }
            }
        }
    }
}

#[test]
fn public_notices_are_byte_identical_to_cli_output() {
    let cli = oom_edit::Args::parse(["oom-edit".to_string(), "--licenses".to_string()]).unwrap();
    let oom_edit::ParseOutcome::Message(message) = cli else {
        panic!("--licenses must return a message");
    };
    assert_eq!(third_party_notices().as_bytes(), message.as_bytes());
}

#[test]
fn config_change_report_distinguishes_live_and_new_pane_settings() {
    let old = Config::default();
    let mut changed = old.clone();
    changed.theme.mode = Some("light".to_string());
    changed.theme.set_dark("default-dark".to_string());
    changed.theme.set_light("default-light".to_string());
    changed.editor.wrap = false;
    changed.relative_line_numbers = true;
    changed.editor.cursor_shapes = false;
    changed.clipboard.copy_format = ClipboardCopyFormat::PlainText;
    changed.spell.enabled = false;
    changed.editor.wrap_width = 72;
    changed.spell.language = "en_CA".to_string();
    changed
        .spell
        .additional_dictionaries
        .push("team.words".into());

    let report = ConfigApplyReport::between(&old, &changed);
    assert_eq!(
        report.applied_now,
        [
            ConfigField::ThemeMode,
            ConfigField::ThemeDark,
            ConfigField::ThemeLight,
            ConfigField::Wrap,
            ConfigField::RelativeLineNumbers,
            ConfigField::CursorShapes,
            ConfigField::ClipboardCopyFormat,
            ConfigField::SpellEnabled,
        ]
    );
    assert_eq!(
        report.requires_new_pane,
        [
            ConfigField::WrapWidth,
            ConfigField::SpellLanguage,
            ConfigField::AdditionalDictionaries,
        ]
    );
    assert_eq!(
        ConfigApplyReport::between(&changed, &changed),
        ConfigApplyReport::default()
    );
}

fn valid_user_theme() -> &'static str {
    r##"appearance = "dark"

[palette]
background = "#101010"
background-alt = "#202020"
gutter-background = "#303030"
gutter-text = "#909090"
gutter-text-active = "#ffffff"
surface = "#252525"
surface-active = "#454545"
border = "#707070"
text = "#eeeeee"
text-muted = "#888888"
text-emphasis = "#ffffff"
primary = "#cc66ff"
secondary = "#66ccff"
info = "#3399ff"
success = "#33cc66"
warning = "#ffcc33"
error = "#ff3366"
attention = "#ff9933"
"##
}

fn terminal_rgb(color: ColorValue) -> [u8; 3] {
    match color {
        ColorValue::Rgb { red, green, blue } => [red, green, blue],
        ColorValue::Indexed(index) => {
            const ANSI: [[u8; 3]; 16] = [
                [0, 0, 0],
                [128, 0, 0],
                [0, 128, 0],
                [128, 128, 0],
                [0, 0, 128],
                [128, 0, 128],
                [0, 128, 128],
                [192, 192, 192],
                [128, 128, 128],
                [255, 0, 0],
                [0, 255, 0],
                [255, 255, 0],
                [0, 0, 255],
                [255, 0, 255],
                [0, 255, 255],
                [255, 255, 255],
            ];
            ANSI[usize::from(index)]
        }
        ColorValue::Default => panic!("a color-capable fence must have an explicit foreground"),
    }
}

fn relative_luminance(rgb: [u8; 3]) -> f64 {
    let linear = rgb.map(|channel| {
        let value = f64::from(channel) / 255.0;
        if value <= 0.04045 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    });
    linear[0] * 0.2126 + linear[1] * 0.7152 + linear[2] * 0.0722
}

#[test]
fn default_dark_fence_foreground_has_contrast_without_terminal_dimming() {
    let catalog = ThemeCatalog::builtins();
    for tier in [Tier::TrueColor, Tier::Color16] {
        let fence = catalog
            .semantic_style("default-dark", tier, SemanticStyle::Muted)
            .unwrap();
        let text = catalog
            .semantic_style("default-dark", tier, SemanticStyle::Text)
            .unwrap();
        assert!(
            !fence.modifiers.dim,
            "terminal dim must not undermine {tier:?} contrast"
        );
        let foreground = relative_luminance(terminal_rgb(fence.foreground.unwrap()));
        let body = relative_luminance(terminal_rgb(text.foreground.unwrap()));
        assert!(
            foreground < body,
            "fences should remain subdued relative to text"
        );
        let background = if tier == Tier::TrueColor {
            relative_luminance(terminal_rgb(
                catalog
                    .ui_style("default-dark", tier, UiSlot::CodeFence)
                    .unwrap()
                    .background
                    .unwrap(),
            ))
        } else {
            0.0
        };
        assert!(
            (foreground + 0.05) / (background + 0.05) >= 3.0,
            "insufficient {tier:?} contrast"
        );
    }
    let mono = catalog
        .semantic_style("default-dark", Tier::Monochrome, SemanticStyle::Muted)
        .unwrap();
    assert!(
        mono.modifiers.dim,
        "monochrome retains its existing non-color signal"
    );
}

#[test]
fn opening_and_closing_backticks_use_the_readable_style_in_owned_source_frames() {
    use oom_edit::{
        AllowAllFileAccess, ClipboardError, ClipboardSink, ConfigPersistError, EditorPane, KeyCode,
        KeyCodeKind, KeyInput, Modifiers, PaneInit, PaneInput, PaneOptions, PaneServices,
        ThemePersistenceSink, ThemeSlot,
    };
    use std::time::Instant;
    struct SilentClipboard;
    impl ClipboardSink for SilentClipboard {
        fn copy(&mut self, _: &str) -> Result<(), ClipboardError> {
            Ok(())
        }
    }
    struct SilentTheme;
    impl ThemePersistenceSink for SilentTheme {
        fn persist_theme(&mut self, _: ThemeSlot, _: &str) -> Result<(), ConfigPersistError> {
            Ok(())
        }
    }
    for tier in [Tier::TrueColor, Tier::Color16] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("fence.md");
        std::fs::write(&path, "```rust\nlet value = 1;\n```\n").unwrap();
        let now = Instant::now();
        let expected = ThemeCatalog::builtins()
            .semantic_style("default-dark", tier, SemanticStyle::Muted)
            .unwrap();
        let mut pane = EditorPane::construct(PaneInit {
            config: Config::default(),
            theme_catalog: ThemeCatalog::builtins(),
            theme_selection: ThemeSelection::new(None, DisplayMode::Dark, tier),
            services: PaneServices {
                clipboard_sink: Box::new(SilentClipboard),
                theme_sink: Box::new(SilentTheme),
                file_access_policy: Box::new(AllowAllFileAccess),
                config_base_directory: directory.path().into(),
                personal_dictionary_path: directory.path().join("words"),
                working_directory: directory.path().into(),
            },
            options: PaneOptions::default(),
            initial_paths: vec![path],
            now,
        })
        .pane;
        pane.render(80, 12, now);
        pane.handle_input(
            PaneInput::Key(KeyInput {
                code: KeyCode {
                    kind: KeyCodeKind::Char('i'),
                },
                mods: Modifiers::default(),
            }),
            now,
        );
        let frame = pane.render(80, 12, now);
        let backticks: Vec<_> = frame
            .cells
            .iter()
            .filter(|cell| cell.symbol == "`")
            .collect();
        assert_eq!(
            backticks.len(),
            6,
            "both fences must remain literal in Insert"
        );
        for cell in backticks {
            assert_eq!(cell.style.foreground, expected.foreground);
            assert!(
                !cell.style.modifiers.dim,
                "fence glyph must not inherit dim"
            );
        }
    }
}
