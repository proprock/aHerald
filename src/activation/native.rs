use super::{Agent, click};
use crate::{
    events::{Event, Source},
    operational_log::{Log, Record},
};
use std::{path::Path, sync::mpsc, thread::JoinHandle};
use windows::Win32::{Foundation::*, System::Threading::*, UI::WindowsAndMessaging::*};

struct Codex;
impl Agent for Codex {
    fn open_thread(&mut self, url: &str) -> Result<(), String> {
        crate::shell::open(std::ffi::OsStr::new(url)).map_err(|_| "agent_link_failed".into())
    }
    fn activate_window(&mut self) -> Result<(), String> {
        let mut found = HWND::default();
        // SAFETY: callback runs synchronously; its output remains valid throughout enumeration.
        unsafe {
            let _ = EnumWindows(Some(find), LPARAM((&mut found as *mut HWND) as isize));
        }
        if found.is_invalid() {
            return Err("agent_window_unavailable".into());
        }
        // SAFETY: a visible top-level window was found in this desktop; OS validates its lifetime.
        unsafe {
            if IsIconic(found).as_bool() {
                let _ = ShowWindow(found, SW_RESTORE);
            }
            if SetForegroundWindow(found).as_bool() {
                Ok(())
            } else {
                Err("agent_activation_failed".into())
            }
        }
    }
}
unsafe extern "system" fn find(hwnd: HWND, data: LPARAM) -> windows::core::BOOL {
    // SAFETY: handles are checked by Win32; path buffer and output pointer live for the call.
    unsafe {
        if !IsWindowVisible(hwnd).as_bool() {
            return true.into();
        }
        let mut pid = 0;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        let Ok(process) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else {
            return true.into();
        };
        let process = crate::local::Handle(process);
        let mut path = vec![0u16; 32768];
        let mut size = path.len() as u32;
        if QueryFullProcessImageNameW(
            process.0,
            PROCESS_NAME_WIN32,
            windows::core::PWSTR(path.as_mut_ptr()),
            &mut size,
        )
        .is_err()
        {
            return true.into();
        }
        let path = String::from_utf16_lossy(&path[..size as usize]).to_ascii_lowercase();
        // The packaged Windows Codex app currently uses ChatGPT.exe. Never match ChatGPT by title alone.
        let supported = path.ends_with("\\codex.exe")
            || (path.ends_with("\\chatgpt.exe") && path.contains("\\windowsapps\\openai.codex_"));
        if supported {
            *(data.0 as *mut HWND) = hwnd;
            false.into()
        } else {
            true.into()
        }
    }
}

/// One owned worker and one pending click; the GUI never waits for shell or process queries.
#[derive(Default)]
pub struct Worker {
    sender: Option<mpsc::SyncSender<Event>>,
    handle: Option<JoinHandle<()>>,
}
impl Worker {
    pub fn submit(
        &mut self,
        event: &Event,
        enabled: bool,
        body: bool,
        directory: &Path,
        level: &str,
    ) {
        if !enabled || !body || event.source != Source::CodexHook {
            return;
        }
        if self.sender.is_none() {
            let (tx, rx) = mpsc::sync_channel::<Event>(1);
            let log_directory = directory.to_owned();
            let log_level = level.to_owned();
            match std::thread::Builder::new()
                .name("agent-activation".into())
                .spawn(move || {
                    let mut log = Log::new(&log_directory, &log_level);
                    for event in rx {
                        if click(&event, true, true, &mut Codex).is_err() {
                            let _ = log.record(Record::AgentActivationFailure);
                        }
                    }
                }) {
                Ok(handle) => {
                    self.sender = Some(tx);
                    self.handle = Some(handle);
                }
                Err(_) => {
                    let _ = Log::new(directory, level).record(Record::AgentActivationFailure);
                }
            }
        }
        if let Some(tx) = &self.sender {
            let _ = tx.try_send(event.clone());
        }
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        self.sender.take();
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}
