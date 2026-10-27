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
