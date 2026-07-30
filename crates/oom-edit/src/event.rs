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
/// Required pause after the most recently observed input before spell work.
pub const SPELL_IDLE_DELAY: Duration = Duration::from_millis(150);
/// Maximum wall-clock slice reserved for one idle drain.
pub const SPELL_SLICE_BUDGET: Duration = Duration::from_millis(8);
/// Deterministic byte unit shared by file loading, engine building, and scans.
pub const SPELL_WORK_UNIT_BYTES: usize = 4 * 1024;

fn poll_duration(now: Instant, deadline: Option<Instant>) -> Duration {
    deadline
        .map(|deadline| deadline.saturating_duration_since(now).min(FRAME_BUDGET))
        .unwrap_or(FRAME_BUDGET)
}

#[cfg(test)]
fn tick_and_poll_duration(app: &mut App, now: Instant) -> Duration {
    poll_duration(now, app.tick(now))
}

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
        let poll_duration = poll_duration(now, deadline);
        terminal.draw(|frame| app.render(frame))?;

        if app.should_quit {
            // T16: Disable bracketed paste on clean exit.
            let _ = execute!(stdout.lock(), crossterm::event::DisableBracketedPaste);
            return Ok(());
        }

        // Poll with deadline tightening: min(FRAME_BUDGET, deadline). Sample
        // immediately after poll and retain the exact pre-poll app deadline.
        let event_ready = event::poll(poll_duration)?;
        let wake_now = Instant::now();
        handle_poll_outcome(
            &mut app,
            event_ready,
            deadline,
            wake_now,
            event::read,
            || event::poll(Duration::ZERO),
            Instant::now,
        )?;
    }
}

fn deadline_has_slice_slack(deadline: Option<Instant>, now: Instant) -> bool {
    deadline.is_none_or(|deadline| deadline.saturating_duration_since(now) > SPELL_SLICE_BUDGET)
}

/// Handle the two post-poll branches without allowing an input event to run
/// spell work on the same loop iteration.
fn handle_poll_outcome<ReadEvent, PendingInput, Sample>(
    app: &mut App,
    event_ready: bool,
    prior_deadline: Option<Instant>,
    wake_now: Instant,
    mut read_event: ReadEvent,
    mut pending_input: PendingInput,
    mut sample: Sample,
) -> std::io::Result<()>
where
    ReadEvent: FnMut() -> std::io::Result<Event>,
    PendingInput: FnMut() -> std::io::Result<bool>,
    Sample: FnMut() -> Instant,
{
    if event_ready {
        let event = read_event()?;
        let event_now = sample();
        app.record_input(event_now);
        dispatch_event_at(app, event, event_now);
        return Ok(());
    }

    if !app.input_idle_for(wake_now, SPELL_IDLE_DELAY)
        || !deadline_has_slice_slack(prior_deadline, wake_now)
    {
        return Ok(());
    }

    let slice_end = wake_now + SPELL_SLICE_BUDGET;
    loop {
        if !app.on_idle_unit(SPELL_WORK_UNIT_BYTES) {
            break;
        }
        if pending_input()? {
            break;
        }
        let now = sample();
        if now >= slice_end || !deadline_has_slice_slack(prior_deadline, now) {
            break;
        }
    }
    Ok(())
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

#[cfg(test)]
fn read_then_sample<T, E>(
    read: impl FnOnce() -> Result<T, E>,
    sample: impl FnOnce() -> Instant,
) -> Result<(T, Instant), E> {
    let event = read()?;
    let now = sample();
    Ok((event, now))
}

#[cfg(test)]
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
    use std::io;

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

    #[test]
    fn poll_deadline_is_evaluated_from_the_pre_poll_tick_timestamp() {
        let tick_now = Instant::now();

        assert_eq!(poll_duration(tick_now, None), FRAME_BUDGET);
        assert_eq!(
            poll_duration(tick_now, Some(tick_now + Duration::from_millis(80))),
            FRAME_BUDGET
        );
        assert_eq!(
            poll_duration(tick_now, Some(tick_now + Duration::from_millis(12))),
            Duration::from_millis(12)
        );
        assert_eq!(poll_duration(tick_now, Some(tick_now)), Duration::ZERO);
        assert_eq!(
            poll_duration(tick_now, Some(tick_now - Duration::from_millis(1))),
            Duration::ZERO,
            "an already-due deadline must not panic or extend the poll"
        );
    }

    #[test]
    fn production_tick_and_poll_step_reuses_one_timestamp() {
        let pending_since = Instant::now();
        let mut app = test_app();
        dispatch_event_at(
            &mut app,
            Event::Key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE)),
            pending_since,
        );

        let tick_now = pending_since + Duration::from_millis(140);
        assert_eq!(
            tick_and_poll_duration(&mut app, tick_now),
            Duration::from_millis(10),
            "the production step must evaluate the tick deadline from the same pre-poll timestamp"
        );
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

    fn test_app_with_spell_at(
        document: &str,
        words: String,
        enabled: bool,
        initial: Instant,
    ) -> App {
        App::new_with_spell(
            EditorSession::from_text(document),
            crate::theme::ResolvedTheme::injected("default-dark", false, Tier::TrueColor),
            true,
            false,
            crate::app::AppServices::new(
                Box::new(RecordingClipboardSink::default()),
                Box::new(crate::config::DisabledConfigStore),
                crate::spell_host::SpellHost::testing(words),
            ),
            enabled,
            initial,
        )
    }

    #[test]
    fn event_branch_records_post_read_time_and_performs_zero_spell_work() {
        let initial = Instant::now();
        let event_time = initial + Duration::from_secs(1);
        let mut app = test_app_with_spell_at("misspelledd\n", "known\n".to_string(), true, initial);
        let mut probed = false;

        handle_poll_outcome(
            &mut app,
            true,
            None,
            event_time,
            || {
                Ok(Event::Key(KeyEvent::new(
                    KeyCode::Char('x'),
                    KeyModifiers::NONE,
                )))
            },
            || {
                probed = true;
                Ok(false)
            },
            || event_time,
        )
        .unwrap();

        assert_eq!(app.spell_host_phase(), "Unbuilt");
        assert!(!probed, "event branch must never enter the idle probe loop");
        assert!(!app.input_idle_for(
            event_time + SPELL_IDLE_DELAY - Duration::from_nanos(1),
            SPELL_IDLE_DELAY
        ));
    }

    #[test]
    fn event_read_error_propagates_without_spell_work() {
        let initial = Instant::now();
        let mut app = test_app_with_spell_at("misspelledd\n", "known\n".to_string(), true, initial);
        let error = handle_poll_outcome(
            &mut app,
            true,
            None,
            initial + SPELL_IDLE_DELAY,
            || Err(io::Error::other("scripted event read failure")),
            || Ok(false),
            || initial + SPELL_IDLE_DELAY,
        )
        .unwrap_err();
        assert_eq!(error.to_string(), "scripted event read failure");
        assert_eq!(app.spell_host_phase(), "Unbuilt");
    }

    #[test]
    fn timeout_requires_full_idle_delay_and_strict_deadline_slack() {
        let initial = Instant::now();
        for (wake_offset, deadline_offset) in [
            (SPELL_IDLE_DELAY - Duration::from_nanos(1), None),
            (SPELL_IDLE_DELAY, Some(Duration::ZERO)),
            (SPELL_IDLE_DELAY, Some(SPELL_SLICE_BUDGET)),
        ] {
            let mut app =
                test_app_with_spell_at("misspelledd\n", "known\n".to_string(), true, initial);
            let wake = initial + wake_offset;
            let deadline = deadline_offset.map(|offset| wake + offset);
            handle_poll_outcome(
                &mut app,
                false,
                deadline,
                wake,
                || panic!("timeout branch must not read an event"),
                || Ok(false),
                || wake,
            )
            .unwrap();
            assert_eq!(app.spell_host_phase(), "Unbuilt");
        }

        let wake = initial + SPELL_IDLE_DELAY;
        let mut crossed =
            test_app_with_spell_at("misspelledd\n", "known\n".to_string(), true, initial);
        handle_poll_outcome(
            &mut crossed,
            false,
            Some(wake - Duration::from_millis(1)),
            wake,
            || panic!("timeout branch must not read an event"),
            || Ok(false),
            || wake,
        )
        .unwrap();
        assert_eq!(crossed.spell_host_phase(), "Unbuilt");

        let mut allowed =
            test_app_with_spell_at("misspelledd\n", "known\n".to_string(), true, initial);
        handle_poll_outcome(
            &mut allowed,
            false,
            Some(wake + SPELL_SLICE_BUDGET + Duration::from_nanos(1)),
            wake,
            || panic!("timeout branch must not read an event"),
            || Ok(true),
            || wake,
        )
        .unwrap();
        assert_eq!(allowed.spell_host_phase(), "Loading");
    }

    #[test]
    fn pending_input_and_probe_errors_interrupt_before_the_next_unit() {
        let initial = Instant::now();
        let wake = initial + SPELL_IDLE_DELAY;
        let mut interrupted =
            test_app_with_spell_at("misspelledd\n", "known\n".to_string(), true, initial);
        let mut probes = 0;
        handle_poll_outcome(
            &mut interrupted,
            false,
            None,
            wake,
            || panic!("timeout branch must not read an event"),
            || {
                probes += 1;
                Ok(true)
            },
            || wake,
        )
        .unwrap();
        assert_eq!(probes, 1);
        assert_eq!(interrupted.spell_host_phase(), "Loading");

        let mut failed =
            test_app_with_spell_at("misspelledd\n", "known\n".to_string(), true, initial);
        let error = handle_poll_outcome(
            &mut failed,
            false,
            None,
            wake,
            || panic!("timeout branch must not read an event"),
            || Err(io::Error::other("scripted pending-input failure")),
            || wake,
        )
        .unwrap_err();
        assert_eq!(error.to_string(), "scripted pending-input failure");
        assert_eq!(failed.spell_host_phase(), "Loading");
    }

    #[test]
    fn sampled_deadline_stops_between_units_at_equality() {
        let initial = Instant::now();
        let wake = initial + SPELL_IDLE_DELAY;
        let deadline = wake + Duration::from_millis(20);
        let mut app = test_app_with_spell_at("misspelledd\n", "known\n".to_string(), true, initial);
        let mut probes = 0;
        handle_poll_outcome(
            &mut app,
            false,
            Some(deadline),
            wake,
            || panic!("timeout branch must not read an event"),
            || {
                probes += 1;
                Ok(false)
            },
            || deadline - SPELL_SLICE_BUDGET,
        )
        .unwrap();
        assert_eq!(probes, 1);
        assert_eq!(app.spell_host_phase(), "Loading");
    }

    #[test]
    fn disabled_session_leaves_host_unbuilt_on_idle_timeout() {
        let initial = Instant::now();
        let wake = initial + SPELL_IDLE_DELAY;
        let mut app =
            test_app_with_spell_at("misspelledd\n", "known\n".to_string(), false, initial);
        handle_poll_outcome(
            &mut app,
            false,
            None,
            wake,
            || panic!("timeout branch must not read an event"),
            || Ok(false),
            || wake,
        )
        .unwrap();
        assert_eq!(app.spell_host_phase(), "Unbuilt");
    }

    #[test]
    fn one_mib_single_line_build_is_interruptible_between_four_kib_units() {
        let initial = Instant::now();
        let wake = initial + SPELL_IDLE_DELAY;
        let mut app = test_app_with_spell_at("text\n", "x".repeat(1024 * 1024), true, initial);
        assert!(app.on_idle_unit(SPELL_WORK_UNIT_BYTES));
        assert!(app.on_idle_unit(SPELL_WORK_UNIT_BYTES));
        assert_eq!(app.spell_host_phase(), "Building");

        handle_poll_outcome(
            &mut app,
            false,
            None,
            wake,
            || panic!("timeout branch must not read an event"),
            || Ok(true),
            || wake,
        )
        .unwrap();
        assert_eq!(
            app.spell_host_phase(),
            "Building",
            "one unit must not synchronously consume the pathological line"
        );
    }

    #[test]
    fn one_mib_single_line_scan_is_interruptible_between_four_kib_units() {
        let initial = Instant::now();
        let wake = initial + SPELL_IDLE_DELAY;
        let mut document = "x".repeat(1024 * 1024);
        document.push('\n');
        let mut app = test_app_with_spell_at(&document, "known\n".to_string(), true, initial);
        for _ in 0..1_000 {
            if app.spell_host_phase() == "Ready" {
                break;
            }
            assert!(app.on_idle_unit(SPELL_WORK_UNIT_BYTES));
        }
        assert_eq!(app.spell_host_phase(), "Ready");
        assert!(app
            .active_mut()
            .unwrap()
            .session_mut()
            .diagnostics_pending());

        let mut probes = 0;
        handle_poll_outcome(
            &mut app,
            false,
            None,
            wake,
            || panic!("timeout branch must not read an event"),
            || {
                probes += 1;
                Ok(true)
            },
            || wake,
        )
        .unwrap();

        assert_eq!(probes, 1);
        assert!(
            app.active_mut()
                .unwrap()
                .session_mut()
                .diagnostics_pending(),
            "one scan unit must not synchronously consume the pathological line"
        );
    }

    #[test]
    fn idle_drain_eventually_builds_engine_and_publishes_diagnostics() {
        let initial = Instant::now();
        let wake = initial + SPELL_IDLE_DELAY;
        let mut app =
            test_app_with_spell_at("known misspelledd\n", "known\n".to_string(), true, initial);

        for _ in 0..1_000 {
            handle_poll_outcome(
                &mut app,
                false,
                None,
                wake,
                || panic!("timeout branch must not read an event"),
                || Ok(false),
                || wake + SPELL_SLICE_BUDGET,
            )
            .unwrap();
            if app.spell_host_phase() == "Ready"
                && !app
                    .active_mut()
                    .unwrap()
                    .session_mut()
                    .diagnostics_pending()
            {
                break;
            }
        }

        let session = app.active_mut().unwrap().session_mut();
        assert!(!session.diagnostics_pending());
        assert_eq!(session.diagnostics().len(), 1);
        assert_eq!(session.diagnostics()[0].source_text, "misspelledd");
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
