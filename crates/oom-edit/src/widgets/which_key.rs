//! Which-key hint bar — shows pending Space-chord continuations after a delay.
//!
//! Three pure functions:
//! 1. **Gate** — `should_show(pending, now)` — 150ms delay check.
//! 2. **Build** — `build_continuations(km, ctx)` — continuations of Space.
//! 3. **Render** — `render(frame, area, text)` — bottom-right of body.

use std::time::{Duration, Instant};

use ratatui::{style::Style, text::Line, widgets::Paragraph, Frame};

use crate::command::{keymap::Keymap, registry::Contexts};

/// Which-key delay: 150ms after Space before the hint bar appears.
const WHICH_KEY_DELAY: Duration = Duration::from_millis(150);

/// Should the which-key hint bar be shown right now?
///
/// Pure gate: checks `pending.since` against `now`.
pub fn should_show(since: Option<Instant>, now: Instant) -> bool {
    since
        .map(|s| now.duration_since(s) >= WHICH_KEY_DELAY)
        .unwrap_or(false)
}

/// Build the which-key hint text from the keymap.
///
/// Pure build: returns the continuations of Space for the given context,
/// formatted as a hint string. Returns `None` if there are fewer than 2
/// continuations (the ≥2 rows rule).
pub fn build_hint(km: &Keymap, ctx: Contexts) -> Option<String> {
    let conts = km.continuations_for(ctx);
    if conts.len() < 2 {
        return None;
    }

    // Format: "Space: h=help  w=save  q=quit  t=cycle-theme"
    let parts: Vec<String> = conts
        .iter()
        .map(|(key, spec)| {
            let key_str = match key.code.kind {
                oom_edit_core::session::KeyCodeKind::Char(c) => c.to_string(),
                _ => format!("{:?}", key.code.kind),
            };
            format!("{}={}", key_str, spec.desc)
        })
        .collect();

    Some(format!("Space: {}", parts.join("  ")))
}

/// Render the which-key hint bar at the bottom-right of the given area.
///
/// Thin render: creates a Paragraph widget positioned in the bottom-right
/// corner of the body area.
pub fn render(frame: &mut Frame<'_>, area: ratatui::layout::Rect, text: &str) {
    let hint_width = Line::from(text).width().min(area.width as usize) as u16;
    let hint_area = ratatui::layout::Rect::new(
        area.x + area.width.saturating_sub(hint_width + 1),
        area.y + area.height.saturating_sub(1),
        hint_width.min(area.width),
        1,
    );

    let paragraph = Paragraph::new(text).style(Style::default());
    frame.render_widget(paragraph, hint_area);
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gate_rejects_zero_delay() {
        let now = Instant::now();
        assert!(!should_show(Some(now), now));
    }

    #[test]
    fn gate_accepts_after_delay() {
        let past = Instant::now() - Duration::from_millis(200);
        assert!(should_show(Some(past), Instant::now()));
    }

    #[test]
    fn gate_rejects_none() {
        assert!(!should_show(None, Instant::now()));
    }

    #[test]
    fn build_hint_returns_some_with_continuations() {
        let km = Keymap::default();
        let hint = build_hint(&km, Contexts::NORMAL);
        assert!(hint.is_some());
        let hint = hint.unwrap();
        assert!(hint.contains("Space:"));
        assert!(hint.contains("h="));
    }

    #[test]
    fn build_hint_returns_none_when_no_continuations() {
        let km = Keymap::default();
        // In OVERLAY context, no Space chords are available.
        let hint = build_hint(&km, Contexts::OVERLAY);
        assert!(hint.is_none());
    }
}
