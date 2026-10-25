use aherald::{
    events::{Event, Source},
    session_source::Service,
};
use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

fn event(id: &str) -> Event {
    let mut event = Event::demo("turn_complete").unwrap();
    event.source = Source::CodexHook;
    event.session_id = Some(id.into());
    event
}

#[test]
fn labels_are_immediate_isolated_cached_and_refreshed() {
    let calls = Arc::new(AtomicUsize::new(0));
    let count = calls.clone();
    let (release, gate) = mpsc::channel();
    let mut labels = Service::new(move |id| {
        count.fetch_add(1, Ordering::Relaxed);
        gate.recv_timeout(Duration::from_secs(2)).ok()?;
        Some(
            if id.ends_with("12345678") {
                "First\nchat\u{202e}"
            } else {
                "Second chat"
            }
            .into(),
        )
    });
    let first = event("019a0000-0000-0000-0000-000012345678");
    let second = event("019a0000-0000-0000-0000-000087654321");
    assert_eq!(
        labels.label_at(&first, Duration::ZERO),
        "Codex: session 12345678"
    );
    assert_eq!(
        labels.label_at(&second, Duration::ZERO),
        "Codex: session 87654321"
    );
    release.send(()).unwrap();
    release.send(()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    while labels.label_at(&second, Duration::ZERO) != "Codex: Second chat" {
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
    assert_eq!(
        labels.label_at(&first, Duration::from_secs(59)),
        "Codex: First chat"
    );
    assert_eq!(calls.load(Ordering::Relaxed), 2);
    assert_eq!(
        labels.label_at(&first, Duration::from_secs(61)),
        "Codex: First chat"
    );
    release.send(()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    while calls.load(Ordering::Relaxed) < 3 {
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
    // Release any queued refresh before joining the worker.
    drop(release);
}

#[test]
fn unavailable_titles_and_demo_keep_useful_fallbacks() {
    let mut labels = Service::new(|_| None);
    assert_eq!(
        labels.label_at(&event("short-id"), Duration::ZERO),
        "Codex: session short-id"
    );
    let mut anonymous = event("unused");
    anonymous.session_id = None;
    assert_eq!(labels.label_at(&anonymous, Duration::ZERO), "Codex");
    assert_eq!(
        labels.label_at(&Event::demo("error").unwrap(), Duration::ZERO),
        "aHerald demo"
    );
}

#[cfg(windows)]
#[test]
#[ignore = "reads only the explicitly selected local Codex thread metadata"]
fn installed_codex_reads_name_without_resuming_thread() {
    use aherald::app_server::{Client, StdioTransport};
    let id = std::env::var("AHERALD_SESSION_ID").unwrap();
    let mut client =
        Client::initialize(StdioTransport::codex().unwrap(), Duration::from_secs(5)).unwrap();
    let name = client.read_thread_name(&id).unwrap();
    // The installed app's automatic sidebar caption is not necessarily thread.name.
    // This thread was explicitly verified to have no stored user-facing name.
    assert_eq!(name, std::env::var("AHERALD_EXPECTED_SESSION_NAME").ok());
}
