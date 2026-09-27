//! Renderer-neutral colors and text modifiers exposed to embedding hosts.

use ratatui::style::{Modifier, Style};

/// One terminal color without a renderer-specific type.
///
/// Renderer conversion is private, including conversion traits.
/// ```compile_fail
/// let renderer_color: ratatui::style::Color = Default::default();
/// let _ = oom_edit::ColorValue::from(renderer_color);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorValue {
    /// Terminal default foreground or background.
    Default,
    /// ANSI palette index, including the first sixteen named colors.
    Indexed(u8),
    /// Twenty-four-bit RGB color.
    Rgb {
        /// Red component.
        red: u8,
        /// Green component.
        green: u8,
        /// Blue component.
        blue: u8,
    },
}

/// Renderer-neutral text effects. Every field has an independent meaning.
///
/// ```compile_fail
/// let _ = oom_edit::StyleModifiers::from(ratatui::style::Modifier::BOLD);
/// ```
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StyleModifiers {
    /// Use bold text.
    pub bold: bool,
    /// Use dimmed text.
    pub dim: bool,
    /// Use italic text.
    pub italic: bool,
    /// Underline text.
    pub underlined: bool,
    /// Use slow blinking.
    pub slow_blink: bool,
    /// Use rapid blinking.
    pub rapid_blink: bool,
    /// Reverse foreground and background.
    pub reversed: bool,
    /// Hide text without changing its cell width.
    pub hidden: bool,
    /// Strike through text.
    pub crossed_out: bool,
}

impl StyleModifiers {
    pub(crate) fn from_ratatui(modifiers: Modifier) -> Self {
        Self {
            bold: modifiers.contains(Modifier::BOLD),
            dim: modifiers.contains(Modifier::DIM),
            italic: modifiers.contains(Modifier::ITALIC),
            underlined: modifiers.contains(Modifier::UNDERLINED),
            slow_blink: modifiers.contains(Modifier::SLOW_BLINK),
            rapid_blink: modifiers.contains(Modifier::RAPID_BLINK),
            reversed: modifiers.contains(Modifier::REVERSED),
            hidden: modifiers.contains(Modifier::HIDDEN),
            crossed_out: modifiers.contains(Modifier::CROSSED_OUT),
        }
    }
}

/// Owned style for host chrome and pane cells.
///
/// A missing color means that the style leaves the existing cell color alone;
/// [`ColorValue::Default`] explicitly requests the terminal default.
///
/// ```compile_fail
/// let _ = oom_edit::OwnedStyle::from(ratatui::style::Style::default());
/// ```
/// ```compile_fail
/// let _ = oom_edit::OwnedStyle::from_ratatui(ratatui::style::Style::default());
/// ```
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct OwnedStyle {
    /// Optional foreground color; Default explicitly requests terminal reset.
    pub foreground: Option<ColorValue>,
    /// Optional background color; Default explicitly requests terminal reset.
    pub background: Option<ColorValue>,
    /// Optional underline color.
    pub underline_color: Option<ColorValue>,
    /// Text effects to apply.
    pub modifiers: StyleModifiers,
    /// Text effects to remove.
    pub removed_modifiers: StyleModifiers,
}

impl OwnedStyle {
    pub(crate) fn to_ratatui(self) -> Style {
        Style {
            fg: self.foreground.map(ColorValue::to_ratatui),
            bg: self.background.map(ColorValue::to_ratatui),
            underline_color: self.underline_color.map(ColorValue::to_ratatui),
            add_modifier: self.modifiers.to_ratatui(),
            sub_modifier: self.removed_modifiers.to_ratatui(),
        }
    }

    pub(crate) fn from_ratatui(style: Style) -> Self {
        Self {
            foreground: style.fg.map(ColorValue::from_ratatui),
            background: style.bg.map(ColorValue::from_ratatui),
            underline_color: style.underline_color.map(ColorValue::from_ratatui),
            modifiers: StyleModifiers::from_ratatui(style.add_modifier),
            removed_modifiers: StyleModifiers::from_ratatui(style.sub_modifier),
        }
    }
}

impl StyleModifiers {
    fn to_ratatui(self) -> Modifier {
        let mut result = Modifier::empty();
        for (enabled, modifier) in [
            (self.bold, Modifier::BOLD),
            (self.dim, Modifier::DIM),
            (self.italic, Modifier::ITALIC),
            (self.underlined, Modifier::UNDERLINED),
            (self.slow_blink, Modifier::SLOW_BLINK),
            (self.rapid_blink, Modifier::RAPID_BLINK),
            (self.reversed, Modifier::REVERSED),
            (self.hidden, Modifier::HIDDEN),
            (self.crossed_out, Modifier::CROSSED_OUT),
        ] {
            if enabled {
                result.insert(modifier);
            }
        }
        result
    }
}
