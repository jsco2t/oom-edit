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
