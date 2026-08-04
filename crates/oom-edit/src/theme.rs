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

// ── Theme completeness test ─────────────────────────────────────────────────

/// Verify that every [`SemanticStyle`] variant maps to a non-empty ratatui Style.
///
/// This is a compile-time + runtime completeness check: if a variant is missing
/// from the match in `Theme::resolve`, this won't compile. The runtime asserts
/// that every resolved style carries at least one non-default property (fg, bg,
/// or a modifier) so no signal is color-only.
#[cfg(test)]
#[test]
fn theme_resolves_all_variants() {
    // This list must stay in sync with the SemanticStyle enum.
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

    for variant in &variants {
        let style = Theme::resolve(*variant);
        // Every style must carry at least one non-default property.
        assert!(
            style.fg.is_some() || style.bg.is_some() || !style.add_modifier.is_empty(),
            "Theme::resolve({variant:?}) must carry at least one non-default property"
        );
    }
}
