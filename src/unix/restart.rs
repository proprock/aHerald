//! A detached replacement waits for the old process, including all worker cleanup.
use std::{
    ffi::{OsStr, OsString},
    io::Read,
    time::{Duration, Instant},
};

const PARENT_EXIT_TIMEOUT: Duration = Duration::from_secs(30);

pub fn launch() -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| format!("cannot locate executable: {e}"))?;
    let mut args = vec![
        OsString::from("--restart-child"),
        OsString::from(std::process::id().to_string()),
    ];
    let mut original: Vec<OsString> = std::env::args_os().skip(1).collect();
    if original.first().is_some_and(|a| a == "--restart-child") {
        original.drain(..2);
    }
    args.extend(original);
    let borrowed: Vec<&OsStr> = args.iter().map(OsString::as_os_str).collect();
    let mut child = crate::detached::spawn(&exe, &borrowed, true)?;
    let mut output = child
        .stdout
        .take()
        .ok_or("restart child has no acknowledgement pipe")?;
    let (tx, rx) = std::sync::mpsc::sync_channel(1);
    std::thread::spawn(move || {
        let mut b = [0];
        let _ = tx.send(output.read_exact(&mut b).is_ok() && b == *b"R");
    });
    if rx.recv_timeout(Duration::from_millis(750)) == Ok(true) {
        Ok(())
    } else {
        let _ = child.kill();
        let _ = child.wait();
        Err("replacement failed to prepare restart".into())
    }
}

pub fn wait_for_parent(pid: u32) -> Result<(), String> {
    crate::process::acknowledge()?;
    wait_until_orphaned(pid, PARENT_EXIT_TIMEOUT)
}

/// Waits until `pid` is no longer this process's parent. The check is on the parent link,
/// not on the pid, so a recycled pid cannot keep the replacement waiting or release it early.
pub(crate) fn wait_until_orphaned(pid: u32, timeout: Duration) -> Result<(), String> {
    let deadline = Instant::now() + timeout;
    // SAFETY: `getppid` takes no arguments and cannot fail.
    while unsafe { libc::getppid() } as u32 == pid {
        if Instant::now() >= deadline {
            return Err("previous aHerald process did not exit within 30 seconds".into());
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pid_that_is_not_the_parent_releases_the_wait_at_once() {
        let unrelated = std::process::id();
        assert!(wait_until_orphaned(unrelated, Duration::from_millis(50)).is_ok());
    }

    #[test]
    fn a_living_parent_keeps_the_wait_until_the_deadline() {
        // SAFETY: `getppid` takes no arguments and cannot fail.
        let parent = unsafe { libc::getppid() } as u32;
        let started = Instant::now();
        assert!(wait_until_orphaned(parent, Duration::from_millis(100)).is_err());
        assert!(started.elapsed() >= Duration::from_millis(100));
    }
}
