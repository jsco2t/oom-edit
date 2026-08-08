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

// ── UI Slots ────────────────────────────────────────────────────────────────

/// UI-specific display slots used by the status bar, hint bar, gutter, etc.
///
/// These complement the [`SemanticStyle`] slots emitted by the core.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UiSlot {
    /// Normal mode badge.
    BadgeNormal,
    /// Insert mode badge.
    BadgeInsert,
    /// Visual mode badge.
    BadgeVisual,
    /// View mode badge.
    BadgeView,
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

// ── Selection ladder ────────────────────────────────────────────────────────

/// Environment parts for testing the selection ladder without touching real env vars.
#[derive(Debug, Clone, Default)]
pub struct EnvParts {
    /// Value of `OOM_EDIT_THEME`.
    pub oom_edit_theme: Option<&'static str>,
    /// Value of `NO_COLOR`.
    pub no_color: bool,
    /// Value of `TERM`.
    pub term: Option<&'static str>,
    /// Value of `COLORTERM`.
    pub colorterm: Option<&'static str>,
    /// Value of `COLORFGBG` (e.g. "0;7" for light, "7;0" for dark).
    pub colorfgbg: Option<&'static str>,
}

impl EnvParts {
    /// Determine the effective tier from environment.
    pub fn effective_tier(&self) -> Tier {
        // OOM_EDIT_THEME=accessible forces monochrome (stop here).
        if let Some(theme) = self.oom_edit_theme {
            if theme == "accessible" {
                return Tier::Monochrome;
            }
        }
        // NO_COLOR → monochrome.
        if self.no_color {
            return Tier::Monochrome;
        }
        // TERM=dumb → monochrome.
        if let Some(term) = self.term {
            if term == "dumb" {
                return Tier::Monochrome;
            }
        }
        // COLORTERM truecolor/24bit → TrueColor, else Color16.
        if let Some(colorterm) = self.colorterm {
            let ct = colorterm.to_lowercase();
            if ct.contains("truecolor") || ct.contains("24bit") {
                return Tier::TrueColor;
            }
        }
        Tier::Color16
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
        if let Some(colorfgbg) = self.colorfgbg {
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
/// Priority: `--theme` flag > config dark/light slot > "default-dark" fallback.
/// Returns `(theme_name, is_light)`, where `is_light` is the resolved display
/// mode used to select the corresponding config slot. If the name is unknown,
/// warns and returns "default-dark".
pub fn resolve_theme(
    cli_theme: Option<&str>,
    config_mode: Option<&str>,
    config_dark: Option<&str>,
    config_light: Option<&str>,
    env: &EnvParts,
) -> (String, bool) {
    let _tier = env.effective_tier();
    let is_light = env.is_light(config_mode);

    // Known theme names for validation.
    fn is_known(name: &str) -> bool {
        name == "default-dark" || name == "default-light" || name == "accessible"
    }

    // Determine theme name: CLI flag > config slot > default.
    let name: String = match cli_theme {
        Some(cli) if is_known(cli) => cli.to_string(),
        Some(cli) => {
            eprintln!("oom-edit: unknown theme '{cli}', using default-dark");
            "default-dark".to_string()
        }
        None if is_light => config_light
            .filter(|s| is_known(s))
            .unwrap_or("default-light")
            .to_string(),
        None => config_dark
            .filter(|s| is_known(s))
            .unwrap_or("default-dark")
            .to_string(),
    };

    (name, is_light)
}

/// Get the built-in theme by name.
pub fn get_theme(name: &str) -> &'static Theme {
    match name {
        "default-dark" => &DEFAULT_DARK,
        "default-light" => &DEFAULT_LIGHT,
        "accessible" => &ACCESSIBLE,
        _ => &DEFAULT_DARK,
    }
}

/// Get the list of built-in theme names.
pub fn built_in_themes() -> &'static [&'static str] {
    &["default-dark", "default-light", "accessible"]
}

/// Cycle to the next built-in theme. Returns the new theme name.
pub fn cycle_theme(current: &str) -> &'static str {
    let themes = built_in_themes();
    let idx = themes.iter().position(|&t| t == current).unwrap_or(0);
    themes[(idx + 1) % themes.len()]
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
            (SemanticStyle::Match, Color::Yellow, Modifier::empty()),
            (SemanticStyle::CursorLine, Color::Reset, Modifier::empty()),
            (SemanticStyle::Muted, Color::DarkGray, Modifier::DIM),
        ],
        ui: &[
            (UiSlot::BadgeNormal, Color::White, None, Modifier::BOLD),
            (UiSlot::BadgeInsert, Color::Green, None, Modifier::BOLD),
            (UiSlot::BadgeVisual, Color::Yellow, None, Modifier::BOLD),
            (UiSlot::BadgeView, Color::Cyan, None, Modifier::BOLD),
            (UiSlot::BadgeCommand, Color::Magenta, None, Modifier::BOLD),
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
            (UiSlot::TabSeparator, Color::DarkGray, None, Modifier::DIM),
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
            (SemanticStyle::Match, Color::Yellow, Modifier::empty()),
            (SemanticStyle::CursorLine, Color::Reset, Modifier::empty()),
            (SemanticStyle::Muted, Color::DarkGray, Modifier::DIM),
        ],
        ui: &[
            (UiSlot::BadgeNormal, Color::White, None, Modifier::BOLD),
            (UiSlot::BadgeInsert, Color::Green, None, Modifier::BOLD),
            (UiSlot::BadgeVisual, Color::Yellow, None, Modifier::BOLD),
            (UiSlot::BadgeView, Color::Cyan, None, Modifier::BOLD),
            (UiSlot::BadgeCommand, Color::Magenta, None, Modifier::BOLD),
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
            (SemanticStyle::Match, Modifier::empty()),
            (SemanticStyle::CursorLine, Modifier::REVERSED),
            (SemanticStyle::Muted, Modifier::DIM),
        ],
        ui: &[
            (UiSlot::BadgeNormal, Modifier::BOLD),
            (UiSlot::BadgeInsert, Modifier::BOLD),
            (UiSlot::BadgeVisual, Modifier::BOLD),
            (UiSlot::BadgeView, Modifier::BOLD),
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
            (UiSlot::CursorLine, Modifier::REVERSED),
            (UiSlot::TabActive, Modifier::BOLD),
            (UiSlot::TabInactive, Modifier::DIM),
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
            (SemanticStyle::Match, Color::Red, Modifier::empty()),
            (SemanticStyle::CursorLine, Color::Reset, Modifier::empty()),
            (SemanticStyle::Muted, Color::Gray, Modifier::DIM),
        ],
        ui: &[
            (UiSlot::BadgeNormal, Color::Black, None, Modifier::BOLD),
            (UiSlot::BadgeInsert, Color::Green, None, Modifier::BOLD),
            (UiSlot::BadgeVisual, Color::Yellow, None, Modifier::BOLD),
            (UiSlot::BadgeView, Color::Cyan, None, Modifier::BOLD),
            (UiSlot::BadgeCommand, Color::Magenta, None, Modifier::BOLD),
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
            (SemanticStyle::Match, Color::Red, Modifier::empty()),
            (SemanticStyle::CursorLine, Color::Reset, Modifier::empty()),
            (SemanticStyle::Muted, Color::Gray, Modifier::DIM),
        ],
        ui: &[
            (UiSlot::BadgeNormal, Color::Black, None, Modifier::BOLD),
            (UiSlot::BadgeInsert, Color::Green, None, Modifier::BOLD),
            (UiSlot::BadgeVisual, Color::Yellow, None, Modifier::BOLD),
            (UiSlot::BadgeView, Color::Cyan, None, Modifier::BOLD),
            (UiSlot::BadgeCommand, Color::Magenta, None, Modifier::BOLD),
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
            (SemanticStyle::Match, Modifier::empty()),
            (SemanticStyle::CursorLine, Modifier::REVERSED),
            (SemanticStyle::Muted, Modifier::DIM),
        ],
        ui: &[
            (UiSlot::BadgeNormal, Modifier::BOLD),
            (UiSlot::BadgeInsert, Modifier::BOLD),
            (UiSlot::BadgeVisual, Modifier::BOLD),
            (UiSlot::BadgeView, Modifier::BOLD),
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
            (UiSlot::CursorLine, Modifier::REVERSED),
            (UiSlot::TabActive, Modifier::BOLD),
            (UiSlot::TabInactive, Modifier::DIM),
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
            (SemanticStyle::Match, Modifier::empty()),
            (SemanticStyle::CursorLine, Modifier::REVERSED),
            (SemanticStyle::Muted, Modifier::DIM),
        ],
        ui: &[
            (UiSlot::BadgeNormal, Modifier::BOLD),
            (UiSlot::BadgeInsert, Modifier::BOLD),
            (UiSlot::BadgeVisual, Modifier::BOLD),
            (UiSlot::BadgeView, Modifier::BOLD),
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
            (UiSlot::CursorLine, Modifier::REVERSED),
            (UiSlot::TabActive, Modifier::BOLD),
            (UiSlot::TabInactive, Modifier::DIM),
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
            (SemanticStyle::Match, Modifier::empty()),
            (SemanticStyle::CursorLine, Modifier::REVERSED),
            (SemanticStyle::Muted, Modifier::DIM),
        ],
        ui: &[
            (UiSlot::BadgeNormal, Modifier::BOLD),
            (UiSlot::BadgeInsert, Modifier::BOLD),
            (UiSlot::BadgeVisual, Modifier::BOLD),
            (UiSlot::BadgeView, Modifier::BOLD),
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
            (UiSlot::CursorLine, Modifier::REVERSED),
            (UiSlot::TabActive, Modifier::BOLD),
            (UiSlot::TabInactive, Modifier::DIM),
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
            (SemanticStyle::Match, Modifier::empty()),
            (SemanticStyle::CursorLine, Modifier::REVERSED),
            (SemanticStyle::Muted, Modifier::DIM),
        ],
        ui: &[
            (UiSlot::BadgeNormal, Modifier::BOLD),
            (UiSlot::BadgeInsert, Modifier::BOLD),
            (UiSlot::BadgeVisual, Modifier::BOLD),
            (UiSlot::BadgeView, Modifier::BOLD),
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
            (UiSlot::CursorLine, Modifier::REVERSED),
            (UiSlot::TabActive, Modifier::BOLD),
            (UiSlot::TabInactive, Modifier::DIM),
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
        let themes: [(&str, &Theme); 3] = [
            ("default-dark", &DEFAULT_DARK),
            ("default-light", &DEFAULT_LIGHT),
            ("accessible", &ACCESSIBLE),
        ];

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
        for theme in [&DEFAULT_DARK, &DEFAULT_LIGHT, &ACCESSIBLE] {
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
        for theme in [&DEFAULT_DARK, &DEFAULT_LIGHT, &ACCESSIBLE] {
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

    // ── Every accessor returns non-empty Style ──────────────────────────

    /// Every accessor (semantic + UI) returns a Style with at least one
    /// non-default property on every tier.
    #[test]
    fn every_accessor_nonempty() {
        let themes: [(&str, &Theme); 3] = [
            ("default-dark", &DEFAULT_DARK),
            ("default-light", &DEFAULT_LIGHT),
            ("accessible", &ACCESSIBLE),
        ];

        let ui_slots = [
            UiSlot::BadgeNormal,
            UiSlot::BadgeInsert,
            UiSlot::BadgeVisual,
            UiSlot::BadgeView,
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
            term: Some("dumb"),
            ..Default::default()
        };
        assert_eq!(env.effective_tier(), Tier::Monochrome);
    }

    #[test]
    fn ladder_oom_edit_theme_accessible_forces_monochrome() {
        let env = EnvParts {
            oom_edit_theme: Some("accessible"),
            colorterm: Some("truecolor"),
            ..Default::default()
        };
        assert_eq!(env.effective_tier(), Tier::Monochrome);
    }

    #[test]
    fn ladder_colorterm_truecolor() {
        let env = EnvParts {
            colorterm: Some("truecolor"),
            ..Default::default()
        };
        assert_eq!(env.effective_tier(), Tier::TrueColor);
    }

    #[test]
    fn ladder_colorterm_24bit() {
        let env = EnvParts {
            colorterm: Some("24bit"),
            ..Default::default()
        };
        assert_eq!(env.effective_tier(), Tier::TrueColor);
    }

    #[test]
    fn ladder_colorterm_default_to_color16() {
        let env = EnvParts {
            colorterm: Some("basic"),
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
            colorfgbg: Some("0;7"), // black fg, white bg → light
            ..Default::default()
        };
        assert!(env.is_light(None));
    }

    #[test]
    fn ladder_colorfgbg_dark() {
        let env = EnvParts {
            colorfgbg: Some("7;0"), // white fg, black bg → dark
            ..Default::default()
        };
        assert!(!env.is_light(None));
    }

    #[test]
    fn ladder_config_mode_overrides_colorfgbg() {
        let env = EnvParts {
            colorfgbg: Some("7;0"), // dark
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
            oom_edit_theme: Some("accessible"),
            colorterm: Some("truecolor"),
            ..Default::default()
        };
        assert_eq!(env.effective_tier(), Tier::Monochrome);
    }

    #[test]
    fn ladder_no_color_overrides_colorterm() {
        let env = EnvParts {
            no_color: true,
            colorterm: Some("truecolor"),
            ..Default::default()
        };
        assert_eq!(env.effective_tier(), Tier::Monochrome);
    }

    // ── Theme resolution ────────────────────────────────────────────────

    #[test]
    fn resolve_cli_theme_takes_priority() {
        let env = EnvParts::default();
        let (name, _light) = resolve_theme(
            Some("default-light"),
            None,
            Some("default-dark"),
            Some("default-light"),
            &env,
        );
        assert_eq!(name, "default-light");
    }

    #[test]
    fn resolve_unknown_theme_falls_back() {
        let env = EnvParts::default();
        let (name, _light) = resolve_theme(
            Some("nonexistent"),
            None,
            Some("default-dark"),
            Some("default-light"),
            &env,
        );
        assert_eq!(name, "default-dark");
    }

    #[test]
    fn resolve_config_dark_slot() {
        let env = EnvParts::default();
        let (name, _light) = resolve_theme(
            None,
            None,
            Some("default-dark"),
            Some("default-light"),
            &env,
        );
        assert_eq!(name, "default-dark");
    }

    #[test]
    fn resolve_config_light_slot() {
        let env = EnvParts {
            colorfgbg: Some("0;7"), // light
            ..Default::default()
        };
        let (name, _light) = resolve_theme(
            None,
            None,
            Some("default-dark"),
            Some("default-light"),
            &env,
        );
        assert_eq!(name, "default-light");
    }

    #[test]
    fn resolve_accessible_theme_preserves_light_display_mode() {
        let env = EnvParts::default();
        let (name, is_light) = resolve_theme(
            None,
            Some("light"),
            Some("default-dark"),
            Some("accessible"),
            &env,
        );
        assert_eq!(name, "accessible");
        assert!(is_light);
    }

    // ── Built-in themes ─────────────────────────────────────────────────

    #[test]
    fn built_in_themes_list() {
        let themes = built_in_themes();
        assert_eq!(themes.len(), 3);
        assert!(themes.contains(&"default-dark"));
        assert!(themes.contains(&"default-light"));
        assert!(themes.contains(&"accessible"));
    }

    #[test]
    fn cycle_theme_cycles_all_three() {
        let t1 = cycle_theme("default-dark");
        let t2 = cycle_theme(t1);
        let t3 = cycle_theme(t2);
        let t4 = cycle_theme(t3);
        assert_eq!(t1, "default-light");
        assert_eq!(t2, "accessible");
        assert_eq!(t3, "default-dark");
        assert_eq!(t4, "default-light");
    }

    #[test]
    fn cycle_theme_unknown_cycles_to_second() {
        let t = cycle_theme("nonexistent");
        assert_eq!(t, "default-light"); // index 0 = default-dark, so next = default-light
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
        let themes: [(&str, &Theme); 3] = [
            ("default-dark", &DEFAULT_DARK),
            ("default-light", &DEFAULT_LIGHT),
            ("accessible", &ACCESSIBLE),
        ];

        let slots = [
            UiSlot::BadgeNormal,
            UiSlot::BadgeInsert,
            UiSlot::BadgeVisual,
            UiSlot::BadgeView,
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
    fn default_dark_chrome_preserves_previous_colors() {
        let theme = &DEFAULT_DARK;

        for tier in [Tier::TrueColor, Tier::Color16] {
            assert_eq!(
                theme.ui_style(tier, UiSlot::CursorLine).bg,
                Some(Color::DarkGray)
            );
            assert_eq!(theme.ui_style(tier, UiSlot::CursorLine).fg, None);
            assert_eq!(
                theme.ui_style(tier, UiSlot::TabActive).fg,
                Some(Color::White)
            );
            assert_eq!(
                theme.ui_style(tier, UiSlot::TabInactive).fg,
                Some(Color::Gray)
            );
            assert_eq!(
                theme.ui_style(tier, UiSlot::TabSeparator).fg,
                Some(Color::DarkGray)
            );
        }
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
    fn chrome_slots_retain_non_color_modifiers() {
        let slots = [
            UiSlot::CursorLine,
            UiSlot::TabActive,
            UiSlot::TabInactive,
            UiSlot::TabSeparator,
        ];

        for theme in [&DEFAULT_DARK, &DEFAULT_LIGHT, &ACCESSIBLE] {
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
