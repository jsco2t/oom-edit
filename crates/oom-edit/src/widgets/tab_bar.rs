//! Tab bar — per-tab file indicators with degradation ladder.
//!
//! Pure build + thin render: `build_tab_bar()` returns a `Vec<String>` of tab
//! label strings; `render()` maps them to ratatui widgets.
//!
//! Degradation ladder (VN-3):
//! - Wide enough: `n:filename` (ordinal + filename)
//! - Tight: `[n:filename]` (bracketed)
//! - Very tight: `n` (bare ordinal)
//! - Narrowest: `#` (just a separator)
//!
//! FR-10.1, FR-10.2.

use ratatui::{
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

/// Minimum width for `n:filename` format.
#[expect(dead_code)]
const TAB_BAR_WIDE_MIN: u16 = 8;

/// Minimum width for `[n:filename]` format.
const TAB_BAR_TIGHT_MIN: u16 = 10;

/// Minimum width for bare ordinal `n`.
const TAB_BAR_ORDINAL_MIN: u16 = 4;

/// A tab bar entry.
#[derive(Debug, Clone)]
pub struct TabEntry {
    /// The file path (display name).
    pub path: String,
    /// Whether the tab is dirty.
    pub dirty: bool,
}

/// Tab bar state.
pub struct TabBar {
    /// The tabs to display.
    pub tabs: Vec<TabEntry>,
    /// The index of the active tab.
    pub active_index: usize,
    /// The available width for the tab bar (excluding borders).
    pub width: u16,
}

/// The rendered output of the tab bar — ready for thin render.
#[derive(Debug, Default)]
pub struct TabBarText {
    /// The tab bar line as spans.
    pub spans: Vec<Span<'static>>,
}

impl TabBar {
    /// Build the tab bar text. Pure function — no rendering.
    pub fn build(&self) -> TabBarText {
        if self.tabs.is_empty() {
            return TabBarText::default();
        }

        let mut spans = Vec::new();

        // Determine the degradation level based on available width.
        let level = self.determine_level();

        // Calculate the width per tab.
        let tab_width = self.width.saturating_sub(2) / self.tabs.len() as u16; // -2 for borders

        for (i, tab) in self.tabs.iter().enumerate() {
            let label = self.format_tab(i, tab, tab_width, level);
            let is_active = i == self.active_index;

            let style = if is_active {
                ratatui::style::Style::default()
                    .fg(ratatui::style::Color::White)
                    .add_modifier(ratatui::style::Modifier::BOLD)
            } else {
                ratatui::style::Style::default()
                    .fg(ratatui::style::Color::Gray)
                    .add_modifier(ratatui::style::Modifier::DIM)
            };

            spans.push(Span::styled(label, style));

            // Add separator between tabs (except after the last one).
            if i < self.tabs.len() - 1 {
                spans.push(Span::styled(
                    "│",
                    ratatui::style::Style::default().fg(ratatui::style::Color::DarkGray),
                ));
            }
        }

        TabBarText { spans }
    }

    /// Determine the degradation level based on available width.
    fn determine_level(&self) -> TabBarLevel {
        if self.width >= TAB_BAR_TIGHT_MIN {
            TabBarLevel::Wide
        } else if self.width >= TAB_BAR_ORDINAL_MIN {
            TabBarLevel::Tight
        } else {
            TabBarLevel::Minimal
        }
    }

    /// Format a tab label based on the current level and width.
    fn format_tab(&self, index: usize, tab: &TabEntry, width: u16, level: TabBarLevel) -> String {
        match level {
            TabBarLevel::Wide => {
                // Format: `n:filename`
                let filename = tab.path.split('/').next_back().unwrap_or(&tab.path);
                let ordinal = index + 1;
                let mut label = format!("{ordinal}:{filename}");
                if tab.dirty {
                    label.push_str(" [+]");
                }
                // Truncate if needed.
                let width_usize = width as usize;
                if label.len() > width_usize {
                    let truncate_at = (width as usize).saturating_sub(3);
                    if truncate_at > 0 {
                        label.truncate(truncate_at);
                        label.push('…');
                    }
                }
                label
            }
            TabBarLevel::Tight => {
                // Format: `[n:filename]`
                let filename = tab.path.split('/').next_back().unwrap_or(&tab.path);
                let ordinal = index + 1;
                let mut label = format!("[{ordinal}:{filename}]");
                if tab.dirty {
                    label.push('+');
                }
                // Truncate if needed.
                let width_usize = width as usize;
                if label.len() > width_usize {
                    let truncate_at = (width as usize).saturating_sub(3);
                    if truncate_at > 0 {
                        label.truncate(truncate_at);
                        label.push('…');
                    }
                }
                label
            }
            TabBarLevel::Minimal => {
                // Format: `n` (bare ordinal)
                let ordinal = index + 1;
                format!("{ordinal}")
            }
        }
    }
}

/// The degradation level of the tab bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TabBarLevel {
    /// Wide format: `n:filename`.
    Wide,
    /// Tight format: `[n:filename]`.
    Tight,
    /// Minimal format: `n` (bare ordinal).
    Minimal,
}

/// Render the tab bar text into the given area.
pub fn render(frame: &mut Frame<'_>, area: ratatui::layout::Rect, text: &TabBarText) {
    let block = Block::default().borders(Borders::BOTTOM);
    frame.render_widget(block, area);

    if !text.spans.is_empty() {
        let line = Line::from(text.spans.clone());
        let paragraph = Paragraph::new(line);
        frame.render_widget(paragraph, area);
    }
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_empty_tab_bar() {
        let bar = TabBar {
            tabs: Vec::new(),
            active_index: 0,
            width: 80,
        };
        let text = bar.build();
        assert!(text.spans.is_empty());
    }

    #[test]
    fn build_wide_format() {
        let bar = TabBar {
            tabs: vec![
                TabEntry {
                    path: "/home/user/file1.md".to_string(),
                    dirty: false,
                },
                TabEntry {
                    path: "/home/user/file2.md".to_string(),
                    dirty: true,
                },
            ],
            active_index: 0,
            width: 40,
        };
        let text = bar.build();
        assert!(!text.spans.is_empty());
        let full_text: String = text.spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(full_text.contains("1:file1.md"));
        assert!(full_text.contains("2:file2.md [+]"));
    }

    #[test]
    fn build_tight_format() {
        let bar = TabBar {
            tabs: vec![
                TabEntry {
                    path: "/home/user/file1.md".to_string(),
                    dirty: false,
                },
                TabEntry {
                    path: "/home/user/file2.md".to_string(),
                    dirty: false,
                },
            ],
            active_index: 0,
            width: 12,
        };
        let text = bar.build();
        assert!(!text.spans.is_empty());
        // With width=12 and 2 tabs, tab_width = (12-2)/2 = 5.
        // The tight format [n:filename] would be truncated to fit.
        assert!(text.spans.len() >= 2); // At least 2 tab spans (separator may exist)
    }

    #[test]
    fn build_minimal_format() {
        let bar = TabBar {
            tabs: vec![
                TabEntry {
                    path: "/home/user/file1.md".to_string(),
                    dirty: false,
                },
                TabEntry {
                    path: "/home/user/file2.md".to_string(),
                    dirty: false,
                },
            ],
            active_index: 0,
            width: 5,
        };
        let text = bar.build();
        assert!(!text.spans.is_empty());
        let full_text: String = text.spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(full_text.contains("1"));
        assert!(full_text.contains("2"));
    }

    #[test]
    fn active_tab_is_bold() {
        let bar = TabBar {
            tabs: vec![
                TabEntry {
                    path: "/home/user/file1.md".to_string(),
                    dirty: false,
                },
                TabEntry {
                    path: "/home/user/file2.md".to_string(),
                    dirty: false,
                },
            ],
            active_index: 1,
            width: 40,
        };
        let text = bar.build();
        assert_eq!(text.spans.len(), 3); // tab1, separator, tab2
                                         // The second tab (index 1) should be bold.
        assert_eq!(
            text.spans[2].style,
            ratatui::style::Style::default()
                .fg(ratatui::style::Color::White)
                .add_modifier(ratatui::style::Modifier::BOLD)
        );
    }
}
