//! Talking to the resident from a hook or the CLI: start it on demand and hand it an event.
//! Nothing here waits for a card to be shown, and every wait is bounded.
use crate::{
    events::Event,
    ipc::{Request, Response, pipe},
    local::Scope,
};
use std::{
    path::Path,
    time::{Duration, Instant},
};

pub fn ensure(exe: &Path, config: &Path, scope: &Scope) -> Result<Response, String> {
    if let Ok(status @ Response::Status { .. }) =
        pipe::request(scope, &Request::Status, Duration::from_millis(30))
    {
        return Ok(status);
    }
    let mut child = crate::detached::spawn(
        exe,
        &[
            std::ffi::OsStr::new("--resident-child"),
            std::ffi::OsStr::new(&scope.namespace),
            std::ffi::OsStr::new("--config"),
            config.as_os_str(),
        ],
        false,
    )?;
    let deadline = Instant::now() + Duration::from_millis(1500);
    while Instant::now() < deadline {
        if let Ok(status @ Response::Status { .. }) =
            pipe::request(scope, &Request::Status, Duration::from_millis(80))
        {
            if let Response::Status { pid, .. } = &status
                && *pid != child.id()
            {
                // A concurrent launch won. Reap our own loser before it can start
                // late and acquire ownership after that resident has been quit.
                let _ = child.kill();
                let _ = child.wait();
            }
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

/// Sends `event` to a resident that is already running; with none, there is nothing to
/// tell and none is started (for example on a session end, where only waits are cleared).
pub fn forward_if_running(scope: &Scope, event: &Event) -> Result<(), String> {
    match pipe::request(
        scope,
        &Request::Event {
            event: event.clone(),
        },
        Duration::from_millis(150),
    ) {
        Ok(Response::Accepted) | Err(_) => Ok(()),
        Ok(_) => Err("resident event queue rejected notification".into()),
    }
}
