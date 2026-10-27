use aherald::ipc;
#[test]
fn frames_round_trip_and_reject_invalid_length_version_and_json() {
    let frame = ipc::encode(&vec!["status"]).unwrap();
    assert_eq!(ipc::decode::<Vec<String>>(&frame).unwrap(), vec!["status"]);
    for bad in [
        vec![],
        vec![255; 6],
        vec![1, 0, 1, 0, 0, 0, b'{'],
        vec![2, 0, 2, 0, 0, 0, b'{', b'}'],
    ] {
        assert!(ipc::decode::<serde_json::Value>(&bad).is_err());
    }
    assert!(ipc::encode(&"x".repeat(ipc::MAX_FRAME)).is_err());
    let mut trailing = frame;
    trailing.push(0);
    assert!(ipc::decode::<serde_json::Value>(&trailing).is_err());
}

#[cfg(windows)]
#[test]
fn pipe_recovers_after_stalled_disconnected_and_malformed_clients() {
    use aherald::{
        ipc::{Request, Response, pipe},
        local::Scope,
    };
    use std::{
        sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        },
        time::{Duration, Instant},
    };
    use windows::{
        Win32::{Foundation::*, Storage::FileSystem::*},
        core::PCWSTR,
    };
    let scope = Scope::current(&format!("fault-test-{}", std::process::id())).unwrap();
    let stop = Arc::new(AtomicBool::new(false));
    let end = stop.clone();
    let server_scope = scope.clone();
    let (tx, rx) = std::sync::mpsc::sync_channel(1);
    let server = std::thread::spawn(move || {
        let _owner = server_scope.acquire().unwrap().unwrap();
        let pipe = server_scope.create_pipe().unwrap();
        tx.send(()).unwrap();
        while !end.load(Ordering::Acquire) {
            let _ = pipe.serve_once(|_| Response::Accepted);
        }
    });
    rx.recv_timeout(Duration::from_secs(2)).unwrap();
    struct Cleanup(Arc<AtomicBool>, Option<std::thread::JoinHandle<()>>);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            self.0.store(true, Ordering::Release);
            let _ = self.1.take().unwrap().join();
        }
    }
    let _cleanup = Cleanup(stop, Some(server));
    let name: Vec<u16> = scope.pipe_name.encode_utf16().chain(Some(0)).collect();
    for bytes in [
        None,
        Some(vec![255u8; 6]),
        Some(vec![1, 0, 1, 0, 0, 0, b'{']),
    ] {
        let deadline = Instant::now() + Duration::from_secs(2);
        let handle = loop {
            if let Ok(h) = unsafe {
                CreateFileW(
                    PCWSTR(name.as_ptr()),
                    GENERIC_READ.0 | GENERIC_WRITE.0,
                    FILE_SHARE_MODE(0),
                    None,
                    OPEN_EXISTING,
                    FILE_FLAGS_AND_ATTRIBUTES(0),
                    None,
                )
            } {
                break h;
            }
            assert!(Instant::now() < deadline);
            std::thread::yield_now();
        };
        if let Some(bytes) = bytes {
            unsafe {
                let mut n = 0;
                WriteFile(handle, Some(&bytes), Some(&mut n), None).unwrap();
            }
        } else {
            let mut response = [0; 1];
            let mut count = 0;
            let before = Instant::now();
            assert!(
                unsafe { ReadFile(handle, Some(&mut response), Some(&mut count), None) }.is_err()
            );
            assert!(before.elapsed() < Duration::from_secs(1));
        }
        unsafe {
            CloseHandle(handle).unwrap();
        }
        assert!(matches!(
            pipe::request(&scope, &Request::Status, Duration::from_millis(500)).unwrap(),
            Response::Accepted
        ));
    }
}
#[cfg(windows)]
#[test]
fn idle_listener_poll_is_not_an_operational_failure() {
    let scope =
        aherald::local::Scope::current(&format!("idle-log-{}", std::process::id())).unwrap();
    let server = scope.create_pipe().unwrap();
    assert!(server.serve_once(|_| panic!("no client connected")).is_ok());
}
#[cfg(windows)]
#[test]
fn a_connected_client_that_stalls_is_a_failure_unlike_an_idle_poll() {
    use aherald::local::Scope;
    use windows::{
        Win32::{Foundation::*, Storage::FileSystem::*},
        core::PCWSTR,
    };
    let scope = Scope::current(&format!("stalled-{}", std::process::id())).unwrap();
    let server = scope.create_pipe().unwrap();
    let name: Vec<u16> = scope.pipe_name.encode_utf16().chain(Some(0)).collect();
    // SAFETY: `name` is NUL-terminated and outlives the call; the handle is closed below.
    let client = unsafe {
        CreateFileW(
            PCWSTR(name.as_ptr()),
            GENERIC_READ.0 | GENERIC_WRITE.0,
            FILE_SHARE_MODE(0),
            None,
            OPEN_EXISTING,
            FILE_FLAGS_AND_ATTRIBUTES(0),
            None,
        )
    }
    .unwrap();
    let result = server.serve_once(|_| panic!("the client sent no request"));
    // SAFETY: `client` is a live handle owned by this test.
    unsafe { CloseHandle(client).unwrap() };
    assert!(
        result.is_err(),
        "a stalled client must not look like an idle poll"
    );
}
#[cfg(windows)]
#[test]
fn listener_blocks_until_a_client_or_the_stop_event_without_polling() {
    use aherald::{
        ipc::{Request, Response, pipe},
        local::Scope,
    };
    use std::time::{Duration, Instant};
    let scope = Scope::current(&format!("stop-event-{}", std::process::id())).unwrap();
    let stop = std::sync::Arc::new(pipe::StopEvent::new().unwrap());
    let thread_stop = stop.clone();
    let thread_scope = scope.clone();
    let (ready_tx, ready_rx) = std::sync::mpsc::channel();
    let listener = std::thread::spawn(move || {
        // The server handle stays on its owning thread, as in the resident.
        let server = thread_scope.create_pipe().unwrap();
        ready_tx.send(()).unwrap();
        let started = Instant::now();
        let idle = server
            .serve_until(&thread_stop, |_| Response::Accepted)
            .unwrap();
        let idle_for = started.elapsed();
        // A request arriving while waiting is served and reported.
        let fresh = pipe::StopEvent::new().unwrap();
        let served = server.serve_until(&fresh, |_| Response::Accepted).unwrap();
        (idle, idle_for, served)
    });
    ready_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    std::thread::sleep(Duration::from_millis(300));
    assert!(
        !listener.is_finished(),
        "an idle listener must keep waiting"
    );
    stop.signal();
    std::thread::sleep(Duration::from_millis(50));
    assert!(matches!(
        pipe::request(&scope, &Request::Status, Duration::from_millis(1000)).unwrap(),
        Response::Accepted
    ));
    let (idle, idle_for, served) = listener.join().unwrap();
    assert!(!idle, "stop wakes the listener without serving");
    assert!(idle_for >= Duration::from_millis(250));
    assert!(served);
}
