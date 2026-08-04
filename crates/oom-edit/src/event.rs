//! Event-loop driver: tick → draw → poll-with-deadline → dispatch (~20 FPS).
//!
//! The poll deadline is `FRAME_BUDGET` (status-message TTL will tighten this
//! in T13; for now it is always `FRAME_BUDGET`). Key events with `kind == Press`
//! only are dispatched; resize events are absorbed (the next draw uses the new
//! size).

use std::io::Stdout;
use std::time::{Duration, Instant};

use crossterm::event::{self, Event, KeyEventKind};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use crate::app::App;

/// Maximum idle interval between redraws (~20 FPS) so the screen stays live
/// without busy-spinning.
pub const FRAME_BUDGET: Duration = Duration::from_millis(50);

/// Run the main event loop until [`App::should_quit`] is set.
///
/// The loop shape:
/// 1. `app.tick(now)` — advance internal timers
/// 2. `terminal.draw(...)` — render the current frame
/// 3. `event::poll(min(FRAME_BUDGET, deadline))` — wait for input
/// 4. On event: dispatch to `app.handle_event`
pub fn run_event_loop(
    mut app: App,
    mut terminal: Terminal<CrosstermBackend<Stdout>>,
) -> Result<(), Box<dyn std::error::Error>> {
    loop {
        let now = Instant::now();
        app.tick(now);
        terminal.draw(|frame| app.render(frame))?;

        if app.should_quit {
            return Ok(());
        }

        // Poll for events (no deadline tightening in T11; status-message TTL
        // arrives in T13).
        if event::poll(FRAME_BUDGET)? {
            let ev = event::read()?;
            match &ev {
                // Key press events only (ignore release/repeat).
                Event::Key(key) if key.kind == KeyEventKind::Press => {
                    app.handle_event(&ev);
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
