//! Event-loop driver: tick → draw → poll-with-deadline → dispatch (~20 FPS).
//!
//! The poll deadline is `min(FRAME_BUDGET, deadline)` where `deadline` is
//! computed from transient TTL expiry and which-key pending+150ms (T13).
//! Key events with `kind == Press` only are dispatched; resize events are
//! forwarded so Rendered-mode layout and cursor state can be remapped.
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
            read_sample_and_dispatch(&mut app, event::read, Instant::now)?;
        }
        // No event → loop; the next tick brings the deadline closer.
    }
}

/// Dispatch one terminal event through the same path used by the event loop.
fn dispatch_event_at(app: &mut App, ev: Event, now: Instant) {
    match &ev {
        // Key press events only (ignore release/repeat).
        Event::Key(key) if key.kind == KeyEventKind::Press => {
            app.handle_event_at(&ev, now);
        }
        // T16: Bracketed paste — paste event.
        Event::Paste(_) => {
            app.handle_event_at(&ev, now);
        }
        // Mouse events: absorb (T16 adds wheel scroll).
        Event::Mouse(_) => {
            app.handle_event_at(&ev, now);
        }
        // Resize: forward to app for Rendered-mode cursor remap (FR-3.1).
        Event::Resize(_, _) => {
            app.handle_event_at(&ev, now);
        }
        _ => {}
    }
}

fn read_then_sample<T, E>(
    read: impl FnOnce() -> Result<T, E>,
    sample: impl FnOnce() -> Instant,
) -> Result<(T, Instant), E> {
    let event = read()?;
    let now = sample();
    Ok((event, now))
}

fn read_sample_and_dispatch<E>(
    app: &mut App,
    read: impl FnOnce() -> Result<Event, E>,
    sample: impl FnOnce() -> Instant,
) -> Result<(), E> {
    let (event, now) = read_then_sample(read, sample)?;
    dispatch_event_at(app, event, now);
    Ok(())
}

#[cfg(test)]
fn dispatch_event(app: &mut App, ev: Event) {
    dispatch_event_at(app, ev, Instant::now());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use oom_edit_core::RecordingClipboardSink;
    use oom_edit_core::{EditorSession, Mode};

    use crate::command::keymap::PendingAppInput;
    use crate::theme::Tier;

    const RESIZE_DOCUMENT: &str = "# Intro\n\nThis opening paragraph is deliberately long enough to wrap at forty columns but not at eighty columns.\n\n## Target heading\n\nThis trailing paragraph is also deliberately long enough to make the narrow layout visibly different.\n";

    #[test]
    fn event_timestamp_is_sampled_after_read_before_dispatch() {
        use std::cell::RefCell;

        let order = RefCell::new(Vec::new());
        let sampled = Instant::now();
        let mut app = test_app();
        read_sample_and_dispatch(
            &mut app,
            || {
                order.borrow_mut().push("read");
                Ok::<_, ()>(Event::Key(KeyEvent::new(
                    KeyCode::Char(' '),
                    KeyModifiers::NONE,
                )))
            },
            || {
                order.borrow_mut().push("sample");
                sampled
            },
        )
        .unwrap();
        order.borrow_mut().push("dispatch");

        assert_eq!(*order.borrow(), ["read", "sample", "dispatch"]);
        assert_eq!(app.pending_input, PendingAppInput::Space { since: sampled });
    }

    fn test_app() -> App {
        App::new(
            EditorSession::from_text(RESIZE_DOCUMENT),
            crate::theme::ResolvedTheme::injected("default-dark", false, Tier::TrueColor),
            true,
            false,
            Box::new(RecordingClipboardSink::default()),
            Box::new(crate::config::DisabledConfigStore),
            std::time::Instant::now(),
        )
    }

    fn build_initial_rendered_layout(app: &mut App) {
        dispatch_event(app, Event::Resize(80, 24));
        assert_eq!(app.active_mut().unwrap().session_mut().mode(), Mode::Normal);
    }

    fn current_content_line(app: &mut App) -> usize {
        let session = app.active_mut().unwrap().session_mut();
        let cursor = session.rendered_cursor_line();
        let text = session.document();
        let source_start = session.rendered_layout().unwrap().lines[cursor]
            .source
            .start;
        text[..source_start]
            .bytes()
            .filter(|byte| *byte == b'\n')
            .count()
    }

    #[test]
    fn test_resize_event_reaches_handler() {
        let mut app = test_app();
        build_initial_rendered_layout(&mut app);

        let wide_line_count = app
            .active_mut()
            .unwrap()
            .session_mut()
            .rendered_layout()
            .unwrap()
            .lines
            .len();

        dispatch_event(&mut app, Event::Resize(40, 24));

        let narrow_line_count = app
            .active_mut()
            .unwrap()
            .session_mut()
            .rendered_layout()
            .unwrap()
            .lines
            .len();
        assert!(
            narrow_line_count > wide_line_count,
            "production dispatch should forward resize and rebuild the narrower rendered layout"
        );
    }

    #[test]
    fn test_rendered_cursor_stable_after_narrow_resize() {
        let mut app = test_app();
        build_initial_rendered_layout(&mut app);

        while current_content_line(&mut app) < 4 {
            dispatch_event(
                &mut app,
                Event::Key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)),
            );
        }
        assert_eq!(current_content_line(&mut app), 4);
        let wide_line_count = app
            .active_mut()
            .unwrap()
            .session_mut()
            .rendered_layout()
            .unwrap()
            .lines
            .len();

        dispatch_event(&mut app, Event::Resize(40, 24));

        let narrow_line_count = app
            .active_mut()
            .unwrap()
            .session_mut()
            .rendered_layout()
            .unwrap()
            .lines
            .len();
        assert!(
            narrow_line_count > wide_line_count,
            "narrow resize should reflow through production dispatch"
        );
        assert_eq!(
            current_content_line(&mut app),
            4,
            "rendered cursor should remain on the Target heading's logical source line"
        );
    }

    #[test]
    fn test_paste_event_reaches_app_handler() {
        let mut app = test_app();
        dispatch_event(
            &mut app,
            Event::Key(KeyEvent::new(KeyCode::Char('i'), KeyModifiers::NONE)),
        );
        assert_eq!(app.active_mut().unwrap().session_mut().mode(), Mode::Insert);

        dispatch_event(&mut app, Event::Paste("pasted λ".to_string()));

        assert!(app
            .active_mut()
            .unwrap()
            .session_mut()
            .document()
            .starts_with("pasted λ"));
    }
}
