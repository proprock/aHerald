//! Process-level resource counters for the debug log: a leak in a long-lived resident
//! shows as a working set, GDI/USER handle or thread count that only grows.
use crate::operational_log::{self, Level, Record};
use windows::Win32::{
    Foundation::CloseHandle,
    System::{
        Diagnostics::ToolHelp::*,
        ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS},
        Threading::{GR_GDIOBJECTS, GR_USEROBJECTS, GetCurrentProcess, GetGuiResources},
    },
};

pub struct Snapshot {
    pub working_set_kb: u64,
    pub gdi: u32,
    pub user: u32,
    pub threads: u32,
}

/// Reads the counters of this process; `None` if the working set cannot be read.
pub fn snapshot() -> Option<Snapshot> {
    // SAFETY: the pseudo-handle needs no closing; the counters struct is sized and live.
    let (working_set_kb, gdi, user) = unsafe {
        let process = GetCurrentProcess();
        let mut counters = PROCESS_MEMORY_COUNTERS {
            cb: std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32,
            ..Default::default()
        };
        GetProcessMemoryInfo(process, &mut counters, counters.cb).ok()?;
        (
            counters.WorkingSetSize as u64 / 1024,
            GetGuiResources(process, GR_GDIOBJECTS),
            GetGuiResources(process, GR_USEROBJECTS),
        )
    };
    Some(Snapshot {
        working_set_kb,
        gdi,
        user,
        threads: thread_count().unwrap_or(0),
    })
}

fn thread_count() -> Option<u32> {
    let own = std::process::id();
    // SAFETY: the snapshot handle is closed below; the entry buffer outlives each call.
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0).ok()?;
        let mut entry = PROCESSENTRY32W {
            dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };
        let mut found = None;
        if Process32FirstW(snapshot, &mut entry).is_ok() {
            loop {
                if entry.th32ProcessID == own {
                    found = Some(entry.cntThreads);
                    break;
                }
                if Process32NextW(snapshot, &mut entry).is_err() {
                    break;
                }
            }
        }
        let _ = CloseHandle(snapshot);
        found
    }
}

/// Writes one `resources` line when `debug` logging is on.
pub fn log_snapshot() {
    if !operational_log::enabled(Level::Debug) {
        return;
    }
    if let Some(s) = snapshot() {
        operational_log::log(Record::Resources {
            working_set_kb: s.working_set_kb,
            gdi: s.gdi,
            user: s.user,
            threads: s.threads,
        });
    }
}
