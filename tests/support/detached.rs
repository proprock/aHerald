//! Exact process handles, verified parentage and graceful shutdown for launcher tests.
use aherald::{
    ipc::{Request, Response, pipe},
    local::Scope,
};
use std::{mem::size_of, time::Duration};
use windows::Win32::{
    Foundation::*,
    System::{Diagnostics::ToolHelp::*, Threading::*},
};
struct Handle(HANDLE);
impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}
pub struct Residents {
    scope: Scope,
    owned: Vec<(u32, Handle)>,
}
impl Residents {
    pub fn new(scope: Scope) -> Self {
        Self {
            scope,
            owned: Vec::new(),
        }
    }
    /// Adopt only a process whose parent is this test or an already retained child.
    pub fn retain(&mut self, pid: u32) -> Result<(), String> {
        if self.owned.iter().any(|(known, _)| *known == pid) {
            return Ok(());
        }
        // SAFETY: owned snapshot and process handles are closed by RAII. Holding each
        // process handle prevents PID reuse while verifying a restart's parent chain.
        unsafe {
            let process = Handle(
                OpenProcess(PROCESS_TERMINATE | PROCESS_SYNCHRONIZE, false, pid)
                    .map_err(|e| e.to_string())?,
            );
            let snapshot =
                Handle(CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0).map_err(|e| e.to_string())?);
            let mut entry = PROCESSENTRY32W {
                dwSize: size_of::<PROCESSENTRY32W>() as u32,
                ..Default::default()
            };
            let mut parent = None;
            if Process32FirstW(snapshot.0, &mut entry).is_ok() {
                loop {
                    if entry.th32ProcessID == pid {
                        parent = Some(entry.th32ParentProcessID);
                        break;
                    }
                    if Process32NextW(snapshot.0, &mut entry).is_err() {
                        break;
                    }
                }
            }
            if !parent.is_some_and(|parent| {
                parent == std::process::id() || self.owned.iter().any(|(id, _)| *id == parent)
            }) {
                return Err("refusing to own a process without verified test parentage".into());
            }
            self.owned.push((pid, process));
        }
        Ok(())
    }
    pub fn retain_current(&mut self) -> Result<(), String> {
        match pipe::request(&self.scope, &Request::Status, Duration::from_millis(300)) {
            Ok(Response::Status { pid, .. }) => self.retain(pid),
            _ => Err("expected running private resident; last status: unavailable".into()),
        }
    }
    fn retain_children(&mut self) {
        // A launcher can fail an assertion after spawning but before publishing IPC.
        // Snapshot only descendants of retained handles, never processes found by name.
        unsafe {
            let Ok(snapshot) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else {
                return;
            };
            let snapshot = Handle(snapshot);
            let mut entry = PROCESSENTRY32W {
                dwSize: size_of::<PROCESSENTRY32W>() as u32,
                ..Default::default()
            };
            if Process32FirstW(snapshot.0, &mut entry).is_ok() {
                loop {
                    if self
                        .owned
                        .iter()
                        .any(|(pid, _)| *pid == entry.th32ParentProcessID)
                    {
                        let _ = self.retain(entry.th32ProcessID);
                    }
                    if Process32NextW(snapshot.0, &mut entry).is_err() {
                        break;
                    }
                }
            }
        }
    }
    pub fn shutdown(&mut self) -> Result<(), String> {
        self.retain_children();
        let mut adoption = Ok(());
        if let Ok(Response::Status { pid, .. }) =
            pipe::request(&self.scope, &Request::Status, Duration::from_millis(200))
        {
            adoption = self.retain(pid);
            if adoption.is_ok() {
                let _ = pipe::request(&self.scope, &Request::Quit, Duration::from_millis(300));
            }
        }
        for (_, process) in &self.owned {
            // SAFETY: signal only handles retained after positive parentage evidence.
            unsafe {
                if WaitForSingleObject(process.0, 3000) == WAIT_TIMEOUT {
                    TerminateProcess(process.0, 1).map_err(|e| e.to_string())?;
                }
                if WaitForSingleObject(process.0, 2000) != WAIT_OBJECT_0 {
                    return Err("owned resident did not exit after bounded cleanup".into());
                }
            }
        }
        adoption
    }
}
impl Drop for Residents {
    fn drop(&mut self) {
        let _ = self.shutdown();
    }
}

/// Directory removal follows observed process exit, including during unwinding.
pub struct Cleanup(pub Residents, pub std::path::PathBuf);
impl Cleanup {
    pub fn new(scope: Scope, root: std::path::PathBuf) -> Self {
        Self(Residents::new(scope), root)
    }
}
impl Drop for Cleanup {
    fn drop(&mut self) {
        if self.0.shutdown().is_ok() {
            let _ = std::fs::remove_dir_all(&self.1);
        }
    }
}
