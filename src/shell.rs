//! User-requested shell actions, without invoking a command interpreter.
use std::path::Path;
use windows::{
    Win32::UI::{Shell::ShellExecuteW, WindowsAndMessaging::SW_SHOWNORMAL},
    core::{PCWSTR, w},
};
pub fn open(target: &std::ffi::OsStr) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    let target: Vec<u16> = target.encode_wide().chain(Some(0)).collect();
    // SAFETY: terminated strings outlive this synchronous shell call; no parameters are interpreted as code.
    let code = unsafe {
        ShellExecuteW(
            None,
            w!("open"),
            PCWSTR(target.as_ptr()),
            None,
            None,
            SW_SHOWNORMAL,
        )
    }
    .0 as isize;
    if code > 32 {
        Ok(())
    } else {
        Err(format!(
            "the shell refused to open the target (code {code})"
        ))
    }
}
/// Opens the configuration in the associated program, or in Notepad when the shell cannot
/// (no program for `.toml`, a handler that is gone, or any other refusal).
pub fn edit_configuration(path: &Path) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    if open(path.as_os_str()).is_ok() {
        return Ok(());
    }
    let editor = std::env::var_os("SystemRoot")
        .map(std::path::PathBuf::from)
        .ok_or("Cannot open the configuration: SystemRoot is not set")?
        .join("System32/notepad.exe");
    std::process::Command::new(editor)
        .arg(path)
        .creation_flags(0x08000000)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("Cannot open the configuration in Notepad: {e}"))
}
