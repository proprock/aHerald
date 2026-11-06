//! Local user/logon-scoped native resources for the resident process.
use std::{marker::PhantomData, rc::Rc};
use windows::{
    Win32::{
        Foundation::*,
        Security::{Authorization::*, *},
        Storage::FileSystem::*,
        System::{Pipes::*, Threading::*},
    },
    core::{PCWSTR, PWSTR},
};

pub(crate) fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}
pub(crate) struct Handle(pub HANDLE);
impl Drop for Handle {
    fn drop(&mut self) {
        // SAFETY: `self.0` is the handle this struct owns and closes exactly once, here.
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}
struct Local(PSECURITY_DESCRIPTOR);
impl Drop for Local {
    fn drop(&mut self) {
        // SAFETY: `self.0` is memory that Windows allocated with LocalAlloc for this value (a
        // security descriptor) and nothing else frees it.
        unsafe {
            let _ = LocalFree(Some(HLOCAL(self.0.0)));
        }
    }
}
fn token_data(token: HANDLE, class: TOKEN_INFORMATION_CLASS) -> Result<Vec<usize>, String> {
    let mut size = 0;
    // SAFETY: asking for the size passes no buffer (null, length 0) and a live `size` to receive
    // the length; the call fails by design.
    unsafe {
        let _ = GetTokenInformation(token, class, None, 0, &mut size);
    }
    if size == 0 || size > 65536 {
        return Err("cannot size process identity".into());
    }
    let mut data = vec![0usize; (size as usize).div_ceil(std::mem::size_of::<usize>())];
    // SAFETY: `data` holds `size` bytes, as GetTokenInformation reported, and stays alive past the
    // call, which writes at most that many.
    unsafe {
        GetTokenInformation(
            token,
            class,
            Some(data.as_mut_ptr().cast()),
            size,
            &mut size,
        )
    }
    .map_err(|_| "cannot read process identity")?;
    Ok(data)
}
fn sid_text(sid: PSID) -> Result<String, String> {
    let mut text = PWSTR::null();
    // SAFETY: `sid` is a valid SID borrowed from the token data the caller still holds, and `text`
    // is a live out-pointer.
    unsafe { ConvertSidToStringSidW(sid, &mut text) }.map_err(|_| "cannot format identity")?;
    // SAFETY: ConvertSidToStringSidW succeeded, so `text` points at a NUL-terminated wide string
    // that is freed only below.
    let result = unsafe { text.to_string() }.map_err(|_| "invalid identity");
    // SAFETY: `text` was allocated by ConvertSidToStringSidW, which requires LocalFree, and
    // `result` already copied what it needs.
    unsafe {
        let _ = LocalFree(Some(HLOCAL(text.0.cast())));
    }
    Ok(result?)
}
#[derive(Clone, Debug)]
pub struct Scope {
    pub namespace: String,
    pub mutex_name: String,
    pub pipe_name: String,
    logon_sid: String,
}
impl Scope {
    pub fn current(namespace: &str) -> Result<Self, String> {
        Self::identity(namespace)
    }
    fn identity(namespace: &str) -> Result<Self, String> {
        if namespace.is_empty()
            || namespace.len() > 64
            || !namespace
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-')
        {
            return Err("invalid instance namespace".into());
        }
        let mut token = HANDLE::default();
        // SAFETY: token buffers are aligned and Win32 writes no more than the queried allocation.
        unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) }
            .map_err(|_| "cannot open process identity")?;
        let token = Handle(token);
        let user = token_data(token.0, TokenUser)?;
        let logon = token_data(token.0, TokenLogonSid)?;
        // SAFETY: `user` is the buffer GetTokenInformation filled with a TOKEN_USER and its SID,
        // sized and aligned for it (a Vec<usize>), and it outlives this expression.
        let user_sid = sid_text(unsafe { (*(user.as_ptr().cast::<TOKEN_USER>())).User.Sid })?;
        // SAFETY: `logon` is the buffer GetTokenInformation filled with a TOKEN_GROUPS (sized and
        // aligned as above) and it outlives `groups`.
        let groups = unsafe { &*logon.as_ptr().cast::<TOKEN_GROUPS>() };
        if groups.GroupCount != 1 {
            return Err("process has no unique logon SID".into());
        }
        let logon_sid = sid_text(groups.Groups[0].Sid)?;
        let suffix = format!("{namespace}-{user_sid}-{logon_sid}");
        Ok(Self {
            namespace: namespace.into(),
            mutex_name: format!("Local\\aHerald-{suffix}"),
            pipe_name: format!(r"\\.\pipe\aHerald-{suffix}"),
            logon_sid,
        })
    }
    fn security(&self) -> Result<Local, String> {
        // OW removes implicit owner WRITE_DAC across logons of the same user. Only this
        // logon SID can exchange data; NETWORK is explicitly denied as defense in depth.
        let sddl = wide(&format!(
            "D:P(D;;GA;;;NU)(A;;GA;;;{})(A;;RC;;;OW)",
            self.logon_sid
        ));
        let mut descriptor = PSECURITY_DESCRIPTOR::default();
        // SAFETY: `sddl` is a NUL-terminated wide string that lives past the call, and `descriptor`
        // is a live out-pointer; the result is owned by `Local`, which frees it.
        unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                PCWSTR(sddl.as_ptr()),
                1,
                &mut descriptor,
                None,
            )
        }
        .map_err(|_| "cannot build local resource ACL")?;
        Ok(Local(descriptor))
    }
    pub fn acquire(&self) -> Result<Option<Ownership>, String> {
        let descriptor = self.security()?;
        let attributes = SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: descriptor.0.0,
            bInheritHandle: false.into(),
        };
        let name = wide(&self.mutex_name);
        // SAFETY: `attributes` points at the descriptor held in `descriptor` and `name` is
        // NUL-terminated; both live until the call returns, which copies them.
        let handle = unsafe { CreateMutexW(Some(&attributes), true, PCWSTR(name.as_ptr())) }
            .map_err(|_| "cannot acquire resident mutex")?;
        // SAFETY: reads the last error of the calling thread immediately after CreateMutexW, with
        // no call between them.
        let existing = unsafe { GetLastError() } == ERROR_ALREADY_EXISTS;
        let handle = Handle(handle);
        if existing {
            // SAFETY: `handle` is the mutex handle created above; a zero timeout only polls it.
            match unsafe { WaitForSingleObject(handle.0, 0) } {
                WAIT_OBJECT_0 | WAIT_ABANDONED => {}
                WAIT_TIMEOUT => return Ok(None),
                _ => return Err("cannot inspect resident mutex".into()),
            }
        }
        Ok(Some(Ownership {
            handle,
            _thread: PhantomData,
        }))
    }
    pub fn create_pipe(&self) -> Result<PipeServer, String> {
        let descriptor = self.security()?;
        let attributes = SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: descriptor.0.0,
            bInheritHandle: false.into(),
        };
        let name = wide(&self.pipe_name);
        // SAFETY: `name` is NUL-terminated and `attributes` points at the descriptor held in
        // `descriptor`; both live until the call returns, which copies them.
        let pipe = unsafe {
            CreateNamedPipeW(
                PCWSTR(name.as_ptr()),
                PIPE_ACCESS_DUPLEX | FILE_FLAG_OVERLAPPED | FILE_FLAG_FIRST_PIPE_INSTANCE,
                PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_REJECT_REMOTE_CLIENTS,
                1,
                4096,
                4096,
                0,
                Some(&attributes),
            )
        };
        if pipe.is_invalid() {
            return Err("cannot create local resident pipe".into());
        }
        Ok(PipeServer {
            handle: Handle(pipe),
        })
    }
}
pub struct Ownership {
    handle: Handle,
    _thread: PhantomData<Rc<()>>,
}
impl Drop for Ownership {
    fn drop(&mut self) {
        // SAFETY: `self.handle` is the mutex this value acquired and still owns; releasing it takes
        // only the handle.
        unsafe {
            let _ = ReleaseMutex(self.handle.0);
        }
    }
}
pub struct PipeServer {
    pub(crate) handle: Handle,
}
impl PipeServer {
    pub fn raw(&self) -> HANDLE {
        self.handle.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn foreign_logon_acl_denies_local_data_access() {
        let mut scope = Scope::current(&format!("acl-{}", std::process::id())).unwrap();
        scope.logon_sid = "S-1-5-5-4294967000-4294967000".into();
        let _owner = scope.acquire().unwrap().unwrap();
        let _pipe = scope.create_pipe().unwrap();
        let mutex = wide(&scope.mutex_name);
        let pipe = wide(&scope.pipe_name);
        // SAFETY: strings remain terminated/live and returned handles, if any, are closed.
        unsafe {
            assert!(
                OpenMutexW(
                    SYNCHRONIZATION_ACCESS_RIGHTS(0x00100000),
                    false,
                    PCWSTR(mutex.as_ptr())
                )
                .is_err()
            );
            let attempt = CreateFileW(
                PCWSTR(pipe.as_ptr()),
                GENERIC_READ.0 | GENERIC_WRITE.0,
                FILE_SHARE_MODE(0),
                None,
                OPEN_EXISTING,
                FILE_FLAG_OVERLAPPED,
                None,
            );
            if let Ok(handle) = attempt {
                drop(Handle(handle));
                panic!("foreign-logon pipe allowed data access");
            }
        }
    }
}
