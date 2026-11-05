//! The shell senders for agents in containers and VMs: both post the hook payload to a `[remote]`
//! listener, and neither ever fails or delays the agent.
#![cfg(unix)]
mod support;
use aherald::{
    config::Config,
    ipc::{Request, Response},
    remote::{Context, Deliver, Remote},
};
use std::{
    fs,
    net::TcpListener,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

const TOKEN: &str = "fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210";
const SCRIPTS: [&str; 2] = [
    "examples/sandbox/aherald-ping.sh",
    "plugins/aherald/hooks/aherald-hook.sh",
];
const STOP: &str = r#"{"hook_event_name":"Stop","session_id":"guest-1","cwd":"/work/project"}"#;
const CODEX_STOP: &str = r#"{"hook_event_name":"Stop","session_id":"c-1","turn_id":"t-1"}"#;

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
    let endpoint = format!("http://{}", remote.local_addr().unwrap());
    Host {
        _remote: remote,
        endpoint,
        seen,
    }
}

fn run(
    script: &str,
    agent: &str,
    payload: &[u8],
    endpoint: Option<&str>,
    token: Option<&str>,
    extra: &[(&str, &Path)],
) -> (std::process::Output, Duration) {
    let mut command = Command::new(Path::new(env!("CARGO_MANIFEST_DIR")).join(script));
    command
        .arg(agent)
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("AHERALD_MDFIND", "/usr/bin/false")
        .env("AHERALD_SYSTEM_APPLICATIONS", "/nonexistent")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(endpoint) = endpoint {
        command.env("AHERALD_ENDPOINT", endpoint);
    }
    if let Some(token) = token {
        command.env("AHERALD_TOKEN", token);
    }
    for (key, value) in extra {
        command.env(key, value);
    }
    let started = Instant::now();
    let output =
        support::run_bounded_with_input(&mut command, payload, Duration::from_secs(30)).unwrap();
    (output, started.elapsed())
}

fn quiet(output: &std::process::Output) {
    assert!(output.status.success(), "{output:?}");
    assert!(output.stderr.is_empty(), "{:?}", output.stderr);
}

#[test]
fn both_senders_deliver_a_stop_to_the_host() {
    for script in SCRIPTS {
        let host = host();
        let (output, _) = run(
            script,
            "claude",
            STOP.as_bytes(),
            Some(&host.endpoint),
            Some(TOKEN),
            &[],
        );
        quiet(&output);
        assert!(output.stdout.is_empty(), "{script}");
        let seen = host.seen.lock().unwrap();
        let [Request::Event { event }] = seen.as_slice() else {
            panic!("{script}: {seen:?}");
        };
        assert_eq!(event.session_id.as_deref(), Some("guest-1"));
    }
}

#[test]
fn a_codex_stop_prints_the_acknowledgement_the_host_returned() {
    for script in SCRIPTS {
        let host = host();
        let (output, _) = run(
            script,
            "codex",
            CODEX_STOP.as_bytes(),
            Some(&host.endpoint),
            Some(TOKEN),
            &[],
        );
        quiet(&output);
        assert_eq!(String::from_utf8_lossy(&output.stdout), "{}\n", "{script}");
    }
}

#[test]
fn a_wrong_or_missing_token_or_endpoint_delivers_nothing_and_stays_silent() {
    for script in SCRIPTS {
        let host = host();
        for (endpoint, token) in [
            (Some(host.endpoint.as_str()), Some("wrong")),
            (Some(host.endpoint.as_str()), None),
            (None, Some(TOKEN)),
            (None, None),
        ] {
            let (output, _) = run(script, "codex", CODEX_STOP.as_bytes(), endpoint, token, &[]);
            quiet(&output);
            assert!(output.stdout.is_empty(), "{script} {endpoint:?} {token:?}");
        }
        assert!(host.seen.lock().unwrap().is_empty(), "{script}");
    }
}

#[test]
fn an_unreachable_or_hung_host_costs_about_a_second_at_most() {
    let refused = TcpListener::bind("127.0.0.1:0").unwrap();
    let refused_endpoint = format!("http://{}", refused.local_addr().unwrap());
    drop(refused);
    // Accepts the connection and never answers.
    let hung = TcpListener::bind("127.0.0.1:0").unwrap();
    let hung_endpoint = format!("http://{}", hung.local_addr().unwrap());
    for script in SCRIPTS {
        for endpoint in [&refused_endpoint, &hung_endpoint] {
            let (output, elapsed) = run(
                script,
                "claude",
                STOP.as_bytes(),
                Some(endpoint),
                Some(TOKEN),
                &[],
            );
            quiet(&output);
            assert!(
                elapsed < Duration::from_secs(3),
                "{script} {endpoint} {elapsed:?}"
            );
        }
    }
}

#[test]
fn an_oversized_payload_and_an_unknown_agent_are_drained_and_ignored() {
    let big = vec![b' '; 300_000];
    for script in SCRIPTS {
        let host = host();
        let (output, _) = run(
            script,
            "claude",
            &big,
            Some(&host.endpoint),
            Some(TOKEN),
            &[],
        );
        quiet(&output);
        let (output, _) = run(
            script,
            "opencode",
            STOP.as_bytes(),
            Some(&host.endpoint),
            Some(TOKEN),
            &[],
        );
        quiet(&output);
        assert!(host.seen.lock().unwrap().is_empty(), "{script}");
    }
}

#[test]
fn an_explicit_endpoint_never_runs_a_local_program() {
    let dir = support::TempDir::new("sandbox-sender-local");
    let marker = dir.join("ran");
    let program = dir.join("aherald");
    fs::write(
        &program,
        format!("#!/bin/sh\ncat >/dev/null\ntouch '{}'\n", marker.display()),
    )
    .unwrap();
    fs::set_permissions(&program, fs::Permissions::from_mode(0o755)).unwrap();
    let host = host();
    let script = SCRIPTS[1];
    let (output, _) = run(
        script,
        "claude",
        STOP.as_bytes(),
        Some(&host.endpoint),
        Some(TOKEN),
        &[("AHERALD_PATH", &program)],
    );
    quiet(&output);
    assert!(!marker.exists(), "the local program ran");
    assert_eq!(host.seen.lock().unwrap().len(), 1);
    // Without the endpoint the launcher is what it always was.
    let (output, _) = run(
        script,
        "claude",
        STOP.as_bytes(),
        None,
        None,
        &[("AHERALD_PATH", &program)],
    );
    quiet(&output);
    assert!(marker.exists(), "the local program did not run");
}

#[test]
fn the_example_hook_files_register_only_turn_ends_and_waits() {
    for (file, expected) in [
        (
            "examples/sandbox/claude-settings.json",
            vec![
                "Notification",
                "PermissionRequest",
                "SessionEnd",
                "Stop",
                "StopFailure",
            ],
        ),
        (
            "examples/sandbox/codex-hooks.json",
            vec!["Interrupt", "PermissionRequest", "SessionEnd", "Stop"],
        ),
    ] {
        let text =
            fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(file)).unwrap();
        let value: serde_json::Value = serde_json::from_str(&text).unwrap();
        let mut events: Vec<_> = value["hooks"]
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect();
        events.sort();
        assert_eq!(events, expected, "{file}");
    }
}

#[test]
fn a_comma_decimal_locale_does_not_stop_the_senders() {
    // curl rejects a fractional option value there; the scripts use whole seconds only.
    for script in SCRIPTS {
        let host = host();
        let locale = Path::new("pl_PL.UTF-8");
        let (output, _) = run(
            script,
            "claude",
            STOP.as_bytes(),
            Some(&host.endpoint),
            Some(TOKEN),
            &[("LC_ALL", locale)],
        );
        quiet(&output);
        assert_eq!(host.seen.lock().unwrap().len(), 1, "{script}");
    }
}

/// The lines between `# BEGIN remote` and `# END remote`, without indentation: the plugin launcher
/// nests the block in a branch, the standalone sender does not.
fn remote_block(script: &str) -> Vec<String> {
    let text = fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join(script)).unwrap();
    let mut lines = text.lines().map(str::trim);
    assert!(
        lines.by_ref().any(|line| line == "# BEGIN remote"),
        "{script} has no `# BEGIN remote` marker"
    );
    let block: Vec<String> = lines
        .by_ref()
        .take_while(|line| *line != "# END remote")
        .map(str::to_owned)
        .collect();
    assert!(
        lines.all(|line| line != "# BEGIN remote"),
        "{script} has more than one remote block"
    );
    assert!(!block.is_empty(), "{script} has an empty remote block");
    block
}

#[test]
fn the_remote_block_is_the_same_in_both_shell_senders() {
    let [standalone, plugin] = SCRIPTS.map(remote_block);
    assert_eq!(standalone, plugin);
    // The shared block is the post itself, so a changed endpoint, header or timeout shows up here.
    assert!(
        standalone
            .iter()
            .any(|line| line.contains("/v1/hook?agent="))
    );
}
