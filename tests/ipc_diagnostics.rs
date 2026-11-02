#![cfg(windows)]
use aherald::{
    events::Event,
    ipc::{Request, Response, pipe},
    local::Scope,
};
use std::{
    path::PathBuf,
    process::{Child, Command},
    time::{Duration, Instant},
};

struct Resident(Child, PathBuf);
impl Drop for Resident {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
        let _ = std::fs::remove_dir_all(&self.1);
    }
}

#[test]
fn rejected_ipc_input_is_logged_once_until_valid_input_recovers() {
    let name = format!("diagnostics-{}", std::process::id());
    let scope = Scope::current(&name).unwrap();
    let root = std::env::temp_dir().join(&name);
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("config.toml");
    std::fs::write(&path, "[notifications]\nenabled=false\n[sounds]\nenabled=false\n[codex.usage]\nenabled=false\n[claude.usage]\nenabled=false\n").unwrap();
    let mut resident = Resident(
        Command::new(env!("CARGO_BIN_EXE_aherald"))
            .args(["--resident-child", &name, "--config"])
            .arg(path)
            .spawn()
            .unwrap(),
        root.clone(),
    );
    let request = |request| pipe::request(&scope, &request, Duration::from_secs(1)).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if pipe::request(&scope, &Request::Status, Duration::from_millis(100)).is_ok() {
            break;
        }
        assert!(resident.0.try_wait().unwrap().is_none());
        assert!(Instant::now() < deadline, "resident did not start");
        std::thread::yield_now();
    }
    for _ in 0..2 {
        for _ in 0..3 {
            let deadline = Instant::now() + Duration::from_secs(2);
            let mut connection = loop {
                if let Ok(file) = std::fs::OpenOptions::new()
                    .read(true)
                    .write(true)
                    .open(&scope.pipe_name)
                {
                    break file;
                }
                assert!(
                    Instant::now() < deadline,
                    "pipe did not accept malformed-input probe"
                );
                std::thread::yield_now();
            };
            use std::io::Write;
            connection.write_all(&[255; 6]).unwrap();
        }
        request(Request::Status);
    }
    for _ in 0..2 {
        for _ in 0..3 {
            let mut bad = Event::demo("turn_complete").unwrap();
            bad.window = Some(0);
            bad.message = Some("PRIVATE_PAYLOAD".into());
            assert!(matches!(
                request(Request::Event { event: bad }),
                Response::Rejected
            ));
            // Status polling does not end an invalid-input episode.
            request(Request::Status);
        }
        assert!(matches!(
            request(Request::Event {
                event: Event::demo("error").unwrap()
            }),
            Response::Accepted
        ));
    }
    // Fill all slots with persistent high-priority cards. Repeated queue failures form
    // one episode; accepted activity is recovery even when it produces no card.
    for id in 0..64 {
        let mut event = Event::demo("permission_required").unwrap();
        event.turn_id = Some(format!("wait-{id}"));
        assert!(matches!(
            request(Request::Event { event }),
            Response::Accepted
        ));
    }
    for _ in 0..2 {
        for _ in 0..3 {
            let mut refused = Event::demo("error").unwrap();
            refused.turn_id = Some("full".into());
            assert!(matches!(
                request(Request::Event { event: refused }),
                Response::Rejected
            ));
            request(Request::Status);
        }
        let mut activity = Event::demo("turn_complete").unwrap();
        activity.activity = true;
        assert!(matches!(
            request(Request::Event { event: activity }),
            Response::Accepted
        ));
    }
    request(Request::Quit);
    let deadline = Instant::now() + Duration::from_secs(5);
    while resident.0.try_wait().unwrap().is_none() {
        assert!(Instant::now() < deadline, "resident did not stop");
        std::thread::yield_now();
    }
    let text = std::fs::read_to_string(root.join("aherald.log")).unwrap_or_default();
    assert_eq!(text.matches("ipc.input_rejected").count(), 2, "{text}");
    assert_eq!(text.matches("ipc.failure").count(), 2, "{text}");
    assert_eq!(text.matches("ipc.queue_rejected").count(), 2, "{text}");
    assert!(!text.contains("PRIVATE"));
    assert!(!text.contains(&root.to_string_lossy().to_string()));
}
