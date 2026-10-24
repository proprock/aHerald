//! Resident lifecycle; IPC never waits for notification presentation.
use crate::{
    config::Config,
    events::Event,
    ipc::{Request, Response, pipe},
    local::Scope,
};
use std::{
    path::Path,
    process::{Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

pub fn ensure(exe: &Path, config: &Path, scope: &Scope) -> Result<Response, String> {
    if let Ok(status @ Response::Status { .. }) =
        pipe::request(scope, &Request::Status, Duration::from_millis(30))
    {
        return Ok(status);
    }
    let mut command = Command::new(exe);
    command
        .args(["--resident-child", &scope.namespace, "--config"])
        .arg(config)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    use std::os::windows::process::CommandExt;
    command.creation_flags(0x00000008 | 0x00000200);
    let mut child = command.spawn().map_err(|_| "cannot start resident")?;
    let deadline = Instant::now() + Duration::from_millis(1500);
    while Instant::now() < deadline {
        if let Ok(status @ Response::Status { .. }) =
            pipe::request(scope, &Request::Status, Duration::from_millis(80))
        {
            return Ok(status);
        }
        if let Some(status) = child
            .try_wait()
            .map_err(|_| "cannot inspect resident startup")?
            && !status.success()
        {
            return Err("resident startup failed".into());
        }
    }
    let _ = child.kill();
    let _ = child.wait();
    Err("resident startup deadline exceeded".into())
}
pub fn forward(exe: &Path, config: &Path, scope: &Scope, event: &Event) -> Result<(), String> {
    ensure(exe, config, scope)?;
    match pipe::request(
        scope,
        &Request::Event {
            event: event.clone(),
        },
        Duration::from_millis(150),
    )? {
        Response::Accepted => Ok(()),
        _ => Err("resident event queue rejected notification".into()),
    }
}
pub fn run(scope: Scope, config: Config, path: &Path) -> Result<(), String> {
    let Some(_owner) = scope.acquire()? else {
        return Ok(());
    };
    let stop = Arc::new(AtomicBool::new(false));
    let (tx, rx) = mpsc::sync_channel::<Event>(64);
    let (ready_tx, ready_rx) = mpsc::sync_channel(1);
    let shutdown = stop.clone();
    let listener = std::thread::spawn(move || {
        let server = match scope.create_pipe() {
            Ok(s) => s,
            Err(e) => {
                let _ = ready_tx.send(Err(e));
                return;
            }
        };
        let _ = ready_tx.send(Ok(()));
        while !shutdown.load(Ordering::Acquire) {
            let _ = server.serve_once(|request| match request {
                Request::Status => Response::Status {
                    pid: std::process::id(),
                    queued: 0,
                    paused: false,
                },
                Request::Quit => {
                    shutdown.store(true, Ordering::Release);
                    Response::Bye
                }
                Request::Event { event } => {
                    if event.validate().is_ok() && tx.try_send(event).is_ok() {
                        Response::Accepted
                    } else {
                        Response::Rejected
                    }
                }
            });
        }
    });
    let startup = ready_rx
        .recv_timeout(Duration::from_secs(1))
        .map_err(|_| "resident listener startup deadline exceeded")?;
    if let Err(error) = startup {
        stop.store(true, Ordering::Release);
        let _ = listener.join();
        return Err(error);
    }
    let exe = std::env::current_exe().map_err(|_| "cannot find resident executable")?;
    let mut children = Vec::new();
    while !stop.load(Ordering::Acquire) {
        if let Ok(event) = rx.recv_timeout(Duration::from_millis(20))
            && config.events[event.kind.name()].enabled
            && (config.notifications.enabled || config.sounds.enabled)
            && let Ok(child) = crate::process::deliver(&exe, path, &event)
        {
            children.push(child);
        }
        children.retain_mut(|child| matches!(child.try_wait(), Ok(None)));
    }
    for child in &mut children {
        let _ = child.kill();
        let _ = child.wait();
    }
    let _ = listener.join();
    Ok(())
}
