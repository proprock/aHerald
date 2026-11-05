//! The static sender inside a real container, against a listener on the Docker bridge.
//! Ignored by default: it needs Docker, the `docker0` bridge and a static build. CI runs it with
//! `AHERALD_PING_STATIC=$PWD/target/x86_64-unknown-linux-musl/ping/aherald-ping cargo test -p
//! aherald-ping --test container -- --ignored`.
#![cfg(target_os = "linux")]
use aherald::{
    config::Config,
    ipc::{Request, Response},
    remote::{Context, Deliver, Remote},
};
use std::{
    path::Path,
    process::Command,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

const TOKEN: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
const BRIDGE: &str = "172.17.0.1";

#[test]
#[ignore = "needs Docker and a static aherald-ping"]
fn a_stop_from_a_container_reaches_the_host() {
    let binary = std::env::var("AHERALD_PING_STATIC").expect("AHERALD_PING_STATIC");
    let binary = std::fs::canonicalize(binary).unwrap();
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
    let endpoint = format!("http://{}", remote.local_addr().unwrap());
    let script = r#"set -e
aherald-ping --self-test
echo '{"hook_event_name":"Stop","session_id":"container-1","cwd":"/work/app"}' | aherald-ping --hook --agent claude
"#;
    let output = Command::new("docker")
        .args(["run", "--rm", "-v"])
        .arg(format!(
            "{}:/usr/local/bin/aherald-ping:ro",
            binary.display()
        ))
        .args(["-e", &format!("AHERALD_ENDPOINT={endpoint}")])
        .args(["-e", &format!("AHERALD_TOKEN={TOKEN}")])
        .args(["alpine:3", "sh", "-c", script])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let seen = seen.lock().unwrap();
    let [Request::Event { event }] = seen.as_slice() else {
        panic!("{seen:?}");
    };
    assert_eq!(event.session_id.as_deref(), Some("container-1"));
    assert_eq!(event.folder.as_deref(), Some("app"));
}
