//! A loopback HTTP server that answers canned responses and records the raw requests.
use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::{Arc, Mutex},
    time::Duration,
};

/// Serves each canned response to one request, in order, and records every raw request
/// (head and body). Returns the port and the recorded requests.
pub fn fake_api(responses: Vec<&'static str>) -> (u16, Arc<Mutex<Vec<String>>>) {
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
