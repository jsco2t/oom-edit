use std::fs;
use std::path::PathBuf;

use oom_edit::{
    ClipboardCopyFormat, Config, ConfigPersistError, ConfigWarning, FileThemeSink,
    ThemePersistenceSink, ThemeSlot,
};

#[test]
fn public_config_roundtrips_as_a_host_subtable_with_all_fields() {
    let host: toml::Value = toml::from_str(
        r#"
[editor_pane]
relative_line_numbers = true
[editor_pane.editor]
wrap = false
wrap_width = 72
cursor_shapes = false
[editor_pane.clipboard]
copy_format = "plain-text"
[editor_pane.theme]
mode = "light"
dark = "default-dark"
light = "default-light"
[editor_pane.spell]
enabled = false
language = "en_CA"
additional_dictionaries = ["team.words"]
"#,
    )
    .unwrap();
    let config: Config = host.get("editor_pane").unwrap().clone().try_into().unwrap();
    assert!(config.relative_line_numbers);
    assert!(!config.editor.wrap);
    assert_eq!(config.editor.wrap_width, 72);
    assert!(!config.editor.cursor_shapes);
    assert_eq!(config.clipboard.copy_format, ClipboardCopyFormat::PlainText);
    assert_eq!(config.theme.mode.as_deref(), Some("light"));
    assert_eq!(config.theme.dark, "default-dark");
    assert_eq!(config.theme.light, "default-light");
    assert!(config.theme.dark_is_explicit());
    assert!(config.theme.light_is_explicit());
    assert_eq!(config.spell.language, "en_CA");
    assert!(!config.spell.enabled);
    assert_eq!(
        config.spell.additional_dictionaries,
        [PathBuf::from("team.words")]
    );
    let roundtrip: Config = toml::from_str(&toml::to_string(&config).unwrap()).unwrap();
    assert_eq!(roundtrip, config);
    let default_roundtrip: Config =
        toml::from_str(&toml::to_string(&Config::default()).unwrap()).unwrap();
    assert_eq!(default_roundtrip, Config::default());
}

#[test]
fn host_built_theme_slots_have_the_same_presence_as_written_slots() {
    let mut host = Config::default();
    host.theme.set_dark("default-dark".to_string());
    assert!(host.theme.dark_is_explicit());
    assert!(!host.theme.light_is_explicit());
    let file: Config = toml::from_str("[theme]\ndark = \"default-dark\"\n").unwrap();
    assert_eq!(host.theme.dark_is_explicit(), file.theme.dark_is_explicit());
    assert_eq!(
        host.theme.light_is_explicit(),
        file.theme.light_is_explicit()
    );
}

#[test]
fn public_validation_reports_exact_fallbacks_and_resolves_from_base() {
    let dir = tempfile::tempdir().unwrap();
    let base = dir.path().join("editor-config");
    fs::create_dir(&base).unwrap();
    let mut config = Config::default();
    config.editor.wrap_width = 0;
    config.theme.mode = Some("twilight".into());
    config.spell.language = "xx_XX".into();
    config
        .spell
        .additional_dictionaries
        .push(PathBuf::from("missing.words"));
    let result = config.validate(&base);
    assert_eq!(result.effective.editor.wrap_width, 100);
    assert_eq!(result.effective.theme.mode.as_deref(), Some("dark"));
    assert_eq!(result.effective.spell.language, "en_US");
    assert!(!result.effective.spell.enabled);
    assert_eq!(result.warnings.len(), 4);
    assert_eq!(result.warnings[0], ConfigWarning::WrapWidthZero);
    assert_eq!(
        result.warnings[1],
        ConfigWarning::InvalidThemeMode("twilight".into())
    );
    assert_eq!(
        result.warnings[2],
        ConfigWarning::UnknownSpellLanguage("xx_XX".into())
    );
    match &result.warnings[3] {
        ConfigWarning::UnreadableDictionary { path, detail } => {
            assert_eq!(path, &base.join("missing.words"));
            assert!(!detail.is_empty());
        }
        warning => panic!("wrong dictionary warning: {warning:?}"),
    }
    assert_eq!(config.editor.wrap_width, 0);
    assert_eq!(config.theme.mode.as_deref(), Some("twilight"));
    assert_eq!(config.spell.language, "xx_XX");
    assert_eq!(
        result.effective.additional_dictionary_paths(&base),
        [base.join("missing.words")]
    );
}

#[test]
fn file_theme_sink_preserves_unrelated_semantics_and_refuses_malformed_source() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    fs::write(
        &path,
        r#"unknown = "keep"
[theme]
dark = "default-dark"
light = "default-light"
[extra.nested]
number = 42
"#,
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();
    }
    let mut sink = FileThemeSink::new(path.clone());
    sink.persist_theme(ThemeSlot::Dark, "accessible").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o640
        );
    }
    let value: toml::Value = toml::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(value["theme"]["dark"].as_str(), Some("accessible"));
    assert_eq!(value["theme"]["light"].as_str(), Some("default-light"));
    assert_eq!(value["unknown"].as_str(), Some("keep"));
    assert_eq!(value["extra"]["nested"]["number"].as_integer(), Some(42));

    let malformed = b"[theme\ndark = 'broken'";
    fs::write(&path, malformed).unwrap();
    assert!(matches!(
        sink.persist_theme(ThemeSlot::Light, "accessible"),
        Err(ConfigPersistError::Malformed(_))
    ));
    assert_eq!(fs::read(&path).unwrap(), malformed);

    let wrong_type = b"[editor]\nwrap_width = 'many'\n";
    fs::write(&path, wrong_type).unwrap();
    assert!(matches!(
        sink.persist_theme(ThemeSlot::Dark, "accessible"),
        Err(ConfigPersistError::Malformed(_))
    ));
    assert_eq!(fs::read(&path).unwrap(), wrong_type);
}
