//! Small bounded waits and owned-process guards shared by integration tests.
#![allow(dead_code)] // every test crate compiles this module and uses a subset of it
use std::{
    io::Read,
    process::{Child, Command, Output, Stdio},
    time::{Duration, Instant},
};

#[cfg(windows)]
pub mod detached;
pub mod fake_http;
#[cfg(windows)]
mod resident;
#[cfg(windows)]
pub mod window;
#[cfg(windows)]
#[allow(unused_imports)] // each integration crate uses a different subset of support
pub use resident::ResidentFixture;

/// A scratch directory owned by one test: created empty with a name unique to this process
/// and call, and removed with its contents on drop, including when the test panics. It never
/// touches a directory it did not create, so concurrent tests cannot remove each other's files.
pub struct TempDir(std::path::PathBuf);

impl TempDir {
    pub fn new(tag: &str) -> Self {
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        loop {
            let unique = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let path =
                std::env::temp_dir().join(format!("ah-{tag}-{}-{unique}", std::process::id()));
            match Self::create(path) {
                Ok(directory) => return directory,
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => panic!("cannot create owned test directory: {error}"),
            }
        }
    }

    fn create(path: std::path::PathBuf) -> std::io::Result<Self> {
        std::fs::create_dir(&path)?;
        Ok(Self(path))
    }
}

impl std::ops::Deref for TempDir {
    type Target = std::path::Path;
    fn deref(&self) -> &std::path::Path {
        &self.0
    }
}

impl AsRef<std::path::Path> for TempDir {
    fn as_ref(&self) -> &std::path::Path {
        &self.0
    }
}

impl AsRef<std::ffi::OsStr> for TempDir {
    fn as_ref(&self) -> &std::ffi::OsStr {
        self.0.as_os_str()
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

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

/// Orders the writing of an executable against every child start made through this module.
/// A forked child inherits each descriptor open at the fork until it execs, and Linux refuses
/// to exec a file (`ETXTBSY`) while any copy of its write descriptor lives, even one held by a
/// sibling test's child. Writers take it exclusively for the open-write-close window; starts
/// share it, so no start can overlap a window. The `ETXTBSY` of a program run by a script is
/// invisible to the test, which is why the retry in [`run_bounded_with_input`] is not enough.
static EXEC_GATE: std::sync::RwLock<()> = std::sync::RwLock::new(());

/// Write `contents` to `path` and make it executable, such that running it at once cannot fail
/// with `ETXTBSY` because of a start made through [`run_bounded_with_input`] or [`output`] in
/// another thread of this test process.
#[cfg(unix)]
pub fn write_executable(path: &std::path::Path, contents: impl AsRef<[u8]>) {
    use std::os::unix::fs::PermissionsExt;
    let _gate = EXEC_GATE.write().unwrap_or_else(|e| e.into_inner());
    std::fs::write(path, contents).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

/// [`run_bounded`] with a generous 30 s deadline for ordinary short CLI runs; panics with the
/// diagnostic on timeout.
pub fn output(command: &mut Command) -> Output {
    run_bounded(command, Duration::from_secs(30)).unwrap_or_else(|error| panic!("{error}"))
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
    // A binary just copied by a parallel test can still have a write handle inherited by a
    // sibling thread's fork; Linux reports ETXTBSY until that child execs. Retry briefly.
    let mut attempts = 0;
    let child = loop {
        let spawned = {
            let _gate = EXEC_GATE.read().unwrap_or_else(|e| e.into_inner());
            command.spawn()
        };
        match spawned {
            Ok(child) => break child,
            Err(error) if error.raw_os_error() == Some(26) && attempts < 100 => {
                attempts += 1;
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(error) => return Err(format!("cannot start `{program}`: {error}")),
        }
    };
    capture_bounded_inner(child, input, timeout, &program)
}

/// Bound an already launched process, retaining specialized spawn/retry behavior.
pub fn capture_bounded(child: Child, input: &[u8], timeout: Duration) -> Result<Output, String> {
    capture_bounded_inner(child, input, timeout, "owned child")
}

fn capture_bounded_inner(
    child: Child,
    input: &[u8],
    timeout: Duration,
    program: &str,
) -> Result<Output, String> {
    let mut child = OwnedChild(child);
    let start = Instant::now();
    fn drain(pipe: Option<impl Read + Send + 'static>) -> std::sync::mpsc::Receiver<Vec<u8>> {
        let (send, recv) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let mut bytes = Vec::new();
            if let Some(mut pipe) = pipe {
                let _ = pipe.read_to_end(&mut bytes);
            }
            let _ = send.send(bytes);
        });
        recv
    }
    if let Some(mut stdin) = child.0.stdin.take() {
        let input = input.to_vec();
        std::thread::spawn(move || {
            let _ = std::io::Write::write_all(&mut stdin, &input);
        });
    }
    let stdout = drain(child.0.stdout.take());
    let stderr = drain(child.0.stderr.take());
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
    // Descendants may inherit a pipe after the owned child exits. Never join a reader
    // without a deadline, and never kill an unowned descendant to close that pipe.
    let receive = |pipe: std::sync::mpsc::Receiver<Vec<u8>>| {
        pipe.recv_timeout(timeout.saturating_sub(start.elapsed())).map_err(|_| {
            format!("`{program}` output did not close within {timeout:?}; child exited, last output state: open")
        })
    };
    Ok(Output {
        status,
        stdout: receive(stdout)?,
        stderr: receive(stderr)?,
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

    #[test]
    fn temp_dir_creation_preserves_an_existing_directory() {
        let existing = TempDir::new("existing");
        std::fs::write(existing.join("sentinel"), b"keep").unwrap();
        let result = TempDir::create(existing.to_path_buf());
        assert!(matches!(result, Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists));
        assert_eq!(std::fs::read(existing.join("sentinel")).unwrap(), b"keep");
    }

    #[test]
    fn temp_dirs_are_unique_and_independent_and_are_removed_with_their_contents() {
        let first = TempDir::new("fixture");
        let second = TempDir::new("fixture");
        assert_ne!(*first, *second);
        std::fs::create_dir_all(first.join("nested")).unwrap();
        std::fs::write(first.join("nested/file"), b"x").unwrap();
        std::fs::write(second.join("file"), b"y").unwrap();
        let (first_path, second_path) = (first.to_path_buf(), second.to_path_buf());
        drop(first);
        assert!(!first_path.exists(), "dropped directory remains");
        assert!(second_path.join("file").exists(), "a sibling was removed");
        drop(second);
        assert!(!second_path.exists());
    }

    #[test]
    fn a_temp_dir_is_removed_when_the_test_panics() {
        let path = std::sync::Mutex::new(std::path::PathBuf::new());
        let result = std::panic::catch_unwind(|| {
            let dir = TempDir::new("fixture-panic");
            std::fs::write(dir.join("file"), b"x").unwrap();
            *path.lock().unwrap() = dir.to_path_buf();
            panic!("assertion failed");
        });
        assert!(result.is_err());
        let path = path.lock().unwrap();
        assert!(!path.as_os_str().is_empty() && !path.exists());
    }

    #[test]
    fn temp_dirs_made_concurrently_never_collide() {
        let paths: Vec<_> = (0..8)
            .map(|_| std::thread::spawn(|| TempDir::new("fixture-race")))
            .collect::<Vec<_>>()
            .into_iter()
            .map(|handle| handle.join().unwrap())
            .collect();
        let mut names: Vec<_> = paths.iter().map(|dir| dir.to_path_buf()).collect();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), 8);
    }

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
