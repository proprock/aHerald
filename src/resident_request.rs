//! What a resident answers to one IPC request, independent of the window system. The
//! platform residents lock their shared state, call `respond` and carry out the `Effects`.
use crate::{
    config::Config,
    events::Source,
    ipc::{Request, Response},
    queue::{Outcome, Queue},
};

/// The resident state a request reads or changes.
pub trait Controls {
    fn queue(&mut self) -> &mut Queue;
    fn warnings(&mut self) -> &mut crate::session_warnings::Warnings;
    fn paused(&self) -> bool;
    fn sounds(&self) -> bool;
    fn visuals(&self) -> bool;
}

/// Called by native loops even when the presentation queue is empty.
pub fn tick(state: &mut impl Controls, settings: &Config, now: u64) {
    let paused = state.paused();
    if paused {
        return;
    }
    state.warnings().due(now, false);
    for event in state.warnings().pending() {
        deliver_warning(event, state, settings, now);
    }
}

fn deliver_warning(
    event: crate::events::Event,
    state: &mut impl Controls,
    settings: &Config,
    now: u64,
) {
    let (response, _) = respond(
        Request::Event {
            event: event.clone(),
        },
        state,
        settings,
        now,
    );
    let accepted = matches!(response, Response::Accepted);
    if state.warnings().admission(&event, accepted) {
        crate::operational_log::log(crate::operational_log::Record::QueueRejected);
    }
}

/// How long an aHerald notice card stays, before its fade-out.
pub const NOTICE_MS: u64 = 8000;

/// What the caller still has to do after the response was decided.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Effects {
    /// Stop the resident.
    pub quit: bool,
    /// Wake the presentation loop: the queue changed or the resident is stopping.
    pub wake: bool,
    /// The queue refused a valid event; log it once the state lock is released.
    pub rejected: bool,
    /// A valid event produced no card on purpose; log why (`disabled`, `duplicate`, `obsolete`)
    /// once the state lock is released.
    pub dropped: Option<&'static str>,
}

/// Puts aHerald's own notices into the resident's queue (see `notice::install`). `wake` runs
/// with the state locked, after a notice was queued, and wakes the presentation loop.
pub fn notice_sink<S: Controls + Send + 'static>(
    state: std::sync::Arc<std::sync::Mutex<S>>,
    settings: Config,
    start: std::time::Instant,
    wake: impl Fn(&S) + Send + Sync + 'static,
) -> impl Fn(crate::events::Event) + Send + Sync + 'static {
    move |event| {
        let Ok(mut state) = state.lock() else { return };
        let now = start.elapsed().as_millis().min(u64::MAX as u128) as u64;
        let (_, effects) = respond(Request::Event { event }, &mut *state, &settings, now);
        if effects.wake {
            wake(&state);
        }
    }
}

fn dropped(why: &'static str) -> Effects {
    Effects {
        dropped: Some(why),
        ..Effects::default()
    }
}

/// Decides the answer to `request`. `now` is the resident's clock in milliseconds.
pub fn respond(
    request: Request,
    state: &mut impl Controls,
    settings: &Config,
    now: u64,
) -> (Response, Effects) {
    match request {
        Request::Status => (
            Response::Status {
                pid: std::process::id(),
                queued: state.queue().items().len(),
                paused: state.paused(),
                sounds: state.sounds(),
                visuals: state.visuals(),
            },
            Effects::default(),
        ),
        Request::StatusLine { report } => {
            // The wrapper already validated the payload, but the pipe is untrusted input.
            if report.session_id.is_empty()
                || report.session_id.len() > 256
                || report.session_id.chars().any(char::is_control)
            {
                return (Response::Rejected, Effects::default());
            }
            let warning = state.warnings().context(
                crate::agent::Agent::Claude,
                &report.session_id,
                report.context_tokens,
                settings,
            );
            crate::statusline::with_store(|store| {
                store.record(report, crate::usage_service::unix_now())
            });
            if let Some(event) = warning {
                deliver_warning(event, state, settings, now);
                return (
                    Response::Accepted,
                    Effects {
                        wake: true,
                        ..Effects::default()
                    },
                );
            }
            (Response::Accepted, Effects::default())
        }
        Request::CodexContext { report } => {
            if !report.valid() {
                return (Response::Rejected, Effects::default());
            }
            let warning = state.warnings().context(
                crate::agent::Agent::Codex,
                &report.session_id,
                Some(report.tokens),
                settings,
            );
            let wake = warning.is_some();
            if let Some(event) = warning {
                deliver_warning(event, state, settings, now);
            }
            (
                Response::Accepted,
                Effects {
                    wake,
                    ..Effects::default()
                },
            )
        }
        Request::Quit => (
            Response::Bye,
            Effects {
                quit: true,
                wake: true,
                ..Effects::default()
            },
        ),
        Request::Event { event } => {
            if event.validate().is_err() {
                return (Response::Rejected, Effects::default());
            }
            state.warnings().observe(&event, settings, now);
            state.queue().clear_session_warnings(&event);
            if event.activity {
                return (
                    Response::Accepted,
                    Effects {
                        wake: true,
                        ..Effects::default()
                    },
                );
            }
            if event.source == Source::App {
                // The settings of the events belong to the agents: a notice is always shown.
                let duration = NOTICE_MS.saturating_add(settings.notifications.fade_out_ms);
                return if state.queue().push(event, now, duration).is_accepted() {
                    (
                        Response::Accepted,
                        Effects {
                            wake: true,
                            ..Effects::default()
                        },
                    )
                } else {
                    (
                        Response::Rejected,
                        Effects {
                            rejected: true,
                            ..Effects::default()
                        },
                    )
                };
            }
            let setting = &settings.events[event.kind];
            if !setting.enabled {
                // A disabled kind shows nothing but still ends its turn's waits.
                state.queue().observe_terminal(&event);
                return (
                    Response::Accepted,
                    Effects {
                        wake: state.warnings().has_deadline() || event.ended,
                        dropped: Some("disabled"),
                        ..Effects::default()
                    },
                );
            }
            let duration = if setting.duration_ms == 0 {
                0
            } else {
                setting
                    .duration_ms
                    .saturating_add(settings.notifications.fade_out_ms)
            };
            match state.queue().push(event, now, duration) {
                Outcome::Full | Outcome::Invalid => (
                    Response::Rejected,
                    Effects {
                        rejected: true,
                        ..Effects::default()
                    },
                ),
                Outcome::Duplicate => (Response::Accepted, dropped("duplicate")),
                Outcome::Obsolete => (Response::Accepted, dropped("obsolete")),
                Outcome::Queued | Outcome::Resolved => (
                    Response::Accepted,
                    Effects {
                        wake: true,
                        ..Effects::default()
                    },
                ),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::Event;

    struct State {
        queue: Queue,
        warnings: crate::session_warnings::Warnings,
        paused: bool,
        sounds: bool,
        visuals: bool,
    }
    impl Controls for State {
        fn warnings(&mut self) -> &mut crate::session_warnings::Warnings {
            &mut self.warnings
        }
        fn queue(&mut self) -> &mut Queue {
            &mut self.queue
        }
        fn paused(&self) -> bool {
            self.paused
        }
        fn sounds(&self) -> bool {
            self.sounds
        }
        fn visuals(&self) -> bool {
            self.visuals
        }
    }
    fn state(capacity: usize) -> State {
        State {
            queue: Queue::new(capacity, 0),
            warnings: Default::default(),
            paused: true,
            sounds: false,
            visuals: true,
        }
    }
    fn event(kind: &str) -> Event {
        Event::demo(kind).unwrap()
    }
    fn send(state: &mut State, settings: &Config, event: Event) -> (Response, Effects) {
        respond(Request::Event { event }, state, settings, 7)
    }

    #[test]
    fn status_reports_the_process_the_queue_and_the_switches() {
        let mut s = state(8);
        let settings = Config::default();
        send(&mut s, &settings, event("turn_complete"));
        let (response, effects) = respond(Request::Status, &mut s, &settings, 0);
        match response {
            Response::Status {
                pid,
                queued,
                paused,
                sounds,
                visuals,
            } => {
                assert_eq!(pid, std::process::id());
                assert_eq!((queued, paused, sounds, visuals), (1, true, false, true));
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(effects, Effects::default());
    }

    #[test]
    fn session_warning_retries_context_and_idle_after_the_queue_has_room() {
        use crate::events::{Kind, Source};
        let settings = Config::parse(
            "[claude.warnings]\ncontext_tokens=100\nidle_minutes=1",
            std::path::Path::new("."),
        )
        .unwrap();
        for context in [true, false] {
            let mut s = state(1);
            s.paused = false;
            let mut wait = Event::demo("permission_required").unwrap();
            wait.source = Source::ClaudeHook;
            wait.session_id = Some("other-session".into());
            respond(Request::Event { event: wait }, &mut s, &settings, 0);
            if context {
                let report = crate::statusline::parse(br#"{"session_id":"full-queue","context_window":{"current_usage":{"input_tokens":101,"cache_read_input_tokens":0,"cache_creation_input_tokens":0}}}"#).unwrap();
                respond(Request::StatusLine { report }, &mut s, &settings, 0);
            } else {
                let mut stop = Event::demo("turn_complete").unwrap();
                stop.source = Source::ClaudeHook;
                stop.session_id = Some("full-queue".into());
                respond(Request::Event { event: stop }, &mut s, &settings, 0);
                tick(&mut s, &settings, 60_001);
            }
            assert_eq!(s.queue.items().len(), 1);
            s.queue.dismiss();
            tick(&mut s, &settings, 60_002);
            assert_eq!(
                s.queue.items().len(),
                1,
                "a full queue must not consume the warning"
            );
            assert_eq!(
                s.queue.items()[0].event.kind,
                if context {
                    Kind::ContextWarning
                } else {
                    Kind::IdleWarning
                }
            );
            s.queue.dismiss();
            tick(&mut s, &settings, 120_000);
            assert!(
                s.queue.items().is_empty(),
                "delivered warning must not repeat"
            );
        }
    }

    #[test]
    fn session_warning_resident_arms_even_with_completion_disabled_and_wakes_on_activity() {
        use crate::events::{Kind, Source};
        let settings = Config::parse(
            "[claude.warnings]\nidle_minutes=1\n[events.turn_complete]\nenabled=false",
            std::path::Path::new("."),
        )
        .unwrap();
        let mut s = state(8);
        s.paused = false;
        let mut completion = Event::demo("turn_complete").unwrap();
        completion.source = Source::ClaudeHook;
        completion.session_id = Some("s".into());
        let (_, effects) = respond(
            Request::Event {
                event: completion.clone(),
            },
            &mut s,
            &settings,
            0,
        );
        assert!(
            effects.wake,
            "an empty native loop must arm its deadline timer"
        );
        assert!(s.queue.items().is_empty());
        tick(&mut s, &settings, 60_000);
        assert!(s.queue.items().is_empty());
        tick(&mut s, &settings, 60_001);
        assert_eq!(s.queue.items().len(), 1);
        assert_eq!(s.queue.items()[0].event.kind, Kind::IdleWarning);
        assert_eq!(s.queue.items()[0].event.session_id.as_deref(), Some("s"));
        completion.activity = true;
        respond(
            Request::Event { event: completion },
            &mut s,
            &settings,
            70_000,
        );
        assert!(
            s.queue.items().is_empty(),
            "new activity dismisses a stale idle reminder"
        );
    }

    #[test]
    fn session_warning_context_report_queues_warning_and_keeps_session_name() {
        use crate::events::Kind;
        let settings = Config::parse(
            "[claude.warnings]\ncontext_tokens=100",
            std::path::Path::new("."),
        )
        .unwrap();
        let mut s = state(8);
        let report = crate::statusline::parse(br#"{"session_id":"context-resident-test","session_name":"Example","context_window":{"current_usage":{"input_tokens":101,"cache_read_input_tokens":0,"cache_creation_input_tokens":0}}}"#).unwrap();
        let (reply, effects) = respond(
            Request::StatusLine {
                report: report.clone(),
            },
            &mut s,
            &settings,
            0,
        );
        assert!(matches!(reply, Response::Accepted));
        assert!(effects.wake);
        assert_eq!(s.queue.items()[0].event.kind, Kind::ContextWarning);
        respond(Request::StatusLine { report }, &mut s, &settings, 1);
        assert_eq!(s.queue.items().len(), 1);
        assert_eq!(
            crate::statusline::with_store(|s| s.name("context-resident-test").map(str::to_owned)),
            Some("Example".into())
        );
    }

    #[test]
    fn codex_context_ipc_queues_once_and_rejects_invalid_identity() {
        let settings = Config::parse(
            "[codex.warnings]\ncontext_tokens=100",
            std::path::Path::new("."),
        )
        .unwrap();
        let mut s = state(8);
        let report = crate::codex_context::Report {
            session_id: "codex-context-test".into(),
            tokens: 101,
        };
        let request = Request::CodexContext {
            report: report.clone(),
        };
        let frame = crate::ipc::encode(&request).unwrap();
        let request = crate::ipc::decode(&frame).unwrap();
        let (reply, effects) = respond(request, &mut s, &settings, 0);
        assert!(matches!(reply, Response::Accepted));
        assert!(effects.wake);
        assert_eq!(
            s.queue.items()[0].event.kind,
            crate::events::Kind::ContextWarning
        );
        assert_eq!(s.queue.items()[0].event.source, Source::CodexHook);
        respond(Request::CodexContext { report }, &mut s, &settings, 1);
        assert_eq!(s.queue.items().len(), 1);
        let (reply, _) = respond(
            Request::CodexContext {
                report: crate::codex_context::Report {
                    session_id: "".into(),
                    tokens: 200,
                },
            },
            &mut s,
            &settings,
            2,
        );
        assert!(matches!(reply, Response::Rejected));
        assert_eq!(s.queue.items().len(), 1);
    }

    #[test]
    fn quit_says_goodbye_and_asks_for_a_stop_and_a_wake() {
        let (response, effects) = respond(Request::Quit, &mut state(8), &Config::default(), 0);
        assert!(matches!(response, Response::Bye));
        assert_eq!(
            effects,
            Effects {
                quit: true,
                wake: true,
                ..Effects::default()
            }
        );
    }

    #[test]
    fn an_enabled_event_is_queued_and_wakes_the_loop() {
        let mut s = state(8);
        let (response, effects) = send(&mut s, &Config::default(), event("turn_complete"));
        assert!(matches!(response, Response::Accepted));
        assert_eq!(
            effects,
            Effects {
                wake: true,
                ..Effects::default()
            }
        );
        assert_eq!(s.queue.items().len(), 1);
    }

    #[test]
    fn a_timed_event_lives_for_its_duration_plus_the_fade_and_a_persistent_one_forever() {
        let settings = Config::default();
        let mut s = state(8);
        send(&mut s, &settings, event("turn_complete"));
        send(&mut s, &settings, event("permission_required"));
        let fade = settings.notifications.fade_out_ms;
        let durations: Vec<(String, u64)> = s
            .queue
            .items()
            .iter()
            .map(|e| (e.event.kind.name().to_owned(), e.duration))
            .collect();
        assert_eq!(durations.len(), 2);
        for (kind, duration) in durations {
            let configured = settings.events[event(&kind).kind].duration_ms;
            let expected = if configured == 0 {
                0
            } else {
                configured + fade
            };
            assert_eq!(duration, expected, "{kind}");
        }
    }

    #[test]
    fn a_disabled_kind_shows_nothing_and_does_not_wake_the_loop() {
        let mut settings = Config::default();
        let kind = event("turn_complete").kind;
        settings.events[kind].enabled = false;
        let mut s = state(8);
        let (response, effects) = send(&mut s, &settings, event("turn_complete"));
        assert!(matches!(response, Response::Accepted));
        assert_eq!(
            effects,
            Effects {
                dropped: Some("disabled"),
                ..Effects::default()
            }
        );
        assert!(s.queue.items().is_empty());
    }

    #[test]
    fn a_disabled_kind_asks_for_a_debug_line_naming_why_it_was_dropped() {
        let mut settings = Config::default();
        let kind = event("turn_complete").kind;
        settings.events[kind].enabled = false;
        let (_, effects) = send(&mut state(8), &settings, event("turn_complete"));
        assert_eq!(effects.dropped, Some("disabled"));
        assert!(!effects.wake);
    }

    #[test]
    fn a_repeat_inside_the_dedup_window_asks_for_a_debug_line() {
        let settings = Config::default();
        let mut s = State {
            queue: Queue::new(8, 5000),
            ..state(8)
        };
        let (_, first) = send(&mut s, &settings, event("turn_complete"));
        assert_eq!(first.dropped, None);
        let (response, second) = send(&mut s, &settings, event("turn_complete"));
        assert!(matches!(response, Response::Accepted));
        assert_eq!(second.dropped, Some("duplicate"));
        assert!(!second.wake);
    }

    #[test]
    fn a_wait_that_arrives_after_its_turn_ended_asks_for_a_debug_line() {
        let settings = Config::default();
        let mut s = state(8);
        let mut done = event("interrupted");
        done.source = crate::events::Source::CodexHook;
        done.session_id = Some("s".into());
        done.turn_id = Some("t".into());
        let mut late = event("permission_required");
        late.source = crate::events::Source::CodexHook;
        late.session_id = Some("s".into());
        late.turn_id = Some("t".into());
        send(&mut s, &settings, done);
        let (_, effects) = send(&mut s, &settings, late);
        assert_eq!(effects.dropped, Some("obsolete"));
    }

    #[test]
    fn an_invalid_event_is_rejected_without_a_log_or_a_wake() {
        let mut bad = event("turn_complete");
        bad.window = Some(0);
        let (response, effects) = send(&mut state(8), &Config::default(), bad);
        assert!(matches!(response, Response::Rejected));
        assert_eq!(effects, Effects::default());
    }

    #[test]
    fn a_full_queue_rejects_a_valid_event_and_asks_for_it_to_be_logged() {
        let settings = Config::default();
        let mut s = state(1);
        send(&mut s, &settings, event("turn_complete"));
        let (response, effects) = send(&mut s, &settings, event("error"));
        assert!(matches!(response, Response::Rejected));
        assert_eq!(
            effects,
            Effects {
                rejected: true,
                ..Effects::default()
            }
        );
    }

    #[test]
    fn an_aherald_notice_ignores_the_error_card_settings_of_the_agents() {
        let mut settings = Config::default();
        let error = Event::notice("x").kind;
        settings.events[error].enabled = false;
        settings.events[error].duration_ms = 0;
        let mut s = state(8);
        let (response, effects) = send(&mut s, &settings, Event::notice("Sound too long"));
        assert!(matches!(response, Response::Accepted));
        assert!(effects.wake);
        let shown = s.queue.items();
        assert_eq!(shown.len(), 1);
        assert_eq!(
            shown[0].duration,
            NOTICE_MS + settings.notifications.fade_out_ms
        );
    }

    #[test]
    fn the_notice_sink_queues_a_notice_and_wakes_the_loop_once() {
        use std::sync::{
            Arc, Mutex,
            atomic::{AtomicUsize, Ordering},
        };
        let shared = Arc::new(Mutex::new(state(1)));
        let wakes = Arc::new(AtomicUsize::new(0));
        let counter = wakes.clone();
        let sink = notice_sink(
            shared.clone(),
            Config::default(),
            std::time::Instant::now(),
            move |s| {
                assert!(s.paused, "the state is the locked one");
                counter.fetch_add(1, Ordering::SeqCst);
            },
        );
        sink(Event::notice("Sound over 5 s: a.wav"));
        assert_eq!(wakes.load(Ordering::SeqCst), 1);
        let queued = shared.lock().unwrap();
        assert_eq!(queued.queue.items().len(), 1);
        assert_eq!(queued.queue.items()[0].event.source, Source::App);
        drop(queued);
        // The queue is full: the notice is dropped without a wake.
        sink(Event::notice("another"));
        assert_eq!(wakes.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn a_statusline_report_is_stored_and_acknowledged_without_touching_the_queue() {
        let mut state = state(8);
        let report = crate::statusline::Report {
            session_id: "respond-statusline-test".into(),
            session_name: Some("Named".into()),
            quota: None,
            context_tokens: None,
        };
        let (response, effects) = respond(
            Request::StatusLine {
                report: report.clone(),
            },
            &mut state,
            &Config::default(),
            0,
        );
        assert!(matches!(response, Response::Accepted));
        assert_eq!(effects, Effects::default());
        assert!(state.queue().items().is_empty());
        let stored =
            crate::statusline::with_store(|s| s.name("respond-statusline-test").map(str::to_owned));
        assert_eq!(stored.as_deref(), Some("Named"));
        let invalid = crate::statusline::Report {
            session_id: String::new(),
            ..report
        };
        let (response, _) = respond(
            Request::StatusLine { report: invalid },
            &mut state,
            &Config::default(),
            0,
        );
        assert!(matches!(response, Response::Rejected));
    }
}
