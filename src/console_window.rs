//! Finds the terminal window that hosts the process tree a hook runs in.
//!
//! A hook has no window of its own and may have no usable console, so the console of an
//! ancestor (the agent or its shell) is borrowed: a classic console window is its own owner and
//! a Windows Terminal pseudo-console window is owned by the visible terminal window.
use crate::local::Handle;
use windows::Win32::{
    System::{Console::*, Diagnostics::ToolHelp::*},
    UI::WindowsAndMessaging::*,
};

const MAX_ANCESTORS: usize = 16;

/// Parent chain of `start` in a `(pid, parent pid)` table, nearest first. Bounded, so pid reuse
/// loops and very deep trees cannot stall a hook.
pub fn ancestors(table: &[(u32, u32)], start: u32) -> Vec<u32> {
    let mut chain = Vec::new();
    let mut pid = start;
    while chain.len() < MAX_ANCESTORS {
        let Some(&(_, parent)) = table.iter().find(|(p, _)| *p == pid) else {
            break;
        };
        if parent == 0 || chain.contains(&parent) {
            break;
        }
        chain.push(parent);
        pid = parent;
    }
    chain
}

/// The visible window of this process's console session, or of an ancestor's. Detaches this
/// process from its console while probing, so call it once the hook has finished reading stdin.
pub fn current() -> Option<u64> {
    if let Some(window) = console_owner() {
        return Some(window);
    }
    let table = process_table()?;
    // SAFETY: only changes this process's console association; no handles are retained.
    unsafe {
        let _ = FreeConsole();
    }
    for pid in ancestors(&table, std::process::id()) {
        // SAFETY: as above; a failed attach leaves the process without a console.
        let attached = unsafe { AttachConsole(pid) }.is_ok();
        if attached {
            let window = console_owner();
            // SAFETY: detaches the console attached just above.
            unsafe {
                let _ = FreeConsole();
            }
            if window.is_some() {
                return window;
            }
        }
    }
    None
}

fn console_owner() -> Option<u64> {
    // SAFETY: plain window-handle queries; Win32 validates the handles.
    unsafe {
        let console = GetConsoleWindow();
        if console.is_invalid() {
            return None;
        }
        let owner = GetAncestor(console, GA_ROOTOWNER);
        let window = if owner.is_invalid() { console } else { owner };
        IsWindowVisible(window)
            .as_bool()
            .then_some(window.0 as usize as u64)
    }
}

fn process_table() -> Option<Vec<(u32, u32)>> {
    let _timer = crate::perf::span(crate::perf::Span::ProcessScan);
    // SAFETY: the snapshot handle is owned by `Handle`; the entry buffer outlives each call.
    unsafe {
        let snapshot = Handle(CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0).ok()?);
        let mut entry = PROCESSENTRY32W {
            dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };
        let mut table = Vec::new();
        if Process32FirstW(snapshot.0, &mut entry).is_ok() {
            loop {
                table.push((entry.th32ProcessID, entry.th32ParentProcessID));
                if Process32NextW(snapshot.0, &mut entry).is_err() {
                    break;
                }
            }
        }
        Some(table)
    }
}
