//! The webhook sink from configuration to the wire, against a local fake server: the
//! `${NAME}` secrets, the body template and how a failure is reported. Telegram is expressed as a
//! plain webhook here, which is the documented example.
#![cfg(any(windows, feature = "https-client"))]
use aherald::{
    config::Config,
    events::{Event, Kind, Source},
    forward::build,
    sink::{Dispatcher, Policy},
};
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
mod support;
use support::fake_http::fake_api;

const OK: &str = "HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok";
const FAILED: &str =
    "HTTP/1.1 500 Internal Server Error\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";

fn event(folder: Option<&str>) -> Event {
    let mut event = Event::notice("ignored");
    event.kind = Kind::InputRequired;
    event.source = Source::ClaudeHook;
    event.folder = folder.map(str::to_owned);
    event
}

fn dispatcher(
    config: &str,
    vars: &'static [(&'static str, &'static str)],
) -> (Dispatcher, Arc<Mutex<Vec<String>>>) {
    let forward = Config::parse(config, &PathBuf::from(".")).unwrap().forward;
    let built = build(
        &forward,
        |name| {
            vars.iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| (*value).to_owned())
        },
        |_: &Path| Err(std::io::ErrorKind::NotFound.into()),
    );
    assert!(built.notices.is_empty(), "{:?}", built.notices);
    let posted = Arc::new(Mutex::new(Vec::new()));
    let record = posted.clone();
    let dispatcher = Dispatcher::start(
        built.routes,
        Policy {
            queue: 4,
            attempts: 1,
            backoff: Duration::ZERO,
        },
        true,
        move |text| record.lock().unwrap().push(text.to_owned()),
    );
    (dispatcher, posted)
}

fn wait_for<T>(mut ready: impl FnMut() -> Option<T>) -> T {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(value) = ready() {
            return value;
        }
        assert!(Instant::now() < deadline, "timed out");
        std::thread::sleep(Duration::from_millis(10));
    }
}

const VARS: &[(&str, &str)] = &[("TOKEN", "123456:SECRET-TOKEN")];

#[test]
fn telegram_as_a_webhook_sends_the_token_in_the_path_and_a_json_body() {
    let (port, requests) = fake_api(vec![OK]);
    let config = format!(
        "[[forward.sinks]]\ntype = \"webhook\"\nkinds = [\"input_required\"]\n\
         url = \"http://127.0.0.1:{port}/bot${{TOKEN}}/sendMessage\"\n\
         content_type = \"application/json\"\n\
         body = '{{\"chat_id\": \"42\", \"text\": \"{{text}}\"}}'\n"
    );
    let (dispatcher, _) = dispatcher(&config, VARS);
    dispatcher.submit_event(&event(Some("project")));
    let request = wait_for(|| requests.lock().unwrap().first().cloned());
    assert!(
        request.starts_with("POST /bot123456:SECRET-TOKEN/sendMessage HTTP/1.1\r\n"),
        "{request}"
    );
    assert!(
        request
            .to_lowercase()
            .contains("content-type: application/json\r\n")
    );
    let body: serde_json::Value =
        serde_json::from_str(request.split_once("\r\n\r\n").unwrap().1).unwrap();
    // The folder is off by default, so the text carries the kind and the harness only.
    assert_eq!(
        body,
        serde_json::json!({"chat_id": "42", "text": "Claude: input required"})
    );
}

#[test]
fn the_default_body_is_the_plain_message_text_and_the_folder_is_opt_in() {
    let (port, requests) = fake_api(vec![OK]);
    let config = format!(
        "[[forward.sinks]]\ntype = \"webhook\"\nkinds = [\"input_required\"]\n\
         include_folder = true\nurl = \"http://127.0.0.1:{port}/topic\"\n"
    );
    let (dispatcher, _) = dispatcher(&config, &[]);
    dispatcher.submit_event(&event(Some("project")));
    let request = wait_for(|| requests.lock().unwrap().first().cloned());
    assert!(request.starts_with("POST /topic HTTP/1.1\r\n"), "{request}");
    assert!(request.to_lowercase().contains("content-type: text/plain"));
    assert_eq!(
        request.split_once("\r\n\r\n").unwrap().1,
        "Claude (project): input required"
    );
}

#[test]
fn a_failure_is_posted_once_and_never_contains_the_substituted_secret() {
    let (port, _) = fake_api(vec![FAILED, FAILED]);
    let config = format!(
        "[[forward.sinks]]\ntype = \"webhook\"\nkinds = [\"input_required\"]\n\
         url = \"http://127.0.0.1:{port}/bot${{TOKEN}}/sendMessage\"\n"
    );
    let (dispatcher, posted) = dispatcher(&config, VARS);
    dispatcher.submit_event(&event(None));
    dispatcher.submit_event(&event(None));
    wait_for(|| (!posted.lock().unwrap().is_empty()).then_some(()));
    std::thread::sleep(Duration::from_millis(200));
    let posted = posted.lock().unwrap();
    assert_eq!(posted.len(), 1, "{posted:?}");
    assert!(posted[0].contains("500"), "{posted:?}");
    assert!(!posted[0].contains("SECRET"), "{posted:?}");
}

#[test]
fn a_missing_variable_leaves_the_sink_out_with_a_notice_naming_it() {
    let forward = Config::parse(
        "[[forward.sinks]]\ntype = \"webhook\"\nurl = \"https://h.example/${NOT_SET}\"\n",
        &PathBuf::from("."),
    )
    .unwrap()
    .forward;
    let built = build(
        &forward,
        |_| None,
        |_: &Path| Err(std::io::ErrorKind::NotFound.into()),
    );
    assert!(built.routes.is_empty());
    assert!(built.notices[0].contains("NOT_SET"), "{:?}", built.notices);
}
