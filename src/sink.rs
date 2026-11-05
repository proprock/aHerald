//! Sinks: where an accepted card is also sent (a messenger, a webhook).
//!
//! A sink sees only events the queue accepted as new cards, so deduplication, per-event
//! `enabled`, session ends and obsolete waits are settled before it. It never feeds anything back,
//! and a slow or hung one cannot delay cards: each sink has its own worker thread and a bounded
//! queue that drops its oldest message when full.
use crate::{events::Event, events::Kind};
use std::{
    collections::VecDeque,
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

/// What a sink may know about a card: the kind, the harness and, only when allowed, the folder
/// name. Never a prompt, an assistant message, a path or a session name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Message {
    pub kind: String,
    pub harness: String,
    pub folder: Option<String>,
    pub text: String,
}

impl Message {
    /// The message for `event`; the folder is included only when `include_folder` is set.
    pub fn from_event(event: &Event, include_folder: bool) -> Self {
        let kind = event.kind.name().to_owned();
        let harness = event.source.label().to_owned();
        let folder = event.folder.clone().filter(|_| include_folder);
        let what = kind.replace('_', " ");
        let text = match &folder {
            Some(folder) => format!("{harness} ({folder}): {what}"),
            None => format!("{harness}: {what}"),
        };
        Self {
            kind,
            harness,
            folder,
            text,
        }
    }
}

/// Why a send failed. `message` must never contain a secret. `retry_after` is the delay the
/// destination asked for (HTTP 429 or 503 with `Retry-After`), which the worker waits instead of
/// its fixed backoff, up to `MAX_RETRY_AFTER`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SendError {
    pub message: String,
    pub retry_after: Option<Duration>,
}

impl From<String> for SendError {
    fn from(message: String) -> Self {
        Self {
            message,
            retry_after: None,
        }
    }
}

impl From<&str> for SendError {
    fn from(message: &str) -> Self {
        message.to_owned().into()
    }
}

/// The longest a sink waits because a server asked it to; a larger request is cut to this, so a
/// hostile or mistaken `Retry-After` cannot park a worker for hours.
pub const MAX_RETRY_AFTER: Duration = Duration::from_secs(30);

/// The pause before the next try: what the server asked for, capped, else the fixed backoff.
pub fn retry_delay(backoff: Duration, retry_after: Option<Duration>) -> Duration {
    retry_after.map_or(backoff, |delay| delay.min(MAX_RETRY_AFTER))
}

/// One destination. `send` blocks until the message is delivered or fails.
pub trait Sink: Send {
    fn name(&self) -> &str;
    fn send(&mut self, message: &Message) -> Result<(), SendError>;
}

/// A sink with the event kinds it wants and the secret values to scrub from its errors.
pub struct Route {
    pub sink: Box<dyn Sink>,
    pub kinds: Vec<Kind>,
    pub secrets: Vec<String>,
    /// Whether messages built from an event for this sink name the folder.
    pub include_folder: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct Policy {
    /// Messages a sink may have waiting; the oldest is dropped beyond this.
    pub queue: usize,
    /// Tries for one message, the first included.
    pub attempts: u32,
    /// Pause before each retry.
    pub backoff: Duration,
}

impl Default for Policy {
    fn default() -> Self {
        Self {
            queue: 32,
            attempts: 3,
            backoff: Duration::from_secs(2),
        }
    }
}

/// Replaces every non-empty secret in `text` with `***`.
pub fn redact(text: &str, secrets: &[String]) -> String {
    secrets
        .iter()
        .filter(|secret| !secret.is_empty())
        .fold(text.to_owned(), |text, secret| text.replace(secret, "***"))
}

/// The messages waiting for one sink's worker.
#[derive(Default)]
struct Queue {
    messages: VecDeque<Message>,
    closed: bool,
}

struct Channel {
    kinds: Vec<Kind>,
    include_folder: bool,
    queue: Arc<(Mutex<Queue>, Condvar)>,
    capacity: usize,
}

pub struct Dispatcher {
    enabled: Arc<AtomicBool>,
    channels: Vec<Channel>,
}

impl Drop for Dispatcher {
    /// Tells the workers to stop. They are not joined: one may be inside a hung network call, and
    /// it ends on its own when that call times out.
    fn drop(&mut self) {
        for channel in &self.channels {
            let (lock, wake) = &*channel.queue;
            lock.lock().unwrap_or_else(|e| e.into_inner()).closed = true;
            wake.notify_all();
        }
    }
}

impl Dispatcher {
    /// Starts one worker per route. `post` receives an error card text; it is called for the first
    /// error of a sink and again only when the text changes or after a success.
    pub fn start(
        routes: Vec<Route>,
        policy: Policy,
        enabled: bool,
        post: impl Fn(&str) + Send + Sync + 'static,
    ) -> Self {
        let post: Arc<dyn Fn(&str) + Send + Sync> = Arc::new(post);
        let channels = routes
            .into_iter()
            .map(|route| {
                let queue = Arc::new((Mutex::new(Queue::default()), Condvar::new()));
                let channel = Channel {
                    kinds: route.kinds.clone(),
                    include_folder: route.include_folder,
                    queue: queue.clone(),
                    capacity: policy.queue.max(1),
                };
                let post = post.clone();
                std::thread::spawn(move || work(route, policy, &queue, &*post));
                channel
            })
            .collect();
        Self {
            enabled: Arc::new(AtomicBool::new(enabled)),
            channels,
        }
    }

    /// Queues `message` for every sink that wants its kind, unless forwarding is off. Never
    /// blocks on the network.
    pub fn submit(&self, kind: Kind, message: Message) {
        self.submit_with(kind, |_| message.clone());
    }

    /// `submit` for an accepted card: each sink gets the message its own `include_folder` allows.
    pub fn submit_event(&self, event: &Event) {
        self.submit_with(event.kind, |channel| {
            Message::from_event(event, channel.include_folder)
        });
    }

    fn submit_with(&self, kind: Kind, message: impl Fn(&Channel) -> Message) {
        if !self.enabled() {
            return;
        }
        for channel in self.channels.iter().filter(|c| c.kinds.contains(&kind)) {
            let (lock, wake) = &*channel.queue;
            let mut queue = lock.lock().unwrap_or_else(|e| e.into_inner());
            if queue.messages.len() >= channel.capacity {
                queue.messages.pop_front();
            }
            queue.messages.push_back(message(channel));
            wake.notify_one();
        }
    }

    pub fn set_enabled(&self, on: bool) {
        self.enabled.store(on, Ordering::SeqCst);
    }

    pub fn enabled(&self) -> bool {
        self.enabled.load(Ordering::SeqCst)
    }
}

/// The worker of one sink: takes messages in order, retries a failure, and reports a failing sink
/// once for each distinct error.
fn work(
    mut route: Route,
    policy: Policy,
    queue: &(Mutex<Queue>, Condvar),
    post: &(dyn Fn(&str) + Send + Sync),
) {
    let (lock, wake) = queue;
    let mut last_error = None;
    loop {
        let message = {
            let mut waiting = lock.lock().unwrap_or_else(|e| e.into_inner());
            loop {
                if waiting.closed {
                    return;
                }
                if let Some(message) = waiting.messages.pop_front() {
                    break message;
                }
                waiting = wake.wait(waiting).unwrap_or_else(|e| e.into_inner());
            }
        };
        let mut result = Ok(());
        let mut asked = None;
        for attempt in 0..policy.attempts.max(1) {
            if attempt > 0 {
                std::thread::sleep(retry_delay(policy.backoff, asked));
            }
            result = route.sink.send(&message);
            match &result {
                Ok(()) => break,
                Err(error) => asked = error.retry_after,
            }
        }
        let name = route.sink.name().to_owned();
        let result =
            result.map_err(|error| format!("{name}: {}", redact(&error.message, &route.secrets)));
        crate::notice::report_changed_with(post, &mut last_error, result);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::{Receiver, Sender, channel};

    const WAIT: Duration = Duration::from_secs(5);

    fn message(text: &str) -> Message {
        Message {
            kind: "turn_complete".into(),
            harness: "Claude".into(),
            folder: None,
            text: text.into(),
        }
    }

    /// Records what it is given; fails the first `failures` calls with `error`.
    struct Stub {
        seen: Sender<String>,
        failures: u32,
        error: String,
    }
    impl Sink for Stub {
        fn name(&self) -> &str {
            "stub"
        }
        fn send(&mut self, message: &Message) -> Result<(), SendError> {
            let _ = self.seen.send(message.text.clone());
            if self.failures > 0 {
                self.failures -= 1;
                return Err(self.error.clone().into());
            }
            Ok(())
        }
    }

    fn stub(failures: u32, error: &str) -> (Stub, Receiver<String>) {
        let (seen, received) = channel();
        let sink = Stub {
            seen,
            failures,
            error: error.into(),
        };
        (sink, received)
    }

    fn route(sink: Stub, kinds: &[Kind]) -> Route {
        Route {
            sink: Box::new(sink),
            kinds: kinds.to_vec(),
            secrets: Vec::new(),
            include_folder: false,
        }
    }

    fn fast(queue: usize, attempts: u32) -> Policy {
        Policy {
            queue,
            attempts,
            backoff: Duration::ZERO,
        }
    }

    fn quiet() -> impl Fn(&str) + Send + Sync + 'static {
        |_| {}
    }

    #[test]
    fn only_the_wanted_kinds_are_delivered() {
        let (sink, seen) = stub(0, "");
        let dispatcher = Dispatcher::start(
            vec![route(sink, &[Kind::PermissionRequired])],
            fast(8, 1),
            true,
            quiet(),
        );
        dispatcher.submit(Kind::TurnComplete, message("skipped"));
        dispatcher.submit(Kind::PermissionRequired, message("wanted"));
        assert_eq!(seen.recv_timeout(WAIT).unwrap(), "wanted");
        assert!(seen.try_recv().is_err());
    }

    #[test]
    fn a_disabled_dispatcher_delivers_nothing_until_enabled() {
        let (sink, seen) = stub(0, "");
        let dispatcher = Dispatcher::start(
            vec![route(sink, &[Kind::TurnComplete])],
            fast(8, 1),
            false,
            quiet(),
        );
        dispatcher.submit(Kind::TurnComplete, message("while off"));
        dispatcher.set_enabled(true);
        dispatcher.submit(Kind::TurnComplete, message("while on"));
        assert_eq!(seen.recv_timeout(WAIT).unwrap(), "while on");
        dispatcher.set_enabled(false);
        dispatcher.submit(Kind::TurnComplete, message("off again"));
        std::thread::sleep(Duration::from_millis(50));
        assert!(seen.try_recv().is_err());
        assert!(!dispatcher.enabled());
    }

    #[test]
    fn a_failed_send_is_retried_up_to_the_attempt_limit() {
        let (sink, seen) = stub(2, "down");
        let dispatcher =
            Dispatcher::start(vec![route(sink, &[Kind::Error])], fast(8, 3), true, quiet());
        dispatcher.submit(Kind::Error, message("m"));
        for _ in 0..3 {
            assert_eq!(seen.recv_timeout(WAIT).unwrap(), "m");
        }
        std::thread::sleep(Duration::from_millis(50));
        assert!(seen.try_recv().is_err(), "a delivered message was resent");
    }

    #[test]
    fn a_message_that_keeps_failing_is_given_up_and_the_next_is_sent() {
        let (sink, seen) = stub(2, "down");
        let dispatcher =
            Dispatcher::start(vec![route(sink, &[Kind::Error])], fast(8, 2), true, quiet());
        dispatcher.submit(Kind::Error, message("lost"));
        dispatcher.submit(Kind::Error, message("next"));
        let order: Vec<_> = (0..3).map(|_| seen.recv_timeout(WAIT).unwrap()).collect();
        assert_eq!(order, ["lost", "lost", "next"]);
    }

    /// Blocks its first call until released, so a test can fill the queue behind it.
    struct Gated {
        started: Sender<()>,
        release: Receiver<()>,
        seen: Sender<String>,
        first: bool,
    }
    impl Sink for Gated {
        fn name(&self) -> &str {
            "gated"
        }
        fn send(&mut self, message: &Message) -> Result<(), SendError> {
            if std::mem::take(&mut self.first) {
                let _ = self.started.send(());
                let _ = self.release.recv_timeout(WAIT);
            }
            let _ = self.seen.send(message.text.clone());
            Ok(())
        }
    }

    #[test]
    fn a_full_queue_drops_its_oldest_messages_and_submit_never_blocks() {
        let (started_tx, started) = channel();
        let (release_tx, release) = channel();
        let (seen_tx, seen) = channel();
        let sink = Gated {
            started: started_tx,
            release,
            seen: seen_tx,
            first: true,
        };
        let route = Route {
            sink: Box::new(sink),
            kinds: vec![Kind::TurnComplete],
            secrets: Vec::new(),
            include_folder: false,
        };
        let dispatcher = Dispatcher::start(vec![route], fast(2, 1), true, quiet());
        dispatcher.submit(Kind::TurnComplete, message("in flight"));
        started.recv_timeout(WAIT).unwrap();
        for n in 1..=4 {
            dispatcher.submit(Kind::TurnComplete, message(&format!("m{n}")));
        }
        release_tx.send(()).unwrap();
        let order: Vec<_> = (0..3).map(|_| seen.recv_timeout(WAIT).unwrap()).collect();
        assert_eq!(order, ["in flight", "m3", "m4"]);
    }

    #[test]
    fn a_repeated_error_posts_one_card_and_a_changed_one_posts_another() {
        let (sink, seen) = stub(3, "unreachable");
        let posted = Arc::new(Mutex::new(Vec::<String>::new()));
        let record = posted.clone();
        let dispatcher = Dispatcher::start(
            vec![route(sink, &[Kind::Error])],
            fast(8, 1),
            true,
            move |text| record.lock().unwrap().push(text.to_owned()),
        );
        for _ in 0..3 {
            dispatcher.submit(Kind::Error, message("m"));
            seen.recv_timeout(WAIT).unwrap();
        }
        // The worker reports after `send` returns; the next accepted message proves it did.
        dispatcher.submit(Kind::Error, message("recovered"));
        seen.recv_timeout(WAIT).unwrap();
        dispatcher.submit(Kind::Error, message("after"));
        seen.recv_timeout(WAIT).unwrap();
        let posted = posted.lock().unwrap();
        assert_eq!(posted.as_slice(), ["stub: unreachable"]);
    }

    #[test]
    fn an_error_is_posted_again_after_a_success_cleared_it() {
        let (seen_tx, seen) = channel();
        struct Flaky(Sender<String>, Vec<Result<(), String>>);
        impl Sink for Flaky {
            fn name(&self) -> &str {
                "flaky"
            }
            fn send(&mut self, message: &Message) -> Result<(), SendError> {
                let _ = self.0.send(message.text.clone());
                self.1.remove(0).map_err(SendError::from)
            }
        }
        let results = vec![Err("boom".into()), Ok(()), Err("boom".into()), Ok(())];
        let route = Route {
            sink: Box::new(Flaky(seen_tx, results)),
            kinds: vec![Kind::Error],
            secrets: Vec::new(),
            include_folder: false,
        };
        let posted = Arc::new(Mutex::new(Vec::<String>::new()));
        let record = posted.clone();
        let dispatcher = Dispatcher::start(vec![route], fast(8, 1), true, move |text| {
            record.lock().unwrap().push(text.to_owned())
        });
        for n in 0..4 {
            dispatcher.submit(Kind::Error, message(&format!("m{n}")));
            seen.recv_timeout(WAIT).unwrap();
        }
        // Wait until the last result has been reported.
        let deadline = std::time::Instant::now() + WAIT;
        while posted.lock().unwrap().len() < 2 && std::time::Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(
            posted.lock().unwrap().as_slice(),
            ["flaky: boom", "flaky: boom"]
        );
    }

    #[test]
    fn a_secret_never_reaches_the_posted_error() {
        let (sink, seen) = stub(1, "request to /bot123:SECRET/sendMessage failed");
        let route = Route {
            sink: Box::new(sink),
            kinds: vec![Kind::Error],
            secrets: vec!["123:SECRET".into()],
            include_folder: false,
        };
        let posted = Arc::new(Mutex::new(Vec::<String>::new()));
        let record = posted.clone();
        let dispatcher = Dispatcher::start(vec![route], fast(8, 1), true, move |text| {
            record.lock().unwrap().push(text.to_owned())
        });
        dispatcher.submit(Kind::Error, message("m"));
        seen.recv_timeout(WAIT).unwrap();
        let deadline = std::time::Instant::now() + WAIT;
        while posted.lock().unwrap().is_empty() && std::time::Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
        }
        let posted = posted.lock().unwrap();
        assert_eq!(
            posted.as_slice(),
            ["stub: request to /bot***/sendMessage failed"]
        );
    }

    #[test]
    fn each_sink_gets_the_folder_only_if_it_asked_for_it() {
        let (with, with_seen) = stub(0, "");
        let (without, without_seen) = stub(0, "");
        let routes = vec![
            Route {
                include_folder: true,
                ..route(with, &[Kind::TurnComplete])
            },
            route(without, &[Kind::TurnComplete]),
        ];
        let dispatcher = Dispatcher::start(routes, fast(8, 1), true, quiet());
        dispatcher.submit_event(&event(Kind::TurnComplete, Some("project")));
        assert!(with_seen.recv_timeout(WAIT).unwrap().contains("project"));
        assert!(!without_seen.recv_timeout(WAIT).unwrap().contains("project"));
    }

    #[test]
    fn redact_replaces_every_occurrence_and_ignores_empty_secrets() {
        let secrets = vec!["abc".to_owned(), String::new()];
        assert_eq!(redact("abc x abc", &secrets), "*** x ***");
        assert_eq!(redact("nothing here", &secrets), "nothing here");
    }

    fn event(kind: Kind, folder: Option<&str>) -> Event {
        let mut event = Event::notice("ignored");
        event.kind = kind;
        event.source = crate::events::Source::ClaudeHook;
        event.folder = folder.map(str::to_owned);
        event.session_id = Some("session-secret-name".into());
        event
    }

    #[test]
    fn a_message_names_the_kind_and_harness_and_leaves_the_folder_out_by_default() {
        let message = Message::from_event(&event(Kind::TurnComplete, Some("project")), false);
        assert_eq!(message.kind, "turn_complete");
        assert_eq!(message.harness, "Claude");
        assert_eq!(message.folder, None);
        assert!(!message.text.contains("project"), "{}", message.text);
        assert!(
            !message.text.contains("session-secret-name"),
            "{}",
            message.text
        );
    }

    #[test]
    fn the_folder_is_in_the_message_only_when_allowed() {
        let message = Message::from_event(&event(Kind::InputRequired, Some("project")), true);
        assert_eq!(message.folder.as_deref(), Some("project"));
        assert!(message.text.contains("project"), "{}", message.text);
        let none = Message::from_event(&event(Kind::InputRequired, None), true);
        assert_eq!(none.folder, None);
    }

    #[test]
    fn a_server_delay_replaces_the_fixed_backoff_up_to_a_cap() {
        let backoff = Duration::from_secs(2);
        assert_eq!(retry_delay(backoff, None), backoff);
        assert_eq!(
            retry_delay(backoff, Some(Duration::from_secs(1))),
            Duration::from_secs(1)
        );
        assert_eq!(retry_delay(backoff, Some(Duration::ZERO)), Duration::ZERO);
        assert_eq!(
            retry_delay(backoff, Some(Duration::from_secs(30))),
            MAX_RETRY_AFTER
        );
        assert_eq!(
            retry_delay(backoff, Some(Duration::from_secs(3600))),
            MAX_RETRY_AFTER
        );
    }

    /// Fails its first call with a server delay, then succeeds; records when each call came.
    struct Asked(Sender<std::time::Instant>, Option<Duration>);
    impl Sink for Asked {
        fn name(&self) -> &str {
            "asked"
        }
        fn send(&mut self, _: &Message) -> Result<(), SendError> {
            let _ = self.0.send(std::time::Instant::now());
            match self.1.take() {
                Some(delay) => Err(SendError {
                    message: "slow down".into(),
                    retry_after: Some(delay),
                }),
                None => Ok(()),
            }
        }
    }

    #[test]
    fn the_worker_waits_for_the_server_delay_instead_of_the_backoff() {
        let (seen_tx, seen) = channel();
        let route = Route {
            sink: Box::new(Asked(seen_tx, Some(Duration::from_millis(300)))),
            kinds: vec![Kind::Error],
            secrets: Vec::new(),
            include_folder: false,
        };
        // A backoff far longer than the test: only the server's delay can explain a quick retry.
        let policy = Policy {
            queue: 4,
            attempts: 2,
            backoff: Duration::from_secs(60),
        };
        let dispatcher = Dispatcher::start(vec![route], policy, true, quiet());
        dispatcher.submit(Kind::Error, message("m"));
        let first = seen.recv_timeout(WAIT).unwrap();
        let second = seen.recv_timeout(WAIT).unwrap();
        let waited = second.duration_since(first);
        assert!(waited >= Duration::from_millis(300), "{waited:?}");
        assert!(waited < Duration::from_secs(5), "{waited:?}");
    }
}
