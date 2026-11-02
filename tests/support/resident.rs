//! Windows resident ownership stays in the child handle, never in a discovered PID.
use super::{OwnedChild, TempDir, wait_until};
use aherald::{
    ipc::{Request, Response, pipe},
    local::Scope,
};
use std::{
    path::PathBuf,
    process::{Command, Stdio},
    time::Duration,
};

pub struct ResidentFixture {
    pub scope: Scope,
    pub root: TempDir,
    pub config: PathBuf,
    child: Option<OwnedChild>,
}

impl ResidentFixture {
    pub fn new(tag: &str, configuration: &str) -> Self {
        let root = TempDir::new(tag);
        let scope = Scope::current(root.file_name().unwrap().to_str().unwrap()).unwrap();
        let config = root.join("config.toml");
        std::fs::write(&config, configuration).unwrap();
        Self {
            scope,
            root,
            config,
            child: None,
        }
    }

    pub fn start(&mut self) -> Result<u32, String> {
        let mut command = Command::new(env!("CARGO_BIN_EXE_aherald"));
        command
            .args(["--resident-child", &self.scope.namespace, "--config"])
            .arg(&self.config);
        self.start_command(&mut command, Duration::from_secs(5))
    }

    pub fn start_command(
        &mut self,
        command: &mut Command,
        timeout: Duration,
    ) -> Result<u32, String> {
        if self.child.is_some() {
            return Err("fixture already owns a process".into());
        }
        let mut child = OwnedChild(
            command
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .map_err(|e| format!("cannot start fixture resident: {e}"))?,
        );
        let pid = child.0.id();
        let mut exited = false;
        wait_until(
            timeout,
            Duration::from_millis(10),
            "fixture resident startup",
            || {
                exited = child.0.try_wait().is_ok_and(|status| status.is_some());
                exited
                    || matches!(pipe::request(&self.scope, &Request::Status, Duration::from_millis(50)),
                Ok(Response::Status { pid: actual, .. }) if actual == pid)
            },
        )?;
        if exited {
            return Err("fixture resident exited during startup".into());
        }
        self.child = Some(child);
        Ok(pid)
    }

    /// Graceful shutdown flushes logs; on failure only the owned child is killed and reaped.
    pub fn shutdown(&mut self) -> Result<(), String> {
        let Some(mut child) = self.child.take() else {
            return Ok(());
        };
        if child.0.try_wait().map_err(|e| e.to_string())?.is_some() {
            return Ok(());
        }
        // Do not quit a replacement process even if somebody reused our private namespace.
        if matches!(pipe::request(&self.scope, &Request::Status, Duration::from_millis(200)),
            Ok(Response::Status { pid, .. }) if pid == child.0.id())
        {
            let _ = pipe::request(&self.scope, &Request::Quit, Duration::from_millis(500));
        }
        wait_until(
            Duration::from_secs(3),
            Duration::from_millis(10),
            "fixture resident exit",
            || child.0.try_wait().is_ok_and(|status| status.is_some()),
        )
        // `child` is killed and reaped here even if the bounded graceful wait failed.
    }
}

impl Drop for ResidentFixture {
    fn drop(&mut self) {
        let _ = self.shutdown();
        // All child handles have been reaped before Rust drops the owned directory.
    }
}
