//! The Telegram sink against a local fake Bot API: what it sends, what it hides, how it fails.
#![cfg(any(windows, feature = "https-client"))]
use aherald::{
    events::Kind,
    sink::{Dispatcher, Message, Policy, Route, Sink},
    telegram::Telegram,
};
use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

const TOKEN: &str = "123456:SECRET-TOKEN";

/// Serves each canned response to one request and records the raw requests.
fn fake_api(responses: Vec<&'static str>) -> (u16, Arc<Mutex<Vec<String>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let requests = Arc::new(Mutex::new(Vec::new()));
    let seen = requests.clone();
    std::thread::spawn(move || {
        for response in responses {
            let Ok((mut stream, _)) = listener.accept() else {
                return;
            };
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut request = Vec::new();
            let mut chunk = [0u8; 1024];
            loop {
                let read = stream.read(&mut chunk).unwrap_or(0);
                request.extend_from_slice(&chunk[..read]);
                let text = String::from_utf8_lossy(&request).into_owned();
                let done = text.split_once("\r\n\r\n").is_some_and(|(head, body)| {
                    let length = head
                        .to_lowercase()
                        .split("content-length:")
                        .nth(1)
                        .and_then(|rest| rest.lines().next()?.trim().parse::<usize>().ok())
                        .unwrap_or(0);
                    body.len() >= length
                });
                if read == 0 || done {
                    break;
                }
            }
            seen.lock()
                .unwrap()
                .push(String::from_utf8_lossy(&request).into_owned());
            let _ = stream.write_all(response.as_bytes());
        }
    });
    (port, requests)
}

fn message(folder: Option<&str>) -> Message {
    Message {
        kind: "permission_required".into(),
        harness: "Claude".into(),
        folder: folder.map(str::to_owned),
        text: "Claude: permission required".into(),
    }
}

const OK: &str = "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 11\r\nConnection: close\r\n\r\n{\"ok\":true}";

#[test]
fn the_request_carries_the_chat_and_the_text_and_nothing_else() {
    let (port, requests) = fake_api(vec![OK]);
    let mut telegram = Telegram::new(TOKEN.into(), "4242".into()).at(false, "127.0.0.1", port);
    telegram.send(&message(None)).unwrap();
    let request = requests.lock().unwrap()[0].clone();
    assert!(
        request.starts_with(&format!("POST /bot{TOKEN}/sendMessage HTTP/1.1\r\n")),
        "{request}"
    );
    let body: serde_json::Value =
        serde_json::from_str(request.split_once("\r\n\r\n").unwrap().1).unwrap();
    assert_eq!(
        body,
        serde_json::json!({"chat_id": "4242", "text": "Claude: permission required"})
    );
}

#[test]
fn a_refusal_is_an_error_that_names_the_status_but_not_the_token() {
    let (port, _) = fake_api(vec![
        "HTTP/1.1 401 Unauthorized\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
    ]);
    let mut telegram = Telegram::new(TOKEN.into(), "4242".into()).at(false, "127.0.0.1", port);
    let error = telegram.send(&message(None)).unwrap_err();
    assert!(error.contains("401"), "{error}");
    assert!(!error.contains("SECRET"), "{error}");
}

#[test]
fn an_unreachable_api_is_an_error_without_the_token() {
    let port = {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.local_addr().unwrap().port()
    };
    let mut telegram = Telegram::new(TOKEN.into(), "4242".into()).at(false, "127.0.0.1", port);
    let error = telegram.send(&message(None)).unwrap_err();
    assert!(!error.contains("SECRET"), "{error}");
}

#[test]
fn a_hung_api_never_delays_submitting_cards() {
    // Accepts the connection and never answers.
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let telegram = Telegram::new(TOKEN.into(), "4242".into()).at(false, "127.0.0.1", port);
    let route = Route {
        sink: Box::new(telegram),
        kinds: vec![Kind::PermissionRequired],
        secrets: vec![TOKEN.into()],
        include_folder: false,
    };
    let dispatcher = Dispatcher::start(
        vec![route],
        Policy {
            queue: 4,
            attempts: 1,
            backoff: Duration::ZERO,
        },
        true,
        |_| {},
    );
    let started = Instant::now();
    for _ in 0..20 {
        dispatcher.submit(Kind::PermissionRequired, message(None));
    }
    assert!(started.elapsed() < Duration::from_millis(500));
    drop(listener);
}

#[test]
fn a_failure_through_the_dispatcher_posts_one_card_without_the_token() {
    let (port, _) = fake_api(vec![
        "HTTP/1.1 500 Internal Server Error\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        "HTTP/1.1 500 Internal Server Error\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
    ]);
    let telegram = Telegram::new(TOKEN.into(), "4242".into()).at(false, "127.0.0.1", port);
    let route = Route {
        sink: Box::new(telegram),
        kinds: vec![Kind::PermissionRequired],
        secrets: vec![TOKEN.into()],
        include_folder: false,
    };
    let posted = Arc::new(Mutex::new(Vec::<String>::new()));
    let record = posted.clone();
    let dispatcher = Dispatcher::start(
        vec![route],
        Policy {
            queue: 4,
            attempts: 1,
            backoff: Duration::ZERO,
        },
        true,
        move |text| record.lock().unwrap().push(text.to_owned()),
    );
    dispatcher.submit(Kind::PermissionRequired, message(None));
    dispatcher.submit(Kind::PermissionRequired, message(None));
    let deadline = Instant::now() + Duration::from_secs(5);
    while posted.lock().unwrap().is_empty() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    std::thread::sleep(Duration::from_millis(200));
    let posted = posted.lock().unwrap();
    assert_eq!(posted.len(), 1, "{posted:?}");
    assert!(posted[0].contains("500"), "{posted:?}");
    assert!(!posted[0].contains("SECRET"), "{posted:?}");
}
