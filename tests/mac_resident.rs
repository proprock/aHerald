//! The macOS resident: it serves IPC while its main thread runs AppKit, queues events and
//! shuts down on request. Notifications are disabled so no card appears during the test.
#![cfg(target_os = "macos")]
use std::{
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

struct Resident {
    child: Child,
    runtime: std::path::PathBuf,
}

impl Resident {
    fn start(tag: &str) -> Self {
        let runtime = std::env::temp_dir().join(format!("ah-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&runtime);
        std::fs::create_dir_all(&runtime).unwrap();
        let child = Command::new(env!("CARGO_BIN_EXE_aherald"))
            .args(["--resident", "--config", "tests/fixtures/quiet.toml"])
            .env("XDG_RUNTIME_DIR", &runtime)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        Self { child, runtime }
    }

    fn run(&self, args: &[&str]) -> String {
        let output = Command::new(env!("CARGO_BIN_EXE_aherald"))
            .args(args)
            .args(["--config", "tests/fixtures/quiet.toml"])
            .env("XDG_RUNTIME_DIR", &self.runtime)
            .stdin(Stdio::null())
            .output()
            .unwrap();
        String::from_utf8_lossy(&output.stdout).into_owned()
    }

    /// Polls `--status` until the resident answers; the process must still be alive.
    fn wait_ready(&mut self) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            assert!(
                self.child.try_wait().unwrap().is_none(),
                "the resident exited early"
            );
            if self.run(&["--status"]).contains("\"queued\"") {
                return;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        panic!("the resident never answered --status");
    }

    fn wait_exit(&mut self) -> std::process::ExitStatus {
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            if let Some(status) = self.child.try_wait().unwrap() {
                return status;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        panic!("the resident did not stop");
    }
}

impl Drop for Resident {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_dir_all(&self.runtime);
    }
}

#[test]
fn the_resident_answers_status_queues_events_and_quits_on_request() {
    let mut resident = Resident::start("mac-life");
    resident.wait_ready();
    assert!(resident.run(&["--status"]).contains("\"queued\":0"));
    resident.run(&["--resident", "--event", "turn_complete"]);
    assert!(resident.run(&["--status"]).contains("\"queued\":1"));
    assert!(!resident.run(&["--quit"]).contains("running\":false"));
    assert!(resident.wait_exit().success());
}

#[test]
fn a_second_resident_for_the_same_user_exits_quietly() {
    let mut first = Resident::start("mac-single");
    first.wait_ready();
    let mut second = Command::new(env!("CARGO_BIN_EXE_aherald"))
        .args(["--resident", "--config", "tests/fixtures/quiet.toml"])
        .env("XDG_RUNTIME_DIR", &first.runtime)
        .stdin(Stdio::null())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    let status = loop {
        if let Some(status) = second.try_wait().unwrap() {
            break status;
        }
        assert!(
            Instant::now() < deadline,
            "the second resident kept running"
        );
        std::thread::sleep(Duration::from_millis(20));
    };
    assert!(status.success());
    assert!(first.child.try_wait().unwrap().is_none());
}
