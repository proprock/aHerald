use crate::events::Event;
use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::mpsc,
    time::Duration,
};
pub const TRANSFER_LIMIT: usize = 2048;
const STARTUP_TIMEOUT: Duration = Duration::from_millis(750);

/// Only sanitized event data crosses the private, inherited anonymous pipe.
/// The child never inherits the hook's output handles or console.
pub fn deliver(executable: &Path, config: &Path, event: &Event) -> Result<Child, String> {
    event.validate()?;
    let payload = serde_json::to_vec(event).map_err(|_| "cannot encode notification")?;
    if payload.len() > TRANSFER_LIMIT {
        return Err("notification transfer exceeds limit".into());
    }
    let executable = executable.to_owned();
    let config = std::path::absolute(config).map_err(|_| "cannot resolve child configuration")?;
    let (tx, rx) = mpsc::sync_channel(1);
    std::thread::spawn(move || {
        let result = spawn_ready(executable, config, payload);
        if let Err(mpsc::SendError(Ok(mut child))) = tx.send(result) {
            let _ = child.kill();
            let _ = child.wait();
        }
    });
    rx.recv_timeout(STARTUP_TIMEOUT)
        .map_err(|_| "notification startup deadline exceeded".to_owned())?
}

fn spawn_ready(executable: PathBuf, config: PathBuf, payload: Vec<u8>) -> Result<Child, String> {
    let mut command = Command::new(executable);
    command
        .arg("--display-child")
        .arg("--config")
        .arg(config)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP; no console window.
        command.creation_flags(0x0000_0008 | 0x0000_0200);
    }
    let mut child = command
        .spawn()
        .map_err(|_| "cannot launch notification child")?;
    let transfer = (|| {
        let mut input = child.stdin.take().ok_or("missing child input")?;
        // Below the 4 KiB Windows anonymous-pipe buffer; one bounded initial write.
        input
            .write_all(&payload)
            .map_err(|_| "cannot transfer notification")?;
        drop(input);
        let mut output = child.stdout.take().ok_or("missing child acknowledgement")?;
        let (tx, rx) = mpsc::sync_channel(1);
        std::thread::spawn(move || {
            let mut ready = [0];
            let result = output.read_exact(&mut ready).map(|_| ready[0] == b'R');
            let _ = tx.send(result);
        });
        match rx.recv_timeout(Duration::from_millis(500)) {
            Ok(Ok(true)) => Ok(()),
            _ => Err("notification child failed readiness deadline"),
        }
    })();
    if let Err(error) = transfer {
        let _ = child.kill();
        let _ = child.wait();
        return Err(error.into());
    }
    Ok(child)
}

pub fn acknowledge() -> Result<(), String> {
    let mut stdout = std::io::stdout().lock();
    stdout
        .write_all(b"R")
        .and_then(|_| stdout.flush())
        .map_err(|_| "cannot acknowledge notification".into())
}
