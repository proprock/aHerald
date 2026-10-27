#![cfg(windows)]
use aherald::{
    events::Event,
    process::{Child, deliver},
};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};
struct OwnedChild(Child);
impl Drop for OwnedChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
#[test]
fn transient_delivery_does_not_wait_for_finite_or_indefinite_lifetime() {
    for duration in [5000, 0] {
        let path = std::env::temp_dir().join(format!(
            "aherald-delivery-{}-{duration}.toml",
            std::process::id()
        ));
        std::fs::write(
            &path,
            format!("[events.turn_complete]\nduration_ms={duration}"),
        )
        .unwrap();
        let start = Instant::now();
        let result = deliver(
            &PathBuf::from(env!("CARGO_BIN_EXE_aherald")),
            &path,
            &Event::demo("turn_complete").unwrap(),
        );

        let mut child = OwnedChild(result.unwrap());
        assert!(start.elapsed() < Duration::from_secs(2));
        assert!(child.0.try_wait().unwrap().is_none());
        std::fs::remove_file(&path).unwrap();
    }
}
#[test]
fn missing_executable_and_invalid_event_fail_safely() {
    assert!(
        deliver(
            &PathBuf::from("missing-ah-executable.exe"),
            &PathBuf::from("examples/config.toml"),
            &Event::demo("error").unwrap()
        )
        .is_err()
    );
    let mut event = Event::demo("error").unwrap();
    event.session_id = Some("x".repeat(257));
    assert!(
        deliver(
            &PathBuf::from(env!("CARGO_BIN_EXE_aherald")),
            &PathBuf::from("examples/config.toml"),
            &event
        )
        .is_err()
    );
}

#[test]
fn detached_child_does_not_inherit_unrelated_hook_collector_handles() {
    use std::{
        io::Read,
        os::windows::io::{FromRawHandle, OwnedHandle},
        sync::mpsc,
    };
    use windows::Win32::{
        Foundation::{HANDLE, HANDLE_FLAG_INHERIT, SetHandleInformation},
        System::Pipes::CreatePipe,
    };
    // A shell/.NET hook launcher may own additional inheritable output handles.
    let (mut read, mut write) = (HANDLE::default(), HANDLE::default());
    unsafe {
        CreatePipe(&mut read, &mut write, None, 0).unwrap();
    }
    let reader = unsafe { OwnedHandle::from_raw_handle(read.0) };
    let writer = unsafe { OwnedHandle::from_raw_handle(write.0) };
    unsafe {
        SetHandleInformation(write, HANDLE_FLAG_INHERIT.0, HANDLE_FLAG_INHERIT).unwrap();
    }
    let path =
        std::env::temp_dir().join(format!("aherald-extra-handle-{}.toml", std::process::id()));
    std::fs::write(
        &path,
        "[events.turn_complete]\nduration_ms=0\n[sounds]\nenabled=false",
    )
    .unwrap();
    let child = deliver(
        &PathBuf::from(env!("CARGO_BIN_EXE_aherald")),
        &path,
        &Event::demo("turn_complete").unwrap(),
    )
    .unwrap();
    let mut child = OwnedChild(child);
    drop(writer);
    let (tx, rx) = mpsc::sync_channel(1);
    std::thread::spawn(move || {
        let mut pipe = std::fs::File::from(reader);
        let mut bytes = Vec::new();
        let _ = tx.send(pipe.read_to_end(&mut bytes));
    });
    let result = rx.recv_timeout(Duration::from_millis(500));
    assert!(child.0.try_wait().unwrap().is_none());
    std::fs::remove_file(path).unwrap();
    assert!(
        matches!(result, Ok(Ok(0))),
        "unrelated output pipe must reach EOF while the notification remains alive: {result:?}"
    );
}
