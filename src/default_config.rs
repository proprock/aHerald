//! Publish a complete default without replacing an existing user file.
use std::{
    fs::OpenOptions,
    io::Write,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Temporary(PathBuf);
impl Drop for Temporary {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}
pub(crate) fn create(path: &Path) -> Result<(), String> {
    let result = (|| -> std::io::Result<()> {
        // Application Support/aHerald does not exist on the first start of a bundle.
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent)?;
        }
        let mut name = path
            .file_name()
            .ok_or_else(|| std::io::Error::other("configuration has no filename"))?
            .to_os_string();
        name.push(format!(
            ".{}.{}.tmp",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let temporary = path.with_file_name(name);
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)?;
        let temporary = Temporary(temporary);
        file.write_all(include_bytes!("../aHerald.example.toml"))?;
        file.sync_all()?;
        drop(file);
        match publish(&temporary.0, path) {
            Ok(()) => Ok(()),
            // Another starter or the user published a file first. Never replace it.
            Err(_) if path.try_exists()? => Ok(()),
            Err(e) => Err(e),
        }
    })();
    result.map_err(|e| {
        format!(
            "cannot create default configuration {}: {e}",
            path.display()
        )
    })
}
#[cfg(windows)]
fn publish(temporary: &Path, path: &Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows::{
        Win32::Storage::FileSystem::{MOVEFILE_WRITE_THROUGH, MoveFileExW},
        core::PCWSTR,
    };
    let from: Vec<u16> = temporary.as_os_str().encode_wide().chain(Some(0)).collect();
    let to: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    // SAFETY: terminated paths live through the call. No REPLACE_EXISTING flag: publication is exclusive.
    unsafe {
        MoveFileExW(
            PCWSTR(from.as_ptr()),
            PCWSTR(to.as_ptr()),
            MOVEFILE_WRITE_THROUGH,
        )
    }
    .map_err(|e| std::io::Error::from_raw_os_error((e.code().0 as u32 & 0xffff) as i32))
}
#[cfg(not(windows))]
fn publish(temporary: &Path, path: &Path) -> std::io::Result<()> {
    std::fs::hard_link(temporary, path)
}
