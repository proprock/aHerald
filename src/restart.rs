//! A detached replacement waits for the old process, including all worker cleanup.
use std::{
    ffi::{OsStr, OsString},
    os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle},
};
use windows::Win32::{
    Foundation::{HANDLE, WAIT_OBJECT_0},
    System::Threading::*,
};
pub fn launch() -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| format!("cannot locate executable: {e}"))?;
    let mut args = vec![
        OsString::from("--restart-child"),
        OsString::from(std::process::id().to_string()),
    ];
    let mut original: Vec<OsString> = std::env::args_os().skip(1).collect();
    if original.first().is_some_and(|a| a == "--restart-child") {
        original.drain(..2);
    }
    args.extend(original);
    let borrowed: Vec<&OsStr> = args.iter().map(OsString::as_os_str).collect();
    use std::io::Read;
    let mut child = crate::detached::spawn(&exe, &borrowed, true)?;
    let mut output = child
        .stdout
        .take()
        .ok_or("restart child has no acknowledgement pipe")?;
    let (tx, rx) = std::sync::mpsc::sync_channel(1);
    std::thread::spawn(move || {
        let mut b = [0];
        let _ = tx.send(output.read_exact(&mut b).is_ok() && b == *b"R");
    });
    if rx.recv_timeout(std::time::Duration::from_millis(750)) == Ok(true) {
        Ok(())
    } else {
        let _ = child.kill();
        let _ = child.wait();
        Err("replacement failed to prepare restart".into())
    }
}
pub fn wait_for_parent(pid: u32) -> Result<(), String> {
    // SAFETY: only synchronization rights are requested, and OwnedHandle closes the result.
    let handle = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, false, pid) }
        .map_err(|e| format!("cannot wait for previous aHerald process: {e}"))?;
    let handle = unsafe { OwnedHandle::from_raw_handle(handle.0) };
    crate::process::acknowledge()?;
    match unsafe { WaitForSingleObject(HANDLE(handle.as_raw_handle()), 30_000) } {
        WAIT_OBJECT_0 => Ok(()),
        _ => Err("previous aHerald process did not exit within 30 seconds".into()),
    }
}
