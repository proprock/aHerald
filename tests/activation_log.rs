use aherald::{
    activation::Worker,
    config::LogLevel,
    events::{Event, Source},
};
use std::{
    sync::{
        Mutex,
        mpsc::{self, Receiver, Sender},
    },
    time::Duration,
};

static COMPLETED: Mutex<Option<Sender<()>>> = Mutex::new(None);
static RELEASE: Mutex<Option<Receiver<()>>> = Mutex::new(None);
fn activate(event: &Event) -> Result<(), String> {
    COMPLETED
        .lock()
        .unwrap()
        .as_ref()
        .unwrap()
        .send(())
        .unwrap();
    if event.turn_id.as_deref() == Some("blocked") {
        RELEASE
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .recv_timeout(Duration::from_secs(5))
            .unwrap();
        return Ok(());
    }
    if event.turn_id.as_deref() == Some("recovered") {
        Ok(())
    } else {
        Err("PRIVATE_PATH_OR_SESSION".into())
    }
}

#[test]
fn missing_agent_is_logged_once_until_activation_recovers() {
    let root = std::env::temp_dir().join(format!("aherald-activation-log-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let log = root.join("aherald.log");
    let _ = std::fs::remove_file(&log);
    let (tx, rx) = mpsc::channel();
    *COMPLETED.lock().unwrap() = Some(tx);
    let mut worker = Worker::with(activate);
    for turn in ["failed", "failed", "recovered", "failed"] {
        let mut event = Event::demo("turn_complete").unwrap();
        event.source = Source::CodexHook;
        event.turn_id = Some(turn.into());
        worker.submit(&event, true, true, &root, LogLevel::Warn);
        rx.recv_timeout(Duration::from_secs(2)).unwrap();
    }
    drop(worker);
    // A blocked activation and one queued click exhaust the bounded mailbox.
    let (release, resume) = mpsc::channel();
    *RELEASE.lock().unwrap() = Some(resume);
    let mut worker = Worker::with(activate);
    let mut blocked = Event::demo("turn_complete").unwrap();
    blocked.source = Source::CodexHook;
    blocked.turn_id = Some("blocked".into());
    worker.submit(&blocked, true, true, &root, LogLevel::Warn);
    rx.recv_timeout(Duration::from_secs(2)).unwrap();
    worker.submit(&blocked, true, true, &root, LogLevel::Warn);
    worker.submit(&blocked, true, true, &root, LogLevel::Warn);
    worker.submit(&blocked, true, true, &root, LogLevel::Warn);
    release.send(()).unwrap();
    rx.recv_timeout(Duration::from_secs(2)).unwrap();
    release.send(()).unwrap();
    drop(worker);
    let text = std::fs::read_to_string(&log).unwrap();
    assert_eq!(text.matches("agent.activation_failed").count(), 2, "{text}");
    assert_eq!(
        text.matches("agent.activation_dropped").count(),
        1,
        "{text}"
    );
    assert!(!text.contains("PRIVATE"));
    std::fs::remove_file(log).unwrap();
    std::fs::remove_dir(root).unwrap();
}
