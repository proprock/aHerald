mod support;
use aherald::{
    app_server::{Error, Transport},
    config::LogLevel,
    events::{Event, Source},
    operational_log,
    session_source::Service,
};
use std::{sync::mpsc, time::Duration};

struct Unused;
impl Transport for Unused {
    fn send(&mut self, _: &serde_json::Value, _: Duration) -> Result<(), Error> {
        unreachable!()
    }
    fn receive(&mut self, _: Duration) -> Result<serde_json::Value, Error> {
        unreachable!()
    }
}

#[test]
fn failed_optional_name_lookups_log_once_and_keep_the_harness_heading() {
    let root = support::TempDir::new("session-lookup-log");
    let log = root.join("aherald.log");
    let _ = std::fs::remove_file(&log);
    operational_log::init(&root, LogLevel::Warn);
    let (sent, calls) = mpsc::channel();
    let mut service = Service::transient::<Unused>(true, move |_| {
        sent.send(()).unwrap();
        Err(Error::Service("PRIVATE_ERROR"))
    });
    for id in ["one", "two", "three"] {
        let mut event = Event::demo("turn_complete").unwrap();
        event.source = Source::CodexHook;
        event.session_id = Some(id.into());
        let heading = service.label_at(&event, Duration::ZERO);
        assert_eq!(heading.title, "Codex");
        assert_eq!(heading.name, None);
        calls.recv_timeout(Duration::from_secs(2)).unwrap();
    }
    drop(service);
    let mut shared = aherald::usage_service::CodexSource::<Unused, _>::new(|| {
        Err(Error::Service("PRIVATE_SHARED_ERROR"))
    });
    for id in ["one", "two", "three"] {
        assert!(
            aherald::usage_service::UsageSource::thread_name(&mut shared, Duration::ZERO, id)
                .is_none()
        );
    }
    let text = std::fs::read_to_string(&log).unwrap_or_default();
    assert_eq!(text.matches("session.lookup_failed").count(), 2, "{text}");
    assert!(!text.contains("PRIVATE"));
    std::fs::remove_file(log).unwrap();
}
