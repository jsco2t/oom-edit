//! Run with `make run-embedded ARGS=path/to/document.md`.

#[path = "support/embedded_host.rs"]
mod embedded_host;

use crossterm::{cursor::SetCursorStyle, event, execute};
use embedded_host::{pane_area, EmbeddedHost};
use oom_edit::{OpenOptions, PaneCursorShape, TerminalGuard, TerminalGuardOptions};
use ratatui::{backend::CrosstermBackend, layout::Rect, Terminal};
use std::io::stdout;
use std::path::PathBuf;
use std::time::{Duration, Instant};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let base = std::env::current_dir()?;
    let mut enhancement = true;
    let paths = std::env::args_os()
        .skip(1)
        .filter(|argument| {
            if argument == "--legacy-keys" {
                enhancement = false;
                false
            } else {
                true
            }
        })
        .map(PathBuf::from)
        .collect::<Vec<_>>();
    let now = Instant::now();
    let mut host = EmbeddedHost::new(&base, paths, now);
    if host.pane.tabs().is_empty() {
        host.pane.new_buffer(OpenOptions::default())?;
    }
    let _guard = TerminalGuard::with_options(TerminalGuardOptions {
        keyboard_enhancement: enhancement,
        ..TerminalGuardOptions::default()
    })?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;
    let mut previous_shape = None;
    while !host.quit {
        let now = Instant::now();
        let tick = host.tick(now);
        terminal.draw(|frame| host.draw(frame, now))?;
        if host.cursor_shape != previous_shape {
            let shape = match host.cursor_shape {
                Some(PaneCursorShape::Block) => SetCursorStyle::SteadyBlock,
                Some(PaneCursorShape::Bar) => SetCursorStyle::SteadyBar,
                Some(PaneCursorShape::Underscore) => SetCursorStyle::SteadyUnderScore,
                None => SetCursorStyle::DefaultUserShape,
            };
            execute!(stdout(), shape)?;
            previous_shape = host.cursor_shape;
        }
        // Events have been handled by the host; retain no unbounded history.
        host.events.clear();
        let deadline = tick
            .next_deadline
            .map_or(Duration::from_millis(50), |deadline| {
                deadline
                    .saturating_duration_since(now)
                    .min(Duration::from_millis(50))
            });
        if event::poll(deadline)? {
            let event = event::read()?;
            let now = Instant::now();
            let size = terminal.size()?;
            host.handle_event(
                event,
                pane_area(Rect::new(0, 0, size.width, size.height)),
                now,
            );
        } else if tick.idle_due {
            let started = Instant::now();
            while started.elapsed() < Duration::from_millis(2) && host.pane.idle_unit(4096).worked {
            }
        }
    }
    Ok(())
}
