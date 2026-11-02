//! Native window observations for bounded integration checks.
use super::wait_until;
use std::time::Duration;
use windows::Win32::{Foundation::*, UI::WindowsAndMessaging::*};

pub fn wait_hidden(window: HWND, timeout: Duration) -> Result<(), String> {
    let mut visible = true;
    wait_until(
        timeout,
        Duration::from_millis(5),
        "paused resident window to hide",
        || {
            // SAFETY: visibility inspection does not dereference application memory.
            visible = unsafe { IsWindowVisible(window) }.as_bool();
            !visible
        },
    )
    .map_err(|error| format!("{error}; expected visible=false, last visible={visible}"))
}

pub fn resident_window(pid: u32) -> Option<HWND> {
    struct Found {
        pid: u32,
        window: Option<HWND>,
    }
    unsafe extern "system" fn find(window: HWND, parameter: LPARAM) -> windows::core::BOOL {
        // SAFETY: EnumWindows invokes this synchronously with the caller's live state.
        unsafe {
            let found = &mut *(parameter.0 as *mut Found);
            let mut pid = 0;
            GetWindowThreadProcessId(window, Some(&mut pid));
            let mut class = [0u16; 64];
            let length = GetClassNameW(window, &mut class);
            if pid == found.pid
                && String::from_utf16_lossy(&class[..length as usize]) == "aHerald.Resident"
            {
                found.window = Some(window);
            }
        }
        true.into()
    }
    let mut found = Found { pid, window: None };
    unsafe {
        let _ = EnumWindows(Some(find), LPARAM(&mut found as *mut _ as isize));
    }
    found.window
}
