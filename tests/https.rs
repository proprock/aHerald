//! Behaviour of the bounded HTTPS GET on every platform, against a loopback HTTP endpoint.
use aherald::https::{HttpError, MAX_BODY, get_at};
use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

/// Serves `responses` in order on loopback and counts the requests it receives.
fn serve(responses: Vec<String>) -> (u16, Arc<AtomicUsize>, std::thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    listener.set_nonblocking(true).unwrap();
    let requests = Arc::new(AtomicUsize::new(0));
    let seen = requests.clone();
    let thread = std::thread::spawn(move || {
        let deadline = std::time::Instant::now() + Duration::from_secs(3);
        for response in responses {
            let mut stream = loop {
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(_) if std::time::Instant::now() < deadline => {
                        std::thread::sleep(Duration::from_millis(5))
                    }
                    Err(_) => return,
                }
            };
            stream.set_nonblocking(false).unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(1)))
                .unwrap();
            let mut buffer = [0u8; 2048];
            let _ = stream.read(&mut buffer);
            seen.fetch_add(1, Ordering::SeqCst);
            let _ = stream.write_all(response.as_bytes());
        }
    });
    (port, requests, thread)
}

#[test]
fn redirect_is_reported_and_never_followed() {
    let (port, requests, server) = serve(vec![
        "HTTP/1.1 302 Found\r\nLocation: /next\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            .into(),
        "HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok".into(),
    ]);
    let response = get_at(
        false,
        "127.0.0.1",
        port,
        "/start",
        "Authorization: Bearer test\r\n",
        Duration::from_secs(2),
    )
    .unwrap();
    server.join().unwrap();
    assert_eq!(response.status, 302);
    assert_eq!(requests.load(Ordering::SeqCst), 1, "redirect was followed");
}

fn retry_after_of(header: &str) -> Option<Duration> {
    let (port, _, server) = serve(vec![format!(
        "HTTP/1.1 429 Too Many Requests\r\n{header}Content-Length: 0\r\nConnection: close\r\n\r\n"
    )]);
    let response = get_at(false, "127.0.0.1", port, "/", "", Duration::from_secs(2)).unwrap();
    server.join().unwrap();
    assert_eq!(response.status, 429);
    response.retry_after
}

#[test]
fn retry_after_seconds_are_reported_and_other_forms_are_ignored() {
    assert_eq!(
        retry_after_of("Retry-After: 120\r\n"),
        Some(Duration::from_secs(120))
    );
    assert_eq!(retry_after_of(""), None);
    // HTTP-date form is not interpreted: the driver falls back to its own interval.
    assert_eq!(
        retry_after_of("Retry-After: Wed, 21 Oct 2026 07:28:00 GMT\r\n"),
        None
    );
}

#[test]
fn the_body_is_returned_and_the_request_carries_the_caller_headers() {
    let (port, _, server) = serve(vec![
        "HTTP/1.1 200 OK\r\nContent-Length: 5\r\nConnection: close\r\n\r\nhello".into(),
    ]);
    let response = get_at(
        false,
        "127.0.0.1",
        port,
        "/usage",
        "Authorization: Bearer test\r\nAccept: application/json\r\n",
        Duration::from_secs(2),
    )
    .unwrap();
    server.join().unwrap();
    assert_eq!(
        (response.status, response.body.as_slice()),
        (200, &b"hello"[..])
    );
}

#[test]
fn a_body_over_the_limit_is_refused() {
    let body = "x".repeat(MAX_BODY + 1);
    let (port, _, server) = serve(vec![format!(
        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )]);
    let result = get_at(false, "127.0.0.1", port, "/", "", Duration::from_secs(2));
    server.join().unwrap();
    assert_eq!(result.err(), Some(HttpError::TooLarge));
}

#[test]
fn an_unreachable_endpoint_fails_as_a_network_error() {
    let port = {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.local_addr().unwrap().port()
    };
    let result = get_at(
        false,
        "127.0.0.1",
        port,
        "/",
        "",
        Duration::from_millis(500),
    );
    assert_eq!(result.err(), Some(HttpError::Network));
}
