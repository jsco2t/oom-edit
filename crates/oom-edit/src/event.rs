//! Event-loop driver: tick → conditional draw → poll-with-deadline → dispatch.
//!
//! The poll deadline is `min(FRAME_BUDGET, deadline)` where `deadline` is
//! computed from transient TTL expiry and which-key pending+150ms.
//! Key events with `kind == Press` only are dispatched; resize events are
//! coalesced in bounded batches before rendered layout and cursor remapping.

use std::io::{Stdout, Write};
use std::time::{Duration, Instant};

use crossterm::cursor::SetCursorStyle;
use crossterm::event::{self, Event, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{BeginSynchronizedUpdate, EndSynchronizedUpdate};
use oom_edit_core::Mode;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use crate::app::App;

/// Maximum idle poll interval, preserving timer and background-work cadence
/// without requesting unchanged frames.
pub const FRAME_BUDGET: Duration = Duration::from_millis(50);
/// Required pause after the most recently observed input before spell work.
pub const SPELL_IDLE_DELAY: Duration = Duration::from_millis(150);
/// Maximum wall-clock slice reserved for one idle drain.
pub const SPELL_SLICE_BUDGET: Duration = Duration::from_millis(8);
/// Deterministic byte unit shared by file loading, engine building, and scans.
pub const SPELL_WORK_UNIT_BYTES: usize = 4 * 1024;
/// Maximum ready events handled before presentation gets another opportunity.
const MAX_EVENT_BATCH: usize = 32;

fn poll_duration(now: Instant, deadline: Option<Instant>) -> Duration {
    deadline
        .map(|deadline| deadline.saturating_duration_since(now).min(FRAME_BUDGET))
        .unwrap_or(FRAME_BUDGET)
}

#[cfg(test)]
fn tick_and_poll_duration(app: &mut App, now: Instant) -> Duration {
    poll_duration(now, app.tick(now).deadline)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct RedrawScheduler {
    pending: bool,
}

impl RedrawScheduler {
    const fn initial() -> Self {
        Self { pending: true }
    }

    fn request(&mut self, redraw: bool) {
        self.pending |= redraw;
    }

    fn take(&mut self) -> bool {
        std::mem::take(&mut self.pending)
    }

    fn draw_if_requested(
        &mut self,
        draw: impl FnOnce() -> std::io::Result<()>,
    ) -> std::io::Result<bool> {
        if !self.take() {
            return Ok(false);
        }
        draw()?;
        Ok(true)
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct PollOutcome {
    redraw: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ModeCursorShape {
    Block,
    Bar,
    Underscore,
}

fn cursor_shape(mode: Mode, cursor_shapes: bool) -> ModeCursorShape {
    if !cursor_shapes {
        return ModeCursorShape::Block;
    }
    match mode {
        Mode::Normal => ModeCursorShape::Block,
        Mode::Insert | Mode::Command => ModeCursorShape::Bar,
        Mode::Select => ModeCursorShape::Underscore,
    }
}

impl ModeCursorShape {
    const fn command(self) -> SetCursorStyle {
        match self {
            Self::Block => SetCursorStyle::SteadyBlock,
            Self::Bar => SetCursorStyle::SteadyBar,
            Self::Underscore => SetCursorStyle::SteadyUnderScore,
        }
    }
}

fn apply_cursor_shape(
    out: &mut impl Write,
    mode: Mode,
    cursor_shapes: bool,
    last_shape: &mut Option<ModeCursorShape>,
) -> std::io::Result<bool> {
    let shape = cursor_shape(mode, cursor_shapes);
    if *last_shape == Some(shape) {
        return Ok(false);
    }
    execute!(out, shape.command())?;
    *last_shape = Some(shape);
    Ok(true)
}

fn synchronized_update<W: Write>(
    out: &mut W,
    operation: impl FnOnce(&mut W) -> std::io::Result<()>,
) -> std::io::Result<()> {
    execute!(out, BeginSynchronizedUpdate)?;
    let operation_result = operation(out);
    let end_result = execute!(out, EndSynchronizedUpdate);
    operation_result.and(end_result)
}

/// Run the main event loop until [`App::should_quit`] is set.
///
/// The loop paints one initial frame, then only paints after an explicit
/// input, timer, resize, or idle-work invalidation.
pub fn run_event_loop(
    mut app: App,
    mut terminal: Terminal<CrosstermBackend<Stdout>>,
    cursor_shapes: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut stdout = std::io::stdout();
    let mut last_cursor_shape = None;
    let mut redraw = RedrawScheduler::initial();
    loop {
        let now = Instant::now();
        let tick = app.tick(now);
        let deadline = tick.deadline;
        redraw.request(tick.redraw);
        let poll_duration = poll_duration(now, deadline);
        redraw.draw_if_requested(|| {
            synchronized_update(&mut stdout, |out| {
                apply_cursor_shape(out, app.mode(), cursor_shapes, &mut last_cursor_shape)?;
                terminal.draw(|frame| app.render(frame)).map(|_| ())
            })
        })?;

        if app.should_quit {
            return Ok(());
        }

        // Poll with deadline tightening: min(FRAME_BUDGET, deadline). Sample
        // immediately after poll and retain the exact pre-poll app deadline.
        let event_ready = event::poll(poll_duration)?;
        let wake_now = Instant::now();
        let outcome = handle_poll_outcome(
            &mut app,
            event_ready,
            deadline,
            wake_now,
            event::read,
            || event::poll(Duration::ZERO),
            Instant::now,
        )?;
        redraw.request(outcome.redraw);
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
) -> std::io::Result<PollOutcome>
where
    ReadEvent: FnMut() -> std::io::Result<Event>,
    PendingInput: FnMut() -> std::io::Result<bool>,
    Sample: FnMut() -> Instant,
{
    if event_ready {
        let mut redraw = false;
        let mut pending_resize = None;
        for index in 0..MAX_EVENT_BATCH {
            let event = read_event()?;
            let event_now = sample();
            app.record_input(event_now);
            if matches!(event, Event::Resize(_, _)) {
                pending_resize = Some((event, event_now));
            } else {
                if let Some((resize, resize_now)) = pending_resize.take() {
                    redraw |= dispatch_event_at(app, resize, resize_now);
                }
                redraw |= dispatch_event_at(app, event, event_now);
            }

            if app.should_quit {
                break;
            }
            if index + 1 == MAX_EVENT_BATCH || !pending_input()? {
                break;
            }
        }
        if let Some((resize, resize_now)) = pending_resize {
            redraw |= dispatch_event_at(app, resize, resize_now);
        }
        return Ok(PollOutcome { redraw });
    }

    if !app.input_idle_for(wake_now, SPELL_IDLE_DELAY)
        || !deadline_has_slice_slack(prior_deadline, wake_now)
    {
        return Ok(PollOutcome::default());
    }

    let slice_end = wake_now + SPELL_SLICE_BUDGET;
    let mut redraw = false;
    loop {
        if !app.on_idle_unit(SPELL_WORK_UNIT_BYTES) {
            break;
        }
        redraw = true;
        if pending_input()? {
            break;
        }
        let now = sample();
        if now >= slice_end || !deadline_has_slice_slack(prior_deadline, now) {
            break;
        }
    }
    Ok(PollOutcome { redraw })
}

/// Dispatch one terminal event through the same path used by the event loop.
fn dispatch_event_at(app: &mut App, ev: Event, now: Instant) -> bool {
    match &ev {
        // Key press events only (ignore release/repeat).
        Event::Key(key) if key.kind == KeyEventKind::Press => {
            app.handle_event_at(&ev, now);
            true
        }
        // Bracketed paste event.
        Event::Paste(_) => {
            app.handle_event_at(&ev, now);
            true
        }
        // Mouse events are routed for wheel scrolling and modal ownership.
        Event::Mouse(_) => {
            app.handle_event_at(&ev, now);
            true
        }
        // Resize: forward the coalesced final dimensions for rendered remap.
        Event::Resize(_, _) => {
            app.handle_event_at(&ev, now);
            true
        }
        _ => false,
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
    fn cursor_shapes_map_every_public_mode_and_disabled_uses_block() {
        assert_eq!(cursor_shape(Mode::Normal, true), ModeCursorShape::Block);
        assert_eq!(cursor_shape(Mode::Insert, true), ModeCursorShape::Bar);
        assert_eq!(
            cursor_shape(Mode::Select, true),
            ModeCursorShape::Underscore
        );
        assert_eq!(cursor_shape(Mode::Command, true), ModeCursorShape::Bar);

        for mode in [Mode::Normal, Mode::Insert, Mode::Select, Mode::Command] {
            assert_eq!(cursor_shape(mode, false), ModeCursorShape::Block);
        }
    }

    #[test]
    fn cursor_shape_command_is_emitted_once_per_mapped_shape_change() {
        let mut sink = Vec::new();
        let mut last_shape = None;

        assert!(apply_cursor_shape(&mut sink, Mode::Normal, true, &mut last_shape).unwrap());
        assert_eq!(sink, b"\x1b[2 q");
        assert!(!apply_cursor_shape(&mut sink, Mode::Normal, true, &mut last_shape).unwrap());
        assert_eq!(sink, b"\x1b[2 q");

        assert!(apply_cursor_shape(&mut sink, Mode::Insert, true, &mut last_shape).unwrap());
        assert_eq!(sink, b"\x1b[2 q\x1b[6 q");
        assert!(!apply_cursor_shape(&mut sink, Mode::Command, true, &mut last_shape).unwrap());
        assert_eq!(sink, b"\x1b[2 q\x1b[6 q");

        assert!(apply_cursor_shape(&mut sink, Mode::Select, true, &mut last_shape).unwrap());
        assert_eq!(sink, b"\x1b[2 q\x1b[6 q\x1b[4 q");
    }

    #[test]
    fn redraw_scheduler_paints_initial_frame_once_and_skips_unchanged_idle_polls() {
        let initial = Instant::now();
        let mut app = test_app();
        let mut scheduler = RedrawScheduler::initial();
        let mut draws = 0;

        scheduler
            .draw_if_requested(|| {
                draws += 1;
                Ok(())
            })
            .unwrap();
        for offset in [0, 50, 100] {
            let now = initial + Duration::from_millis(offset);
            let tick = app.tick(now);
            scheduler.request(tick.redraw);
            let outcome = handle_poll_outcome(
                &mut app,
                false,
                tick.deadline,
                now,
                || panic!("idle poll must not read an event"),
                || Ok(false),
                || now,
            )
            .unwrap();
            scheduler.request(outcome.redraw);
            scheduler
                .draw_if_requested(|| {
                    draws += 1;
                    Ok(())
                })
                .unwrap();
        }

        assert_eq!(draws, 1);
    }

    #[test]
    fn synchronized_update_encloses_success_and_error_paths() {
        const BEGIN: &[u8] = b"\x1b[?2026h";
        const END: &[u8] = b"\x1b[?2026l";

        let mut success = Vec::new();
        synchronized_update(&mut success, |out| out.write_all(b"frame")).unwrap();
        assert_eq!(success, [BEGIN, b"frame", END].concat());

        let mut failure = Vec::new();
        let error = synchronized_update(&mut failure, |out| {
            out.write_all(b"partial")?;
            Err(std::io::Error::other("forced draw failure"))
        })
        .unwrap_err();
        assert_eq!(error.to_string(), "forced draw failure");
        assert_eq!(failure, [BEGIN, b"partial", END].concat());
    }

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
            crate::theme::ThemeCatalog::builtins(),
            crate::theme::ResolvedTheme::injected("default-dark", false, Tier::TrueColor),
            crate::app::AppStartupOptions::new(
                true,
                false,
                crate::config::ClipboardCopyFormat::Markdown,
                enabled,
            ),
            crate::app::AppServices::new(
                Box::new(RecordingClipboardSink::default()),
                Box::new(crate::config::DisabledConfigStore),
                crate::spell_host::SpellHost::testing(words),
                std::path::PathBuf::from("/"),
            ),
            initial,
        )
    }

    #[test]
    fn event_branch_records_post_read_time_and_performs_zero_spell_work() {
        let initial = Instant::now();
        let event_time = initial + Duration::from_secs(1);
        let mut app = test_app_with_spell_at("misspelledd\n", "known\n".to_string(), true, initial);
        let mut probed = false;

        let outcome = handle_poll_outcome(
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
        assert!(probed, "event branch must probe for a queued event batch");
        assert!(outcome.redraw);
        assert!(!app.input_idle_for(
            event_time + SPELL_IDLE_DELAY - Duration::from_nanos(1),
            SPELL_IDLE_DELAY
        ));
    }

    #[test]
    fn queued_resize_burst_dispatches_only_the_final_dimensions() {
        use std::collections::VecDeque;

        let initial = Instant::now();
        let mut app = test_app();
        let mut events = VecDeque::from([
            Event::Resize(40, 10),
            Event::Resize(60, 12),
            Event::Resize(100, 20),
        ]);
        let mut pending = VecDeque::from([true, true, false]);
        let mut sample_count = 0;

        let mut scheduler = RedrawScheduler::initial();
        let mut draws = 0;
        scheduler
            .draw_if_requested(|| {
                draws += 1;
                Ok(())
            })
            .unwrap();
        let outcome = handle_poll_outcome(
            &mut app,
            true,
            None,
            initial,
            || Ok(events.pop_front().expect("scripted resize")),
            || Ok(pending.pop_front().expect("scripted pending probe")),
            || {
                sample_count += 1;
                initial + Duration::from_millis(sample_count)
            },
        )
        .unwrap();

        let (viewport_width, scroll_follows, last_input) = app.event_loop_test_state();
        assert!(outcome.redraw);
        assert_eq!(sample_count, 3);
        assert_eq!(scroll_follows, 1);
        assert_eq!(viewport_width, 97);
        assert_eq!(last_input, initial + Duration::from_millis(3));
        scheduler.request(outcome.redraw);
        scheduler
            .draw_if_requested(|| {
                draws += 1;
                Ok(())
            })
            .unwrap();
        assert_eq!(draws, 2, "the resize burst requests one subsequent paint");
    }

    #[test]
    fn queued_non_resize_events_preserve_order_and_each_post_read_sample() {
        use std::collections::VecDeque;

        let initial = Instant::now();
        let mut app = test_app();
        let mut events = VecDeque::from([
            Event::Key(KeyEvent::new(KeyCode::Char('i'), KeyModifiers::NONE)),
            Event::Paste("typed".to_string()),
            Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)),
        ]);
        let mut pending = VecDeque::from([true, true, false]);
        let mut sample_count = 0;

        let outcome = handle_poll_outcome(
            &mut app,
            true,
            None,
            initial,
            || Ok(events.pop_front().expect("scripted event")),
            || Ok(pending.pop_front().expect("scripted pending probe")),
            || {
                sample_count += 1;
                initial + Duration::from_millis(sample_count)
            },
        )
        .unwrap();

        assert!(outcome.redraw);
        assert_eq!(sample_count, 3);
        assert_eq!(app.mode(), Mode::Normal);
        assert!(app
            .active_mut()
            .unwrap()
            .session_mut()
            .document()
            .contains("typed"));
        assert_eq!(
            app.event_loop_test_state().2,
            initial + Duration::from_millis(3)
        );
    }

    #[test]
    fn ready_event_batch_is_bounded_before_the_next_paint() {
        let initial = Instant::now();
        let mut app = test_app();
        let mut reads = 0;
        let mut probes = 0;
        let mut samples = 0usize;

        let outcome = handle_poll_outcome(
            &mut app,
            true,
            None,
            initial,
            || {
                reads += 1;
                Ok(Event::Key(KeyEvent::new(
                    KeyCode::Char('x'),
                    KeyModifiers::NONE,
                )))
            },
            || {
                probes += 1;
                Ok(true)
            },
            || {
                samples += 1;
                initial + Duration::from_millis(samples as u64)
            },
        )
        .unwrap();

        assert!(outcome.redraw);
        assert_eq!(reads, MAX_EVENT_BATCH);
        assert_eq!(samples, MAX_EVENT_BATCH);
        assert_eq!(probes, MAX_EVENT_BATCH - 1);
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
        let outcome = handle_poll_outcome(
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
        assert!(outcome.redraw, "idle spell progress must request a paint");
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
    fn pending_input_interrupts_between_bounded_gutter_units() {
        let initial = Instant::now();
        let wake = initial + SPELL_IDLE_DELAY;
        let mut app = test_app_with_spell_at(
            &"misspelledd\n".repeat(130),
            "known\n".to_string(),
            true,
            initial,
        );
        for _ in 0..10_000 {
            assert!(app.on_idle_unit(SPELL_WORK_UNIT_BYTES));
            if app.gutter_projection_pending() {
                break;
            }
        }
        assert!(app.gutter_projection_pending());
        assert_eq!(app.gutter_projection_count(), 0);
        let mut probes = 0;

        let outcome = handle_poll_outcome(
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

        assert!(outcome.redraw);
        assert_eq!(probes, 1);
        assert_eq!(app.gutter_projection_count(), 64);
        assert!(app.gutter_projection_pending());
        assert_eq!(app.gutter_snapshot_len(), 0);
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
