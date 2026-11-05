//! One way to write a file that must never be seen half-written: a sibling temporary file, flushed to
//! disk, then renamed over the target (AH-243).
use std::{
    fs::OpenOptions,
    io::{self, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT: AtomicU64 = AtomicU64::new(0);

/// A sibling temporary file that is removed when dropped, whether or not it was published.
pub(crate) struct Temporary(pub(crate) PathBuf);

impl Drop for Temporary {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// A unique name beside `path`, for the temporary file of a write to it.
pub(crate) fn temporary_sibling(path: &Path) -> io::Result<PathBuf> {
    let mut name = path
        .file_name()
        .ok_or_else(|| io::Error::other("the file has no name"))?
        .to_os_string();
    name.push(format!(
        ".{}.{}.tmp",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    Ok(path.with_file_name(name))
}

/// Moves `temporary` to `path`. With `replace` an existing file is replaced atomically; without it
/// the move fails if `path` exists, so a file somebody else published first is never overwritten.
pub(crate) fn publish(temporary: &Path, path: &Path, replace: bool) -> io::Result<()> {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows::{
            Win32::Storage::FileSystem::{
                MOVE_FILE_FLAGS, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
            },
            core::PCWSTR,
        };
        let from: Vec<u16> = temporary.as_os_str().encode_wide().chain(Some(0)).collect();
        let to: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        let mut flags: MOVE_FILE_FLAGS = MOVEFILE_WRITE_THROUGH;
        if replace {
            flags |= MOVEFILE_REPLACE_EXISTING;
        }
        // SAFETY: both paths are NUL-terminated and live through the call.
        unsafe { MoveFileExW(PCWSTR(from.as_ptr()), PCWSTR(to.as_ptr()), flags) }
            .map_err(|e| io::Error::from_raw_os_error((e.code().0 as u32 & 0xffff) as i32))
    }
    #[cfg(not(windows))]
    {
        if replace {
            std::fs::rename(temporary, path)
        } else {
            // A hard link fails if the name exists; the temporary name is dropped by the caller.
            std::fs::hard_link(temporary, path)
        }
    }
}

/// Writes `bytes` to `path` atomically. An existing file keeps its permissions and, through a
/// symlink, stays a symlink (the target is replaced). With `backup`, the old content is first copied
/// to `<name>.aherald-backup`. Missing parent directories are created.
pub(crate) fn write(path: &Path, bytes: &[u8], backup: bool) -> io::Result<()> {
    let exists = path.try_exists()?;
    let target = if exists {
        std::fs::canonicalize(path)?
    } else {
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent)?;
        }
        path.to_path_buf()
    };
    if exists && backup {
        let mut name = target
            .file_name()
            .ok_or_else(|| io::Error::other("the file has no name"))?
            .to_os_string();
        name.push(".aherald-backup");
        std::fs::copy(&target, target.with_file_name(name))?;
    }
    let temporary = Temporary(temporary_sibling(&target)?);
    {
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary.0)?;
        file.write_all(bytes)?;
        file.sync_all()?;
    }
    if exists && let Ok(meta) = std::fs::metadata(&target) {
        let _ = std::fs::set_permissions(&temporary.0, meta.permissions());
    }
    publish(&temporary.0, &target, true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    struct Dir(PathBuf);
    impl Dir {
        fn new(tag: &str) -> Self {
            let path = std::env::temp_dir().join(format!("ah-atomic-{tag}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn names(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn a_new_file_is_created_with_its_parents_and_leaves_nothing_else() {
        let dir = Dir::new("new");
        let file = dir.0.join("a/b/settings.json");
        write(&file, b"one", true).unwrap();
        assert_eq!(fs::read(&file).unwrap(), b"one");
        // Nothing to back up for a new file, and no temporary file stays.
        assert_eq!(names(file.parent().unwrap()), ["settings.json"]);
    }

    #[test]
    fn an_existing_file_is_replaced_and_the_old_content_is_kept_when_asked() {
        let dir = Dir::new("replace");
        let file = dir.0.join("settings.json");
        fs::write(&file, b"old").unwrap();
        write(&file, b"new", true).unwrap();
        assert_eq!(fs::read(&file).unwrap(), b"new");
        assert_eq!(
            fs::read(dir.0.join("settings.json.aherald-backup")).unwrap(),
            b"old"
        );
        assert_eq!(
            names(&dir.0),
            ["settings.json", "settings.json.aherald-backup"]
        );

        write(&file, b"newer", false).unwrap();
        assert_eq!(fs::read(&file).unwrap(), b"newer");
        assert_eq!(
            fs::read(dir.0.join("settings.json.aherald-backup")).unwrap(),
            b"old",
            "a write without backup leaves the earlier backup alone"
        );
    }

    #[test]
    fn a_failed_publish_leaves_the_target_and_no_temporary_file() {
        let dir = Dir::new("fail");
        // A directory cannot be replaced by a file: the rename fails after the data was written.
        let target = dir.0.join("target");
        fs::create_dir(&target).unwrap();
        fs::write(target.join("keep"), b"x").unwrap();
        assert!(write(&target, b"data", false).is_err());
        assert_eq!(names(&dir.0), ["target"]);
        assert_eq!(fs::read(target.join("keep")).unwrap(), b"x");
    }

    #[test]
    fn exclusive_publication_never_replaces_a_file_that_is_already_there() {
        let dir = Dir::new("exclusive");
        let file = dir.0.join("config.toml");
        fs::write(&file, b"user").unwrap();
        let temporary = Temporary(temporary_sibling(&file).unwrap());
        fs::write(&temporary.0, b"default").unwrap();
        assert!(publish(&temporary.0, &file, false).is_err());
        assert_eq!(fs::read(&file).unwrap(), b"user");
        let free = dir.0.join("free.toml");
        publish(&temporary.0, &free, false).unwrap();
        assert_eq!(fs::read(&free).unwrap(), b"default");
    }

    #[test]
    fn temporary_names_are_unique_siblings() {
        let file = Path::new("/x/y/settings.json");
        let (a, b) = (
            temporary_sibling(file).unwrap(),
            temporary_sibling(file).unwrap(),
        );
        assert_ne!(a, b);
        assert_eq!(a.parent(), file.parent());
        assert!(
            a.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("settings.json.")
        );
    }

    #[cfg(unix)]
    #[test]
    fn permissions_survive_and_a_symlink_stays_a_symlink() {
        use std::os::unix::fs::{PermissionsExt, symlink};
        let dir = Dir::new("unix");
        let real = dir.0.join("real.json");
        fs::write(&real, b"old").unwrap();
        fs::set_permissions(&real, fs::Permissions::from_mode(0o640)).unwrap();
        let link = dir.0.join("link.json");
        symlink(&real, &link).unwrap();
        write(&link, b"new", false).unwrap();
        assert!(
            fs::symlink_metadata(&link)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(fs::read(&real).unwrap(), b"new");
        assert_eq!(
            fs::metadata(&real).unwrap().permissions().mode() & 0o777,
            0o640
        );
    }
}
