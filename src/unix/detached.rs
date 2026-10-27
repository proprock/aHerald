//! Detached children: a new session, no controlling terminal and no inherited descriptors
//! beyond the explicitly piped standard streams.
use std::{
    ffi::OsStr,
    os::unix::process::CommandExt,
    path::Path,
    process::{ChildStdin, ChildStdout, Command, ExitStatus, Stdio},
};

pub struct Child {
    process: std::process::Child,
    pub(crate) stdin: Option<ChildStdin>,
    pub(crate) stdout: Option<ChildStdout>,
}
impl Child {
    pub fn id(&self) -> u32 {
        self.process.id()
    }
    pub fn kill(&mut self) -> Result<(), String> {
        self.process
            .kill()
            .map_err(|e| format!("cannot terminate notification child: {e}"))
    }
    pub fn try_wait(&mut self) -> Result<Option<ExitStatus>, String> {
        self.process
            .try_wait()
            .map_err(|e| format!("cannot read child exit: {e}"))
    }
    pub fn wait(&mut self) -> Result<ExitStatus, String> {
        self.process
            .wait()
            .map_err(|e| format!("cannot wait for notification child: {e}"))
    }
}

/// Starts `executable` detached. With `transfer`, stdin and stdout are pipes for the one
/// bounded hand-off; otherwise the child gets no standard streams. stderr is always closed.
pub(crate) fn spawn(executable: &Path, args: &[&OsStr], transfer: bool) -> Result<Child, String> {
    let mut command = Command::new(executable);
    command.args(args).stderr(Stdio::null());
    if transfer {
        command.stdin(Stdio::piped()).stdout(Stdio::piped());
    } else {
        command.stdin(Stdio::null()).stdout(Stdio::null());
    }
    // SAFETY: the closure runs between fork and exec and calls only async-signal-safe
    // functions (`setsid`, `fcntl`); it allocates nothing and takes no locks.
    unsafe {
        command.pre_exec(|| {
            if libc::setsid() < 0 {
                return Err(std::io::Error::last_os_error());
            }
            // Rust opens its own descriptors close-on-exec; this covers any a library or the
            // launching shell left inheritable. Marking rather than closing keeps the pipe
            // `std` uses to report an exec failure intact.
            for descriptor in 3..4096 {
                let flags = libc::fcntl(descriptor, libc::F_GETFD);
                if flags >= 0 {
                    libc::fcntl(descriptor, libc::F_SETFD, flags | libc::FD_CLOEXEC);
                }
            }
            Ok(())
        });
    }
    let mut process = command
        .spawn()
        .map_err(|_| "cannot launch notification child".to_owned())?;
    Ok(Child {
        stdin: process.stdin.take(),
        stdout: process.stdout.take(),
        process,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_child_leads_its_own_session() {
        let mut child = spawn(Path::new("sleep"), &[OsStr::new("5")], false).unwrap();
        // SAFETY: `getsid` only reads kernel state for the given pid.
        let session = unsafe { libc::getsid(child.id() as libc::pid_t) };
        let id = child.id() as libc::pid_t;
        let _ = child.kill();
        let _ = child.wait();
        assert_eq!(session, id);
    }

    #[test]
    fn an_inheritable_descriptor_does_not_reach_the_child() {
        // `dup` returns a descriptor that is not close-on-exec.
        // SAFETY: duplicating stdin is valid and the copy is closed below.
        let leaked = unsafe { libc::dup(0) };
        assert!(leaked > 2);
        let number = leaked.to_string();
        let mut child = spawn(
            Path::new("sh"),
            &[
                OsStr::new("-c"),
                OsStr::new(": >&\"$0\""),
                OsStr::new(&number),
            ],
            true,
        )
        .unwrap();
        let status = child.wait().unwrap();
        // SAFETY: closes the descriptor created above, once.
        unsafe { libc::close(leaked) };
        assert!(
            !status.success(),
            "the child could still use descriptor {leaked}"
        );
    }

    #[test]
    fn a_missing_executable_is_reported() {
        assert!(spawn(Path::new("/nonexistent/aherald-test"), &[], false).is_err());
    }
}
