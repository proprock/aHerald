//! Small bounded waits and owned-process guards shared by integration tests.
#![allow(dead_code)] // every test crate compiles this module and uses a subset of it
use std::{
    io::Read,
    process::{Child, Command, Output, Stdio},
    time::{Duration, Instant},
};

/// A child process the test owns: killed and reaped on drop, so a failed assertion or a
/// panic cannot leave it running. Only this exact process is signalled, never one found by name.
pub struct OwnedChild(pub Child);

impl OwnedChild {
    /// Kill and reap now; returns once the process is gone.
    pub fn stop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

impl Drop for OwnedChild {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Run `command` to completion within `timeout`, capturing stdout and stderr.
///
/// Both pipes are drained on their own threads, so a chatty child cannot deadlock on a full
/// pipe. On timeout the child is killed and reaped and the error names the command and
/// deadline but never the captured output, which may hold payloads.
pub fn run_bounded(command: &mut Command, timeout: Duration) -> Result<Output, String> {
    run_bounded_with_input(command, &[], timeout)
}

/// As [`run_bounded`], writing `input` to the child's stdin (configure `Stdio::piped()` for it
/// on the command) and then closing it. The write happens on its own thread, so a child that
/// never reads cannot block the test past `timeout`.
pub fn run_bounded_with_input(
    command: &mut Command,
    input: &[u8],
    timeout: Duration,
) -> Result<Output, String> {
    let program = command.get_program().to_string_lossy().into_owned();
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = OwnedChild(
        command
            .spawn()
            .map_err(|error| format!("cannot start `{program}`: {error}"))?,
    );
    fn drain(pipe: Option<impl Read + Send + 'static>) -> std::thread::JoinHandle<Vec<u8>> {
        std::thread::spawn(move || {
            let mut bytes = Vec::new();
            if let Some(mut pipe) = pipe {
                let _ = pipe.read_to_end(&mut bytes);
            }
            bytes
        })
    }
    if let Some(mut stdin) = child.0.stdin.take() {
        let input = input.to_vec();
        std::thread::spawn(move || {
            let _ = std::io::Write::write_all(&mut stdin, &input);
        });
    }
    let stdout = drain(child.0.stdout.take());
    let stderr = drain(child.0.stderr.take());
    let start = Instant::now();
    let status = loop {
        match child.0.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if start.elapsed() < timeout => {
                std::thread::sleep(Duration::from_millis(5));
            }
            Ok(None) => {
                child.stop();
                return Err(format!(
                    "`{program}` did not exit within {timeout:?}; it was killed"
                ));
            }
            Err(error) => return Err(format!("cannot wait for `{program}`: {error}")),
        }
    };
    Ok(Output {
        status,
        stdout: stdout.join().unwrap_or_default(),
        stderr: stderr.join().unwrap_or_default(),
    })
}

/// Poll a boolean state, retaining a useful timeout diagnostic without printing payloads.
pub fn wait_until(
    timeout: Duration,
    interval: Duration,
    what: &str,
    mut ready: impl FnMut() -> bool,
) -> Result<(), String> {
    let start = Instant::now();
    loop {
        if ready() {
            return Ok(());
        }
        let elapsed = start.elapsed();
        if elapsed >= timeout {
            return Err(format!(
                "timed out waiting for {what} after {elapsed:?}; last observation: false"
            ));
        }
        std::thread::sleep(interval.min(timeout.saturating_sub(elapsed)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    fn shell(script: &str) -> Command {
        let mut command = Command::new("sh");
        command.args(["-c", script]);
        command
    }

    #[cfg(unix)]
    #[test]
    fn a_bounded_run_returns_delayed_output_and_does_not_deadlock_on_large_output() {
        let output = run_bounded(
            &mut shell("sleep 0.1; echo done; head -c 300000 /dev/zero; echo err >&2"),
            Duration::from_secs(10),
        )
        .unwrap();
        assert!(output.status.success());
        assert_eq!(output.stdout.len(), 300_005);
        assert_eq!(output.stderr, b"err\n");
    }

    #[cfg(unix)]
    #[test]
    fn a_bounded_run_kills_a_hung_child_and_hides_its_output_in_the_diagnostic() {
        let start = Instant::now();
        let error = run_bounded(
            &mut shell("echo secret-payload; exec sleep 30"),
            Duration::from_millis(300),
        )
        .unwrap_err();
        assert!(start.elapsed() < Duration::from_secs(5), "child not killed");
        assert!(
            error.contains("did not exit") && !error.contains("secret"),
            "{error}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn an_owned_child_is_killed_and_reaped_on_drop_even_when_the_test_panics() {
        let child = Command::new("sleep").arg("30").spawn().unwrap();
        let pid = child.id();
        let result = std::panic::catch_unwind(move || {
            let _owned = OwnedChild(child);
            panic!("assertion failed");
        });
        assert!(result.is_err());
        // Reaped: the pid no longer names a process of ours.
        let alive = process_exists(pid);
        assert!(!alive, "owned child {pid} survived its guard");
    }

    #[cfg(unix)]
    fn process_exists(pid: u32) -> bool {
        // `kill -0` succeeds only while the process (or an unreaped zombie) exists.
        Command::new("kill")
            .args(["-0", &pid.to_string()])
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|status| status.success())
    }

    #[test]
    fn bounded_wait_accepts_a_delayed_change_and_reports_a_missing_effect() {
        let start = Instant::now();
        assert!(
            wait_until(
                Duration::from_secs(1),
                Duration::from_millis(5),
                "delayed state",
                || start.elapsed() >= Duration::from_millis(25)
            )
            .is_ok()
        );
        let error = wait_until(
            Duration::from_millis(25),
            Duration::from_millis(5),
            "missing state",
            || false,
        )
        .unwrap_err();
        assert!(
            error.contains("missing state") && error.contains("last observation: false"),
            "{error}"
        );
    }
}
