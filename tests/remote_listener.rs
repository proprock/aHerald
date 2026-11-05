//! The `[remote]` listener of a real resident: a guest's POST reaches the queue, and nothing
//! listens unless the configuration asks for it.
#![cfg(unix)]
mod support;
use std::{
    io::{Read, Write},
    net::TcpStream,
    path::PathBuf,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

const STOP: &str = r#"{"hook_event_name":"Stop","session_id":"guest-1","cwd":"/work/project"}"#;

struct Resident {
    root: PathBuf,
    config: PathBuf,
    child: Child,
}

impl Resident {
    fn start(remote: Option<u16>) -> Self {
        let root = std::env::temp_dir().join(format!(
            "aherald-remote-{}-{}",
            std::process::id(),
            remote.unwrap_or(0)
        ));
        std::fs::create_dir_all(root.join("run")).unwrap();
        let mut text = String::from("[notifications]\nenabled=false\n[sounds]\nenabled=false\n");
        if let Some(port) = remote {
            text += &format!("[remote]\nenabled = true\nbind = \"127.0.0.1:{port}\"\n");
        }
        let config = root.join("config.toml");
        std::fs::write(&config, text).unwrap();
        let child = Command::new(env!("CARGO_BIN_EXE_aherald"))
            .args(["--resident-child", "resident", "--config"])
            .arg(&config)
            .env("XDG_RUNTIME_DIR", root.join("run"))
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let resident = Self {
            root,
            config,
            child,
        };
        let deadline = Instant::now() + Duration::from_secs(10);
        while !resident.status().contains("queued") {
            assert!(Instant::now() < deadline, "the resident did not start");
            std::thread::sleep(Duration::from_millis(50));
        }
        resident
    }

    fn aherald(&self, args: &[&str]) -> String {
        let output = Command::new(env!("CARGO_BIN_EXE_aherald"))
            .args(args)
            .arg("--config")
            .arg(&self.config)
            .env("XDG_RUNTIME_DIR", self.root.join("run"))
            .stdin(Stdio::null())
            .output()
            .unwrap();
        String::from_utf8_lossy(&output.stdout).into_owned()
    }

    fn status(&self) -> String {
        self.aherald(&["--status"])
    }

    fn queued(&self) -> u64 {
        let status: serde_json::Value = serde_json::from_str(self.status().trim()).unwrap();
        // The response is externally tagged: {"Status":{"queued":N,...}} or flat; take either.
        status
            .pointer("/Status/queued")
            .or_else(|| status.get("queued"))
            .and_then(serde_json::Value::as_u64)
            .unwrap()
    }

    fn token(&self) -> String {
        std::fs::read_to_string(self.root.join("remote-token")).unwrap()
    }
}

impl Drop for Resident {
    fn drop(&mut self) {
        let _ = self.aherald(&["--quit"]);
        let deadline = Instant::now() + Duration::from_secs(3);
        while self.child.try_wait().ok().flatten().is_none() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

fn connect(port: u16) -> Option<TcpStream> {
    TcpStream::connect(("127.0.0.1", port)).ok()
}

fn post(port: u16, token: &str, agent: &str, body: &str) -> (u16, String) {
    let mut stream = connect(port).expect("the listener is up");
    write!(
        stream,
        "POST /v1/hook?agent={agent} HTTP/1.1\r\nHost: x\r\nAuthorization: Bearer {token}\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    )
    .unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let mut text = String::new();
    let _ = stream.read_to_string(&mut text);
    let status = text
        .split(' ')
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    (
        status,
        text.split_once("\r\n\r\n").map_or("", |(_, b)| b).into(),
    )
}

fn wait_for_listener(port: u16) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while connect(port).is_none() {
        assert!(
            Instant::now() < deadline,
            "nothing listens on the remote port"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[test]
fn nothing_listens_unless_the_configuration_enables_it() {
    let port = free_port();
    let _resident = Resident::start(None);
    std::thread::sleep(Duration::from_millis(300));
    assert!(connect(port).is_none());
}

#[test]
fn a_forwarded_stop_reaches_the_resident_queue_and_a_wrong_token_does_not() {
    let port = free_port();
    let resident = Resident::start(Some(port));
    wait_for_listener(port);
    let token = resident.token();
    assert_eq!(token.trim().len(), 64);
    assert_eq!(resident.queued(), 0);
    let wrong = token.trim().replace(|c: char| c.is_ascii_hexdigit(), "0");
    assert_eq!(post(port, &wrong, "claude", STOP).0, 401);
    assert_eq!(resident.queued(), 0);
    assert_eq!(post(port, token.trim(), "claude", STOP).0, 200);
    assert_eq!(resident.queued(), 1);
    // The same session's second stop within the dedupe window is one card.
    assert_eq!(post(port, token.trim(), "claude", STOP).0, 200);
    assert_eq!(resident.queued(), 1);
    // A different agent's session of the same id is independent.
    let codex = r#"{"hook_event_name":"Stop","session_id":"guest-1","turn_id":"t","cwd":"/w"}"#;
    let (status, body) = post(port, token.trim(), "codex", codex);
    assert_eq!((status, body.as_str()), (200, "{}"));
    assert_eq!(resident.queued(), 2);
}

#[test]
fn remote_info_prints_the_endpoint_and_the_token_the_resident_uses() {
    let port = free_port();
    let resident = Resident::start(Some(port));
    wait_for_listener(port);
    let info = resident.aherald(&["--remote-info"]);
    assert!(
        info.contains(&format!("AHERALD_ENDPOINT=http://127.0.0.1:{port}\n")),
        "{info}"
    );
    assert!(
        info.contains(&format!("AHERALD_TOKEN={}\n", resident.token().trim())),
        "{info}"
    );
}

#[test]
fn the_listener_goes_away_with_the_resident() {
    let port = free_port();
    let resident = Resident::start(Some(port));
    wait_for_listener(port);
    drop(resident);
    assert!(connect(port).is_none());
}
