//! A transient process shows one card, so it looks up the Codex session name (one
//! `codex app-server` child per card) only when `[codex] transient_names` asks for it.
use aherald::{
    app_server::{Error, Transport},
    config::Config,
    events::{Event, Source},
    session_source,
};
use serde_json::{Value, json};
use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

struct Server(Option<Value>);
impl Transport for Server {
    fn send(&mut self, value: &Value, _: Duration) -> Result<(), Error> {
        if let Some(id) = value.get("id") {
            self.0 = Some(match value["method"].as_str() {
                Some("thread/read") => {
                    json!({"id":id,"result":{"thread":{"id":"named","name":"My task"}}})
                }
                _ => json!({"id":id,"result":{}}),
            });
        }
        Ok(())
    }
    fn receive(&mut self, _: Duration) -> Result<Value, Error> {
        self.0.take().ok_or(Error::Timeout)
    }
}

fn labels(enabled: bool) -> (session_source::Service, Arc<AtomicUsize>) {
    let starts = Arc::new(AtomicUsize::new(0));
    let counter = starts.clone();
    let service = session_source::Service::transient(enabled, move |_cancel| {
        counter.fetch_add(1, Ordering::SeqCst);
        Ok(Server(None))
    });
    (service, starts)
}

fn codex_event() -> Event {
    let mut event = Event::demo("turn_complete").unwrap();
    event.source = Source::CodexHook;
    event.session_id = Some("named".into());
    event
}

#[test]
fn transient_names_are_off_by_default() {
    assert!(!Config::default().codex.session_names);
    let default: Config = toml::from_str("[codex]\n").unwrap();
    assert!(!default.codex.session_names);
    assert!(!Config::default().codex.transient_names);
    let config: Config = toml::from_str("[codex]\ntransient_names = true").unwrap();
    assert!(config.codex.transient_names);
}

#[test]
fn resident_names_follow_the_session_names_switch() {
    for enabled in [false, true] {
        let mut config = Config::default();
        config.codex.session_names = enabled;
        let starts = Arc::new(AtomicUsize::new(0));
        let counter = starts.clone();
        let mut service = session_source::Service::for_mode_with(
            &config,
            aherald::config::Mode::Resident,
            None,
            move |_| {
                counter.fetch_add(1, Ordering::SeqCst);
                Ok(Server(None))
            },
        );
        let event = codex_event();
        let deadline = Instant::now() + Duration::from_millis(if enabled { 3000 } else { 150 });
        loop {
            let heading = service.label(&event);
            assert_eq!(heading.title, "Codex");
            if enabled && heading.name.as_deref() == Some("My task") {
                break;
            }
            if !enabled {
                assert_eq!(heading.name, None);
            }
            if Instant::now() >= deadline {
                assert!(!enabled, "name never resolved");
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(starts.load(Ordering::SeqCst), usize::from(enabled));
    }
}

#[test]
fn transient_requires_both_name_switches() {
    for session_names in [false, true] {
        for transient_names in [false, true] {
            let mut config = Config::default();
            config.codex.session_names = session_names;
            config.codex.transient_names = transient_names;
            let starts = Arc::new(AtomicUsize::new(0));
            let counter = starts.clone();
            let mut service = session_source::Service::for_mode_with(
                &config,
                aherald::config::Mode::Transient,
                None,
                move |_| {
                    counter.fetch_add(1, Ordering::SeqCst);
                    Ok(Server(None))
                },
            );
            let enabled = session_names && transient_names;
            let deadline = Instant::now() + Duration::from_millis(if enabled { 3000 } else { 100 });
            loop {
                let heading = service.label(&codex_event());
                if enabled && heading.name.as_deref() == Some("My task") {
                    break;
                }
                if !enabled {
                    assert_eq!(heading.name, None);
                }
                if Instant::now() >= deadline {
                    assert!(!enabled, "name never resolved");
                    break;
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            assert_eq!(starts.load(Ordering::SeqCst), usize::from(enabled));
        }
    }
}

#[test]
fn a_transient_card_starts_no_codex_child_unless_names_are_enabled() {
    let (mut service, starts) = labels(false);
    let event = codex_event();
    let until = Instant::now() + Duration::from_millis(300);
    while Instant::now() < until {
        assert_eq!(service.label(&event).name, None);
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(starts.load(Ordering::SeqCst), 0, "a child was started");
}

#[test]
fn an_enabled_transient_card_resolves_the_name_with_one_child() {
    let (mut service, starts) = labels(true);
    let event = codex_event();
    let deadline = Instant::now() + Duration::from_secs(3);
    while service.label(&event).name.as_deref() != Some("My task") {
        assert!(Instant::now() < deadline, "name never resolved");
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(starts.load(Ordering::SeqCst), 1);
}
