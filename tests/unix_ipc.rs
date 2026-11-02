//! Unix-domain-socket twin of the Windows named-pipe contract: per-user scope,
//! single instance, framed exchange, rejected peers and recovery from bad clients.
#![cfg(unix)]
mod support;
use aherald::{
    ipc::{Request, Response, pipe},
    local::Scope,
};
use std::{
    io::{Read, Write},
    os::unix::{
        fs::PermissionsExt,
        net::{UnixListener, UnixStream},
    },
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

fn scope(tag: &str) -> Scope {
    Scope::current(&format!("{tag}-{}", std::process::id())).unwrap()
}

fn status() -> Response {
    Response::Status {
        pid: std::process::id(),
        queued: 0,
        paused: false,
        sounds: true,
        visuals: true,
    }
}

/// Serves one request at a time until told to stop; returns the requests it handled.
fn serve(
    scope: &Scope,
    handler: impl Fn(Request) -> Response + Send + 'static,
) -> (
    Arc<AtomicBool>,
    Arc<pipe::StopEvent>,
    std::thread::JoinHandle<()>,
) {
    let server = scope.create_pipe().unwrap();
    let stop = Arc::new(pipe::StopEvent::new().unwrap());
    let done = Arc::new(AtomicBool::new(false));
    let (thread_stop, thread_done) = (stop.clone(), done.clone());
    let thread = std::thread::spawn(move || {
        while !thread_done.load(Ordering::SeqCst) {
            match server.serve_until(&thread_stop, &handler) {
                Ok(true) => {}
                Ok(false) => break,
                Err(_) => {}
            }
        }
    });
    (done, stop, thread)
}

fn finish(done: Arc<AtomicBool>, stop: Arc<pipe::StopEvent>, thread: std::thread::JoinHandle<()>) {
    done.store(true, Ordering::SeqCst);
    stop.signal();
    thread.join().unwrap();
}

#[test]
fn a_request_gets_the_handlers_response() {
    let scope = scope("round-trip");
    let _owner = scope.acquire().unwrap().unwrap();
    let (done, stop, thread) = serve(&scope, |request| match request {
        Request::Status => status(),
        Request::Quit => Response::Bye,
        _ => Response::Rejected,
    });
    let response = pipe::request(&scope, &Request::Status, Duration::from_millis(500)).unwrap();
    assert!(matches!(response, Response::Status { .. }), "{response:?}");
    let response = pipe::request(&scope, &Request::Quit, Duration::from_millis(500)).unwrap();
    assert!(matches!(response, Response::Bye), "{response:?}");
    finish(done, stop, thread);
}

#[test]
fn a_request_without_a_resident_fails_within_its_deadline() {
    let scope = scope("nobody-home");
    let started = Instant::now();
    assert!(pipe::request(&scope, &Request::Status, Duration::from_millis(100)).is_err());
    assert!(started.elapsed() < Duration::from_secs(1));
}

#[test]
fn the_instance_lock_admits_one_owner_and_recovers_after_it_is_dropped() {
    let scope = scope("single-instance");
    let owner = scope.acquire().unwrap().unwrap();
    // flock conflicts between separate open file descriptions even inside one process,
    // so a second attempt from another thread stands in for a second process.
    let rival = scope.clone();
    assert!(
        !std::thread::spawn(move || rival.acquire().unwrap().is_some())
            .join()
            .unwrap()
    );
    drop(owner);
    // A child another test is spawning right now briefly holds a copy of the lock's
    // descriptor until its exec closes it, so recovery is awaited for a bounded time.
    support::wait_until(
        Duration::from_secs(2),
        Duration::from_millis(10),
        "the instance lock to be released",
        || scope.acquire().unwrap().is_some(),
    )
    .unwrap_or_else(|error| panic!("{error}"));
}

/// Not a test of its own: the body of the process that `the_instance_lock_is_released_...`
/// starts to own a scope. Without `AHERALD_TEST_OWNER` it returns at once.
#[test]
fn process_owner_helper() {
    let Ok(namespace) = std::env::var("AHERALD_TEST_OWNER") else {
        return;
    };
    let scope = Scope::current(&namespace).unwrap();
    let _owner = scope.acquire().unwrap().unwrap();
    std::fs::write(std::env::var("AHERALD_TEST_READY").unwrap(), b"ready").unwrap();
    std::thread::sleep(Duration::from_secs(30));
}

#[test]
fn the_instance_lock_is_released_when_the_owner_process_dies() {
    let namespace = format!("owner-exit-{}", std::process::id());
    let scope = Scope::current(&namespace).unwrap();
    let ready = std::env::temp_dir().join(format!("{namespace}.ready"));
    let _ = std::fs::remove_file(&ready);
    let mut owner = support::OwnedChild(
        std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "process_owner_helper", "--nocapture"])
            .env("AHERALD_TEST_OWNER", &namespace)
            .env("AHERALD_TEST_READY", &ready)
            .spawn()
            .unwrap(),
    );
    support::wait_until(
        Duration::from_secs(5),
        Duration::from_millis(10),
        "the owner process to take the scope",
        || ready.exists(),
    )
    .unwrap_or_else(|error| panic!("{error}"));
    assert!(
        scope.acquire().unwrap().is_none(),
        "a live owner was displaced"
    );
    owner.stop();
    assert!(
        scope.acquire().unwrap().is_some(),
        "a dead owner kept the lock"
    );
    let _ = std::fs::remove_file(ready);
}

#[test]
fn only_one_listener_exists_per_scope() {
    let scope = scope("one-listener");
    let _owner = scope.acquire().unwrap().unwrap();
    let first = scope.create_pipe().unwrap();
    assert!(scope.create_pipe().is_err());
    drop(first);
}

#[test]
fn the_socket_and_its_directory_are_private_to_the_user() {
    let scope = scope("private");
    let _owner = scope.acquire().unwrap().unwrap();
    let _server = scope.create_pipe().unwrap();
    let socket = scope.socket_path();
    let socket_mode = std::fs::metadata(socket).unwrap().permissions().mode() & 0o777;
    assert_eq!(socket_mode, 0o600);
    let directory_mode = std::fs::metadata(socket.parent().unwrap())
        .unwrap()
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(directory_mode, 0o700);
}

#[test]
fn a_stale_socket_file_is_replaced_by_the_new_owner() {
    let scope = scope("stale");
    // A dead resident leaves its socket file behind: bind one and drop the listener, which
    // closes the descriptor but does not unlink the path.
    drop(UnixListener::bind(scope.socket_path()).unwrap());
    assert!(scope.socket_path().exists());
    let _owner = scope.acquire().unwrap().unwrap();
    let (done, stop, thread) = serve(&scope, |_| status());
    assert!(pipe::request(&scope, &Request::Status, Duration::from_millis(500)).is_ok());
    finish(done, stop, thread);
}

#[test]
fn an_oversized_namespace_or_an_overlong_socket_path_is_refused() {
    assert!(Scope::current("").is_err());
    assert!(Scope::current("has/slash").is_err());
    assert!(Scope::current(&"a".repeat(65)).is_err());
    // The longest valid namespace must still yield a usable socket path
    // (macOS limits sun_path to 104 bytes).
    let longest = Scope::current(&"a".repeat(64)).unwrap();
    assert!(longest.socket_path().as_os_str().len() < 104);
}

#[test]
fn a_peer_with_another_user_id_is_rejected_without_a_response() {
    let mut scope = scope("foreign-peer");
    let _owner = scope.acquire().unwrap().unwrap();
    scope.expect_peer_uid(u32::MAX - 1);
    let handled = Arc::new(AtomicBool::new(false));
    let seen = handled.clone();
    let (done, stop, thread) = serve(&scope, move |_| {
        seen.store(true, Ordering::SeqCst);
        status()
    });
    assert!(pipe::request(&scope, &Request::Status, Duration::from_millis(300)).is_err());
    assert!(
        !handled.load(Ordering::SeqCst),
        "the handler ran for a foreign peer"
    );
    finish(done, stop, thread);
}

#[test]
fn the_server_recovers_after_stalled_disconnected_and_malformed_clients() {
    let scope = scope("bad-clients");
    let _owner = scope.acquire().unwrap().unwrap();
    let (done, stop, thread) = serve(&scope, |_| status());
    let connect = || UnixStream::connect(scope.socket_path()).unwrap();
    // Connects and says nothing.
    let stalled = connect();
    // Connects and leaves at once.
    drop(connect());
    // Sends a header promising more than the limit.
    let mut oversized = connect();
    oversized
        .write_all(&[1, 0, 0xff, 0xff, 0xff, 0x7f])
        .unwrap();
    // Sends a valid header and then garbage.
    let mut malformed = connect();
    malformed
        .write_all(&[1, 0, 3, 0, 0, 0, b'x', b'y', b'z'])
        .unwrap();
    let mut sink = [0u8; 16];
    malformed
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let _ = malformed.read(&mut sink);
    // A well-behaved client is still served afterwards.
    let deadline = Instant::now() + Duration::from_secs(3);
    let response = loop {
        match pipe::request(&scope, &Request::Status, Duration::from_millis(500)) {
            Ok(response) => break response,
            Err(error) => assert!(Instant::now() < deadline, "never recovered: {error}"),
        }
    };
    assert!(matches!(response, Response::Status { .. }));
    drop(stalled);
    drop(oversized);
    finish(done, stop, thread);
}

#[test]
fn a_blocked_server_wakes_when_the_stop_event_is_signaled() {
    let scope = scope("stop");
    let _owner = scope.acquire().unwrap().unwrap();
    let server = scope.create_pipe().unwrap();
    let stop = Arc::new(pipe::StopEvent::new().unwrap());
    let waiting = stop.clone();
    let (ready_tx, ready_rx) = std::sync::mpsc::channel();
    let thread = std::thread::spawn(move || {
        ready_tx.send(()).unwrap();
        server.serve_until(&waiting, |_| status()).unwrap()
    });
    ready_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    stop.signal();
    assert!(!thread.join().unwrap(), "stop must report no client served");
}

#[test]
fn an_idle_accept_poll_returns_without_a_client() {
    let scope = scope("idle");
    let _owner = scope.acquire().unwrap().unwrap();
    let server = scope.create_pipe().unwrap();
    let started = Instant::now();
    server.serve_once(|_| status()).unwrap();
    assert!(started.elapsed() < Duration::from_secs(1));
}
