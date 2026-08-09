//! Status bar — mode badge, file info, transient messages, ruler, command line.
//!
//! Pure build + thin render: `build_status()` returns a ready-to-render
//! `StatusBarText`; `render()` maps it to ratatui widgets.
//!
//! Layout: `[badge Length(MODE_BADGE_COLS), middle Min(0), ruler Length(RULER_COLS)]`.
//! Command and View-search prompts use the middle plus ruler region while the
//! badge remains fixed at the left edge.
//!
//! FR-6.2, FR-6.4.

use std::time::{Duration, Instant};

use ratatui::{
    layout::{Alignment, Position, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Block, Paragraph},
    Frame,
};

use oom_edit_core::session::{Mode, Severity};

use crate::theme::{Theme, Tier, UiSlot};

/// Status bar transient message TTL: 4 seconds.
pub const TRANSIENT_TTL: Duration = Duration::from_secs(4);

/// Ruler column width: `line:col  n%` — max ~22 chars for large files.
pub const RULER_COLS: u16 = 22;

/// Fixed mode-badge width, sized for the longest label: ` V-BLOCK `.
pub const MODE_BADGE_COLS: u16 = 9;

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
    /// Mode label and its theme-selected badge style.
    pub badge: Span<'static>,
    /// File/transient content for the flexible middle region.
    pub content: Vec<Span<'static>>,
    /// Ruler text for the fixed right region.
    pub ruler: Span<'static>,
    /// Active command or View-search prompt, including its prefix.
    pub prompt: Option<String>,
    /// Whether the cursor should be visible at the end of the prompt.
    pub prompt_cursor: bool,
}

impl StatusBar {
    /// Build the status bar text. Pure function — no rendering.
    pub fn build(&self, transient: Option<&Transient>, theme: &Theme, tier: Tier) -> StatusBarText {
        let badge = Span::styled(
            mode_badge(self.mode),
            mode_badge_style(self.mode, theme, tier),
        );

        // Flexible content: file + dirty + transient messages.
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

        let content = (!left.is_empty())
            .then(|| Span::raw(left))
            .into_iter()
            .collect();
        let ruler = Span::styled(
            ruler_text(self.cursor_line, self.line_count),
            Style::default().add_modifier(ratatui::style::Modifier::DIM),
        );

        StatusBarText {
            badge,
            content,
            ruler,
            prompt: self.command_line.clone(),
            prompt_cursor: self.command_line.is_some(),
        }
    }
}

/// Render the status bar text into the given area.
pub fn render(
    frame: &mut Frame<'_>,
    area: Rect,
    text: &StatusBarText,
    middle: &str,
    show_ruler: bool,
    theme: &Theme,
    tier: Tier,
) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    let row_style = theme.ui_style(tier, UiSlot::StatusBar);
    frame.render_widget(Block::default().style(row_style), area);

    let badge_width = MODE_BADGE_COLS.min(area.width);
    let badge_area = Rect::new(area.x, area.y, badge_width, 1.min(area.height));
    frame.render_widget(Block::default().style(text.badge.style), badge_area);
    frame.render_widget(Paragraph::new(Line::from(text.badge.clone())), badge_area);

    let flexible_x = area.x.saturating_add(badge_width);
    let flexible_width = area.width.saturating_sub(badge_width);
    if flexible_width == 0 {
        return;
    }

    if let Some(prompt) = &text.prompt {
        let prompt_area = Rect::new(flexible_x, area.y, flexible_width, 1.min(area.height));
        frame.render_widget(
            Paragraph::new(prompt.as_str()).style(row_style),
            prompt_area,
        );

        if text.prompt_cursor {
            let prompt_width = Line::from(prompt.as_str()).width() as u16;
            let col = flexible_x
                .saturating_add(prompt_width)
                .min(area.x.saturating_add(area.width).saturating_sub(1));
            frame.set_cursor_position(Position::new(col, area.y));
        }
        return;
    }

    let ruler_width = if show_ruler {
        RULER_COLS.min(flexible_width)
    } else {
        0
    };
    let middle_width = flexible_width.saturating_sub(ruler_width);
    let middle_area = Rect::new(flexible_x, area.y, middle_width, 1.min(area.height));
    let ruler_area = Rect::new(
        flexible_x.saturating_add(middle_width),
        area.y,
        ruler_width,
        1.min(area.height),
    );

    if middle_width > 0 {
        let middle_line = if middle.is_empty() {
            Line::from(text.content.clone())
        } else {
            Line::from(middle).style(Style::default().add_modifier(ratatui::style::Modifier::DIM))
        };
        frame.render_widget(Paragraph::new(middle_line).style(row_style), middle_area);
    }
    if ruler_width > 0 {
        frame.render_widget(
            Paragraph::new(Line::from(text.ruler.clone()))
                .alignment(Alignment::Right)
                .style(row_style),
            ruler_area,
        );
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
            DEFAULT_DARK.ui_style(Tier::TrueColor, UiSlot::StatusBar),
            DEFAULT_LIGHT.ui_style(Tier::TrueColor, UiSlot::StatusBar)
        );
    }

    fn render_status(
        mode: Mode,
        width: u16,
        tier: Tier,
    ) -> ratatui::Terminal<ratatui::backend::TestBackend> {
        let status = StatusBar {
            mode,
            path: "test.md".to_string(),
            dirty: false,
            is_new: false,
            cursor_line: 1,
            line_count: 10,
            command_line: None,
        };
        let text = status.build(None, &DEFAULT_DARK, tier);
        let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(width, 1))
            .expect("create status test terminal");
        terminal
            .draw(|frame| {
                render(
                    frame,
                    frame.area(),
                    &text,
                    "Space h=help",
                    true,
                    &DEFAULT_DARK,
                    tier,
                );
            })
            .expect("render status row");
        terminal
    }

    #[test]
    fn status_row_badge_starts_at_column_zero_and_has_constant_width() {
        let modes = [
            Mode::Normal,
            Mode::Insert,
            Mode::Visual,
            Mode::VisualLine,
            Mode::VisualBlock,
            Mode::Command,
            Mode::View,
        ];

        for mode in modes {
            let terminal = render_status(mode, 80, Tier::TrueColor);
            let buffer = terminal.backend().buffer();
            let badge_style = mode_badge_style(mode, &DEFAULT_DARK, Tier::TrueColor);
            let label: String = (0..MODE_BADGE_COLS)
                .map(|x| buffer.cell((x, 0)).expect("badge cell").symbol())
                .collect();

            assert!(label.starts_with(mode_badge(mode)));
            for x in 0..MODE_BADGE_COLS {
                let cell = buffer.cell((x, 0)).expect("badge cell");
                assert_eq!(cell.fg, badge_style.fg.expect("badge foreground"));
                assert_eq!(cell.bg, badge_style.bg.expect("badge background"));
                assert!(cell.modifier.contains(ratatui::style::Modifier::BOLD));
            }
            assert_eq!(
                buffer.cell((MODE_BADGE_COLS, 0)).expect("middle cell").bg,
                DEFAULT_DARK
                    .ui_style(Tier::TrueColor, UiSlot::StatusBar)
                    .bg
                    .expect("status-row background")
            );
        }
    }

    #[test]
    fn status_row_background_covers_blank_cells() {
        let terminal = render_status(Mode::Normal, 80, Tier::TrueColor);
        let status_background = DEFAULT_DARK
            .ui_style(Tier::TrueColor, UiSlot::StatusBar)
            .bg
            .expect("status-row background");
        let buffer = terminal.backend().buffer();
        assert_eq!(
            buffer.cell((40, 0)).expect("blank status cell").symbol(),
            " "
        );
        for x in MODE_BADGE_COLS..80 {
            assert_eq!(
                buffer.cell((x, 0)).expect("status-row cell").bg,
                status_background,
                "status-row background missing at column {x}"
            );
        }
    }

    #[test]
    fn status_row_geometry_saturates_at_supported_widths() {
        let text = StatusBar {
            mode: Mode::Normal,
            path: "test.md".to_string(),
            dirty: false,
            is_new: false,
            cursor_line: 1,
            line_count: 10,
            command_line: None,
        }
        .build(None, &DEFAULT_DARK, Tier::TrueColor);

        for width in [0, 1, 5, 40, 80, 200] {
            let backend_width = width.max(1);
            let mut terminal =
                ratatui::Terminal::new(ratatui::backend::TestBackend::new(backend_width, 1))
                    .expect("create geometry test terminal");
            terminal
                .draw(|frame| {
                    render(
                        frame,
                        Rect::new(0, 0, width, 1),
                        &text,
                        "Space h=help",
                        true,
                        &DEFAULT_DARK,
                        Tier::TrueColor,
                    );
                })
                .unwrap_or_else(|error| panic!("width {width} must render: {error}"));

            if width >= MODE_BADGE_COLS + RULER_COLS {
                let row: String = (0..width)
                    .map(|x| {
                        terminal
                            .backend()
                            .buffer()
                            .cell((x, 0))
                            .expect("row cell")
                            .symbol()
                    })
                    .collect();
                assert!(
                    row.ends_with("Top"),
                    "ruler must be right-aligned at width {width}"
                );
            }
        }
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
    fn build_preserves_badge_during_prompt() {
        let sb = StatusBar {
            mode: Mode::Command,
            path: "test.md".to_string(),
            dirty: false,
            is_new: false,
            cursor_line: 1,
            line_count: 10,
            command_line: Some(":w".to_string()),
        };
        let text = sb.build(None, &DEFAULT_DARK, Tier::TrueColor);
        assert_eq!(text.prompt.as_deref(), Some(":w"));
        assert!(text.prompt_cursor);
        assert_eq!(text.badge.content.as_ref(), " :CMD ");
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
        assert!(text.prompt.is_none());
        assert!(!text.content.is_empty());
    }

    #[test]
    fn test_status_bar_zero_width_no_panic() {
        use ratatui::{backend::TestBackend, layout::Rect, Terminal};

        let text = StatusBar {
            mode: Mode::Command,
            path: "test.md".to_string(),
            dirty: false,
            is_new: false,
            cursor_line: 1,
            line_count: 1,
            command_line: Some(":w".to_string()),
        }
        .build(None, &DEFAULT_DARK, Tier::TrueColor);
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
                    "",
                    true,
                    &DEFAULT_DARK,
                    Tier::TrueColor,
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
        let full_text: String = text.content.iter().map(|s| s.content.as_ref()).collect();
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
        let full_text: String = text.content.iter().map(|s| s.content.as_ref()).collect();
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
        let full_text: String = text.content.iter().map(|s| s.content.as_ref()).collect();
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
        let full_text: String = text.content.iter().map(|s| s.content.as_ref()).collect();
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
