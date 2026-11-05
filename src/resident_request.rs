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
    /// Called for each event the queue accepted as a new card, for the messenger sinks. It must
    /// return at once; the default hands it to the resident's sinks (`crate::forward`).
    fn forward(&mut self, event: &crate::events::Event) {
        crate::forward::submit(event);
    }
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

/// Whether an idle resident, with nothing shown, still has to wake up on its own: a usage source
/// (Codex or Claude) refreshes the tray icon and tooltip, and a session warning has a deadline.
/// Without any, it blocks until an event arrives. Shared by the Windows timer and the macOS wait, so
/// they cannot disagree about which usage sources count (AH-232).
pub fn idle_poll_needed(codex_usage: bool, claude_usage: bool, has_deadline: bool) -> bool {
    codex_usage || claude_usage || has_deadline
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
    /// An untrusted input failed validation; never include its contents in diagnostics.
    pub invalid: bool,
    /// A valid event produced no card on purpose; log why (`disabled`, `duplicate`, `obsolete`)
    /// once the state lock is released.
    pub dropped: Option<&'static str>,
}

/// Failure episodes belong to one listener. Status polling does not constitute recovery.
#[derive(Default)]
pub struct Diagnostics {
    invalid: bool,
    full: bool,
}
impl Diagnostics {
    pub fn record(&mut self, response: &Response, effects: Effects) {
        use crate::operational_log::{Record, log};
        if effects.invalid && !self.invalid {
            log(Record::InputRejected);
        }
        if effects.rejected && !self.full {
            log(Record::QueueRejected);
        }
        if matches!(response, Response::Accepted) {
            self.invalid = false;
            self.full = false;
        } else {
            self.invalid |= effects.invalid;
            self.full |= effects.rejected;
        }
        if let Some(reason) = effects.dropped {
            log(Record::EventDropped(reason));
        }
    }
}

fn invalid() -> (Response, Effects) {
    (
        Response::Rejected,
        Effects {
            invalid: true,
            ..Effects::default()
        },
    )
}

/// Puts aHerald's own notices into the resident's queue (see `notice::install`). `wake` runs
/// with the state locked, after a notice was queued, and wakes the presentation loop.
pub fn notice_sink<S: Controls + Send + 'static>(
    state: std::sync::Arc<std::sync::Mutex<S>>,
    settings: Config,
    start: std::time::Instant,
    wake: impl Fn(&S) + Send + Sync + 'static,
) -> impl Fn(crate::events::Event) + Send + Sync + 'static {
    let diagnostics = std::sync::Mutex::new(Diagnostics::default());
    move |event| {
        let Ok(mut state) = state.lock() else { return };
        let now = start.elapsed().as_millis().min(u64::MAX as u128) as u64;
        let (response, effects) = respond(Request::Event { event }, &mut *state, &settings, now);
        if effects.wake {
            wake(&state);
        }
        drop(state);
        diagnostics
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .record(&response, effects);
    }
}

/// How the `[remote]` listener hands an event to this resident: the decision the local pipe
/// makes, on the caller's thread. `wake` runs with the state locked, after the queue changed.
pub fn remote_deliver<S: Controls + Send + 'static>(
    state: std::sync::Arc<std::sync::Mutex<S>>,
    settings: Config,
    start: std::time::Instant,
    wake: impl Fn(&S) + Send + Sync + 'static,
) -> crate::remote::Deliver {
    let diagnostics = std::sync::Mutex::new(Diagnostics::default());
    std::sync::Arc::new(move |request| {
        let Ok(mut state) = state.lock() else {
            return Response::Rejected;
        };
        let now = start.elapsed().as_millis().min(u64::MAX as u128) as u64;
        let (response, effects) = respond(request, &mut *state, &settings, now);
        if effects.wake {
            wake(&state);
        }
        drop(state);
        diagnostics
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .record(&response, effects);
        response
    })
}

/// The last unparsed answer of `source`, or why there is none.
fn raw_output(source: crate::ipc::RawSource, settings: &Config) -> Response {
    use crate::{agent::Agent, ipc::RawSource};
    let unix = crate::usage_service::unix_now();
    let found = |note: &str, at: i64, body: &str| Response::RawOutput {
        found: true,
        note: note.to_owned(),
        age_secs: unix.saturating_sub(at).max(0) as u64,
        body: body.to_owned(),
    };
    let missing = |note: &str| Response::RawOutput {
        found: false,
        note: note.to_owned(),
        age_secs: 0,
        body: String::new(),
    };
    let usage =
        |agent: Agent, enabled: bool, setting: &str| match crate::usage_service::last_raw(agent) {
            Some((note, at, body)) => found(&note, at, &body),
            None if enabled => {
                missing("no answer received yet; usage is refreshed every few minutes")
            }
            None => missing(&format!(
                "usage is off; set {setting} enabled = true and apply"
            )),
        };
    match source {
        RawSource::CodexUsage => usage(Agent::Codex, settings.codex.usage.enabled, "[codex.usage]"),
        RawSource::ClaudeUsage => usage(
            Agent::Claude,
            settings.claude.usage.enabled,
            "[claude.usage]",
        ),
        RawSource::ClaudeStatusLine => crate::statusline::with_store(|store| match store.raw() {
            Some((body, at)) => found("statusLine payload", at, body),
            None => missing(
                "no statusLine report yet; install the wrapper (aherald --install-statusline) \
                 and use Claude Code",
            ),
        }),
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
    respond_with_focus(request, state, settings, now, crate::focus::observe)
}

fn respond_with_focus(
    request: Request,
    state: &mut impl Controls,
    settings: &Config,
    now: u64,
    observe: impl Fn(&crate::events::Event, &Config) -> crate::focus::Evidence,
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
        Request::StatusLine { report, raw } => {
            // The wrapper already validated the payload, but the pipe is untrusted input.
            if report.session_id.is_empty()
                || report.session_id.len() > 256
                || report.session_id.chars().any(char::is_control)
            {
                return invalid();
            }
            let warning = state.warnings().context(
                crate::agent::Agent::Claude,
                &report.session_id,
                report.context_tokens,
                settings,
            );
            crate::statusline::with_store(|store| {
                let now = crate::usage_service::unix_now();
                store.record(report, now);
                if let Some(raw) = raw {
                    store.record_raw(raw, now);
                }
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
        Request::RawOutput { source } => (raw_output(source, settings), Effects::default()),
        Request::CodexContext { report } => {
            if !report.valid() {
                return invalid();
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
                return invalid();
            }
            state.warnings().observe(&event, settings, now);
            state.queue().clear_session_warnings(&event);
            if event.activity {
                state.queue().observe_activity(&event);
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
            let outputs = crate::focus::outputs(settings, observe(&event, settings));
            let accepted = event.clone();
            match state
                .queue()
                .push_with_outputs(event, now, duration, outputs)
            {
                Outcome::Full | Outcome::Invalid => (
                    Response::Rejected,
                    Effects {
                        rejected: true,
                        ..Effects::default()
                    },
                ),
                Outcome::Duplicate => (Response::Accepted, dropped("duplicate")),
                Outcome::Obsolete => (Response::Accepted, dropped("obsolete")),
                outcome @ (Outcome::Queued | Outcome::Resolved) => {
                    if outcome == Outcome::Queued {
                        state.forward(&accepted);
                    }
                    (
                        Response::Accepted,
                        Effects {
                            wake: true,
                            ..Effects::default()
                        },
                    )
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn an_idle_resident_polls_for_usage_or_a_warning_deadline_and_for_nothing_else() {
        for codex_usage in [false, true] {
            for claude_usage in [false, true] {
                for has_deadline in [false, true] {
                    assert_eq!(
                        idle_poll_needed(codex_usage, claude_usage, has_deadline),
                        codex_usage || claude_usage || has_deadline,
                        "codex {codex_usage}, claude {claude_usage}, deadline {has_deadline}"
                    );
                }
            }
        }
        // The case that macOS got wrong: Claude usage alone keeps the tray fresh.
        assert!(idle_poll_needed(false, true, false));
        assert!(!idle_poll_needed(false, false, false));
    }

    use super::*;
    use crate::events::Event;

    struct State {
        queue: Queue,
        warnings: crate::session_warnings::Warnings,
        paused: bool,
        sounds: bool,
        visuals: bool,
        forwarded: Vec<Event>,
    }
    impl Controls for State {
        fn forward(&mut self, event: &Event) {
            self.forwarded.push(event.clone());
        }
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
            forwarded: Vec::new(),
        }
    }
    fn event(kind: &str) -> Event {
        Event::demo(kind).unwrap()
    }
    fn send(state: &mut State, settings: &Config, event: Event) -> (Response, Effects) {
        respond(Request::Event { event }, state, settings, 7)
    }

    fn hook(kind: &str, session: &str) -> Event {
        let mut event = event(kind);
        event.source = Source::ClaudeHook;
        event.session_id = Some(session.into());
        event
    }

    #[test]
    fn only_a_card_the_queue_accepted_is_forwarded_once() {
        let settings = Config::default();
        let mut s = state(8);
        send(&mut s, &settings, hook("input_required", "a"));
        assert_eq!(s.forwarded.len(), 1);
        assert_eq!(s.forwarded[0].kind.name(), "input_required");
    }

    #[test]
    fn a_disabled_kind_a_full_queue_and_an_aherald_notice_are_not_forwarded() {
        let mut settings = Config::default();
        settings.events[crate::events::Kind::TurnComplete].enabled = false;
        let mut s = state(1);
        send(&mut s, &settings, hook("turn_complete", "a"));
        assert!(s.forwarded.is_empty(), "disabled kind");
        send(&mut s, &settings, hook("permission_required", "a"));
        send(&mut s, &settings, hook("permission_required", "b"));
        assert_eq!(s.forwarded.len(), 1, "the second did not fit the queue");
        send(&mut s, &settings, Event::notice("a problem"));
        assert_eq!(s.forwarded.len(), 1, "an aHerald notice is not forwarded");
    }

    #[test]
    fn activity_and_a_repeated_wait_for_the_same_request_are_not_forwarded() {
        let settings = Config::default();
        let mut s = state(8);
        let mut activity = hook("turn_complete", "a");
        activity.activity = true;
        send(&mut s, &settings, activity);
        assert!(s.forwarded.is_empty(), "activity is evidence, never a card");
        let mut s = State {
            queue: Queue::new(8, 60_000),
            ..state(8)
        };
        send(&mut s, &settings, hook("permission_required", "a"));
        send(&mut s, &settings, hook("permission_required", "a"));
        assert_eq!(s.forwarded.len(), 1, "a duplicate is not a new card");
    }

    #[test]
    fn focused_output_decisions_survive_admission_and_preserve_terminal_evidence() {
        use crate::focus::{Evidence, Outputs, Session};
        for visual in [false, true] {
            for sound in [false, true] {
                for focused in [None, Some(false), Some(true)] {
                    for session in [Session::Same, Session::Different, Session::Unknown] {
                        let mut settings = Config::default();
                        settings.notifications.suppress_when_focused = visual;
                        settings.sounds.suppress_when_focused = sound;
                        let mut s = state(8);
                        let mut wait = event("permission_required");
                        wait.source = Source::ClaudeHook;
                        wait.session_id = Some("session-a".into());
                        let evidence = Evidence {
                            window_focused: focused,
                            session,
                        };
                        let (response, _) = respond_with_focus(
                            Request::Event { event: wait },
                            &mut s,
                            &settings,
                            0,
                            |_, _| evidence,
                        );
                        assert!(matches!(response, Response::Accepted));
                        let expected = if focused == Some(true) && session == Session::Same {
                            Outputs {
                                window: !visual,
                                sound: !sound,
                            }
                        } else {
                            Outputs::default()
                        };
                        assert_eq!(s.queue.items()[0].outputs, expected);
                        let fresh = s.queue.take_unannounced(8);
                        assert_eq!(fresh.len(), usize::from(expected.sound));
                        assert_eq!(s.queue.visible_count(), usize::from(expected.window));
                        assert!(s.queue.take_unannounced(8).is_empty());
                        let mut end = event("turn_complete");
                        end.source = Source::ClaudeHook;
                        end.session_id = Some("session-a".into());
                        // Focus changes cannot resurrect the hidden wait; matching terminal
                        // evidence clears it even if the completion kind is disabled.
                        settings.events[crate::events::Kind::TurnComplete].enabled = false;
                        send(&mut s, &settings, end);
                        assert!(s.queue.items().is_empty());
                    }
                }
            }
        }
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
            "[claude.warnings]\nidle_enabled=true\ncontext_tokens=100\nidle_minutes=1",
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
                respond(
                    Request::StatusLine { report, raw: None },
                    &mut s,
                    &settings,
                    0,
                );
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
            "[claude.warnings]\nidle_enabled=true\nidle_minutes=1\n[events.turn_complete]\nenabled=false",
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
                raw: None,
            },
            &mut s,
            &settings,
            0,
        );
        assert!(matches!(reply, Response::Accepted));
        assert!(effects.wake);
        assert_eq!(s.queue.items()[0].event.kind, Kind::ContextWarning);
        respond(
            Request::StatusLine { report, raw: None },
            &mut s,
            &settings,
            1,
        );
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

    fn external(harness: &str, kind: &str, session: &str) -> Event {
        crate::events::external_event(
            harness,
            Event::demo(kind).unwrap().kind,
            false,
            false,
            format!(r#"{{"session_id":"{session}"}}"#).as_bytes(),
        )
    }

    #[test]
    fn external_harness_events_queue_per_harness_and_resolve_by_session() {
        let settings = Config::default();
        let mut s = state(8);
        for harness in ["opencode", "pi"] {
            let (response, effects) = send(
                &mut s,
                &settings,
                external(harness, "permission_required", "s1"),
            );
            assert!(matches!(response, Response::Accepted), "{harness}");
            assert!(effects.wake);
        }
        assert_eq!(
            s.queue.items().len(),
            2,
            "the same session id in two harnesses"
        );

        // Resumed activity of one harness resolves only its own wait and still wakes the loop.
        let mut resumed = external("opencode", "turn_complete", "s1");
        resumed.activity = true;
        let (response, effects) = send(&mut s, &settings, resumed);
        assert!(matches!(response, Response::Accepted));
        assert!(effects.wake);
        let left: Vec<&str> = s
            .queue
            .items()
            .iter()
            .map(|i| i.event.source.agent_key())
            .collect();
        assert_eq!(left, ["pi"]);

        // The end of that turn resolves the other wait; a card of the turn itself is shown.
        send(&mut s, &settings, external("pi", "turn_complete", "s1"));
        let kinds: Vec<_> = s.queue.items().iter().map(|i| i.event.kind).collect();
        assert_eq!(kinds, [crate::events::Kind::TurnComplete]);
    }

    #[test]
    fn an_external_event_with_a_reserved_or_invalid_harness_is_rejected() {
        let mut s = state(8);
        for key in ["claude", "Bad Key", ""] {
            let mut event = external("opencode", "turn_complete", "s1");
            event.source = crate::events::Source::External(key.into());
            let (response, effects) = send(&mut s, &Config::default(), event);
            assert!(matches!(response, Response::Rejected), "{key:?}");
            assert!(!effects.wake);
        }
        assert!(s.queue.items().is_empty());
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
    fn an_invalid_event_is_rejected_and_asks_for_a_log_without_a_wake() {
        let mut bad = event("turn_complete");
        bad.window = Some(0);
        let (response, effects) = send(&mut state(8), &Config::default(), bad);
        assert!(matches!(response, Response::Rejected));
        assert_ne!(effects, Effects::default());
        assert!(!effects.wake);
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
    fn raw_output_returns_the_kept_answer_or_says_why_there_is_none() {
        use crate::ipc::RawSource;
        let ask = |source, settings: &Config| {
            let mut state = state(8);
            match respond(Request::RawOutput { source }, &mut state, settings, 0).0 {
                Response::RawOutput {
                    found, note, body, ..
                } => (found, note, body),
                other => panic!("{other:?}"),
            }
        };
        // Usage that is switched off says how to switch it on; nothing is invented.
        let off = Config::default();
        assert!(!off.claude.usage.enabled);
        let (found, note, _) = ask(RawSource::ClaudeUsage, &off);
        assert!(
            found || note.contains("[claude.usage] enabled = true"),
            "{note}"
        );
        // A statusLine payload travels with its report and comes back untouched.
        let mut state = state(8);
        let report = crate::statusline::Report {
            session_id: "respond-raw-test".into(),
            session_name: None,
            quota: None,
            context_tokens: None,
        };
        let payload = r#"{"session_id":"respond-raw-test","cost":{"total_cost_usd":0.1}}"#;
        respond(
            Request::StatusLine {
                report,
                raw: Some(payload.into()),
            },
            &mut state,
            &off,
            0,
        );
        let (found, note, body) = ask(RawSource::ClaudeStatusLine, &off);
        assert!(found && body == payload, "{note}");
        // Whatever a usage source last said is returned as it was kept.
        crate::usage_service::keep_raw(crate::agent::Agent::Claude, "HTTP 200", b"{}");
        assert!(ask(RawSource::ClaudeUsage, &off).0);
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
                raw: None,
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
            Request::StatusLine {
                report: invalid,
                raw: None,
            },
            &mut state,
            &Config::default(),
            0,
        );
        assert!(matches!(response, Response::Rejected));
    }
}
