use super::{Agent, click};
use crate::{
    events::Event,
    operational_log::{Record, log},
};
use std::{path::Path, sync::mpsc, thread::JoinHandle, time::Instant};
use windows::Win32::{Foundation::*, System::Threading::*, UI::WindowsAndMessaging::*};

fn open_link(url: &str) -> Result<(), String> {
    crate::shell::open(std::ffi::OsStr::new(url)).map_err(|_| "agent_link_failed".into())
}
/// Lowercased full image path of the process that owns `hwnd`.
/// `path` is scratch space the caller reuses across windows (a long-path buffer is 64 KB).
fn window_image(hwnd: HWND, path: &mut Vec<u16>) -> Option<String> {
    // SAFETY: handles are checked by Win32; path buffer lives for the call.
    unsafe {
        let mut pid = 0;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        let process =
            crate::local::Handle(OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?);
        path.resize(32768, 0);
        let mut size = path.len() as u32;
        QueryFullProcessImageNameW(
            process.0,
            PROCESS_NAME_WIN32,
            windows::core::PWSTR(path.as_mut_ptr()),
            &mut size,
        )
        .ok()?;
        Some(String::from_utf16_lossy(&path[..size as usize]).to_ascii_lowercase())
    }
}
fn raise(hwnd: HWND) -> Result<(), String> {
    // SAFETY: the window was validated by the caller; the OS validates its lifetime.
    unsafe {
        if IsIconic(hwnd).as_bool() {
            let _ = ShowWindow(hwnd, SW_RESTORE);
        }
        if SetForegroundWindow(hwnd).as_bool() {
            Ok(())
        } else {
            Err("agent_activation_failed".into())
        }
    }
}
struct Search {
    supported: fn(&str) -> bool,
    found: HWND,
    scratch: Vec<u16>,
}
/// Raises the first visible top-level window whose executable path satisfies `supported`.
fn activate_matching(supported: fn(&str) -> bool) -> Result<(), String> {
    let mut search = Search {
        supported,
        found: HWND::default(),
        scratch: Vec::new(),
    };
    // SAFETY: callback runs synchronously; `search` outlives the enumeration.
    unsafe {
        let _ = EnumWindows(Some(find), LPARAM((&mut search as *mut Search) as isize));
    }
    if search.found.is_invalid() {
        return Err("agent_window_unavailable".into());
    }
    raise(search.found)
}
unsafe extern "system" fn find(hwnd: HWND, data: LPARAM) -> windows::core::BOOL {
    // SAFETY: `data` points to the live `Search` of the enclosing `activate_matching` call.
    unsafe {
        let search = &mut *(data.0 as *mut Search);
        if IsWindowVisible(hwnd).as_bool()
            && window_image(hwnd, &mut search.scratch).is_some_and(|path| (search.supported)(&path))
        {
            search.found = hwnd;
            return false.into();
        }
        true.into()
    }
}

struct Codex;
impl Agent for Codex {
    fn open_thread(&mut self, url: &str) -> Result<(), String> {
        open_link(url)
    }
    fn activate_window(&mut self) -> Result<(), String> {
        // The packaged Windows Codex app currently uses ChatGPT.exe. Never match ChatGPT by title alone.
        activate_matching(|path| {
            path.ends_with("\\codex.exe")
                || (path.ends_with("\\chatgpt.exe")
                    && path.contains("\\windowsapps\\openai.codex_"))
        })
    }
}

struct Claude;
impl Agent for Claude {
    fn open_thread(&mut self, url: &str) -> Result<(), String> {
        open_link(url)
    }
    fn activate_window(&mut self) -> Result<(), String> {
        // Claude Desktop (Store package); the CLI's claude.exe never owns a visible window.
        activate_matching(|path| {
            path.ends_with("\\claude.exe") && path.contains("\\windowsapps\\claude_")
        })
    }
    fn desktop_session(&mut self, cli_session_id: &str) -> Option<String> {
        let base = std::env::var_os("LOCALAPPDATA")?;
        super::claude_desktop::session_roots(Path::new(&base))
            .iter()
            .find_map(|root| super::claude_desktop::local_session_id(root, cli_session_id))
    }
    fn focus_window(&mut self, window: u64) -> Result<(), String> {
        let hwnd = HWND(window as usize as *mut std::ffi::c_void);
        // The handle came over IPC: accept only a live, visible window of a terminal host.
        // SAFETY: IsWindow/IsWindowVisible accept any handle value.
        let live = unsafe { IsWindow(Some(hwnd)).as_bool() && IsWindowVisible(hwnd).as_bool() };
        let terminal = window_image(hwnd, &mut Vec::new()).is_some_and(|path| {
            [
                "\\conhost.exe",
                "\\openconsole.exe",
                "\\windowsterminal.exe",
            ]
            .iter()
            .any(|name| path.ends_with(name))
        });
        if live && terminal {
            raise(hwnd)
        } else {
            Err("agent_window_unavailable".into())
        }
    }
}

/// One owned worker and one pending click; the GUI never waits for shell or process queries.
#[derive(Default)]
pub struct Worker {
    sender: Option<mpsc::SyncSender<(Event, Instant)>>,
    handle: Option<JoinHandle<()>>,
}
impl Worker {
    pub fn submit(
        &mut self,
        event: &Event,
        enabled: bool,
        body: bool,
        directory: &Path,
        level: crate::config::LogLevel,
    ) {
        if !enabled || !body || event.source.agent().is_none() {
            return;
        }
        if self.sender.is_none() {
            let (tx, rx) = mpsc::sync_channel::<(Event, Instant)>(1);
            crate::operational_log::init(directory, level);
            match std::thread::Builder::new()
                .name("agent-activation".into())
                .spawn(move || {
                    for (event, clicked) in rx {
                        let result = match event.source.agent() {
                            Some(crate::agent::Agent::Codex) => {
                                click(&event, true, true, &mut Codex)
                            }
                            Some(crate::agent::Agent::Claude) => {
                                click(&event, true, true, &mut Claude)
                            }
                            None => Ok(()),
                        };
                        log(if result.is_err() {
                            Record::AgentActivationFailure
                        } else {
                            Record::AgentActivated {
                                millis: clicked.elapsed().as_millis() as u64,
                            }
                        });
                    }
                }) {
                Ok(handle) => {
                    self.sender = Some(tx);
                    self.handle = Some(handle);
                }
                Err(_) => log(Record::AgentActivationFailure),
            }
        }
        if let Some(tx) = &self.sender {
            let _ = tx.try_send((event.clone(), Instant::now()));
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
