//! Status bar — mode badge, file info, transient messages, ruler, command line.
//!
//! Pure build + thin render: `build_status()` returns a ready-to-render
//! `StatusBarText`; `render()` maps it to ratatui widgets.
//!
//! Layout: `[left Min(1), badge Length(badge_w), ruler Length(RULER_COLS)]`
//! When Command mode is active, the whole row becomes `:{text}`.
//!
//! FR-6.2, FR-6.4.

use std::time::{Duration, Instant};

use ratatui::{
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use oom_edit_core::session::{Mode, Severity};

use crate::theme::{Theme, Tier, UiSlot};

/// Status bar transient message TTL: 4 seconds.
pub const TRANSIENT_TTL: Duration = Duration::from_secs(4);

/// Ruler column width: `line:col  n%` — max ~22 chars for large files.
pub const RULER_COLS: u16 = 22;

/// Severity glyph prefix for status messages.
fn severity_glyph(severity: Severity) -> &'static str {
    match severity {
        Severity::Error => "\u{2717} ",   // ✗
        Severity::Warning => "\u{26a0} ", // ⚠
        _ => "",
    }
}

/// A transient status message with TTL expiry.
#[derive(Debug, Clone)]
pub struct Transient {
    /// The message text (without severity glyph — the glyph is added at render time).
    pub text: String,
    /// Message severity.
    pub severity: Severity,
    /// When this message expires.
    pub expires_at: Instant,
}

impl Transient {
    /// Is this message still valid at `now`?
    pub fn is_expired(&self, now: Instant) -> bool {
        now >= self.expires_at
    }
}

/// Status bar state.
///
/// Pure build: `build()` takes all inputs and returns a `StatusBarText`
/// that the thin render layer maps to ratatui widgets.
pub struct StatusBar {
    /// Current mode for the badge.
    pub mode: Mode,
    /// File path (display name).
    pub path: String,
    /// Whether the buffer is dirty.
    pub dirty: bool,
    /// Whether the file is new (never saved — FR V-X5).
    pub is_new: bool,
    /// Cursor position (line, col) — 1-based for display.
    pub cursor_line: usize,
    /// Total line count (for percentage calculation).
    pub line_count: usize,
    /// Command-line text (when in Command mode or View-search active).
    pub command_line: Option<String>,
}

/// The rendered output of the status bar — ready for thin render.
#[derive(Debug, Default)]
pub struct StatusBarText {
    /// The full status line as spans (for rendering in the status area).
    /// Empty when command_line takeover is active.
    pub spans: Vec<Span<'static>>,
    /// When command-line takeover is active, the text to display.
    pub cmdline: Option<String>,
    /// Whether the cursor should be visible at the end of the command line.
    pub cmdline_cursor: bool,
}

impl StatusBar {
    /// Build the status bar text. Pure function — no rendering.
    pub fn build(&self, transient: Option<&Transient>, theme: &Theme, tier: Tier) -> StatusBarText {
        // Command-line takeover: when in Command mode or View-search,
        // the entire row becomes the command line.
        if let Some(ref cmdline) = self.command_line {
            let mut text = String::from(':');
            text.push_str(cmdline);
            return StatusBarText {
                spans: Vec::new(),
                cmdline: Some(text),
                cmdline_cursor: true,
            };
        }

        let mut spans = Vec::new();

        // Mode badge.
        let badge = mode_badge(self.mode);
        spans.push(Span::styled(
            badge,
            mode_badge_style(self.mode, theme, tier),
        ));

        // Left region: file + dirty + transient messages.
        let mut left = String::new();

        // File path (just the file name, not the full path).
        let display_name = self.path.split('/').next_back().unwrap_or(&self.path);
        left.push_str(display_name);

        if self.is_new {
            left.push_str(" [new file]");
        }

        if self.dirty {
            left.push_str(" [+]");
        }

        // Transient message with severity glyph.
        if let Some(t) = transient {
            let glyph = severity_glyph(t.severity);
            left.push(' ');
            left.push_str(glyph);
            left.push_str(&t.text);
        }

        if !left.is_empty() {
            spans.push(Span::raw(left));
        }

        // Right region: ruler.
        let ruler = ruler_text(self.cursor_line, self.line_count);
        spans.push(Span::raw(" ".to_string()));
        spans.push(Span::styled(
            ruler,
            Style::default().add_modifier(ratatui::style::Modifier::DIM),
        ));

        StatusBarText {
            spans,
            cmdline: None,
            cmdline_cursor: false,
        }
    }
}

/// Render the status bar text into the given area.
pub fn render(frame: &mut Frame<'_>, area: ratatui::layout::Rect, text: &StatusBarText) {
    let block = Block::default().borders(Borders::BOTTOM);
    frame.render_widget(block, area);

    if let Some(ref cmdline) = text.cmdline {
        // Command-line takeover: render the full text with a cursor at the end.
        let paragraph = Paragraph::new(cmdline.as_str());
        frame.render_widget(paragraph, area);

        if text.cmdline_cursor {
            // Place cursor at the end of the text.
            let col = area.x + cmdline.len().min(area.width as usize) as u16;
            frame.set_cursor_position(ratatui::layout::Position::new(
                col.min(area.x + area.width.saturating_sub(1)),
                area.y,
            ));
        }
    } else if !text.spans.is_empty() {
        let line = Line::from(text.spans.clone());
        let paragraph = Paragraph::new(line);
        frame.render_widget(paragraph, area);
    }
}

/// Return the mode badge string.
fn mode_badge(mode: Mode) -> &'static str {
    match mode {
        Mode::Normal => " NORMAL ",
        Mode::Insert => " INSERT ",
        Mode::Visual => " VISUAL ",
        Mode::VisualLine => " V-LINE ",
        Mode::VisualBlock => " V-BLOCK ",
        Mode::Command => " :CMD ",
        Mode::View => " VIEW ",
    }
}

/// Return the style for a mode badge.
fn mode_badge_style(mode: Mode, theme: &Theme, tier: Tier) -> Style {
    let slot = match mode {
        Mode::Normal => UiSlot::BadgeNormal,
        Mode::Insert => UiSlot::BadgeInsert,
        Mode::Visual | Mode::VisualLine | Mode::VisualBlock => UiSlot::BadgeVisual,
        Mode::Command => UiSlot::BadgeCommand,
        Mode::View => UiSlot::BadgeView,
    };
    theme.ui_style(tier, slot)
}

/// Build the ruler text: `line:col  n%` with Top/Bot at extremes.
///
/// Percent = cursor_line / line_count (1-based cursor, 1-based line_count).
/// At the top (line 1): shows `Top`. At the bottom (line == line_count): shows `Bot`.
/// Otherwise: `n%` or `Top`/`Bot` at extremes.
pub fn ruler_text(cursor_line: usize, line_count: usize) -> String {
    let cursor_display = cursor_line.max(1);
    let lc = line_count.max(1);

    if lc <= 1 {
        return "1:1  Top".to_string();
    }

    // Determine percentage and edge label.
    let pct = (cursor_display as f64 / lc as f64) * 100.0;
    let pct_int = pct as u64;

    let edge = if cursor_display == 1 {
        "Top"
    } else if cursor_display == lc || pct_int >= 100 || pct > 99.0 {
        "Bot"
    } else if pct < 1.0 {
        "Top"
    } else {
        ""
    };

    if edge.is_empty() {
        format!("{cursor_display}:1  {pct_int}%")
    } else {
        format!("{cursor_display}:1  {pct_int}% {edge}")
    }
}

/// Build the line-number gutter.
///
/// With relative line numbers disabled, all modes use absolute line numbers.
/// When enabled, Normal/Visual/Command modes use hybrid numbering — the
/// current line is absolute and other lines show their signed distance from
/// the cursor. Insert/View modes remain absolute.
///
/// `top_line` is the 0-based index of the first visible line.
/// `cursor_line` is the 0-based cursor line.
/// `height` is the viewport height in lines.
/// `line_count` is the total number of lines in the document.
/// `gutter_w` is the gutter width in columns.
///
/// Returns `Vec<String>` — one string per visible line.
pub fn build_gutter(
    mode: Mode,
    top_line: usize,
    cursor_line: usize,
    height: usize,
    line_count: usize,
    relative_line_numbers: bool,
    gutter_w: usize,
) -> Vec<String> {
    let mut lines = Vec::with_capacity(height);

    let show_absolute = !relative_line_numbers || matches!(mode, Mode::Insert | Mode::View);

    for i in 0..height {
        let line_num = top_line + i;
        if line_num >= line_count {
            // Padding line beyond document — blank with spaces.
            lines.push(" ".repeat(gutter_w));
            continue;
        }

        // Display line numbers are 1-based.
        let display_line = line_num + 1;

        if show_absolute {
            // Absolute line numbers, right-aligned.
            let text = format!("{display_line}");
            lines.push(format_gutter_cell(&text, gutter_w));
        } else {
            // Hybrid mode: current line absolute + highlighted, others relative.
            if line_num == cursor_line {
                // Current line: absolute number, right-aligned with padding for highlighting.
                let text = format!("{display_line}");
                lines.push(format_gutter_cell(&text, gutter_w));
            } else if line_num < cursor_line {
                // Above cursor: relative distance, right-aligned.
                let dist = cursor_line - line_num;
                let text = format!("-{dist}");
                lines.push(format_gutter_cell(&text, gutter_w));
            } else {
                // Below cursor: relative distance, right-aligned.
                let dist = line_num - cursor_line;
                let text = format!("+{dist}");
                lines.push(format_gutter_cell(&text, gutter_w));
            }
        }
    }

    lines
}

/// Blank terminal cells reserved between the final gutter digit and source text.
pub const GUTTER_CONTENT_GAP: usize = 2;

/// Right-align a gutter label while reserving the trailing content gap.
fn format_gutter_cell(text: &str, width: usize) -> String {
    let number_width = width.saturating_sub(GUTTER_CONTENT_GAP);
    format!("{text:>number_width$}{}", " ".repeat(GUTTER_CONTENT_GAP))
}

/// Compute the gutter width: number/sign width plus the trailing content gap.
pub fn gutter_width(line_count: usize) -> usize {
    let digits = if line_count == 0 {
        1
    } else {
        line_count.to_string().len()
    };
    digits.max(3) + 1 + GUTTER_CONTENT_GAP // +1 for the sign column in hybrid mode
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::{DEFAULT_DARK, DEFAULT_LIGHT};

    // ── Mode badge ──────────────────────────────────────────────────────

    #[test]
    fn mode_badge_all_modes() {
        assert_eq!(mode_badge(Mode::Normal), " NORMAL ");
        assert_eq!(mode_badge(Mode::Insert), " INSERT ");
        assert_eq!(mode_badge(Mode::Visual), " VISUAL ");
        assert_eq!(mode_badge(Mode::VisualLine), " V-LINE ");
        assert_eq!(mode_badge(Mode::VisualBlock), " V-BLOCK ");
        assert_eq!(mode_badge(Mode::Command), " :CMD ");
        assert_eq!(mode_badge(Mode::View), " VIEW ");
    }

    #[test]
    fn mode_badge_uses_selected_theme_and_tier() {
        let cases = [
            (Mode::Normal, UiSlot::BadgeNormal),
            (Mode::Insert, UiSlot::BadgeInsert),
            (Mode::Visual, UiSlot::BadgeVisual),
            (Mode::VisualLine, UiSlot::BadgeVisual),
            (Mode::VisualBlock, UiSlot::BadgeVisual),
            (Mode::Command, UiSlot::BadgeCommand),
            (Mode::View, UiSlot::BadgeView),
        ];

        for (mode, slot) in cases {
            assert_eq!(
                mode_badge_style(mode, &DEFAULT_DARK, Tier::TrueColor),
                DEFAULT_DARK.ui_style(Tier::TrueColor, slot)
            );
            assert_eq!(
                mode_badge_style(mode, &DEFAULT_LIGHT, Tier::TrueColor),
                DEFAULT_LIGHT.ui_style(Tier::TrueColor, slot)
            );
            assert_eq!(
                mode_badge_style(mode, &DEFAULT_DARK, Tier::Monochrome),
                DEFAULT_DARK.ui_style(Tier::Monochrome, slot)
            );
        }

        assert_ne!(
            mode_badge_style(Mode::Normal, &DEFAULT_DARK, Tier::TrueColor),
            mode_badge_style(Mode::Normal, &DEFAULT_LIGHT, Tier::TrueColor)
        );
    }

    // ── Severity glyph ──────────────────────────────────────────────────

    #[test]
    fn severity_glyph_error() {
        assert_eq!(severity_glyph(Severity::Error), "\u{2717} ");
    }

    #[test]
    fn severity_glyph_warning() {
        assert_eq!(severity_glyph(Severity::Warning), "\u{26a0} ");
    }

    #[test]
    fn severity_glyph_info() {
        assert_eq!(severity_glyph(Severity::Info), "");
    }

    #[test]
    fn severity_glyph_success() {
        assert_eq!(severity_glyph(Severity::Success), "");
    }

    // ── Ruler ───────────────────────────────────────────────────────────

    #[test]
    fn ruler_single_line() {
        let r = ruler_text(1, 1);
        assert!(r.contains("1:1"));
        assert!(r.contains("Top"));
    }

    #[test]
    fn ruler_top_edge() {
        let r = ruler_text(1, 100);
        assert!(r.contains("1:1"));
        assert!(r.contains("Top"));
    }

    #[test]
    fn ruler_bottom_edge() {
        let r = ruler_text(100, 100);
        assert!(r.contains("100:1"));
        assert!(r.contains("Bot"));
    }

    #[test]
    fn ruler_middle() {
        let r = ruler_text(50, 100);
        assert!(r.contains("50:1"));
        assert!(r.contains("50%"));
    }

    #[test]
    fn ruler_near_top() {
        let r = ruler_text(2, 100);
        assert!(r.contains("2:1"));
        // At line 2 of 100, percentage is ~2%, not "Top"
        assert!(r.contains("2%"));
    }

    #[test]
    fn ruler_near_bottom() {
        let r = ruler_text(99, 100);
        assert!(r.contains("99:1"));
        assert!(r.contains("99%"));
    }

    // ── Gutter ──────────────────────────────────────────────────────────

    #[test]
    fn gutter_absolute_insert_mode() {
        let gutter = build_gutter(
            Mode::Insert,
            0,   // top_line
            5,   // cursor_line
            10,  // height
            100, // line_count
            true,
            6, // gutter_w
        );
        assert_eq!(gutter.len(), 10);
        assert_eq!(gutter[0], "   1  "); // 1-based line numbers
        assert_eq!(gutter[5], "   6  ");
        assert_eq!(gutter[9], "  10  ");
    }

    #[test]
    fn gutter_absolute_view_mode() {
        let gutter = build_gutter(Mode::View, 0, 0, 5, 50, false, 6);
        assert_eq!(gutter[0], "   1  ");
        assert_eq!(gutter[4], "   5  ");
    }

    #[test]
    fn gutter_absolute_normal_when_relative_disabled() {
        let gutter = build_gutter(Mode::Normal, 0, 2, 5, 20, false, 6);
        assert_eq!(gutter, ["   1  ", "   2  ", "   3  ", "   4  ", "   5  "]);
    }

    #[test]
    fn gutter_hybrid_normal_mode() {
        let gutter = build_gutter(
            Mode::Normal,
            0,   // top_line
            5,   // cursor_line
            10,  // height
            100, // line_count
            true,
            6, // gutter_w
        );
        assert_eq!(gutter[5], "   6  "); // current line is absolute (0-based 5 = 1-based 6)
                                         // Lines above cursor show negative distance
        assert_eq!(gutter[4], "  -1  ");
        assert_eq!(gutter[3], "  -2  ");
        // Lines below cursor show positive distance
        assert_eq!(gutter[6], "  +1  ");
        assert_eq!(gutter[9], "  +4  ");
    }

    #[test]
    fn gutter_hybrid_visual_mode() {
        // Visual mode uses same hybrid as Normal
        let gutter = build_gutter(Mode::Visual, 0, 3, 5, 20, true, 6);
        assert_eq!(gutter[3], "   4  "); // current line absolute
        assert_eq!(gutter[2], "  -1  ");
        assert_eq!(gutter[4], "  +1  ");
    }

    #[test]
    fn gutter_opt_in_policy_covers_every_source_mode() {
        for mode in [
            Mode::Normal,
            Mode::Visual,
            Mode::VisualLine,
            Mode::VisualBlock,
            Mode::Command,
        ] {
            let gutter = build_gutter(mode, 0, 1, 3, 10, true, 6);
            assert_eq!(gutter, ["  -1  ", "   2  ", "  +1  "], "{mode:?}");
        }

        let insert = build_gutter(Mode::Insert, 0, 1, 3, 10, true, 6);
        assert_eq!(insert, ["   1  ", "   2  ", "   3  "]);
    }

    #[test]
    fn gutter_padding_beyond_document() {
        let gutter = build_gutter(
            Mode::Insert,
            95,  // top_line
            97,  // cursor_line
            10,  // height
            100, // line_count
            false,
            6, // gutter_w
        );
        // Lines 96-100 (0-based 95-99) are content, lines 101+ are padding
        assert_eq!(gutter[4], " 100  "); // last content line (top_line+4=99 < 100)
        assert_eq!(gutter[5], "      "); // first padding line (top_line+5=100 >= 100)
        assert_eq!(gutter[9], "      "); // padding beyond document
    }

    #[test]
    fn gutter_rows_end_with_two_column_gap() {
        let absolute = build_gutter(Mode::Insert, 999, 999, 2, 1000, false, 7);
        let relative = build_gutter(Mode::Normal, 0, 1, 2, 1000, true, 7);

        for row in [&absolute[0], &relative[0], &relative[1]] {
            assert_eq!(row.len(), 7);
            assert!(row.ends_with("  "));
            assert!(!row.ends_with("   "));
        }
        assert_eq!(absolute[0], " 1000  ");
        assert_eq!(absolute[1], "       ");
    }

    #[test]
    fn gutter_width_calculation() {
        assert_eq!(gutter_width(0), 6); // min number/sign width 4 + gap 2
        assert_eq!(gutter_width(9), 6);
        assert_eq!(gutter_width(99), 6);
        assert_eq!(gutter_width(100), 6);
        assert_eq!(gutter_width(999), 6);
        assert_eq!(gutter_width(1000), 7);
    }

    // ── Transient TTL ───────────────────────────────────────────────────

    #[test]
    fn transient_not_expired_before_ttl() {
        let now = Instant::now();
        let t = Transient {
            text: "test".to_string(),
            severity: Severity::Info,
            expires_at: now + Duration::from_secs(4),
        };
        assert!(!t.is_expired(now + Duration::from_secs(3)));
    }

    #[test]
    fn transient_expired_after_ttl() {
        let now = Instant::now();
        let t = Transient {
            text: "test".to_string(),
            severity: Severity::Info,
            expires_at: now + Duration::from_secs(4),
        };
        assert!(t.is_expired(now + Duration::from_secs(5)));
    }

    #[test]
    fn transient_ttl_expires() {
        let now = Instant::now();
        let expired_transient = Transient {
            text: "test".to_string(),
            severity: Severity::Info,
            expires_at: now - Duration::from_secs(1),
        };
        assert!(expired_transient.is_expired(now));
    }

    #[test]
    fn transient_ttl_keeps_valid() {
        let now = Instant::now();
        let valid_transient = Transient {
            text: "test".to_string(),
            severity: Severity::Info,
            expires_at: now + Duration::from_secs(10),
        };
        assert!(!valid_transient.is_expired(now));
    }

    // ── Command-line takeover ───────────────────────────────────────────

    #[test]
    fn build_command_line_takes_over() {
        let sb = StatusBar {
            mode: Mode::Command,
            path: "test.md".to_string(),
            dirty: false,
            is_new: false,
            cursor_line: 1,
            line_count: 10,
            command_line: Some("w".to_string()),
        };
        let text = sb.build(None, &DEFAULT_DARK, Tier::TrueColor);
        assert!(text.cmdline.is_some());
        assert_eq!(text.cmdline.as_deref(), Some(":w"));
        assert!(text.cmdline_cursor);
        assert!(text.spans.is_empty());
    }

    #[test]
    fn build_normal_no_takeover() {
        let sb = StatusBar {
            mode: Mode::Normal,
            path: "test.md".to_string(),
            dirty: false,
            is_new: false,
            cursor_line: 1,
            line_count: 10,
            command_line: None,
        };
        let text = sb.build(None, &DEFAULT_DARK, Tier::TrueColor);
        assert!(text.cmdline.is_none());
        assert!(!text.spans.is_empty());
    }

    #[test]
    fn test_status_bar_zero_width_no_panic() {
        use ratatui::{backend::TestBackend, layout::Rect, Terminal};

        let text = StatusBarText {
            spans: Vec::new(),
            cmdline: Some(":w".to_string()),
            cmdline_cursor: true,
        };
        let mut terminal = Terminal::new(TestBackend::new(1, 1)).expect("create test terminal");

        terminal
            .draw(|frame| {
                render(
                    frame,
                    Rect {
                        x: 0,
                        y: 0,
                        width: 0,
                        height: 1,
                    },
                    &text,
                );
            })
            .expect("render zero-width status bar");
    }

    // ── Status bar build ────────────────────────────────────────────────

    #[test]
    fn build_shows_dirty_marker() {
        let sb = StatusBar {
            mode: Mode::Normal,
            path: "test.md".to_string(),
            dirty: true,
            is_new: false,
            cursor_line: 1,
            line_count: 10,
            command_line: None,
        };
        let text = sb.build(None, &DEFAULT_DARK, Tier::TrueColor);
        let full_text: String = text.spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(full_text.contains("[+]"));
    }

    #[test]
    fn build_shows_transient_with_glyph() {
        let now = Instant::now();
        let transient = Transient {
            text: "read-only view".to_string(),
            severity: Severity::Info,
            expires_at: now + Duration::from_secs(4),
        };
        let sb = StatusBar {
            mode: Mode::Normal,
            path: "test.md".to_string(),
            dirty: false,
            is_new: false,
            cursor_line: 1,
            line_count: 10,
            command_line: None,
        };
        let text = sb.build(Some(&transient), &DEFAULT_DARK, Tier::TrueColor);
        let full_text: String = text.spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(full_text.contains("read-only view"));
    }

    #[test]
    fn build_error_message_has_x_glyph() {
        let now = Instant::now();
        let transient = Transient {
            text: "externally modified".to_string(),
            severity: Severity::Error,
            expires_at: now + Duration::from_secs(4),
        };
        let sb = StatusBar {
            mode: Mode::Normal,
            path: "test.md".to_string(),
            dirty: false,
            is_new: false,
            cursor_line: 1,
            line_count: 10,
            command_line: None,
        };
        let text = sb.build(Some(&transient), &DEFAULT_DARK, Tier::TrueColor);
        let full_text: String = text.spans.iter().map(|s| s.content.as_ref()).collect();
        // FR-6.2: error message renders with ✗ even on monochrome
        assert!(full_text.contains("\u{2717}"));
    }

    #[test]
    fn build_warning_message_has_triangle_glyph() {
        let now = Instant::now();
        let transient = Transient {
            text: "something off".to_string(),
            severity: Severity::Warning,
            expires_at: now + Duration::from_secs(4),
        };
        let sb = StatusBar {
            mode: Mode::Normal,
            path: "test.md".to_string(),
            dirty: false,
            is_new: false,
            cursor_line: 1,
            line_count: 10,
            command_line: None,
        };
        let text = sb.build(Some(&transient), &DEFAULT_DARK, Tier::TrueColor);
        let full_text: String = text.spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(full_text.contains("\u{26a0}"));
    }

    // ── Deadline computation ────────────────────────────────────────────

    /// Compute the next deadline from transient expiry and which-key pending.
    /// Used by the event loop to tighten poll deadlines.
    pub fn next_deadline(
        transient: Option<&Transient>,
        which_key_pending_since: Option<Instant>,
        now: Instant,
    ) -> Option<Instant> {
        let transient_deadline = transient.map(|t| t.expires_at);
        let which_key_deadline = which_key_pending_since.map(|s| s + Duration::from_millis(150));

        transient_deadline
            .into_iter()
            .chain(which_key_deadline)
            .min()
            .filter(|d| *d > now)
    }

    #[test]
    fn deadline_transient_only() {
        let now = Instant::now();
        let t = Transient {
            text: "test".to_string(),
            severity: Severity::Info,
            expires_at: now + Duration::from_secs(2),
        };
        let deadline = next_deadline(Some(&t), None, now);
        assert_eq!(deadline, Some(now + Duration::from_secs(2)));
    }

    #[test]
    fn deadline_which_key_only() {
        let now = Instant::now();
        let pending = now - Duration::from_millis(50);
        let deadline = next_deadline(None, Some(pending), now);
        // which_key_deadline = pending + 150ms = now + 100ms
        assert_eq!(deadline, Some(now + Duration::from_millis(100)));
    }

    #[test]
    fn deadline_min_of_both() {
        let now = Instant::now();
        let t = Transient {
            text: "test".to_string(),
            severity: Severity::Info,
            expires_at: now + Duration::from_secs(5),
        };
        let pending = now - Duration::from_millis(50);
        let deadline = next_deadline(Some(&t), Some(pending), now);
        // min(5s, 100ms) = 100ms
        assert_eq!(deadline, Some(now + Duration::from_millis(100)));
    }

    #[test]
    fn deadline_none_when_expired() {
        let now = Instant::now();
        let t = Transient {
            text: "test".to_string(),
            severity: Severity::Info,
            expires_at: now - Duration::from_secs(1), // already expired
        };
        let deadline = next_deadline(Some(&t), None, now);
        assert!(deadline.is_none());
    }

    #[test]
    fn deadline_none_when_no_inputs() {
        let deadline = next_deadline(None, None, Instant::now());
        assert!(deadline.is_none());
    }
}
