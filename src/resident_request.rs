//! What a resident answers to one IPC request, independent of the window system. The
//! platform residents lock their shared state, call `respond` and carry out the `Effects`.
use crate::{
    config::Config,
    ipc::{Request, Response},
    queue::Queue,
};

/// The resident state a request reads or changes.
pub trait Controls {
    fn queue(&mut self) -> &mut Queue;
    fn paused(&self) -> bool;
    fn sounds(&self) -> bool;
    fn visuals(&self) -> bool;
}

/// What the caller still has to do after the response was decided.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Effects {
    /// Stop the resident.
    pub quit: bool,
    /// Wake the presentation loop: the queue changed or the resident is stopping.
    pub wake: bool,
    /// The queue refused a valid event; log it once the state lock is released.
    pub rejected: bool,
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
        Request::Quit => (
            Response::Bye,
            Effects {
                quit: true,
                wake: true,
                rejected: false,
            },
        ),
        Request::Event { event } => {
            if event.validate().is_err() {
                return (Response::Rejected, Effects::default());
            }
            let setting = &settings.events[event.kind];
            if !setting.enabled {
                // A disabled kind shows nothing but still ends its turn's waits.
                state.queue().observe_terminal(&event);
                return (Response::Accepted, Effects::default());
            }
            let duration = if setting.duration_ms == 0 {
                0
            } else {
                setting
                    .duration_ms
                    .saturating_add(settings.notifications.fade_out_ms)
            };
            if state.queue().push(event, now, duration).is_accepted() {
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
        paused: bool,
        sounds: bool,
        visuals: bool,
    }
    impl Controls for State {
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
    fn quit_says_goodbye_and_asks_for_a_stop_and_a_wake() {
        let (response, effects) = respond(Request::Quit, &mut state(8), &Config::default(), 0);
        assert!(matches!(response, Response::Bye));
        assert_eq!(
            effects,
            Effects {
                quit: true,
                wake: true,
                rejected: false
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
                quit: false,
                wake: true,
                rejected: false
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
        assert_eq!(effects, Effects::default());
        assert!(s.queue.items().is_empty());
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
                quit: false,
                wake: false,
                rejected: true
            }
        );
    }
}
