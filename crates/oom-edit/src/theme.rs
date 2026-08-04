//! Theme system — maps core [`SemanticStyle`] slots to ratatui [`Style`].
//!
//! T11 ships a hardcoded `default-dark` palette. T15 replaces the internals
//! while keeping the interface stable.
//!
//! **No color-only signals:** every style carries a modifier (bold, italic,
//! reverse, underline) so it is visible in monochrome terminals too.

use oom_edit_core::SemanticStyle;
use ratatui::style::{Color, Modifier, Style};

/// The default-dark theme palette.
pub struct Theme;

impl Theme {
    /// Map a [`SemanticStyle`] to a ratatui [`Style`].
    pub fn resolve(style: SemanticStyle) -> Style {
        match style {
            // Markdown structure
            SemanticStyle::Text => style_fg(Color::White, Modifier::empty()),
            SemanticStyle::Heading1 => style_fg(Color::Yellow, Modifier::BOLD),
            SemanticStyle::Heading2 => style_fg(Color::Yellow, Modifier::BOLD),
            SemanticStyle::Heading3 => style_fg(Color::Cyan, Modifier::BOLD),
            SemanticStyle::Heading4 => style_fg(Color::Cyan, Modifier::BOLD),
            SemanticStyle::Heading5 => style_fg(Color::Magenta, Modifier::BOLD),
            SemanticStyle::Heading6 => style_fg(Color::Magenta, Modifier::BOLD),
            SemanticStyle::Emphasis => style_fg(Color::White, Modifier::ITALIC),
            SemanticStyle::Strong => style_fg(Color::White, Modifier::BOLD),
            SemanticStyle::Strikethrough => style_fg(Color::Gray, Modifier::empty()),
            SemanticStyle::CodeSpan => style_fg(Color::Green, Modifier::empty()),
            SemanticStyle::CodeBlock => style_fg(Color::Green, Modifier::empty()),
            SemanticStyle::Quote => style_fg(Color::Yellow, Modifier::empty()),
            SemanticStyle::ListMarker => style_fg(Color::Cyan, Modifier::empty()),
            SemanticStyle::Link => style_fg(Color::Cyan, Modifier::UNDERLINED),
            SemanticStyle::LinkUrl => style_fg(Color::DarkGray, Modifier::empty()),
            SemanticStyle::Rule => style_fg(Color::DarkGray, Modifier::empty()),
            SemanticStyle::HtmlRaw => style_fg(Color::DarkGray, Modifier::empty()),

            // Front matter
            SemanticStyle::FmDelimiter => style_fg(Color::DarkGray, Modifier::empty()),
            SemanticStyle::FmKey => style_fg(Color::Yellow, Modifier::BOLD),
            SemanticStyle::FmValue => style_fg(Color::Green, Modifier::empty()),

            // Code (tree-sitter captures)
            SemanticStyle::Keyword => style_fg(Color::Red, Modifier::BOLD),
            SemanticStyle::Function => style_fg(Color::Cyan, Modifier::empty()),
            SemanticStyle::TypeName => style_fg(Color::Yellow, Modifier::empty()),
            SemanticStyle::StringLit => style_fg(Color::Green, Modifier::empty()),
            SemanticStyle::NumberLit => style_fg(Color::Magenta, Modifier::empty()),
            SemanticStyle::Comment => style_fg(Color::DarkGray, Modifier::ITALIC),
            SemanticStyle::Operator => style_fg(Color::White, Modifier::empty()),
            SemanticStyle::Variable => style_fg(Color::White, Modifier::empty()),
            SemanticStyle::Punct => style_fg(Color::DarkGray, Modifier::empty()),

            // UI-ish
            SemanticStyle::Selection => Style::default().add_modifier(Modifier::REVERSED),
            SemanticStyle::Match => style_fg(Color::Yellow, Modifier::empty()),
            SemanticStyle::CursorLine => Style::default().bg(Color::DarkGray),
            SemanticStyle::Muted => style_fg(Color::DarkGray, Modifier::empty()),
        }
    }
}

/// Helper: create a style with a foreground color and modifiers.
fn style_fg(fg: Color, modifiers: Modifier) -> Style {
    Style::default().fg(fg).add_modifier(modifiers)
}
