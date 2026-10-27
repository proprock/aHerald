//! Per-user local resources for the resident process on Unix: a `flock`-guarded lock file
//! and a unix-domain socket inside a private directory.
//!
//! Isolation is per user id. Unix has no logon-session identity to add to it.
use std::{
    fs::{File, OpenOptions},
    io::ErrorKind,
    os::unix::{
        fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt},
        io::AsRawFd,
        net::{UnixListener, UnixStream},
    },
    path::{Path, PathBuf},
};

/// Longest socket path accepted. `sun_path` holds 104 bytes on macOS and 108 on Linux,
/// including the terminating NUL.
const MAX_SOCKET_PATH: usize = 100;
/// Namespaces up to this length name the socket file as they are.
const MAX_PLAIN_NAME: usize = 24;

fn effective_uid() -> u32 {
    // SAFETY: `geteuid` takes no arguments, has no preconditions and cannot fail.
    unsafe { libc::geteuid() }
}

/// FNV-1a keeps the socket name of a long namespace short and stable without a hash crate.
fn fingerprint(text: &str) -> u32 {
    text.bytes().fold(0x811c_9dc5u32, |hash, byte| {
        (hash ^ u32::from(byte)).wrapping_mul(0x0100_0193)
    })
}

#[derive(Clone, Debug)]
pub struct Scope {
    pub namespace: String,
    socket: PathBuf,
    lock: PathBuf,
    expected_uid: u32,
}

impl Scope {
    pub fn current(namespace: &str) -> Result<Self, String> {
        if namespace.is_empty()
            || namespace.len() > 64
            || !namespace
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-')
        {
            return Err("invalid instance namespace".into());
        }
        let uid = effective_uid();
        let directory = runtime_directory(uid)?;
        let name = if namespace.len() <= MAX_PLAIN_NAME {
            namespace.to_owned()
        } else {
            format!("{}-{:08x}", &namespace[..15], fingerprint(namespace))
        };
        let socket = directory.join(format!("{name}.sock"));
        if socket.as_os_str().len() > MAX_SOCKET_PATH {
            return Err("local socket path is too long; shorten TMPDIR".into());
        }
        Ok(Self {
            namespace: namespace.into(),
            lock: directory.join(format!("{name}.lock")),
            socket,
            expected_uid: uid,
        })
    }

    pub fn socket_path(&self) -> &Path {
        &self.socket
    }

    /// Accepts only peers of this user id. Naming another uid makes every real peer foreign,
    /// which is how the rejection path is exercised without a second account.
    pub fn expect_peer_uid(&mut self, uid: u32) {
        self.expected_uid = uid;
    }

    /// `Ok(None)` when another process owns this scope. The lock is tied to the open file
    /// description, so it is released when the owner exits, however it exits.
    pub fn acquire(&self) -> Result<Option<Ownership>, String> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .open(&self.lock)
            .map_err(|_| "cannot open resident lock")?;
        // SAFETY: the descriptor stays open for the duration of the call.
        if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0 {
            return Ok(Some(Ownership { _file: file }));
        }
        match std::io::Error::last_os_error().kind() {
            ErrorKind::WouldBlock => Ok(None),
            _ => Err("cannot inspect resident lock".into()),
        }
    }

    /// Binds the listening socket. The caller holds the lock from `acquire`, so a socket file
    /// that refuses connections is the leftover of a dead resident and is replaced.
    pub fn create_pipe(&self) -> Result<PipeServer, String> {
        if UnixStream::connect(&self.socket).is_ok() {
            return Err("a resident is already listening".into());
        }
        match std::fs::remove_file(&self.socket) {
            Ok(()) => {}
            Err(e) if e.kind() == ErrorKind::NotFound => {}
            Err(_) => return Err("cannot replace stale resident socket".into()),
        }
        let listener = UnixListener::bind(&self.socket)
            .map_err(|_| "cannot create local resident socket".to_owned())?;
        // The directory is already 0700; the socket is narrowed as well.
        std::fs::set_permissions(&self.socket, std::fs::Permissions::from_mode(0o600))
            .map_err(|_| "cannot restrict local resident socket")?;
        listener
            .set_nonblocking(true)
            .map_err(|_| "cannot configure local resident socket")?;
        Ok(PipeServer {
            listener,
            expected_uid: self.expected_uid,
        })
    }
}

/// Holds the instance lock until dropped.
pub struct Ownership {
    _file: File,
}

pub struct PipeServer {
    pub(crate) listener: UnixListener,
    pub(crate) expected_uid: u32,
}

/// `$XDG_RUNTIME_DIR/aherald` when the session provides one, else `<tmp>/aherald-<uid>`. The
/// directory must be a real directory (not a symlink) owned by this user; its mode is
/// forced to 0700.
fn runtime_directory(uid: u32) -> Result<PathBuf, String> {
    let directory = match std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from) {
        Some(base) if base.is_absolute() => base.join("aherald"),
        _ => std::env::temp_dir().join(format!("aherald-{uid}")),
    };
    match std::fs::DirBuilder::new().mode(0o700).create(&directory) {
        Ok(()) => {}
        Err(e) if e.kind() == ErrorKind::AlreadyExists => {}
        Err(_) => return Err("cannot create local resident directory".into()),
    }
    // `symlink_metadata` does not follow a link planted at the path.
    let metadata = std::fs::symlink_metadata(&directory)
        .map_err(|_| "cannot inspect local resident directory")?;
    if !metadata.is_dir() || metadata.uid() != uid {
        return Err("local resident directory belongs to another user".into());
    }
    if metadata.mode() & 0o077 != 0 {
        std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700))
            .map_err(|_| "cannot restrict local resident directory")?;
    }
    Ok(directory)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn long_namespaces_get_a_short_stable_socket_name() {
        let long = "a".repeat(64);
        let first = Scope::current(&long).unwrap();
        let second = Scope::current(&long).unwrap();
        assert_eq!(first.socket_path(), second.socket_path());
        let other = Scope::current(&format!("{}b", "a".repeat(63))).unwrap();
        assert_ne!(first.socket_path(), other.socket_path());
        let name = first.socket_path().file_name().unwrap().len();
        assert!(name < 40, "{name}");
    }
}
