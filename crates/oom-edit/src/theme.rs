//! Theme system — maps core [`SemanticStyle`] slots to ratatui [`Style`].
//!
//! Three capability tiers: `TrueColor` (16M), `Color16` (ANSI 16), `Monochrome`
//! (modifiers only). Every accessor always carries a modifier so no signal is
//! color-only (NFR-7).
//!
//! Selection ladder (highest-priority first):
//! `OOM_EDIT_THEME=accessible` | `NO_COLOR` | `TERM=dumb` → Monochrome, stop;
//! `COLORTERM` truecolor/24bit → TrueColor tier else Color16;
//! light/dark from config `mode` override else `COLORFGBG` heuristic else dark.
//!
//! **No color-only signals:** every style carries a modifier (bold, italic,
//! reverse, underline) so it is visible in monochrome terminals too.

use oom_edit_core::SemanticStyle;
use ratatui::style::{Color, Modifier, Style};
use std::fmt;

#[cfg(test)]
pub(crate) const ZED_UI_TEXT: Color = Color::Rgb(200, 204, 212);
#[cfg(test)]
pub(crate) const ZED_CYAN: Color = Color::Rgb(110, 180, 191);
#[cfg(test)]
pub(crate) const ZED_ORANGE: Color = Color::Rgb(191, 149, 106);
#[cfg(test)]
pub(crate) const TEST_EXACT_BLACK: Color = Color::Rgb(0, 0, 0);

// ── UI Slots ────────────────────────────────────────────────────────────────

/// UI-specific display slots used by the status bar, hint bar, gutter, etc.
///
/// These complement the [`SemanticStyle`] slots emitted by the core.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UiSlot {
    /// Full bottom application-row surface.
    StatusBar,
    /// Normal mode badge.
    BadgeNormal,
    /// Insert mode badge.
    BadgeInsert,
    /// Select mode badge.
    BadgeSelect,
    /// Command mode badge.
    BadgeCommand,
    /// Border / separator.
    Border,
    /// Hint bar key label.
    HintKey,
    /// Hint bar description.
    HintDesc,
    /// Status bar success message.
    StatusSuccess,
    /// Status bar info message.
    StatusInfo,
    /// Status bar warning message.
    StatusWarning,
    /// Status bar error message.
    StatusError,
    /// Gutter background.
    Gutter,
    /// Gutter current-line highlight.
    GutterCurrent,
    /// Full-width cursor-line chrome in the source editor.
    CursorLine,
    /// Renderer-owned surface behind YAML/TOML metadata rows.
    MetadataPanel,
    /// Active tab label.
    TabActive,
    /// Inactive tab label.
    TabInactive,
    /// Separator between tab labels.
    TabSeparator,
}

// ── Palette tiers ───────────────────────────────────────────────────────────

/// A color palette for one capability tier.
#[derive(Debug, Clone)]
pub enum Palette {
    /// Full TrueColor (16M) palette.
    TrueColor {
        /// Semantic style → (foreground, modifiers).
        semantic: &'static [(SemanticStyle, Color, Modifier)],
        /// UI slot → (foreground, background, modifiers). `Reset` foreground
        /// inherits the styled cell's existing semantic foreground.
        ui: &'static [(UiSlot, Color, Option<Color>, Modifier)],
    },
    /// 16-color ANSI palette.
    Color16 {
        semantic: &'static [(SemanticStyle, Color, Modifier)],
        ui: &'static [(UiSlot, Color, Option<Color>, Modifier)],
    },
    /// Monochrome: foreground is `Color::Reset` (ignored), all signal in modifiers.
    Monochrome {
        semantic: &'static [(SemanticStyle, Modifier)],
        ui: &'static [(UiSlot, Modifier)],
    },
}

#[expect(dead_code)]
impl Palette {
    /// Resolve a [`SemanticStyle`] to a ratatui [`Style`].
    pub fn resolve_semantic(&self, style: SemanticStyle) -> Style {
        match self {
            Palette::TrueColor { semantic, .. } | Palette::Color16 { semantic, .. } => {
                for &(s, fg, modif) in semantic.iter() {
                    if s == style {
                        return Style::default().fg(fg).add_modifier(modif);
                    }
                }
                Style::default().add_modifier(Modifier::BOLD)
            }
            Palette::Monochrome { semantic, .. } => {
                for &(s, modif) in semantic.iter() {
                    if s == style {
                        return Style::default().fg(Color::Reset).add_modifier(modif);
                    }
                }
                Style::default()
                    .fg(Color::Reset)
                    .add_modifier(Modifier::BOLD)
            }
        }
    }

    /// Resolve a [`UiSlot`] to a ratatui [`Style`].
    pub fn resolve_ui(&self, slot: UiSlot) -> Style {
        match self {
            Palette::TrueColor { ui, .. } | Palette::Color16 { ui, .. } => {
                for &(s, fg, bg, modif) in ui.iter() {
                    if s == slot {
                        let mut s = Style::default().add_modifier(modif);
                        // Reset is the palette sentinel for chrome that must
                        // preserve an existing cell's semantic foreground.
                        if fg != Color::Reset {
                            s = s.fg(fg);
                        }
                        if let Some(bg) = bg {
                            s = s.bg(bg);
                        }
                        return s;
                    }
                }
                Style::default().add_modifier(Modifier::BOLD)
            }
            Palette::Monochrome { ui, .. } => {
                for &(s, modif) in ui.iter() {
                    if s == slot {
                        return Style::default().fg(Color::Reset).add_modifier(modif);
                    }
                }
                Style::default()
                    .fg(Color::Reset)
                    .add_modifier(Modifier::BOLD)
            }
        }
    }

    /// Does this palette have any true foreground colors (not Reset)?
    fn has_colors(&self) -> bool {
        match self {
            Palette::Monochrome { .. } => false,
            Palette::TrueColor { semantic, ui } | Palette::Color16 { semantic, ui, .. } => {
                semantic.iter().any(|&(_, fg, _)| fg != Color::Reset)
                    || ui.iter().any(|&(_, fg, _, _)| fg != Color::Reset)
            }
        }
    }
}

// ── Theme ───────────────────────────────────────────────────────────────────

/// A named theme with a palette for each capability tier.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct Theme {
    /// The theme's display name (e.g. "default-dark", "accessible").
    pub name: &'static str,
    /// TrueColor palette.
    pub truecolor: Palette,
    /// 16-color palette.
    pub color16: Palette,
    /// Monochrome palette.
    pub monochrome: Palette,
}

impl Theme {
    /// Resolve a [`SemanticStyle`] to a ratatui [`Style`] for the active tier.
    pub fn style(&self, tier: Tier, style: SemanticStyle) -> Style {
        match tier {
            Tier::TrueColor => self.truecolor.resolve_semantic(style),
            Tier::Color16 => self.color16.resolve_semantic(style),
            Tier::Monochrome => self.monochrome.resolve_semantic(style),
        }
    }

    /// Resolve a [`UiSlot`] to a ratatui [`Style`] for the active tier.
    #[allow(dead_code)]
    pub fn ui_style(&self, tier: Tier, slot: UiSlot) -> Style {
        match tier {
            Tier::TrueColor => self.truecolor.resolve_ui(slot),
            Tier::Color16 => self.color16.resolve_ui(slot),
            Tier::Monochrome => self.monochrome.resolve_ui(slot),
        }
    }

    /// Get the palette for the active tier (for completeness tests).
    #[allow(dead_code)]
    pub fn palette_for(&self, tier: Tier) -> &Palette {
        match tier {
            Tier::TrueColor => &self.truecolor,
            Tier::Color16 => &self.color16,
            Tier::Monochrome => &self.monochrome,
        }
    }
}

/// Capability tier: the terminal's color capability.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Tier {
    /// TrueColor (16M colors).
    TrueColor,
    /// 16-color ANSI.
    Color16,
    /// Monochrome (modifiers only).
    Monochrome,
}

/// Resolved light/dark display mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisplayMode {
    /// Dark terminal surface.
    Dark,
    /// Light terminal surface.
    Light,
}

/// Effective palette kind supplied by the active theme at the capability tier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaletteKind {
    /// RGB true-color palette.
    TrueColor,
    /// ANSI 16-color palette.
    Color16,
    /// Color-free palette.
    Monochrome,
}

impl fmt::Display for PaletteKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::TrueColor => "truecolor",
            Self::Color16 => "color16",
            Self::Monochrome => "monochrome",
        })
    }
}

/// Winning source for active-theme selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeSource {
    /// Explicit `--theme` CLI flag.
    Cli,
    /// `OOM_EDIT_THEME` environment override.
    Environment,
    /// Dark-mode persisted slot.
    ConfigDark,
    /// Light-mode persisted slot.
    ConfigLight,
    /// Built-in display-mode fallback.
    Fallback,
}

impl fmt::Display for ThemeSource {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Cli => "--theme",
            Self::Environment => "environment",
            Self::ConfigDark => "config.dark",
            Self::ConfigLight => "config.light",
            Self::Fallback => "fallback",
        })
    }
}

/// Complete, pure result of startup theme resolution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedTheme {
    /// Active built-in theme name.
    pub name: String,
    /// Light/dark display mode.
    pub display_mode: DisplayMode,
    /// Terminal/environment color capability.
    pub capability: Tier,
    /// Palette actually supplied by the active theme.
    pub palette_kind: PaletteKind,
    /// Winning selection source.
    pub source: ThemeSource,
}

impl ResolvedTheme {
    /// Whether the resolved display mode is light.
    pub fn is_light(&self) -> bool {
        self.display_mode == DisplayMode::Light
    }

    /// Construct a fully coherent resolved value for injected hosts/tests
    /// that already chose a built-in name, display mode, and capability.
    #[cfg(test)]
    pub(crate) fn injected(name: &str, is_light: bool, capability: Tier) -> Self {
        let palette_kind = match get_theme(name).palette_for(capability) {
            Palette::TrueColor { .. } => PaletteKind::TrueColor,
            Palette::Color16 { .. } => PaletteKind::Color16,
            Palette::Monochrome { .. } => PaletteKind::Monochrome,
        };
        Self {
            name: name.to_string(),
            display_mode: if is_light {
                DisplayMode::Light
            } else {
                DisplayMode::Dark
            },
            capability,
            palette_kind,
            source: ThemeSource::Fallback,
        }
    }
}

impl fmt::Display for ResolvedTheme {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "theme={} palette={} capability={:?} source={} {}",
            self.name,
            self.palette_kind,
            self.capability,
            self.source,
            if self.is_light() { "light" } else { "dark" }
        )
    }
}

// ── Selection ladder ────────────────────────────────────────────────────────

/// Environment parts for testing the selection ladder without touching real env vars.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct EnvParts {
    /// Value of `OOM_EDIT_THEME`.
    pub(crate) oom_edit_theme: Option<String>,
    /// Value of `NO_COLOR`.
    pub(crate) no_color: bool,
    /// Value of `TERM`.
    pub(crate) term: Option<String>,
    /// Value of `COLORTERM`.
    pub(crate) colorterm: Option<String>,
    /// Value of `COLORFGBG` (e.g. "0;7" for light, "7;0" for dark).
    pub(crate) colorfgbg: Option<String>,
}

impl EnvParts {
    pub(crate) fn from_current_process() -> Self {
        Self::from_lookup(|name| std::env::var(name).ok())
    }

    fn from_lookup(mut lookup: impl FnMut(&str) -> Option<String>) -> Self {
        Self {
            oom_edit_theme: lookup("OOM_EDIT_THEME"),
            no_color: lookup("NO_COLOR").is_some(),
            term: lookup("TERM"),
            colorterm: lookup("COLORTERM"),
            colorfgbg: lookup("COLORFGBG"),
        }
    }

    /// Determine terminal/environment capability without conflating it with
    /// the selected theme's effective palette.
    pub fn capability(&self) -> Tier {
        if self.no_color || self.term.as_deref() == Some("dumb") {
            return Tier::Monochrome;
        }
        if let Some(colorterm) = self.colorterm.as_deref() {
            let colorterm = colorterm.to_lowercase();
            if colorterm.contains("truecolor") || colorterm.contains("24bit") {
                return Tier::TrueColor;
            }
        }
        Tier::Color16
    }

    /// Determine the effective tier from environment.
    #[cfg(test)]
    pub fn effective_tier(&self) -> Tier {
        // OOM_EDIT_THEME=accessible forces monochrome (stop here).
        if let Some(theme) = self.oom_edit_theme.as_deref() {
            if theme == "accessible" {
                return Tier::Monochrome;
            }
        }
        self.capability()
    }

    /// Determine light vs dark mode preference.
    ///
    /// Returns `true` for light, `false` for dark.
    pub fn is_light(&self, config_mode: Option<&str>) -> bool {
        // Config `mode` override takes priority.
        if let Some(mode) = config_mode {
            return mode == "light";
        }
        // COLORFGBG heuristic: "foreground;background".
        // Light theme: bg > fg (e.g. "0;7" = black fg, white bg).
        // Dark theme: fg > bg (e.g. "7;0" = white fg, black bg).
        if let Some(colorfgbg) = self.colorfgbg.as_deref() {
            if let Some((fg, bg)) = colorfgbg.split_once(';') {
                if let (Ok(fg_val), Ok(bg_val)) = (fg.parse::<u32>(), bg.parse::<u32>()) {
                    return bg_val > fg_val;
                }
            }
        }
        // Default: dark.
        false
    }
}

/// Resolve the active theme name through the selection ladder.
///
/// Priority: `--theme` flag > compatible config dark/light slot > the display
/// mode's built-in default.
/// The returned value is the sole input for startup diagnostics and App theme
/// construction, so selection provenance cannot drift from presentation.
pub fn resolve_theme(
    cli_theme: Option<&str>,
    config_mode: Option<&str>,
    config_dark: Option<&str>,
    config_light: Option<&str>,
    env: &EnvParts,
) -> ResolvedTheme {
    let is_light = env.is_light(config_mode);

    // Determine theme name: CLI flag > config slot > default.
    let fallback = if is_light {
        "default-light"
    } else {
        "default-dark"
    };
    let configured = if is_light { config_light } else { config_dark };
    let (name, source): (String, ThemeSource) = match cli_theme {
        Some(cli) if is_known(cli) => (cli.to_string(), ThemeSource::Cli),
        Some(cli) => {
            eprintln!("oom-edit: unknown theme '{cli}', using {fallback}");
            (fallback.to_string(), ThemeSource::Fallback)
        }
        None => match env.oom_edit_theme.as_deref().filter(|name| is_known(name)) {
            Some(name) => (name.to_string(), ThemeSource::Environment),
            None => match configured.filter(|name| is_known(name) && supports_mode(name, is_light))
            {
                Some(name) => (
                    name.to_string(),
                    if is_light {
                        ThemeSource::ConfigLight
                    } else {
                        ThemeSource::ConfigDark
                    },
                ),
                None => (fallback.to_string(), ThemeSource::Fallback),
            },
        },
    };

    let capability = env.capability();
    let palette_kind = match get_theme(&name).palette_for(capability) {
        Palette::TrueColor { .. } => PaletteKind::TrueColor,
        Palette::Color16 { .. } => PaletteKind::Color16,
        Palette::Monochrome { .. } => PaletteKind::Monochrome,
    };

    ResolvedTheme {
        name,
        display_mode: if is_light {
            DisplayMode::Light
        } else {
            DisplayMode::Dark
        },
        capability,
        palette_kind,
        source,
    }
}

#[derive(Debug, Clone, Copy)]
struct BuiltinThemeSpec {
    theme: &'static Theme,
    compatible_mode: Option<DisplayMode>,
}

static BUILTIN_THEMES: &[BuiltinThemeSpec] = &[
    BuiltinThemeSpec {
        theme: &DEFAULT_DARK,
        compatible_mode: Some(DisplayMode::Dark),
    },
    BuiltinThemeSpec {
        theme: &DEFAULT_LIGHT,
        compatible_mode: Some(DisplayMode::Light),
    },
    BuiltinThemeSpec {
        theme: &ACCESSIBLE,
        compatible_mode: None,
    },
];

/// Get the built-in theme by name.
pub fn get_theme(name: &str) -> &'static Theme {
    BUILTIN_THEMES
        .iter()
        .find(|spec| spec.theme.name == name)
        .unwrap_or(&BUILTIN_THEMES[0])
        .theme
}

/// Get the list of built-in theme names.
#[cfg(test)]
pub fn built_in_themes() -> impl ExactSizeIterator<Item = &'static str> + Clone {
    BUILTIN_THEMES.iter().map(|spec| spec.theme.name)
}

fn is_known(name: &str) -> bool {
    BUILTIN_THEMES.iter().any(|spec| spec.theme.name == name)
}

fn supports_mode(name: &str, is_light: bool) -> bool {
    let mode = if is_light {
        DisplayMode::Light
    } else {
        DisplayMode::Dark
    };
    BUILTIN_THEMES.iter().any(|spec| {
        spec.theme.name == name
            && spec
                .compatible_mode
                .is_none_or(|compatible| compatible == mode)
    })
}

/// Cycle to the next built-in theme compatible with the display mode.
/// Returns the mode's default when `current` is unknown or incompatible.
pub fn cycle_theme(current: &str, is_light: bool) -> &'static str {
    let mode = if is_light {
        DisplayMode::Light
    } else {
        DisplayMode::Dark
    };
    let compatible = |spec: &&BuiltinThemeSpec| {
        spec.compatible_mode
            .is_none_or(|compatible| compatible == mode)
    };
    let fallback = BUILTIN_THEMES
        .iter()
        .find(compatible)
        .expect("each display mode has a built-in theme")
        .theme
        .name;
    let Some(current_index) = BUILTIN_THEMES
        .iter()
        .position(|spec| spec.theme.name == current && compatible(&spec))
    else {
        return fallback;
    };
    BUILTIN_THEMES
        .iter()
        .cycle()
        .skip(current_index + 1)
        .find(compatible)
        .expect("compatible theme cycle is nonempty")
        .theme
        .name
}

// ── Built-in: default-dark ──────────────────────────────────────────────────

#[expect(dead_code)]
fn heading_fg_dark(tier: &mut Vec<Color>) -> Color {
    // TrueColor uses yellow; Color16 uses Yellow.
    match tier.len() {
        0 => Color::Yellow,
        _ => tier.remove(0),
    }
}

/// Heading ladder for dark theme: each level gets a distinct color.
#[expect(dead_code)]
fn heading_colors_dark() -> [Color; 6] {
    [
        Color::Yellow,
        Color::Yellow,
        Color::Cyan,
        Color::Cyan,
        Color::Magenta,
        Color::Magenta,
    ]
}

pub static DEFAULT_DARK: Theme = Theme {
    name: "default-dark",
    truecolor: Palette::TrueColor {
        semantic: &[
            (
                SemanticStyle::Text,
                Color::Rgb(200, 204, 212),
                Modifier::empty(),
            ),
            (
                SemanticStyle::Heading1,
                Color::Rgb(110, 180, 191),
                Modifier::BOLD,
            ),
            (
                SemanticStyle::Heading2,
                Color::Rgb(191, 149, 106),
                Modifier::BOLD,
            ),
            (
                SemanticStyle::Heading3,
                Color::Rgb(223, 193, 132),
                Modifier::BOLD,
            ),
            (
                SemanticStyle::Heading4,
                Color::Rgb(161, 193, 129),
                Modifier::BOLD,
            ),
            (
                SemanticStyle::Heading5,
                Color::Rgb(110, 180, 191),
                Modifier::BOLD,
            ),
            (
                SemanticStyle::Heading6,
                Color::Rgb(180, 119, 207),
                Modifier::BOLD,
            ),
            (
                SemanticStyle::Emphasis,
                Color::Rgb(180, 119, 207),
                Modifier::ITALIC,
            ),
            (
                SemanticStyle::Strong,
                Color::Rgb(223, 193, 132),
                Modifier::BOLD,
            ),
            (
                SemanticStyle::Strikethrough,
                Color::Rgb(200, 204, 212),
                Modifier::empty(),
            ),
            (
                SemanticStyle::CodeSpan,
                Color::Rgb(161, 193, 129),
                Modifier::empty(),
            ),
            (
                SemanticStyle::CodeBlock,
                Color::Rgb(161, 193, 129),
                Modifier::empty(),
            ),
            (
                SemanticStyle::Quote,
                Color::Rgb(223, 193, 132),
                Modifier::empty(),
            ),
            (
                SemanticStyle::ListMarker,
                Color::Rgb(208, 114, 119),
                Modifier::empty(),
            ),
            (
                SemanticStyle::Link,
                Color::Rgb(180, 119, 207),
                Modifier::UNDERLINED,
            ),
            (
                SemanticStyle::LinkUrl,
                Color::Rgb(110, 180, 191),
                Modifier::UNDERLINED,
            ),
            (
                SemanticStyle::Rule,
                Color::Rgb(59, 64, 72),
                Modifier::empty(),
            ),
            (
                SemanticStyle::HtmlRaw,
                Color::Rgb(93, 99, 111),
                Modifier::empty(),
            ),
            (
                SemanticStyle::FmDelimiter,
                Color::Rgb(59, 64, 72),
                Modifier::empty(),
            ),
            (
                SemanticStyle::FmKey,
                Color::Rgb(223, 193, 132),
                Modifier::BOLD,
            ),
            (
                SemanticStyle::FmValue,
                Color::Rgb(161, 193, 129),
                Modifier::empty(),
            ),
            (
                SemanticStyle::Keyword,
                Color::Rgb(180, 119, 207),
                Modifier::BOLD,
            ),
            (
                SemanticStyle::Function,
                Color::Rgb(115, 173, 233),
                Modifier::empty(),
            ),
            (
                SemanticStyle::TypeName,
                Color::Rgb(110, 180, 191),
                Modifier::empty(),
            ),
            (
                SemanticStyle::StringLit,
                Color::Rgb(161, 193, 129),
                Modifier::empty(),
            ),
            (
                SemanticStyle::NumberLit,
                Color::Rgb(191, 149, 106),
                Modifier::empty(),
            ),
            (
                SemanticStyle::Comment,
                Color::Rgb(93, 99, 111),
                Modifier::ITALIC,
            ),
            (
                SemanticStyle::Operator,
                Color::Rgb(200, 204, 212),
                Modifier::empty(),
            ),
            (
                SemanticStyle::Variable,
                Color::Rgb(200, 204, 212),
                Modifier::empty(),
            ),
            (
                SemanticStyle::Punct,
                Color::Rgb(200, 204, 212),
                Modifier::empty(),
            ),
            (SemanticStyle::Selection, Color::Reset, Modifier::REVERSED),
            (
                SemanticStyle::Match,
                Color::Rgb(115, 173, 233),
                Modifier::UNDERLINED,
            ),
            (SemanticStyle::CursorLine, Color::Reset, Modifier::empty()),
            (SemanticStyle::Muted, Color::Rgb(59, 64, 72), Modifier::DIM),
        ],
        ui: &[
            (
                UiSlot::StatusBar,
                Color::Rgb(200, 204, 212),
                Some(Color::Rgb(47, 52, 62)),
                Modifier::DIM,
            ),
            (
                UiSlot::BadgeNormal,
                Color::Rgb(0, 0, 0),
                Some(Color::Rgb(115, 173, 233)),
                Modifier::BOLD,
            ),
            (
                UiSlot::BadgeInsert,
                Color::Rgb(0, 0, 0),
                Some(Color::Rgb(161, 193, 129)),
                Modifier::BOLD,
            ),
            (
                UiSlot::BadgeSelect,
                Color::Rgb(0, 0, 0),
                Some(Color::Rgb(180, 119, 207)),
                Modifier::BOLD,
            ),
            (
                UiSlot::BadgeCommand,
                Color::Rgb(0, 0, 0),
                Some(Color::Rgb(223, 193, 132)),
                Modifier::BOLD,
            ),
            (
                UiSlot::Border,
                Color::Rgb(47, 52, 62),
                None,
                Modifier::empty(),
            ),
            (
                UiSlot::HintKey,
                Color::Rgb(223, 193, 132),
                None,
                Modifier::BOLD,
            ),
            (
                UiSlot::HintDesc,
                Color::Rgb(200, 204, 212),
                None,
                Modifier::empty(),
            ),
            (
                UiSlot::StatusSuccess,
                Color::Rgb(161, 193, 129),
                None,
                Modifier::empty(),
            ),
            (
                UiSlot::StatusInfo,
                Color::Rgb(115, 173, 233),
                None,
                Modifier::empty(),
            ),
            (
                UiSlot::StatusWarning,
                Color::Rgb(223, 193, 132),
                None,
                Modifier::empty(),
            ),
            (
                UiSlot::StatusError,
                Color::Rgb(208, 114, 119),
                None,
                Modifier::empty(),
            ),
            (UiSlot::Gutter, Color::Rgb(93, 99, 111), None, Modifier::DIM),
            (
                UiSlot::GutterCurrent,
                Color::Rgb(200, 204, 212),
                None,
                Modifier::BOLD,
            ),
            (
                UiSlot::CursorLine,
                Color::Reset,
                Some(Color::Rgb(47, 52, 62)),
                Modifier::DIM,
            ),
            (
                UiSlot::TabActive,
                Color::Rgb(200, 204, 212),
                None,
                Modifier::BOLD,
            ),
            (
                UiSlot::TabInactive,
                Color::Rgb(93, 99, 111),
                None,
                Modifier::DIM,
            ),
            (
                UiSlot::MetadataPanel,
                Color::Reset,
                Some(Color::Rgb(47, 52, 62)),
                Modifier::DIM,
            ),
            (
                UiSlot::TabSeparator,
                Color::Rgb(47, 52, 62),
                None,
                Modifier::DIM,
            ),
        ],
    },
    color16: Palette::Color16 {
        semantic: &[
            (SemanticStyle::Text, Color::White, Modifier::empty()),
            (SemanticStyle::Heading1, Color::Yellow, Modifier::BOLD),
            (SemanticStyle::Heading2, Color::Yellow, Modifier::BOLD),
            (SemanticStyle::Heading3, Color::Cyan, Modifier::BOLD),
            (SemanticStyle::Heading4, Color::Cyan, Modifier::BOLD),
            (SemanticStyle::Heading5, Color::Magenta, Modifier::BOLD),
            (SemanticStyle::Heading6, Color::Magenta, Modifier::BOLD),
            (SemanticStyle::Emphasis, Color::White, Modifier::ITALIC),
            (SemanticStyle::Strong, Color::White, Modifier::BOLD),
            (SemanticStyle::Strikethrough, Color::Gray, Modifier::empty()),
            (SemanticStyle::CodeSpan, Color::Green, Modifier::empty()),
            (SemanticStyle::CodeBlock, Color::Green, Modifier::empty()),
            (SemanticStyle::Quote, Color::Yellow, Modifier::empty()),
            (SemanticStyle::ListMarker, Color::Cyan, Modifier::empty()),
            (SemanticStyle::Link, Color::Cyan, Modifier::UNDERLINED),
            (SemanticStyle::LinkUrl, Color::DarkGray, Modifier::empty()),
            (SemanticStyle::Rule, Color::DarkGray, Modifier::empty()),
            (SemanticStyle::HtmlRaw, Color::DarkGray, Modifier::empty()),
            (
                SemanticStyle::FmDelimiter,
                Color::DarkGray,
                Modifier::empty(),
            ),
            (SemanticStyle::FmKey, Color::Yellow, Modifier::BOLD),
            (SemanticStyle::FmValue, Color::Green, Modifier::empty()),
            (SemanticStyle::Keyword, Color::Red, Modifier::BOLD),
            (SemanticStyle::Function, Color::Cyan, Modifier::empty()),
            (SemanticStyle::TypeName, Color::Yellow, Modifier::empty()),
            (SemanticStyle::StringLit, Color::Green, Modifier::empty()),
            (SemanticStyle::NumberLit, Color::Magenta, Modifier::empty()),
            (SemanticStyle::Comment, Color::DarkGray, Modifier::ITALIC),
            (SemanticStyle::Operator, Color::White, Modifier::empty()),
            (SemanticStyle::Variable, Color::White, Modifier::empty()),
            (SemanticStyle::Punct, Color::DarkGray, Modifier::empty()),
            (SemanticStyle::Selection, Color::Reset, Modifier::REVERSED),
            (SemanticStyle::Match, Color::Yellow, Modifier::UNDERLINED),
            (SemanticStyle::CursorLine, Color::Reset, Modifier::empty()),
            (SemanticStyle::Muted, Color::DarkGray, Modifier::DIM),
        ],
        ui: &[
            (
                UiSlot::StatusBar,
                Color::White,
                Some(Color::DarkGray),
                Modifier::DIM,
            ),
            (
                UiSlot::BadgeNormal,
                Color::Black,
                Some(Color::White),
                Modifier::BOLD,
            ),
            (
                UiSlot::BadgeInsert,
                Color::Black,
                Some(Color::Green),
                Modifier::BOLD,
            ),
            (
                UiSlot::BadgeSelect,
                Color::Black,
                Some(Color::Yellow),
                Modifier::BOLD,
            ),
            (
                UiSlot::BadgeCommand,
                Color::Black,
                Some(Color::Magenta),
                Modifier::BOLD,
            ),
            (UiSlot::Border, Color::DarkGray, None, Modifier::empty()),
            (UiSlot::HintKey, Color::Yellow, None, Modifier::BOLD),
            (UiSlot::HintDesc, Color::White, None, Modifier::empty()),
            (UiSlot::StatusSuccess, Color::Green, None, Modifier::empty()),
            (UiSlot::StatusInfo, Color::White, None, Modifier::empty()),
            (
                UiSlot::StatusWarning,
                Color::Yellow,
                None,
                Modifier::empty(),
            ),
            (UiSlot::StatusError, Color::Red, None, Modifier::empty()),
            (UiSlot::Gutter, Color::DarkGray, None, Modifier::DIM),
            (UiSlot::GutterCurrent, Color::Yellow, None, Modifier::BOLD),
            (
                UiSlot::CursorLine,
                Color::Reset,
                Some(Color::DarkGray),
                Modifier::DIM,
            ),
            (UiSlot::TabActive, Color::White, None, Modifier::BOLD),
            (UiSlot::TabInactive, Color::Gray, None, Modifier::DIM),
            (
                UiSlot::MetadataPanel,
                Color::Reset,
                Some(Color::DarkGray),
                Modifier::DIM,
            ),
            (UiSlot::TabSeparator, Color::DarkGray, None, Modifier::DIM),
        ],
    },
    monochrome: Palette::Monochrome {
        semantic: &[
            (SemanticStyle::Text, Modifier::empty()),
            (SemanticStyle::Heading1, Modifier::BOLD),
            (SemanticStyle::Heading2, Modifier::BOLD),
            (SemanticStyle::Heading3, Modifier::BOLD),
            (SemanticStyle::Heading4, Modifier::BOLD),
            (SemanticStyle::Heading5, Modifier::BOLD),
            (SemanticStyle::Heading6, Modifier::BOLD),
            (SemanticStyle::Emphasis, Modifier::ITALIC),
            (SemanticStyle::Strong, Modifier::BOLD),
            (SemanticStyle::Strikethrough, Modifier::empty()),
            (SemanticStyle::CodeSpan, Modifier::empty()),
            (SemanticStyle::CodeBlock, Modifier::empty()),
            (SemanticStyle::Quote, Modifier::empty()),
            (SemanticStyle::ListMarker, Modifier::empty()),
            (SemanticStyle::Link, Modifier::UNDERLINED),
            (SemanticStyle::LinkUrl, Modifier::DIM),
            (SemanticStyle::Rule, Modifier::DIM),
            (SemanticStyle::HtmlRaw, Modifier::DIM),
            (SemanticStyle::FmDelimiter, Modifier::DIM),
            (SemanticStyle::FmKey, Modifier::BOLD),
            (SemanticStyle::FmValue, Modifier::empty()),
            (SemanticStyle::Keyword, Modifier::BOLD),
            (SemanticStyle::Function, Modifier::empty()),
            (SemanticStyle::TypeName, Modifier::empty()),
            (SemanticStyle::StringLit, Modifier::empty()),
            (SemanticStyle::NumberLit, Modifier::empty()),
            (SemanticStyle::Comment, Modifier::DIM),
            (SemanticStyle::Operator, Modifier::empty()),
            (SemanticStyle::Variable, Modifier::empty()),
            (SemanticStyle::Punct, Modifier::DIM),
            (SemanticStyle::Selection, Modifier::REVERSED),
            (SemanticStyle::Match, Modifier::UNDERLINED),
            (SemanticStyle::CursorLine, Modifier::UNDERLINED),
            (SemanticStyle::Muted, Modifier::DIM),
        ],
        ui: &[
            (UiSlot::StatusBar, Modifier::DIM),
            (UiSlot::BadgeNormal, Modifier::BOLD),
            (UiSlot::BadgeInsert, Modifier::BOLD),
            (UiSlot::BadgeSelect, Modifier::BOLD),
            (UiSlot::BadgeCommand, Modifier::BOLD),
            (UiSlot::Border, Modifier::DIM),
            (UiSlot::HintKey, Modifier::BOLD),
            (UiSlot::HintDesc, Modifier::empty()),
            (UiSlot::StatusSuccess, Modifier::empty()),
            (UiSlot::StatusInfo, Modifier::empty()),
            (UiSlot::StatusWarning, Modifier::empty()),
            (UiSlot::StatusError, Modifier::empty()),
            (UiSlot::Gutter, Modifier::DIM),
            (UiSlot::GutterCurrent, Modifier::BOLD),
            (UiSlot::CursorLine, Modifier::UNDERLINED),
            (UiSlot::TabActive, Modifier::BOLD),
            (UiSlot::TabInactive, Modifier::DIM),
            (UiSlot::MetadataPanel, Modifier::DIM),
            (UiSlot::TabSeparator, Modifier::DIM),
        ],
    },
};

// ── Built-in: default-light ─────────────────────────────────────────────────

pub static DEFAULT_LIGHT: Theme = Theme {
    name: "default-light",
    truecolor: Palette::TrueColor {
        semantic: &[
            (SemanticStyle::Text, Color::Black, Modifier::empty()),
            (SemanticStyle::Heading1, Color::Red, Modifier::BOLD),
            (SemanticStyle::Heading2, Color::Red, Modifier::BOLD),
            (SemanticStyle::Heading3, Color::Blue, Modifier::BOLD),
            (SemanticStyle::Heading4, Color::Blue, Modifier::BOLD),
            (SemanticStyle::Heading5, Color::Magenta, Modifier::BOLD),
            (SemanticStyle::Heading6, Color::Magenta, Modifier::BOLD),
            (SemanticStyle::Emphasis, Color::Black, Modifier::ITALIC),
            (SemanticStyle::Strong, Color::Black, Modifier::BOLD),
            (SemanticStyle::Strikethrough, Color::Gray, Modifier::empty()),
            (SemanticStyle::CodeSpan, Color::Green, Modifier::empty()),
            (SemanticStyle::CodeBlock, Color::Green, Modifier::empty()),
            (SemanticStyle::Quote, Color::Red, Modifier::empty()),
            (SemanticStyle::ListMarker, Color::Blue, Modifier::empty()),
            (SemanticStyle::Link, Color::Blue, Modifier::UNDERLINED),
            (SemanticStyle::LinkUrl, Color::Gray, Modifier::empty()),
            (SemanticStyle::Rule, Color::Gray, Modifier::empty()),
            (SemanticStyle::HtmlRaw, Color::Gray, Modifier::empty()),
            (SemanticStyle::FmDelimiter, Color::Gray, Modifier::empty()),
            (SemanticStyle::FmKey, Color::Red, Modifier::BOLD),
            (SemanticStyle::FmValue, Color::Green, Modifier::empty()),
            (SemanticStyle::Keyword, Color::Red, Modifier::BOLD),
            (SemanticStyle::Function, Color::Blue, Modifier::empty()),
            (SemanticStyle::TypeName, Color::Red, Modifier::empty()),
            (SemanticStyle::StringLit, Color::Green, Modifier::empty()),
            (SemanticStyle::NumberLit, Color::Magenta, Modifier::empty()),
            (SemanticStyle::Comment, Color::Gray, Modifier::ITALIC),
            (SemanticStyle::Operator, Color::Black, Modifier::empty()),
            (SemanticStyle::Variable, Color::Black, Modifier::empty()),
            (SemanticStyle::Punct, Color::Gray, Modifier::empty()),
            (SemanticStyle::Selection, Color::Reset, Modifier::REVERSED),
            (SemanticStyle::Match, Color::Red, Modifier::UNDERLINED),
            (SemanticStyle::CursorLine, Color::Reset, Modifier::empty()),
            (SemanticStyle::Muted, Color::Gray, Modifier::DIM),
        ],
        ui: &[
            (
                UiSlot::StatusBar,
                Color::Black,
                Some(Color::Rgb(225, 228, 232)),
                Modifier::DIM,
            ),
            (
                UiSlot::BadgeNormal,
                Color::Black,
                Some(Color::White),
                Modifier::BOLD,
            ),
            (
                UiSlot::BadgeInsert,
                Color::Black,
                Some(Color::Green),
                Modifier::BOLD,
            ),
            (
                UiSlot::BadgeSelect,
                Color::Black,
                Some(Color::Yellow),
                Modifier::BOLD,
            ),
            (
                UiSlot::BadgeCommand,
                Color::Black,
                Some(Color::Magenta),
                Modifier::BOLD,
            ),
            (UiSlot::Border, Color::Gray, None, Modifier::empty()),
            (UiSlot::HintKey, Color::Yellow, None, Modifier::BOLD),
            (UiSlot::HintDesc, Color::Black, None, Modifier::empty()),
            (UiSlot::StatusSuccess, Color::Green, None, Modifier::empty()),
            (UiSlot::StatusInfo, Color::Black, None, Modifier::empty()),
            (
                UiSlot::StatusWarning,
                Color::Yellow,
                None,
                Modifier::empty(),
            ),
            (UiSlot::StatusError, Color::Red, None, Modifier::empty()),
            (UiSlot::Gutter, Color::Gray, None, Modifier::DIM),
            (UiSlot::GutterCurrent, Color::Yellow, None, Modifier::BOLD),
            (
                UiSlot::CursorLine,
                Color::Reset,
                Some(Color::Gray),
                Modifier::DIM,
            ),
            (UiSlot::TabActive, Color::Black, None, Modifier::BOLD),
            (UiSlot::TabInactive, Color::Gray, None, Modifier::DIM),
            (
                UiSlot::MetadataPanel,
                Color::Reset,
                Some(Color::Gray),
                Modifier::DIM,
            ),
            (UiSlot::TabSeparator, Color::Gray, None, Modifier::DIM),
        ],
    },
    color16: Palette::Color16 {
        semantic: &[
            (SemanticStyle::Text, Color::Black, Modifier::empty()),
            (SemanticStyle::Heading1, Color::Red, Modifier::BOLD),
            (SemanticStyle::Heading2, Color::Red, Modifier::BOLD),
            (SemanticStyle::Heading3, Color::Blue, Modifier::BOLD),
            (SemanticStyle::Heading4, Color::Blue, Modifier::BOLD),
            (SemanticStyle::Heading5, Color::Magenta, Modifier::BOLD),
            (SemanticStyle::Heading6, Color::Magenta, Modifier::BOLD),
            (SemanticStyle::Emphasis, Color::Black, Modifier::ITALIC),
            (SemanticStyle::Strong, Color::Black, Modifier::BOLD),
            (SemanticStyle::Strikethrough, Color::Gray, Modifier::empty()),
            (SemanticStyle::CodeSpan, Color::Green, Modifier::empty()),
            (SemanticStyle::CodeBlock, Color::Green, Modifier::empty()),
            (SemanticStyle::Quote, Color::Red, Modifier::empty()),
            (SemanticStyle::ListMarker, Color::Blue, Modifier::empty()),
            (SemanticStyle::Link, Color::Blue, Modifier::UNDERLINED),
            (SemanticStyle::LinkUrl, Color::Gray, Modifier::empty()),
            (SemanticStyle::Rule, Color::Gray, Modifier::empty()),
            (SemanticStyle::HtmlRaw, Color::Gray, Modifier::empty()),
            (SemanticStyle::FmDelimiter, Color::Gray, Modifier::empty()),
            (SemanticStyle::FmKey, Color::Red, Modifier::BOLD),
            (SemanticStyle::FmValue, Color::Green, Modifier::empty()),
            (SemanticStyle::Keyword, Color::Red, Modifier::BOLD),
            (SemanticStyle::Function, Color::Blue, Modifier::empty()),
            (SemanticStyle::TypeName, Color::Red, Modifier::empty()),
            (SemanticStyle::StringLit, Color::Green, Modifier::empty()),
            (SemanticStyle::NumberLit, Color::Magenta, Modifier::empty()),
            (SemanticStyle::Comment, Color::Gray, Modifier::ITALIC),
            (SemanticStyle::Operator, Color::Black, Modifier::empty()),
            (SemanticStyle::Variable, Color::Black, Modifier::empty()),
            (SemanticStyle::Punct, Color::Gray, Modifier::empty()),
            (SemanticStyle::Selection, Color::Reset, Modifier::REVERSED),
            (SemanticStyle::Match, Color::Red, Modifier::UNDERLINED),
            (SemanticStyle::CursorLine, Color::Reset, Modifier::empty()),
            (SemanticStyle::Muted, Color::Gray, Modifier::DIM),
        ],
        ui: &[
            (
                UiSlot::StatusBar,
                Color::Black,
                Some(Color::Gray),
                Modifier::DIM,
            ),
            (
                UiSlot::BadgeNormal,
                Color::Black,
                Some(Color::White),
                Modifier::BOLD,
            ),
            (
                UiSlot::BadgeInsert,
                Color::Black,
                Some(Color::Green),
                Modifier::BOLD,
            ),
            (
                UiSlot::BadgeSelect,
                Color::Black,
                Some(Color::Yellow),
                Modifier::BOLD,
            ),
            (
                UiSlot::BadgeCommand,
                Color::Black,
                Some(Color::Magenta),
                Modifier::BOLD,
            ),
            (UiSlot::Border, Color::Gray, None, Modifier::empty()),
            (UiSlot::HintKey, Color::Yellow, None, Modifier::BOLD),
            (UiSlot::HintDesc, Color::Black, None, Modifier::empty()),
            (UiSlot::StatusSuccess, Color::Green, None, Modifier::empty()),
            (UiSlot::StatusInfo, Color::Black, None, Modifier::empty()),
            (
                UiSlot::StatusWarning,
                Color::Yellow,
                None,
                Modifier::empty(),
            ),
            (UiSlot::StatusError, Color::Red, None, Modifier::empty()),
            (UiSlot::Gutter, Color::Gray, None, Modifier::DIM),
            (UiSlot::GutterCurrent, Color::Yellow, None, Modifier::BOLD),
            (
                UiSlot::CursorLine,
                Color::Reset,
                Some(Color::Gray),
                Modifier::DIM,
            ),
            (UiSlot::TabActive, Color::Black, None, Modifier::BOLD),
            (UiSlot::TabInactive, Color::Gray, None, Modifier::DIM),
            (
                UiSlot::MetadataPanel,
                Color::Reset,
                Some(Color::Gray),
                Modifier::DIM,
            ),
            (UiSlot::TabSeparator, Color::Gray, None, Modifier::DIM),
        ],
    },
    monochrome: Palette::Monochrome {
        semantic: &[
            (SemanticStyle::Text, Modifier::empty()),
            (SemanticStyle::Heading1, Modifier::BOLD),
            (SemanticStyle::Heading2, Modifier::BOLD),
            (SemanticStyle::Heading3, Modifier::BOLD),
            (SemanticStyle::Heading4, Modifier::BOLD),
            (SemanticStyle::Heading5, Modifier::BOLD),
            (SemanticStyle::Heading6, Modifier::BOLD),
            (SemanticStyle::Emphasis, Modifier::ITALIC),
            (SemanticStyle::Strong, Modifier::BOLD),
            (SemanticStyle::Strikethrough, Modifier::empty()),
            (SemanticStyle::CodeSpan, Modifier::empty()),
            (SemanticStyle::CodeBlock, Modifier::empty()),
            (SemanticStyle::Quote, Modifier::empty()),
            (SemanticStyle::ListMarker, Modifier::empty()),
            (SemanticStyle::Link, Modifier::UNDERLINED),
            (SemanticStyle::LinkUrl, Modifier::DIM),
            (SemanticStyle::Rule, Modifier::DIM),
            (SemanticStyle::HtmlRaw, Modifier::DIM),
            (SemanticStyle::FmDelimiter, Modifier::DIM),
            (SemanticStyle::FmKey, Modifier::BOLD),
            (SemanticStyle::FmValue, Modifier::empty()),
            (SemanticStyle::Keyword, Modifier::BOLD),
            (SemanticStyle::Function, Modifier::empty()),
            (SemanticStyle::TypeName, Modifier::empty()),
            (SemanticStyle::StringLit, Modifier::empty()),
            (SemanticStyle::NumberLit, Modifier::empty()),
            (SemanticStyle::Comment, Modifier::DIM),
            (SemanticStyle::Operator, Modifier::empty()),
            (SemanticStyle::Variable, Modifier::empty()),
            (SemanticStyle::Punct, Modifier::DIM),
            (SemanticStyle::Selection, Modifier::REVERSED),
            (SemanticStyle::Match, Modifier::UNDERLINED),
            (SemanticStyle::CursorLine, Modifier::UNDERLINED),
            (SemanticStyle::Muted, Modifier::DIM),
        ],
        ui: &[
            (UiSlot::StatusBar, Modifier::DIM),
            (UiSlot::BadgeNormal, Modifier::BOLD),
            (UiSlot::BadgeInsert, Modifier::BOLD),
            (UiSlot::BadgeSelect, Modifier::BOLD),
            (UiSlot::BadgeCommand, Modifier::BOLD),
            (UiSlot::Border, Modifier::DIM),
            (UiSlot::HintKey, Modifier::BOLD),
            (UiSlot::HintDesc, Modifier::empty()),
            (UiSlot::StatusSuccess, Modifier::empty()),
            (UiSlot::StatusInfo, Modifier::empty()),
            (UiSlot::StatusWarning, Modifier::empty()),
            (UiSlot::StatusError, Modifier::empty()),
            (UiSlot::Gutter, Modifier::DIM),
            (UiSlot::GutterCurrent, Modifier::BOLD),
            (UiSlot::CursorLine, Modifier::UNDERLINED),
            (UiSlot::TabActive, Modifier::BOLD),
            (UiSlot::TabInactive, Modifier::DIM),
            (UiSlot::MetadataPanel, Modifier::DIM),
            (UiSlot::TabSeparator, Modifier::DIM),
        ],
    },
};

// ── Built-in: accessible (Monochrome) ───────────────────────────────────────

pub static ACCESSIBLE: Theme = Theme {
    name: "accessible",
    truecolor: Palette::Monochrome {
        semantic: &[
            (SemanticStyle::Text, Modifier::empty()),
            (SemanticStyle::Heading1, Modifier::BOLD),
            (SemanticStyle::Heading2, Modifier::BOLD),
            (SemanticStyle::Heading3, Modifier::BOLD),
            (SemanticStyle::Heading4, Modifier::BOLD),
            (SemanticStyle::Heading5, Modifier::BOLD),
            (SemanticStyle::Heading6, Modifier::BOLD),
            (SemanticStyle::Emphasis, Modifier::ITALIC),
            (SemanticStyle::Strong, Modifier::BOLD),
            (SemanticStyle::Strikethrough, Modifier::empty()),
            (SemanticStyle::CodeSpan, Modifier::empty()),
            (SemanticStyle::CodeBlock, Modifier::empty()),
            (SemanticStyle::Quote, Modifier::empty()),
            (SemanticStyle::ListMarker, Modifier::empty()),
            (SemanticStyle::Link, Modifier::UNDERLINED),
            (SemanticStyle::LinkUrl, Modifier::DIM),
            (SemanticStyle::Rule, Modifier::DIM),
            (SemanticStyle::HtmlRaw, Modifier::DIM),
            (SemanticStyle::FmDelimiter, Modifier::DIM),
            (SemanticStyle::FmKey, Modifier::BOLD),
            (SemanticStyle::FmValue, Modifier::empty()),
            (SemanticStyle::Keyword, Modifier::BOLD),
            (SemanticStyle::Function, Modifier::empty()),
            (SemanticStyle::TypeName, Modifier::empty()),
            (SemanticStyle::StringLit, Modifier::empty()),
            (SemanticStyle::NumberLit, Modifier::empty()),
            (SemanticStyle::Comment, Modifier::DIM),
            (SemanticStyle::Operator, Modifier::empty()),
            (SemanticStyle::Variable, Modifier::empty()),
            (SemanticStyle::Punct, Modifier::DIM),
            (SemanticStyle::Selection, Modifier::REVERSED),
            (SemanticStyle::Match, Modifier::UNDERLINED),
            (SemanticStyle::CursorLine, Modifier::UNDERLINED),
            (SemanticStyle::Muted, Modifier::DIM),
        ],
        ui: &[
            (UiSlot::StatusBar, Modifier::DIM),
            (UiSlot::BadgeNormal, Modifier::BOLD),
            (UiSlot::BadgeInsert, Modifier::BOLD),
            (UiSlot::BadgeSelect, Modifier::BOLD),
            (UiSlot::BadgeCommand, Modifier::BOLD),
            (UiSlot::Border, Modifier::DIM),
            (UiSlot::HintKey, Modifier::BOLD),
            (UiSlot::HintDesc, Modifier::empty()),
            (UiSlot::StatusSuccess, Modifier::empty()),
            (UiSlot::StatusInfo, Modifier::empty()),
            (UiSlot::StatusWarning, Modifier::empty()),
            (UiSlot::StatusError, Modifier::empty()),
            (UiSlot::Gutter, Modifier::DIM),
            (UiSlot::GutterCurrent, Modifier::BOLD),
            (UiSlot::CursorLine, Modifier::UNDERLINED),
            (UiSlot::TabActive, Modifier::BOLD),
            (UiSlot::TabInactive, Modifier::DIM),
            (UiSlot::MetadataPanel, Modifier::DIM),
            (UiSlot::TabSeparator, Modifier::DIM),
        ],
    },
    color16: Palette::Monochrome {
        semantic: &[
            (SemanticStyle::Text, Modifier::empty()),
            (SemanticStyle::Heading1, Modifier::BOLD),
            (SemanticStyle::Heading2, Modifier::BOLD),
            (SemanticStyle::Heading3, Modifier::BOLD),
            (SemanticStyle::Heading4, Modifier::BOLD),
            (SemanticStyle::Heading5, Modifier::BOLD),
            (SemanticStyle::Heading6, Modifier::BOLD),
            (SemanticStyle::Emphasis, Modifier::ITALIC),
            (SemanticStyle::Strong, Modifier::BOLD),
            (SemanticStyle::Strikethrough, Modifier::empty()),
            (SemanticStyle::CodeSpan, Modifier::empty()),
            (SemanticStyle::CodeBlock, Modifier::empty()),
            (SemanticStyle::Quote, Modifier::empty()),
            (SemanticStyle::ListMarker, Modifier::empty()),
            (SemanticStyle::Link, Modifier::UNDERLINED),
            (SemanticStyle::LinkUrl, Modifier::DIM),
            (SemanticStyle::Rule, Modifier::DIM),
            (SemanticStyle::HtmlRaw, Modifier::DIM),
            (SemanticStyle::FmDelimiter, Modifier::DIM),
            (SemanticStyle::FmKey, Modifier::BOLD),
            (SemanticStyle::FmValue, Modifier::empty()),
            (SemanticStyle::Keyword, Modifier::BOLD),
            (SemanticStyle::Function, Modifier::empty()),
            (SemanticStyle::TypeName, Modifier::empty()),
            (SemanticStyle::StringLit, Modifier::empty()),
            (SemanticStyle::NumberLit, Modifier::empty()),
            (SemanticStyle::Comment, Modifier::DIM),
            (SemanticStyle::Operator, Modifier::empty()),
            (SemanticStyle::Variable, Modifier::empty()),
            (SemanticStyle::Punct, Modifier::DIM),
            (SemanticStyle::Selection, Modifier::REVERSED),
            (SemanticStyle::Match, Modifier::UNDERLINED),
            (SemanticStyle::CursorLine, Modifier::UNDERLINED),
            (SemanticStyle::Muted, Modifier::DIM),
        ],
        ui: &[
            (UiSlot::StatusBar, Modifier::DIM),
            (UiSlot::BadgeNormal, Modifier::BOLD),
            (UiSlot::BadgeInsert, Modifier::BOLD),
            (UiSlot::BadgeSelect, Modifier::BOLD),
            (UiSlot::BadgeCommand, Modifier::BOLD),
            (UiSlot::Border, Modifier::DIM),
            (UiSlot::HintKey, Modifier::BOLD),
            (UiSlot::HintDesc, Modifier::empty()),
            (UiSlot::StatusSuccess, Modifier::empty()),
            (UiSlot::StatusInfo, Modifier::empty()),
            (UiSlot::StatusWarning, Modifier::empty()),
            (UiSlot::StatusError, Modifier::empty()),
            (UiSlot::Gutter, Modifier::DIM),
            (UiSlot::GutterCurrent, Modifier::BOLD),
            (UiSlot::CursorLine, Modifier::UNDERLINED),
            (UiSlot::TabActive, Modifier::BOLD),
            (UiSlot::TabInactive, Modifier::DIM),
            (UiSlot::MetadataPanel, Modifier::DIM),
            (UiSlot::TabSeparator, Modifier::DIM),
        ],
    },
    monochrome: Palette::Monochrome {
        semantic: &[
            (SemanticStyle::Text, Modifier::empty()),
            (SemanticStyle::Heading1, Modifier::BOLD),
            (SemanticStyle::Heading2, Modifier::BOLD),
            (SemanticStyle::Heading3, Modifier::BOLD),
            (SemanticStyle::Heading4, Modifier::BOLD),
            (SemanticStyle::Heading5, Modifier::BOLD),
            (SemanticStyle::Heading6, Modifier::BOLD),
            (SemanticStyle::Emphasis, Modifier::ITALIC),
            (SemanticStyle::Strong, Modifier::BOLD),
            (SemanticStyle::Strikethrough, Modifier::empty()),
            (SemanticStyle::CodeSpan, Modifier::empty()),
            (SemanticStyle::CodeBlock, Modifier::empty()),
            (SemanticStyle::Quote, Modifier::empty()),
            (SemanticStyle::ListMarker, Modifier::empty()),
            (SemanticStyle::Link, Modifier::UNDERLINED),
            (SemanticStyle::LinkUrl, Modifier::DIM),
            (SemanticStyle::Rule, Modifier::DIM),
            (SemanticStyle::HtmlRaw, Modifier::DIM),
            (SemanticStyle::FmDelimiter, Modifier::DIM),
            (SemanticStyle::FmKey, Modifier::BOLD),
            (SemanticStyle::FmValue, Modifier::empty()),
            (SemanticStyle::Keyword, Modifier::BOLD),
            (SemanticStyle::Function, Modifier::empty()),
            (SemanticStyle::TypeName, Modifier::empty()),
            (SemanticStyle::StringLit, Modifier::empty()),
            (SemanticStyle::NumberLit, Modifier::empty()),
            (SemanticStyle::Comment, Modifier::DIM),
            (SemanticStyle::Operator, Modifier::empty()),
            (SemanticStyle::Variable, Modifier::empty()),
            (SemanticStyle::Punct, Modifier::DIM),
            (SemanticStyle::Selection, Modifier::REVERSED),
            (SemanticStyle::Match, Modifier::UNDERLINED),
            (SemanticStyle::CursorLine, Modifier::UNDERLINED),
            (SemanticStyle::Muted, Modifier::DIM),
        ],
        ui: &[
            (UiSlot::StatusBar, Modifier::DIM),
            (UiSlot::BadgeNormal, Modifier::BOLD),
            (UiSlot::BadgeInsert, Modifier::BOLD),
            (UiSlot::BadgeSelect, Modifier::BOLD),
            (UiSlot::BadgeCommand, Modifier::BOLD),
            (UiSlot::Border, Modifier::DIM),
            (UiSlot::HintKey, Modifier::BOLD),
            (UiSlot::HintDesc, Modifier::empty()),
            (UiSlot::StatusSuccess, Modifier::empty()),
            (UiSlot::StatusInfo, Modifier::empty()),
            (UiSlot::StatusWarning, Modifier::empty()),
            (UiSlot::StatusError, Modifier::empty()),
            (UiSlot::Gutter, Modifier::DIM),
            (UiSlot::GutterCurrent, Modifier::BOLD),
            (UiSlot::CursorLine, Modifier::UNDERLINED),
            (UiSlot::TabActive, Modifier::BOLD),
            (UiSlot::TabInactive, Modifier::DIM),
            (UiSlot::MetadataPanel, Modifier::DIM),
            (UiSlot::TabSeparator, Modifier::DIM),
        ],
    },
};

// ── Legacy compatibility ────────────────────────────────────────────────────

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Semantic style completeness ─────────────────────────────────────

    /// Every SemanticStyle variant maps to a non-empty Style on every tier
    /// of every built-in theme.
    #[test]
    fn all_semantic_styles_resolve() {
        let themes: Vec<_> = BUILTIN_THEMES
            .iter()
            .map(|spec| (spec.theme.name, spec.theme))
            .collect();

        let variants = [
            SemanticStyle::Text,
            SemanticStyle::Heading1,
            SemanticStyle::Heading2,
            SemanticStyle::Heading3,
            SemanticStyle::Heading4,
            SemanticStyle::Heading5,
            SemanticStyle::Heading6,
            SemanticStyle::Emphasis,
            SemanticStyle::Strong,
            SemanticStyle::Strikethrough,
            SemanticStyle::CodeSpan,
            SemanticStyle::CodeBlock,
            SemanticStyle::Quote,
            SemanticStyle::ListMarker,
            SemanticStyle::Link,
            SemanticStyle::LinkUrl,
            SemanticStyle::Rule,
            SemanticStyle::HtmlRaw,
            SemanticStyle::FmDelimiter,
            SemanticStyle::FmKey,
            SemanticStyle::FmValue,
            SemanticStyle::Keyword,
            SemanticStyle::Function,
            SemanticStyle::TypeName,
            SemanticStyle::StringLit,
            SemanticStyle::NumberLit,
            SemanticStyle::Comment,
            SemanticStyle::Operator,
            SemanticStyle::Variable,
            SemanticStyle::Punct,
            SemanticStyle::Selection,
            SemanticStyle::Match,
            SemanticStyle::CursorLine,
            SemanticStyle::Muted,
        ];

        for (name, theme) in &themes {
            for tier in [Tier::TrueColor, Tier::Color16, Tier::Monochrome] {
                for variant in &variants {
                    let style = theme.style(tier, *variant);
                    assert!(
                        style.fg.is_some() || style.bg.is_some() || !style.add_modifier.is_empty(),
                        "Theme {name} tier {tier:?} resolve({variant:?}) must carry at least one non-default property"
                    );
                }
            }
        }
    }

    // ── Monochrome invariant: no foreground colors ──────────────────────

    /// Monochrome palettes must not have any foreground color other than Reset.
    #[test]
    fn monochrome_has_no_fg_colors() {
        for theme in BUILTIN_THEMES.iter().map(|spec| spec.theme) {
            let mono = &theme.monochrome;
            if let Palette::Monochrome { semantic, ui } = mono {
                for (style, _) in semantic.iter() {
                    let s = theme.style(Tier::Monochrome, *style);
                    assert_eq!(
                        s.fg,
                        Some(Color::Reset),
                        "monochrome {style:?} must use Color::Reset fg"
                    );
                }
                for (slot, _) in ui.iter() {
                    let s = theme.ui_style(Tier::Monochrome, *slot);
                    assert_eq!(
                        s.fg,
                        Some(Color::Reset),
                        "monochrome ui slot {slot:?} must use Color::Reset fg"
                    );
                }
            }
        }
    }

    // ── Selection carries REVERSED ──────────────────────────────────────

    /// Selection style always carries REVERSED on every tier of every theme.
    #[test]
    fn selection_carries_reversed() {
        for theme in BUILTIN_THEMES.iter().map(|spec| spec.theme) {
            for tier in [Tier::TrueColor, Tier::Color16, Tier::Monochrome] {
                let style = theme.style(tier, SemanticStyle::Selection);
                assert!(
                    style.add_modifier.contains(Modifier::REVERSED),
                    "Selection must carry REVERSED on {tier:?} in {}",
                    theme.name
                );
            }
        }
    }

    #[test]
    fn cursor_line_has_a_distinct_non_color_carrier() {
        for theme in BUILTIN_THEMES.iter().map(|spec| spec.theme) {
            for tier in [Tier::TrueColor, Tier::Color16, Tier::Monochrome] {
                let selection = theme.style(tier, SemanticStyle::Selection);
                let cursor = theme.ui_style(tier, UiSlot::CursorLine);
                assert_ne!(
                    cursor.add_modifier, selection.add_modifier,
                    "Normal cursor and Select rows must differ on {tier:?} in {}",
                    theme.name
                );
                assert!(
                    cursor
                        .add_modifier
                        .intersects(Modifier::DIM | Modifier::UNDERLINED),
                    "cursor line needs a non-color carrier on {tier:?} in {}",
                    theme.name
                );
            }
        }
    }

    /// Search matches must remain visible when foreground colors are absent.
    #[test]
    fn search_match_carries_underline() {
        for theme in BUILTIN_THEMES.iter().map(|spec| spec.theme) {
            for tier in [Tier::TrueColor, Tier::Color16, Tier::Monochrome] {
                let style = theme.style(tier, SemanticStyle::Match);
                assert!(
                    style.add_modifier.contains(Modifier::UNDERLINED),
                    "Search match must carry UNDERLINED on {tier:?} in {}",
                    theme.name
                );
            }
        }
    }

    // ── Every accessor returns non-empty Style ──────────────────────────

    /// Every accessor (semantic + UI) returns a Style with at least one
    /// non-default property on every tier.
    #[test]
    fn every_accessor_nonempty() {
        let themes: Vec<_> = BUILTIN_THEMES
            .iter()
            .map(|spec| (spec.theme.name, spec.theme))
            .collect();

        let ui_slots = [
            UiSlot::StatusBar,
            UiSlot::BadgeNormal,
            UiSlot::BadgeInsert,
            UiSlot::BadgeSelect,
            UiSlot::BadgeCommand,
            UiSlot::Border,
            UiSlot::HintKey,
            UiSlot::HintDesc,
            UiSlot::StatusSuccess,
            UiSlot::StatusInfo,
            UiSlot::StatusWarning,
            UiSlot::StatusError,
            UiSlot::Gutter,
            UiSlot::GutterCurrent,
            UiSlot::CursorLine,
            UiSlot::TabActive,
            UiSlot::TabInactive,
            UiSlot::MetadataPanel,
            UiSlot::TabSeparator,
        ];

        for (name, theme) in &themes {
            for tier in [Tier::TrueColor, Tier::Color16, Tier::Monochrome] {
                for slot in &ui_slots {
                    let style = theme.ui_style(tier, *slot);
                    assert!(
                        style.fg.is_some() || style.bg.is_some() || !style.add_modifier.is_empty(),
                        "UI slot {slot:?} on {name} tier {tier:?} must carry at least one property"
                    );
                }
            }
        }
    }

    // ── Selection ladder ────────────────────────────────────────────────

    #[test]
    fn env_parts_lookup_maps_exact_environment_names() {
        let mut queried = Vec::new();
        let env = EnvParts::from_lookup(|name| {
            queried.push(name.to_string());
            match name {
                "OOM_EDIT_THEME" => Some(["access", "ible"].concat()),
                "NO_COLOR" => Some(String::new()),
                "TERM" => Some(["xterm", "-256color"].concat()),
                "COLORTERM" => Some(["true", "color"].concat()),
                "COLORFGBG" => Some(format!("{};{}", 0, 7)),
                _ => None,
            }
        });

        assert_eq!(
            queried,
            [
                "OOM_EDIT_THEME",
                "NO_COLOR",
                "TERM",
                "COLORTERM",
                "COLORFGBG"
            ]
        );
        assert_eq!(env.oom_edit_theme.as_deref(), Some("accessible"));
        assert!(env.no_color);
        assert_eq!(env.term.as_deref(), Some("xterm-256color"));
        assert_eq!(env.colorterm.as_deref(), Some("truecolor"));
        assert_eq!(env.colorfgbg.as_deref(), Some("0;7"));
        assert_eq!(env.capability(), Tier::Monochrome);
        assert!(env.is_light(None));
        assert_eq!(
            resolve_theme(
                None,
                None,
                Some("default-dark"),
                Some("default-light"),
                &env,
            )
            .source,
            ThemeSource::Environment
        );
    }

    #[test]
    fn startup_environment_snapshot_requires_no_leaks() {
        let startup = include_str!("lib.rs");
        assert!(startup.contains("EnvParts::from_current_process()"));
        assert!(!startup.contains("Box::leak"));
    }

    #[test]
    fn ladder_no_color_forces_monochrome() {
        let env = EnvParts {
            no_color: true,
            ..Default::default()
        };
        assert_eq!(env.effective_tier(), Tier::Monochrome);
    }

    #[test]
    fn ladder_term_dumb_forces_monochrome() {
        let env = EnvParts {
            term: Some("dumb".to_string()),
            ..Default::default()
        };
        assert_eq!(env.effective_tier(), Tier::Monochrome);
    }

    #[test]
    fn ladder_oom_edit_theme_accessible_forces_monochrome() {
        let env = EnvParts {
            oom_edit_theme: Some("accessible".to_string()),
            colorterm: Some("truecolor".to_string()),
            ..Default::default()
        };
        assert_eq!(env.effective_tier(), Tier::Monochrome);
    }

    #[test]
    fn ladder_colorterm_truecolor() {
        let env = EnvParts {
            colorterm: Some("truecolor".to_string()),
            ..Default::default()
        };
        assert_eq!(env.effective_tier(), Tier::TrueColor);
    }

    #[test]
    fn ladder_colorterm_24bit() {
        let env = EnvParts {
            colorterm: Some("24bit".to_string()),
            ..Default::default()
        };
        assert_eq!(env.effective_tier(), Tier::TrueColor);
    }

    #[test]
    fn ladder_colorterm_default_to_color16() {
        let env = EnvParts {
            colorterm: Some("basic".to_string()),
            ..Default::default()
        };
        assert_eq!(env.effective_tier(), Tier::Color16);
    }

    #[test]
    fn ladder_no_colorterm_default_to_color16() {
        let env = EnvParts {
            colorterm: None,
            ..Default::default()
        };
        assert_eq!(env.effective_tier(), Tier::Color16);
    }

    #[test]
    fn ladder_colorfgbg_light() {
        let env = EnvParts {
            colorfgbg: Some("0;7".to_string()), // black fg, white bg → light
            ..Default::default()
        };
        assert!(env.is_light(None));
    }

    #[test]
    fn ladder_colorfgbg_dark() {
        let env = EnvParts {
            colorfgbg: Some("7;0".to_string()), // white fg, black bg → dark
            ..Default::default()
        };
        assert!(!env.is_light(None));
    }

    #[test]
    fn ladder_config_mode_overrides_colorfgbg() {
        let env = EnvParts {
            colorfgbg: Some("7;0".to_string()), // dark
            ..Default::default()
        };
        assert!(env.is_light(Some("light")));
    }

    #[test]
    fn ladder_default_is_dark() {
        let env = EnvParts::default();
        assert!(!env.is_light(None));
    }

    #[test]
    fn ladder_precedence_oom_edit_theme_overrides_colorterm() {
        // OOM_EDIT_THEME=accessible should force monochrome even with truecolor colorterm
        let env = EnvParts {
            oom_edit_theme: Some("accessible".to_string()),
            colorterm: Some("truecolor".to_string()),
            ..Default::default()
        };
        assert_eq!(env.effective_tier(), Tier::Monochrome);
        let resolved = resolve_theme(
            None,
            None,
            Some("default-dark"),
            Some("default-light"),
            &env,
        );
        assert_eq!(resolved.name, "accessible");
        assert_eq!(resolved.source, ThemeSource::Environment);
        assert_eq!(resolved.capability, Tier::TrueColor);
        assert_eq!(resolved.palette_kind, PaletteKind::Monochrome);
    }

    #[test]
    fn ladder_no_color_overrides_colorterm() {
        let env = EnvParts {
            no_color: true,
            colorterm: Some("truecolor".to_string()),
            ..Default::default()
        };
        assert_eq!(env.effective_tier(), Tier::Monochrome);
    }

    // ── Theme resolution ────────────────────────────────────────────────

    #[test]
    fn resolve_cli_theme_takes_priority() {
        let env = EnvParts::default();
        let resolved = resolve_theme(
            Some("default-light"),
            None,
            Some("default-dark"),
            Some("default-light"),
            &env,
        );
        assert_eq!(resolved.name, "default-light");
        assert_eq!(resolved.source, ThemeSource::Cli);
    }

    #[test]
    fn resolve_unknown_theme_falls_back() {
        let env = EnvParts::default();
        let resolved = resolve_theme(
            Some("nonexistent"),
            None,
            Some("default-dark"),
            Some("default-light"),
            &env,
        );
        assert_eq!(resolved.name, "default-dark");
        assert_eq!(resolved.source, ThemeSource::Fallback);
    }

    #[test]
    fn resolve_config_dark_slot() {
        let env = EnvParts::default();
        let resolved = resolve_theme(
            None,
            None,
            Some("default-dark"),
            Some("default-light"),
            &env,
        );
        assert_eq!(resolved.name, "default-dark");
        assert_eq!(resolved.source, ThemeSource::ConfigDark);
    }

    #[test]
    fn missing_config_defaults_to_default_dark() {
        let resolved = resolve_theme(
            None,
            None,
            None,
            None,
            &EnvParts {
                colorterm: Some("truecolor".to_string()),
                ..Default::default()
            },
        );
        assert_eq!(resolved.name, "default-dark");
        assert_eq!(resolved.source, ThemeSource::Fallback);
        assert_eq!(resolved.capability, Tier::TrueColor);
        assert_eq!(resolved.palette_kind, PaletteKind::TrueColor);
    }

    #[test]
    fn resolved_theme_reports_config_dark_accessible_as_monochrome() {
        let resolved = resolve_theme(
            None,
            Some("dark"),
            Some("accessible"),
            Some("default-light"),
            &EnvParts {
                colorterm: Some("truecolor".to_string()),
                ..Default::default()
            },
        );
        assert_eq!(resolved.name, "accessible");
        assert_eq!(resolved.source, ThemeSource::ConfigDark);
        assert_eq!(resolved.capability, Tier::TrueColor);
        assert_eq!(resolved.palette_kind, PaletteKind::Monochrome);
        assert_eq!(
            resolved.to_string(),
            "theme=accessible palette=monochrome capability=TrueColor source=config.dark dark"
        );
    }

    #[test]
    fn resolve_config_light_slot() {
        let env = EnvParts {
            colorfgbg: Some("0;7".to_string()), // light
            ..Default::default()
        };
        let resolved = resolve_theme(
            None,
            None,
            Some("default-dark"),
            Some("default-light"),
            &env,
        );
        assert_eq!(resolved.name, "default-light");
        assert_eq!(resolved.source, ThemeSource::ConfigLight);
    }

    #[test]
    fn resolve_accessible_theme_preserves_light_display_mode() {
        let env = EnvParts::default();
        let resolved = resolve_theme(
            None,
            Some("light"),
            Some("default-dark"),
            Some("accessible"),
            &env,
        );
        assert_eq!(resolved.name, "accessible");
        assert!(resolved.is_light());
        assert_eq!(resolved.palette_kind, PaletteKind::Monochrome);
    }

    #[test]
    fn resolve_config_slots_reject_opposite_mode_theme() {
        let env = EnvParts::default();

        let dark = resolve_theme(
            None,
            Some("dark"),
            Some("default-light"),
            Some("default-light"),
            &env,
        );
        assert_eq!(dark.name, "default-dark");
        assert!(!dark.is_light());

        let light = resolve_theme(
            None,
            Some("light"),
            Some("default-dark"),
            Some("default-dark"),
            &env,
        );
        assert_eq!(light.name, "default-light");
        assert!(light.is_light());

        for mode in ["dark", "light"] {
            let resolved = resolve_theme(
                None,
                Some(mode),
                Some("accessible"),
                Some("accessible"),
                &env,
            );
            assert_eq!(
                resolved.name, "accessible",
                "accessible must support {mode} mode"
            );
        }

        let dark_cli_override = resolve_theme(
            Some("default-light"),
            Some("dark"),
            Some("default-dark"),
            Some("default-light"),
            &env,
        );
        assert_eq!(dark_cli_override.name, "default-light");

        let light_cli_override = resolve_theme(
            Some("default-dark"),
            Some("light"),
            Some("default-dark"),
            Some("default-light"),
            &env,
        );
        assert_eq!(light_cli_override.name, "default-dark");
    }

    // ── Built-in themes ─────────────────────────────────────────────────

    #[test]
    fn built_in_themes_list() {
        let themes: Vec<_> = built_in_themes().collect();
        assert_eq!(themes.len(), 3);
        assert!(themes.contains(&"default-dark"));
        assert!(themes.contains(&"default-light"));
        assert!(themes.contains(&"accessible"));
    }

    #[test]
    fn builtin_theme_registry_has_unique_names_and_mode_defaults() {
        let mut names = std::collections::HashSet::new();
        for spec in BUILTIN_THEMES {
            assert!(names.insert(spec.theme.name));
        }
        let first_dark = BUILTIN_THEMES
            .iter()
            .find(|spec| {
                spec.compatible_mode
                    .is_none_or(|mode| mode == DisplayMode::Dark)
            })
            .unwrap();
        let first_light = BUILTIN_THEMES
            .iter()
            .find(|spec| {
                spec.compatible_mode
                    .is_none_or(|mode| mode == DisplayMode::Light)
            })
            .unwrap();
        assert_eq!(first_dark.theme.name, "default-dark");
        assert_eq!(first_light.theme.name, "default-light");
    }

    #[test]
    fn builtin_theme_registry_drives_all_projections() {
        assert_eq!(
            built_in_themes().collect::<Vec<_>>(),
            BUILTIN_THEMES
                .iter()
                .map(|spec| spec.theme.name)
                .collect::<Vec<_>>()
        );
        for (index, spec) in BUILTIN_THEMES.iter().enumerate() {
            assert!(std::ptr::eq(get_theme(spec.theme.name), spec.theme));
            for mode in [DisplayMode::Dark, DisplayMode::Light] {
                let is_light = mode == DisplayMode::Light;
                assert_eq!(
                    supports_mode(spec.theme.name, is_light),
                    spec.compatible_mode
                        .is_none_or(|compatible| compatible == mode)
                );
                if supports_mode(spec.theme.name, is_light) {
                    let expected = BUILTIN_THEMES
                        .iter()
                        .cycle()
                        .skip(index + 1)
                        .find(|candidate| {
                            candidate
                                .compatible_mode
                                .is_none_or(|compatible| compatible == mode)
                        })
                        .unwrap()
                        .theme
                        .name;
                    assert_eq!(cycle_theme(spec.theme.name, is_light), expected);
                }
            }
        }
    }

    #[test]
    fn cycle_theme_stays_within_display_mode() {
        assert_eq!(cycle_theme("default-dark", false), "accessible");
        assert_eq!(cycle_theme("accessible", false), "default-dark");
        assert_eq!(cycle_theme("default-light", true), "accessible");
        assert_eq!(cycle_theme("accessible", true), "default-light");

        assert_eq!(cycle_theme("default-light", false), "default-dark");
        assert_eq!(cycle_theme("default-dark", true), "default-light");
        assert_eq!(cycle_theme("nonexistent", false), "default-dark");
        assert_eq!(cycle_theme("nonexistent", true), "default-light");
    }

    #[test]
    fn default_dark_truecolor_matches_zed_onedark_md() {
        let expected_semantic = [
            (SemanticStyle::Text, Color::Rgb(200, 204, 212)),
            (SemanticStyle::Heading1, Color::Rgb(110, 180, 191)),
            (SemanticStyle::Heading2, Color::Rgb(191, 149, 106)),
            (SemanticStyle::Heading3, Color::Rgb(223, 193, 132)),
            (SemanticStyle::Heading4, Color::Rgb(161, 193, 129)),
            (SemanticStyle::Heading5, Color::Rgb(110, 180, 191)),
            (SemanticStyle::Heading6, Color::Rgb(180, 119, 207)),
            (SemanticStyle::Emphasis, Color::Rgb(180, 119, 207)),
            (SemanticStyle::Strong, Color::Rgb(223, 193, 132)),
            (SemanticStyle::CodeSpan, Color::Rgb(161, 193, 129)),
            (SemanticStyle::CodeBlock, Color::Rgb(161, 193, 129)),
            (SemanticStyle::ListMarker, Color::Rgb(208, 114, 119)),
            (SemanticStyle::Quote, Color::Rgb(223, 193, 132)),
            (SemanticStyle::Link, Color::Rgb(180, 119, 207)),
            (SemanticStyle::LinkUrl, Color::Rgb(110, 180, 191)),
            (SemanticStyle::Keyword, Color::Rgb(180, 119, 207)),
            (SemanticStyle::Function, Color::Rgb(115, 173, 233)),
            (SemanticStyle::TypeName, Color::Rgb(110, 180, 191)),
            (SemanticStyle::StringLit, Color::Rgb(161, 193, 129)),
            (SemanticStyle::NumberLit, Color::Rgb(191, 149, 106)),
            (SemanticStyle::Comment, Color::Rgb(93, 99, 111)),
            (SemanticStyle::Operator, Color::Rgb(200, 204, 212)),
            (SemanticStyle::Variable, Color::Rgb(200, 204, 212)),
        ];

        for (slot, expected) in expected_semantic {
            assert_eq!(
                DEFAULT_DARK.style(Tier::TrueColor, slot).fg,
                Some(expected),
                "unexpected Zed foreground for {slot:?}"
            );
        }

        let expected_ui = [
            (UiSlot::StatusBar, Color::Rgb(200, 204, 212)),
            (UiSlot::Border, Color::Rgb(47, 52, 62)),
            (UiSlot::HintKey, Color::Rgb(223, 193, 132)),
            (UiSlot::StatusError, Color::Rgb(208, 114, 119)),
            (UiSlot::Gutter, Color::Rgb(93, 99, 111)),
            (UiSlot::TabActive, Color::Rgb(200, 204, 212)),
        ];
        for (slot, expected) in expected_ui {
            assert_eq!(
                DEFAULT_DARK.ui_style(Tier::TrueColor, slot).fg,
                Some(expected),
                "unexpected Zed foreground for {slot:?}"
            );
        }
        for slot in [
            UiSlot::BadgeNormal,
            UiSlot::BadgeInsert,
            UiSlot::BadgeSelect,
            UiSlot::BadgeCommand,
        ] {
            assert_eq!(
                DEFAULT_DARK.ui_style(Tier::TrueColor, slot).fg,
                Some(Color::Rgb(0, 0, 0)),
                "{slot:?} must use deterministic black independently of the Zed palette"
            );
        }
        assert_eq!(
            DEFAULT_DARK.ui_style(Tier::TrueColor, UiSlot::StatusBar).bg,
            Some(Color::Rgb(47, 52, 62))
        );
        assert_eq!(
            DEFAULT_DARK
                .ui_style(Tier::TrueColor, UiSlot::CursorLine)
                .bg,
            Some(Color::Rgb(47, 52, 62))
        );
    }

    #[test]
    fn default_dark_truecolor_colored_entries_are_rgb() {
        fn assert_rgb(color: Color, description: &str) {
            assert!(
                matches!(color, Color::Rgb(_, _, _)),
                "{description} must use an exact RGB color, got {color:?}"
            );
        }

        let Palette::TrueColor { semantic, ui } = &DEFAULT_DARK.truecolor else {
            panic!("default-dark TrueColor palette changed tiers");
        };

        for (slot, foreground, _) in *semantic {
            if matches!(slot, SemanticStyle::Selection | SemanticStyle::CursorLine) {
                assert_eq!(
                    *foreground,
                    Color::Reset,
                    "semantic slot {slot:?} is an intentional composition sentinel"
                );
            } else {
                assert_rgb(*foreground, &format!("semantic slot {slot:?}"));
            }
        }
        for (slot, foreground, background, _) in *ui {
            if matches!(slot, UiSlot::CursorLine | UiSlot::MetadataPanel) {
                assert_eq!(
                    *foreground,
                    Color::Reset,
                    "UI cursor-line foreground is an intentional composition sentinel"
                );
            } else {
                assert_rgb(*foreground, &format!("UI slot {slot:?} foreground"));
            }
            if let Some(background) = background {
                assert_rgb(*background, &format!("UI slot {slot:?} background"));
            }
        }
    }

    // ── get_theme ───────────────────────────────────────────────────────

    #[test]
    fn get_theme_returns_correct_theme() {
        assert_eq!(get_theme("default-dark").name, "default-dark");
        assert_eq!(get_theme("default-light").name, "default-light");
        assert_eq!(get_theme("accessible").name, "accessible");
    }

    #[test]
    fn get_theme_unknown_returns_default_dark() {
        assert_eq!(get_theme("nonexistent").name, "default-dark");
    }

    // ── Legacy compatibility ────────────────────────────────────────────

    // ── Slot completeness ───────────────────────────────────────────────

    /// Every UiSlot variant is covered in every tier of every built-in theme.
    #[test]
    fn all_ui_slots_covered() {
        let themes: Vec<_> = BUILTIN_THEMES
            .iter()
            .map(|spec| (spec.theme.name, spec.theme))
            .collect();

        let slots = [
            UiSlot::StatusBar,
            UiSlot::BadgeNormal,
            UiSlot::BadgeInsert,
            UiSlot::BadgeSelect,
            UiSlot::BadgeCommand,
            UiSlot::Border,
            UiSlot::HintKey,
            UiSlot::HintDesc,
            UiSlot::StatusSuccess,
            UiSlot::StatusInfo,
            UiSlot::StatusWarning,
            UiSlot::StatusError,
            UiSlot::Gutter,
            UiSlot::GutterCurrent,
            UiSlot::CursorLine,
            UiSlot::TabActive,
            UiSlot::TabInactive,
            UiSlot::MetadataPanel,
            UiSlot::TabSeparator,
        ];

        for (name, theme) in &themes {
            for tier in [Tier::TrueColor, Tier::Color16, Tier::Monochrome] {
                let palette = theme.palette_for(tier);
                match palette {
                    Palette::TrueColor { ui, .. } | Palette::Color16 { ui, .. } => {
                        let ui_slots: Vec<UiSlot> = ui.iter().map(|(s, _, _, _)| *s).collect();
                        for slot in &slots {
                            assert!(
                                ui_slots.contains(slot),
                                "Theme {name} tier {tier:?} missing UI slot {slot:?}"
                            );
                        }
                    }
                    Palette::Monochrome { ui, .. } => {
                        let ui_slots: Vec<UiSlot> = ui.iter().map(|(s, _)| *s).collect();
                        for slot in &slots {
                            assert!(
                                ui_slots.contains(slot),
                                "Theme {name} tier {tier:?} missing UI slot {slot:?}"
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn metadata_panel_uses_theme_surface_at_every_tier() {
        for theme in [&DEFAULT_DARK, &DEFAULT_LIGHT] {
            for tier in [Tier::TrueColor, Tier::Color16] {
                let style = theme.ui_style(tier, UiSlot::MetadataPanel);
                assert!(style.bg.is_some(), "{} {tier:?}", theme.name);
                assert!(!style.add_modifier.is_empty(), "{} {tier:?}", theme.name);
            }
        }
        for theme in BUILTIN_THEMES.iter().map(|spec| spec.theme) {
            let style = theme.ui_style(Tier::Monochrome, UiSlot::MetadataPanel);
            assert_eq!(style.fg, Some(Color::Reset));
            assert!(!style.add_modifier.is_empty(), "{} monochrome", theme.name);
        }
    }

    #[test]
    fn default_dark_color16_chrome_preserves_previous_colors() {
        let theme = &DEFAULT_DARK;

        assert_eq!(
            theme.ui_style(Tier::Color16, UiSlot::CursorLine).bg,
            Some(Color::DarkGray)
        );
        assert_eq!(theme.ui_style(Tier::Color16, UiSlot::CursorLine).fg, None);
        assert_eq!(
            theme.ui_style(Tier::Color16, UiSlot::TabActive).fg,
            Some(Color::White)
        );
        assert_eq!(
            theme.ui_style(Tier::Color16, UiSlot::TabInactive).fg,
            Some(Color::Gray)
        );
        assert_eq!(
            theme.ui_style(Tier::Color16, UiSlot::TabSeparator).fg,
            Some(Color::DarkGray)
        );
    }

    #[test]
    fn default_light_chrome_and_text_use_light_palette() {
        let dark = &DEFAULT_DARK;
        let light = &DEFAULT_LIGHT;

        assert_ne!(
            dark.style(Tier::TrueColor, SemanticStyle::Text).fg,
            light.style(Tier::TrueColor, SemanticStyle::Text).fg
        );
        assert_ne!(
            dark.ui_style(Tier::TrueColor, UiSlot::TabActive).fg,
            light.ui_style(Tier::TrueColor, UiSlot::TabActive).fg
        );
        assert_ne!(
            dark.ui_style(Tier::TrueColor, UiSlot::CursorLine).bg,
            light.ui_style(Tier::TrueColor, UiSlot::CursorLine).bg
        );
    }

    #[test]
    fn color_badges_use_black_text_and_mode_backgrounds() {
        let badge_slots = [
            UiSlot::BadgeNormal,
            UiSlot::BadgeInsert,
            UiSlot::BadgeSelect,
            UiSlot::BadgeCommand,
        ];

        for theme in [&DEFAULT_DARK, &DEFAULT_LIGHT] {
            for tier in [Tier::TrueColor, Tier::Color16] {
                let status_background = theme
                    .ui_style(tier, UiSlot::StatusBar)
                    .bg
                    .expect("color status row must have a background");
                let mut backgrounds = std::collections::HashSet::new();
                for slot in badge_slots {
                    let style = theme.ui_style(tier, slot);
                    let expected_black = if theme.name == "default-dark" && tier == Tier::TrueColor
                    {
                        Color::Rgb(0, 0, 0)
                    } else {
                        Color::Black
                    };
                    assert_eq!(style.fg, Some(expected_black));
                    assert!(
                        style.add_modifier.contains(Modifier::BOLD),
                        "{slot:?} must retain its text carrier in {} at {tier:?}",
                        theme.name
                    );
                    let badge_background = style.bg.expect("color badge must have a background");
                    assert_ne!(
                        badge_background, status_background,
                        "{slot:?} must be visually distinct from the status row in {} at {tier:?}",
                        theme.name
                    );
                    backgrounds.insert(badge_background);
                }
                assert_eq!(
                    backgrounds.len(),
                    badge_slots.len(),
                    "mode badge backgrounds must be distinct in {} at {tier:?}",
                    theme.name
                );
            }
        }
    }

    #[test]
    fn monochrome_status_row_keeps_non_color_carriers() {
        let slots = [
            UiSlot::StatusBar,
            UiSlot::BadgeNormal,
            UiSlot::BadgeInsert,
            UiSlot::BadgeSelect,
            UiSlot::BadgeCommand,
        ];

        for theme in BUILTIN_THEMES.iter().map(|spec| spec.theme) {
            for slot in slots {
                let style = theme.ui_style(Tier::Monochrome, slot);
                assert_eq!(style.fg, Some(Color::Reset));
                assert_eq!(style.bg, None);
                assert!(
                    !style.add_modifier.is_empty(),
                    "{slot:?} needs a monochrome modifier carrier in {}",
                    theme.name
                );
            }
        }
    }

    #[test]
    fn accessible_status_row_is_color_free_at_every_tier() {
        let slots = [
            UiSlot::StatusBar,
            UiSlot::BadgeNormal,
            UiSlot::BadgeInsert,
            UiSlot::BadgeSelect,
            UiSlot::BadgeCommand,
        ];

        for tier in [Tier::TrueColor, Tier::Color16, Tier::Monochrome] {
            for slot in slots {
                let style = ACCESSIBLE.ui_style(tier, slot);
                assert_eq!(style.fg, Some(Color::Reset));
                assert_eq!(style.bg, None);
                assert!(
                    !style.add_modifier.is_empty(),
                    "{slot:?} needs an accessible modifier carrier at {tier:?}"
                );
            }
        }
    }

    #[test]
    fn chrome_slots_retain_non_color_modifiers() {
        let slots = [
            UiSlot::CursorLine,
            UiSlot::TabActive,
            UiSlot::TabInactive,
            UiSlot::MetadataPanel,
            UiSlot::TabSeparator,
        ];

        for theme in BUILTIN_THEMES.iter().map(|spec| spec.theme) {
            for tier in [Tier::TrueColor, Tier::Color16, Tier::Monochrome] {
                for slot in slots {
                    assert!(
                        !theme.ui_style(tier, slot).add_modifier.is_empty(),
                        "{slot:?} must retain a modifier for {} at {tier:?}",
                        theme.name
                    );
                }
            }
        }
    }

    fn rust_files_below(directory: &std::path::Path, files: &mut Vec<std::path::PathBuf>) {
        for entry in std::fs::read_dir(directory).expect("read TUI source directory") {
            let path = entry.expect("read TUI source entry").path();
            if path.is_dir() {
                rust_files_below(&path, files);
            } else if path.extension().is_some_and(|extension| extension == "rs") {
                files.push(path);
            }
        }
    }

    fn hardcoded_color_lines(source: &str) -> Vec<usize> {
        source
            .lines()
            .enumerate()
            .filter_map(|(index, line)| line.contains("Color::").then_some(index + 1))
            .collect()
    }

    #[test]
    fn hardcoded_color_detector_rejects_color_literal() {
        assert_eq!(
            hardcoded_color_lines("let style = Style::default().fg(Color::Red);"),
            vec![1]
        );
        assert_eq!(hardcoded_color_lines("ratatui::style::Color::Red"), vec![1]);
    }

    #[test]
    fn test_no_hardcoded_colors_outside_theme() {
        let source_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut files = Vec::new();
        rust_files_below(&source_root, &mut files);

        let mut violations = Vec::new();
        for path in files {
            if path.file_name().is_some_and(|name| name == "theme.rs") {
                continue;
            }
            let source = std::fs::read_to_string(&path).expect("read TUI Rust source");
            for line in hardcoded_color_lines(&source) {
                violations.push(format!("{}:{line}", path.display()));
            }
        }

        assert!(
            violations.is_empty(),
            "hardcoded colors must be defined only in theme.rs:\n{}",
            violations.join("\n")
        );
    }
}
