#![cfg(unix)]
//! The real `aherald-ping` against the real `[remote]` listener of aHerald.
use aherald::{
    config::Config,
    ipc::{Request, Response},
    remote::{Context, Deliver, Remote},
};
use std::{
    io::Write,
    net::TcpListener,
    path::Path,
    process::{Command, Stdio},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

const TOKEN: &str = "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff";

struct Host {
    _remote: Remote,
    endpoint: String,
    seen: Arc<Mutex<Vec<Request>>>,
}

fn host() -> Host {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let deliver: Deliver = {
        let seen = seen.clone();
        Arc::new(move |request| {
            seen.lock().unwrap().push(request);
            Response::Accepted
        })
    };
    let context = Arc::new(Context {
        config: Config::parse("", Path::new(".")).unwrap(),
        token: TOKEN.into(),
        deliver,
    });
    let remote = Remote::start("127.0.0.1:0".parse().unwrap(), context);
    let deadline = Instant::now() + Duration::from_secs(5);
    while remote.local_addr().is_none() {
        assert!(Instant::now() < deadline, "the listener did not start");
        std::thread::sleep(Duration::from_millis(5));
    }
    Host {
        endpoint: format!("http://{}", remote.local_addr().unwrap()),
        _remote: remote,
        seen,
    }
}

fn ping(
    args: &[&str],
    stdin: &[u8],
    endpoint: Option<&str>,
    token: Option<&str>,
) -> (std::process::Output, Duration) {
    let mut command = Command::new(env!("CARGO_BIN_EXE_aherald-ping"));
    command
        .args(args)
        .env_clear()
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(endpoint) = endpoint {
        command.env("AHERALD_ENDPOINT", endpoint);
    }
    if let Some(token) = token {
        command.env("AHERALD_TOKEN", token);
    }
    let started = Instant::now();
    let mut child = command.spawn().unwrap();
    let mut pipe = child.stdin.take().unwrap();
    let input = stdin.to_vec();
    // The writer may outlive a program that stops reading; never block the test on it.
    let writer = std::thread::spawn(move || {
        let _ = pipe.write_all(&input);
    });
    let output = child.wait_with_output().unwrap();
    let _ = writer.join();
    (output, started.elapsed())
}

const STOP: &[u8] =
    br#"{"hook_event_name":"Stop","session_id":"guest-1","cwd":"/home/me/project"}"#;
const CODEX_STOP: &[u8] = br#"{"hook_event_name":"Stop","session_id":"c-1","turn_id":"t-1"}"#;

#[test]
fn a_stop_arrives_as_an_event_with_the_folder_and_nothing_private() {
    let host = host();
    let payload = br#"{"hook_event_name":"Stop","session_id":"guest-1","cwd":"/home/me/project","transcript_path":"/home/me/x.jsonl","prompt":"private"}"#;
    let (output, _) = ping(
        &["--hook", "--agent", "claude", "--resident"],
        payload,
        Some(&host.endpoint),
        Some(TOKEN),
    );
    assert!(
        output.status.success() && output.stdout.is_empty() && output.stderr.is_empty(),
        "{output:?}"
    );
    let seen = host.seen.lock().unwrap();
    let [Request::Event { event }] = seen.as_slice() else {
        panic!("{seen:?}");
    };
    assert_eq!(event.session_id.as_deref(), Some("guest-1"));
    assert_eq!(event.folder.as_deref(), Some("project"));
}

#[test]
fn a_codex_stop_prints_the_acknowledgement() {
    let host = host();
    let (output, _) = ping(&["--hook"], CODEX_STOP, Some(&host.endpoint), Some(TOKEN));
    assert_eq!(String::from_utf8_lossy(&output.stdout), "{}\n");
    assert_eq!(host.seen.lock().unwrap().len(), 1);
}

#[test]
fn tool_activity_never_leaves_the_guest() {
    let host = host();
    for name in [
        "PreToolUse",
        "PostToolUse",
        "UserPromptSubmit",
        "SubagentStop",
    ] {
        let body = format!(
            r#"{{"hook_event_name":"{name}","session_id":"s","tool_input":{{"command":"ls"}}}}"#
        );
        let (output, elapsed) = ping(
            &["--hook", "--agent", "claude"],
            body.as_bytes(),
            Some(&host.endpoint),
            Some(TOKEN),
        );
        assert!(output.status.success(), "{name}");
        // No request at all: it returns without touching the network.
        assert!(elapsed < Duration::from_millis(900), "{name} {elapsed:?}");
    }
    assert!(host.seen.lock().unwrap().is_empty());
}

#[test]
fn a_wrong_token_or_missing_settings_deliver_nothing_and_exit_zero() {
    let host = host();
    for (endpoint, token) in [
        (Some(host.endpoint.as_str()), Some("wrong")),
        (Some(host.endpoint.as_str()), None),
        (None, Some(TOKEN)),
        (None, None),
        (Some("https://localhost:1"), Some(TOKEN)),
    ] {
        let (output, _) = ping(&["--hook", "--agent", "claude"], STOP, endpoint, token);
        assert!(
            output.status.success() && output.stdout.is_empty(),
            "{endpoint:?} {token:?}"
        );
    }
    assert!(host.seen.lock().unwrap().is_empty());
}

#[test]
fn an_unknown_argument_still_exits_zero_and_drains_input() {
    let (output, _) = ping(&["--nonsense"], STOP, None, None);
    assert!(output.status.success(), "{output:?}");
    assert!(String::from_utf8_lossy(&output.stderr).contains("unknown argument"));
}

#[test]
fn a_refused_or_hung_host_costs_about_a_second_at_most() {
    let refused = TcpListener::bind("127.0.0.1:0").unwrap();
    let refused_endpoint = format!("http://{}", refused.local_addr().unwrap());
    drop(refused);
    let hung = TcpListener::bind("127.0.0.1:0").unwrap();
    let hung_endpoint = format!("http://{}", hung.local_addr().unwrap());
    for endpoint in [&refused_endpoint, &hung_endpoint] {
        let (output, elapsed) = ping(
            &["--hook", "--agent", "claude"],
            STOP,
            Some(endpoint),
            Some(TOKEN),
        );
        assert!(
            output.status.success() && output.stdout.is_empty(),
            "{endpoint}"
        );
        assert!(elapsed < Duration::from_secs(3), "{endpoint} {elapsed:?}");
    }
}

#[test]
fn an_oversized_payload_is_dropped_without_hanging() {
    let host = host();
    let big = vec![b' '; 2_000_000];
    let (output, elapsed) = ping(
        &["--hook", "--agent", "claude"],
        &big,
        Some(&host.endpoint),
        Some(TOKEN),
    );
    assert!(output.status.success(), "{output:?}");
    assert!(elapsed < Duration::from_secs(5));
    assert!(host.seen.lock().unwrap().is_empty());
}

#[test]
fn the_self_test_tells_a_working_setup_from_a_broken_one() {
    let host = host();
    let (ok, _) = ping(&["--self-test"], b"", Some(&host.endpoint), Some(TOKEN));
    assert!(ok.status.success(), "{ok:?}");
    assert!(String::from_utf8_lossy(&ok.stdout).starts_with("ok:"));
    let (bad_token, _) = ping(&["--self-test"], b"", Some(&host.endpoint), Some("wrong"));
    assert_eq!(bad_token.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&bad_token.stderr).contains("refused the token"));
    let (no_settings, _) = ping(&["--self-test"], b"", None, None);
    assert_eq!(no_settings.status.code(), Some(1));
    // A self-test makes no card.
    assert!(host.seen.lock().unwrap().is_empty());
}
