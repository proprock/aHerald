//! The senders under the sandbox runtimes of coding agents (AH-223). Ignored by default: they need
//! the runtime installed and a Docker-style bridge address, and they only document what works.
//!
//! `AHERALD_SRT=/path/to/srt` (and `rg` on PATH), `AHERALD_NONO=/path/to/nono`,
//! `AHERALD_PING_STATIC=/abs/path/to/aherald-ping`; then
//! `cargo test -p aherald-ping --test sandbox_runtimes -- --ignored`.
#![cfg(target_os = "linux")]
use aherald::{
    config::Config,
    ipc::{Request, Response},
    remote::{Context, Deliver, Remote},
};
use std::{
    path::{Path, PathBuf},
    process::Command,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

const TOKEN: &str = "89abcdef0123456789abcdef0123456789abcdef0123456789abcdef01234567";
const BRIDGE: &str = "172.17.0.1";

struct Host {
    _remote: Remote,
    endpoint: String,
    seen: Arc<Mutex<Vec<Request>>>,
}

fn host() -> Host {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let deliver: Deliver = {
        let seen = seen.clone();
        Arc::new(move |request: Request| {
            seen.lock().unwrap().push(request);
            Response::Accepted
        })
    };
    let context = Arc::new(Context {
        config: Config::parse("", Path::new(".")).unwrap(),
        token: TOKEN.into(),
        deliver,
    });
    let remote = Remote::start(format!("{BRIDGE}:0").parse().unwrap(), context);
    let deadline = Instant::now() + Duration::from_secs(5);
    while remote.local_addr().is_none() {
        assert!(Instant::now() < deadline, "no listener on {BRIDGE}");
        std::thread::sleep(Duration::from_millis(10));
    }
    Host {
        endpoint: format!("http://{}", remote.local_addr().unwrap()),
        _remote: remote,
        seen,
    }
}

fn path(variable: &str) -> PathBuf {
    std::fs::canonicalize(std::env::var(variable).unwrap_or_else(|_| panic!("set {variable}")))
        .unwrap()
}

/// nono 0.81 on a kernel with Landlock V1 (no TCP filtering): the network stays open, so the
/// static sender works; `--block-net` stops it.
#[test]
#[ignore = "needs nono"]
fn nono_lets_the_sender_through_unless_the_network_is_blocked() {
    let (nono, ping) = (path("AHERALD_NONO"), path("AHERALD_PING_STATIC"));
    let host = host();
    let run = |extra: &[&str]| {
        Command::new(&nono)
            .args(["run", "--allow"])
            .arg(ping.parent().unwrap())
            .args(extra)
            .arg("--")
            .arg(&ping)
            .arg("--self-test")
            .env("AHERALD_ENDPOINT", &host.endpoint)
            .env("AHERALD_TOKEN", TOKEN)
            .output()
            .unwrap()
    };
    assert!(run(&[]).status.success());
    assert!(!run(&["--block-net"]).status.success());
}

/// srt 0.0.81: its network namespace only has the proxy, so the proxy-aware shell sender works
/// for an allowed non-loopback host and the static sender (no proxy support) does not.
#[test]
#[ignore = "needs srt and rg"]
fn srt_passes_the_shell_sender_through_its_proxy() {
    let (srt, ping) = (path("AHERALD_SRT"), path("AHERALD_PING_STATIC"));
    let script =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/sandbox/aherald-ping.sh");
    let host = host();
    let settings = std::env::temp_dir().join(format!("srt-ping-{}.json", std::process::id()));
    std::fs::write(
        &settings,
        format!(
            r#"{{"network":{{"allowedDomains":["{BRIDGE}"],"deniedDomains":[]}},"filesystem":{{"denyRead":[],"allowWrite":["."],"denyWrite":[]}}}}"#
        ),
    )
    .unwrap();
    let run = |command: String| {
        Command::new(&srt)
            .arg("-s")
            .arg(&settings)
            .arg("-c")
            .arg(command)
            .env("AHERALD_ENDPOINT", &host.endpoint)
            .env("AHERALD_TOKEN", TOKEN)
            .output()
            .unwrap()
    };
    let stop = r#"echo '{"hook_event_name":"Stop","session_id":"srt-1"}'"#;
    let sent = run(format!("{stop} | {} claude", script.display()));
    assert!(sent.status.success(), "{sent:?}");
    assert_eq!(host.seen.lock().unwrap().len(), 1);
    assert!(
        !run(format!("{} --self-test", ping.display()))
            .status
            .success()
    );
    let _ = std::fs::remove_file(settings);
}
