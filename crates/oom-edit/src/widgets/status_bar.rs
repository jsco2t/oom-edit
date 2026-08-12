//! Status bar — mode badge, file info, transient messages, ruler, command line.
//!
//! Pure build + thin render: `build_status()` returns a ready-to-render
//! `StatusBarText`; `render()` maps it to ratatui widgets.
//!
//! Layout: `[badge Length(MODE_BADGE_COLS), gap Length(MODE_BADGE_GAP_COLS),
//! middle Min(0), ruler Length(RULER_COLS)]`. Command and rendered-search prompts
//! use the middle plus ruler region while the badge and gap remain fixed at the
//! left edge.
//!
//! FR-6.2, FR-6.4.

use std::time::{Duration, Instant};

use ratatui::{
    layout::{Alignment, Position, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Paragraph},
    Frame,
};

use oom_edit_core::{Mode, Severity};

use crate::theme::{Theme, Tier, UiSlot};

/// Status bar transient message TTL: 4 seconds.
pub const TRANSIENT_TTL: Duration = Duration::from_secs(4);

/// Ruler column width: `line:col  n%` — max ~22 chars for large files.
pub const RULER_COLS: u16 = 22;

/// Fixed mode-badge width, sized for the longest public mode label.
pub const MODE_BADGE_COLS: u16 = 8;

/// Blank status-row-colored gap after the mode badge.
pub const MODE_BADGE_GAP_COLS: u16 = 1;

/// Fixed offset where flexible status content begins.
pub const STATUS_CONTENT_OFFSET: u16 = MODE_BADGE_COLS + MODE_BADGE_GAP_COLS;

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
    /// Cursor column — 1-based for display.
    pub cursor_col: usize,
    /// Total line count (for percentage calculation).
    pub line_count: usize,
    /// Command-line text (when Command or rendered search is active).
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
    /// Active command or rendered-search prompt, including its prefix.
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
            ruler_text(self.cursor_line, self.cursor_col, self.line_count),
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
    // The badge is composited over the dimmed status row. Explicitly remove
    // DIM so the mode label retains the intended black, bold contrast.
    let badge_style = text.badge.style.remove_modifier(Modifier::DIM);
    frame.render_widget(Block::default().style(badge_style), badge_area);
    frame.render_widget(Paragraph::new(Line::from(text.badge.clone())), badge_area);

    let gap_width = MODE_BADGE_GAP_COLS.min(area.width.saturating_sub(badge_width));
    let content_offset = badge_width.saturating_add(gap_width);
    let flexible_x = area.x.saturating_add(content_offset);
    let flexible_width = area.width.saturating_sub(content_offset);
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
        Mode::Select => " SELECT ",
        Mode::Command => " :CMD ",
    }
}

/// Return the style for a mode badge.
fn mode_badge_style(mode: Mode, theme: &Theme, tier: Tier) -> Style {
    let slot = match mode {
        Mode::Normal => UiSlot::BadgeNormal,
        Mode::Insert => UiSlot::BadgeInsert,
        Mode::Select => UiSlot::BadgeSelect,
        Mode::Command => UiSlot::BadgeCommand,
    };
    theme.ui_style(tier, slot)
}

/// Build the ruler text: `line:col  n%` with Top/Bot at extremes.
///
/// Percent = cursor_line / line_count (1-based cursor, 1-based line_count).
/// At the top (line 1): shows `Top`. At the bottom (line == line_count): shows `Bot`.
/// Otherwise: `n%` or `Top`/`Bot` at extremes.
pub fn ruler_text(cursor_line: usize, cursor_col: usize, line_count: usize) -> String {
    let cursor_display = cursor_line.max(1);
    let column_display = cursor_col.max(1);
    let lc = line_count.max(1);

    if lc <= 1 {
        return format!("1:{column_display}  Top");
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
        format!("{cursor_display}:{column_display}  {pct_int}%")
    } else {
        format!("{cursor_display}:{column_display}  {pct_int}% {edge}")
    }
}

/// Build the line-number gutter.
///
/// With relative line numbers disabled, all modes use absolute line numbers.
/// When enabled, rendered Normal/Select/Command use hybrid numbering — the
/// current line is absolute and other lines show their signed distance from
/// the cursor. Source Insert remains absolute.
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

    let show_absolute = !relative_line_numbers || mode == Mode::Insert;

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::{Tier, UiSlot, DEFAULT_DARK, DEFAULT_LIGHT};
    use ratatui::{backend::TestBackend, Terminal};

    fn status(mode: Mode) -> StatusBar {
        StatusBar {
            mode,
            path: "test.md".to_string(),
            dirty: false,
            is_new: false,
            cursor_line: 1,
            cursor_col: 1,
            line_count: 10,
            command_line: None,
        }
    }

    fn render_status(mode: Mode, width: u16, tier: Tier) -> Terminal<TestBackend> {
        let text = status(mode).build(None, &DEFAULT_DARK, tier);
        let mut terminal = Terminal::new(TestBackend::new(width, 1)).unwrap();
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
            .unwrap();
        terminal
    }

    #[test]
    fn badges_cover_exactly_four_public_modes() {
        let cases = [
            (Mode::Normal, " NORMAL ", UiSlot::BadgeNormal),
            (Mode::Insert, " INSERT ", UiSlot::BadgeInsert),
            (Mode::Select, " SELECT ", UiSlot::BadgeSelect),
            (Mode::Command, " :CMD ", UiSlot::BadgeCommand),
        ];
        for (mode, label, slot) in cases {
            assert_eq!(mode_badge(mode), label);
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
    }

    #[test]
    fn status_badge_has_fixed_geometry_dark_text_and_visual_gap() {
        for mode in [Mode::Normal, Mode::Insert, Mode::Select, Mode::Command] {
            let terminal = render_status(mode, 80, Tier::TrueColor);
            let buffer = terminal.backend().buffer();
            let badge_style = mode_badge_style(mode, &DEFAULT_DARK, Tier::TrueColor);
            let label: String = (0..MODE_BADGE_COLS)
                .map(|x| buffer.cell((x, 0)).unwrap().symbol())
                .collect();
            assert_eq!(
                label,
                format!(
                    "{:<width$}",
                    mode_badge(mode),
                    width = MODE_BADGE_COLS as usize
                )
            );
            for x in 0..MODE_BADGE_COLS {
                let cell = buffer.cell((x, 0)).unwrap();
                assert_eq!(cell.fg, crate::theme::TEST_EXACT_BLACK);
                assert_eq!(cell.bg, badge_style.bg.unwrap());
                assert!(cell.modifier.contains(ratatui::style::Modifier::BOLD));
                assert!(!cell.modifier.contains(ratatui::style::Modifier::DIM));
            }
            let row_style = DEFAULT_DARK.ui_style(Tier::TrueColor, UiSlot::StatusBar);
            let gap = buffer.cell((MODE_BADGE_COLS, 0)).unwrap();
            assert_eq!(gap.symbol(), " ");
            assert_eq!(gap.fg, row_style.fg.unwrap());
            assert_eq!(gap.bg, row_style.bg.unwrap());
            assert!(gap.modifier.contains(ratatui::style::Modifier::DIM));
            assert_eq!(
                buffer.cell((STATUS_CONTENT_OFFSET, 0)).unwrap().symbol(),
                "S"
            );
        }
    }

    #[test]
    fn status_render_saturates_at_badge_gap_boundaries() {
        let text = status(Mode::Normal).build(None, &DEFAULT_DARK, Tier::TrueColor);
        let badge_style = mode_badge_style(Mode::Normal, &DEFAULT_DARK, Tier::TrueColor);
        let row_style = DEFAULT_DARK.ui_style(Tier::TrueColor, UiSlot::StatusBar);
        for width in [0, 1, 7, 8, 9, 10, 40, 80, 200] {
            let mut terminal = Terminal::new(TestBackend::new(width.max(1), 1)).unwrap();
            terminal
                .draw(|frame| {
                    render(
                        frame,
                        Rect::new(0, 0, width, 1),
                        &text,
                        "Space h=help",
                        false,
                        &DEFAULT_DARK,
                        Tier::TrueColor,
                    );
                })
                .unwrap();

            if width == 0 {
                continue;
            }
            let buffer = terminal.backend().buffer();
            for x in 0..MODE_BADGE_COLS.min(width) {
                let cell = buffer.cell((x, 0)).unwrap();
                assert_eq!(cell.fg, badge_style.fg.unwrap());
                assert_eq!(cell.bg, badge_style.bg.unwrap());
            }
            if width > MODE_BADGE_COLS {
                let gap = buffer.cell((MODE_BADGE_COLS, 0)).unwrap();
                assert_eq!(gap.symbol(), " ");
                assert_eq!(gap.fg, row_style.fg.unwrap());
                assert_eq!(gap.bg, row_style.bg.unwrap());
            }
            if width > STATUS_CONTENT_OFFSET {
                let first_content = buffer.cell((STATUS_CONTENT_OFFSET, 0)).unwrap();
                assert_eq!(first_content.symbol(), "S");
                assert_eq!(first_content.fg, row_style.fg.unwrap());
                assert_eq!(first_content.bg, row_style.bg.unwrap());
            }
        }
    }

    #[test]
    fn ruler_reports_top_middle_and_bottom() {
        assert_eq!(ruler_text(1, 1, 10), "1:1  10% Top");
        assert_eq!(ruler_text(5, 7, 10), "5:7  50%");
        assert_eq!(ruler_text(10, 2, 10), "10:2  100% Bot");
    }

    #[test]
    fn ruler_handles_single_line_and_document_boundaries() {
        assert_eq!(ruler_text(1, 9, 1), "1:9  Top");
        assert!(ruler_text(2, 3, 100).contains("2%"));
        assert!(ruler_text(99, 4, 100).contains("99%"));
    }

    #[test]
    fn rendered_normal_and_select_honor_relative_numbers() {
        for mode in [Mode::Normal, Mode::Select, Mode::Command] {
            let gutter = build_gutter(mode, 0, 2, 5, 10, true, 4);
            assert!(gutter[2].contains('3'));
            assert!(gutter[0].contains('2'));
        }
        let insert = build_gutter(Mode::Insert, 0, 2, 5, 10, true, 4);
        assert!(insert[0].contains('1'));
        assert!(insert[2].contains('3'));
    }

    #[test]
    fn gutter_width_tracks_digit_boundaries() {
        assert_eq!(gutter_width(9), 6);
        assert_eq!(gutter_width(10), 6);
        assert_eq!(gutter_width(100), 6);
        assert_eq!(gutter_width(1000), 7);
    }

    #[test]
    fn gutter_padding_alignment_and_document_bounds_are_stable() {
        let gutter = build_gutter(Mode::Insert, 95, 97, 10, 100, false, 6);
        assert_eq!(gutter[4], " 100  ");
        assert_eq!(gutter[5], "      ");
        assert_eq!(gutter[9], "      ");

        for row in build_gutter(Mode::Normal, 0, 1, 3, 1000, true, 7) {
            assert_eq!(row.len(), 7);
            assert!(row.ends_with("  "));
        }
    }

    #[test]
    fn transient_expiry_uses_the_injected_deadline() {
        let now = Instant::now();
        let transient = Transient {
            text: "saved".to_string(),
            severity: Severity::Success,
            expires_at: now + TRANSIENT_TTL,
        };
        assert!(!transient.is_expired(now + TRANSIENT_TTL - Duration::from_nanos(1)));
        assert!(transient.is_expired(now + TRANSIENT_TTL));
    }

    #[test]
    fn severity_glyphs_are_non_color_signals() {
        assert_eq!(severity_glyph(Severity::Error), "✗ ");
        assert_eq!(severity_glyph(Severity::Warning), "⚠ ");
        assert_eq!(severity_glyph(Severity::Info), "");
        assert_eq!(severity_glyph(Severity::Success), "");
    }

    #[test]
    fn build_covers_file_flags_transients_and_prompts() {
        let mut bar = status(Mode::Normal);
        bar.path = "/tmp/new.md".to_string();
        bar.dirty = true;
        bar.is_new = true;
        let transient = Transient {
            text: "externally modified".to_string(),
            severity: Severity::Error,
            expires_at: Instant::now() + TRANSIENT_TTL,
        };
        let built = bar.build(Some(&transient), &DEFAULT_DARK, Tier::TrueColor);
        let content: String = built
            .content
            .iter()
            .map(|span| span.content.as_ref())
            .collect();
        assert!(content.contains("new.md [new file] [+]"));
        assert!(content.contains("✗ externally modified"));

        bar.mode = Mode::Command;
        bar.command_line = Some(":w".to_string());
        let prompt = bar.build(None, &DEFAULT_DARK, Tier::TrueColor);
        assert_eq!(prompt.badge.content.as_ref(), " :CMD ");
        assert_eq!(prompt.prompt.as_deref(), Some(":w"));
        assert!(prompt.prompt_cursor);
    }
}

// ── Tests ───────────────────────────────────────────────────────────────────
