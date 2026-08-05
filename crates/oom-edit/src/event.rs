//! Event-loop driver: tick → draw → poll-with-deadline → dispatch (~20 FPS).
//!
//! The poll deadline is `min(FRAME_BUDGET, deadline)` where `deadline` is
//! computed from transient TTL expiry and which-key pending+150ms (T13).
//! Key events with `kind == Press` only are dispatched; resize events are
//! absorbed (the next draw uses the new size).
//!
//! T16: Bracketed paste is enabled on startup via crossterm.

use std::io::Stdout;
use std::time::{Duration, Instant};

use crossterm::event::{self, EnableBracketedPaste, Event, KeyEventKind};
use crossterm::execute;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use crate::app::App;

/// Maximum idle interval between redraws (~20 FPS) so the screen stays live
/// without busy-spinning.
pub const FRAME_BUDGET: Duration = Duration::from_millis(50);

/// Run the main event loop until [`App::should_quit`] is set.
///
/// The loop shape:
/// 1. `app.tick(now)` — advance internal timers, compute deadline
/// 2. `terminal.draw(...)` — render the current frame
/// 3. `event::poll(min(FRAME_BUDGET, deadline))` — wait for input
/// 4. On event: dispatch to `app.handle_event`
///
/// T16: Bracketed paste is enabled before the loop starts.
pub fn run_event_loop(
    mut app: App,
    mut terminal: Terminal<CrosstermBackend<Stdout>>,
) -> Result<(), Box<dyn std::error::Error>> {
    // T16: Enable bracketed paste mode.
    // If this fails, we continue without it (some terminals don't support it).
    let stdout = std::io::stdout();
    if let Err(e) = execute!(stdout.lock(), EnableBracketedPaste) {
        eprintln!("oom-edit: warning: failed to enable bracketed paste: {e}");
    }

    loop {
        let now = Instant::now();
        let deadline = app.tick(now);
        terminal.draw(|frame| app.render(frame))?;

        if app.should_quit {
            // T16: Disable bracketed paste on clean exit.
            let _ = execute!(stdout.lock(), crossterm::event::DisableBracketedPaste);
            return Ok(());
        }

        // Poll with deadline tightening: min(FRAME_BUDGET, deadline).
        let poll_duration = deadline
            .map(|d| {
                let remaining = d.duration_since(now);
                if remaining < FRAME_BUDGET {
                    remaining
                } else {
                    FRAME_BUDGET
                }
            })
            .unwrap_or(FRAME_BUDGET);

        if event::poll(poll_duration)? {
            let ev = event::read()?;
            match &ev {
                // Key press events only (ignore release/repeat).
                Event::Key(key) if key.kind == KeyEventKind::Press => {
                    app.handle_event(&ev);
                }
                // T16: Bracketed paste — paste event.
                Event::Paste(text) => {
                    if let Some(ref mut entry) = app.active_mut() {
                        entry.session_mut().insert_paste(text);
                    }
                    app.scroll_follow();
                }
                // Mouse events: absorb (T16 adds wheel scroll).
                Event::Mouse(_) => {
                    app.handle_event(&ev);
                }
                // Resize: the next draw call reads the new terminal size.
                Event::Resize(_, _) => {}
                _ => {}
            }
        }
        // No event → loop; the next tick brings the deadline closer.
    }
}
