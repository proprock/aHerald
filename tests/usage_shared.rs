//! Session names are read over the usage worker's own app-server connection, so the
//! resident runs one `codex app-server` child instead of two.
use aherald::{
    app_server::{Error, Transport},
    usage_service::Service,
};
use serde_json::{Value, json};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

/// A fake app-server: answers every request it understands, in order.
struct Server {
    response: Option<Value>,
    broken: bool,
}
impl Transport for Server {
    fn send(&mut self, value: &Value, _: Duration) -> Result<(), Error> {
        let Some(id) = value.get("id") else {
            return Ok(());
        };
        self.response = Some(match value["method"].as_str() {
            Some("thread/read") => match value["params"]["threadId"].as_str() {
                Some("named") => {
                    json!({"id":id,"result":{"thread":{"id":"named","name":"My task"}}})
                }
                Some("unknown") => json!({"id":id,"error":{"code":-32600,"message":"no thread"}}),
                Some("boom") => {
                    self.broken = true;
                    return Ok(());
                }
                Some(other) => json!({"id":id,"result":{"thread":{"id":other}}}),
                None => json!({"id":id,"error":{"code":-32602,"message":"bad params"}}),
            },
            Some("account/rateLimits/read") => {
                json!({"id":id,"result":{"rateLimits":{"primary":{"usedPercent":3}}}})
            }
            _ => json!({"id":id,"result":{}}),
        });
        Ok(())
    }
    fn receive(&mut self, _: Duration) -> Result<Value, Error> {
        if self.broken {
            return Err(Error::Disconnected);
        }
        self.response.take().ok_or(Error::Timeout)
    }
}

fn service(enabled: bool) -> (Service, Arc<AtomicUsize>) {
    let starts = Arc::new(AtomicUsize::new(0));
    let counter = starts.clone();
    let service = Service::spawn(
        enabled,
        Duration::from_secs(300),
        Arc::new(AtomicBool::new(false)),
        move || {
            counter.fetch_add(1, Ordering::SeqCst);
            Ok(Server {
                response: None,
                broken: false,
            })
        },
    );
    (service, starts)
}

fn wait_for_usage(service: &Service) {
    let deadline = Instant::now() + Duration::from_secs(3);
    while service.view().snapshot.is_none() {
        assert!(Instant::now() < deadline, "usage never loaded");
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn names_and_usage_share_one_connection() {
    let (service, starts) = service(true);
    let cancel = AtomicBool::new(false);
    let names = service.names();
    assert_eq!(names.name("named", &cancel).as_deref(), Some("My task"));
    wait_for_usage(&service);
    assert_eq!(names.name("named", &cancel).as_deref(), Some("My task"));
    assert_eq!(starts.load(Ordering::SeqCst), 1, "one child connection");
}

#[test]
fn an_unknown_thread_is_unnamed_and_keeps_the_connection() {
    let (service, starts) = service(true);
    let cancel = AtomicBool::new(false);
    let names = service.names();
    assert_eq!(names.name("unknown", &cancel), None);
    assert_eq!(names.name("unnamed", &cancel), None);
    assert_eq!(names.name("named", &cancel).as_deref(), Some("My task"));
    wait_for_usage(&service);
    assert_eq!(starts.load(Ordering::SeqCst), 1);
}

#[test]
fn a_broken_connection_yields_no_name_and_does_not_block() {
    let (service, _starts) = service(true);
    let cancel = AtomicBool::new(false);
    let begun = Instant::now();
    assert_eq!(service.names().name("boom", &cancel), None);
    assert!(begun.elapsed() < Duration::from_secs(5));
}

#[test]
fn a_disabled_service_answers_at_once_without_launching_a_child() {
    let (service, starts) = service(false);
    let cancel = AtomicBool::new(false);
    let begun = Instant::now();
    assert_eq!(service.names().name("named", &cancel), None);
    assert!(begun.elapsed() < Duration::from_millis(500));
    assert_eq!(starts.load(Ordering::SeqCst), 0);
}

#[test]
fn a_cancelled_lookup_returns_promptly() {
    let (service, _starts) = service(true);
    let cancel = AtomicBool::new(true);
    let begun = Instant::now();
    let _ = service.names().name("named", &cancel);
    assert!(begun.elapsed() < Duration::from_secs(1));
}

#[test]
fn session_labels_resolve_through_the_usage_connection() {
    use aherald::{
        events::{Event, Source},
        session_source,
    };
    let (usage, starts) = service(true);
    let mut labels = session_source::Service::shared(usage.names());
    let mut event = Event::demo("turn_complete").unwrap();
    event.source = Source::CodexHook;
    event.session_id = Some("named".into());
    // The first label has no name yet; it arrives from the shared child.
    assert_eq!(labels.label(&event).name, None);
    let deadline = Instant::now() + Duration::from_secs(3);
    while labels.label(&event).name.as_deref() != Some("My task") {
        assert!(Instant::now() < deadline, "name never resolved");
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(starts.load(Ordering::SeqCst), 1);
}
