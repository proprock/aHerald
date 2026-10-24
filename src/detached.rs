//! Detached children inherit only their explicitly listed transfer handles.
use std::{
    ffi::OsStr,
    fs::File,
    mem::size_of,
    os::windows::{
        ffi::OsStrExt,
        io::{AsRawHandle, FromRawHandle, OwnedHandle},
        process::ExitStatusExt,
    },
    path::Path,
    process::ExitStatus,
};
use windows::{
    Win32::{
        Foundation::{
            HANDLE, HANDLE_FLAG_INHERIT, SetHandleInformation, WAIT_OBJECT_0, WAIT_TIMEOUT,
        },
        System::{Pipes::CreatePipe, Threading::*},
    },
    core::{PCWSTR, PWSTR},
};

pub struct Child {
    process: OwnedHandle,
    pid: u32,
    pub(crate) stdin: Option<File>,
    pub(crate) stdout: Option<File>,
}
impl Child {
    pub fn id(&self) -> u32 {
        self.pid
    }
    pub fn kill(&mut self) -> Result<(), String> {
        // SAFETY: the process handle is owned and valid for this child's lifetime.
        unsafe { TerminateProcess(HANDLE(self.process.as_raw_handle()), 0) }
            .map_err(|e| format!("cannot terminate notification child: {e}"))
    }
    pub fn try_wait(&mut self) -> Result<Option<ExitStatus>, String> {
        self.wait_for(0)
    }
    pub fn wait(&mut self) -> Result<ExitStatus, String> {
        self.wait_for(INFINITE)?
            .ok_or_else(|| "child wait returned without exit".into())
    }
    fn wait_for(&self, timeout: u32) -> Result<Option<ExitStatus>, String> {
        let handle = HANDLE(self.process.as_raw_handle());
        // SAFETY: borrowed handle remains valid for both calls.
        unsafe {
            match WaitForSingleObject(handle, timeout) {
                WAIT_TIMEOUT => Ok(None),
                WAIT_OBJECT_0 => {
                    let mut code = 0;
                    GetExitCodeProcess(handle, &mut code)
                        .map_err(|e| format!("cannot read child exit: {e}"))?;
                    Ok(Some(ExitStatus::from_raw(code)))
                }
                _ => Err("cannot wait for notification child".into()),
            }
        }
    }
}

struct Attributes {
    list: LPPROC_THREAD_ATTRIBUTE_LIST,
    _storage: Vec<usize>,
}
impl Drop for Attributes {
    fn drop(&mut self) {
        // SAFETY: list was initialized in the backing storage, which is still alive.
        unsafe {
            DeleteProcThreadAttributeList(self.list);
        }
    }
}
fn attributes(handles: &[HANDLE]) -> Result<Attributes, String> {
    let mut bytes = 0;
    // SAFETY: the first call queries the required allocation size.
    unsafe {
        let _ = InitializeProcThreadAttributeList(None, 1, None, &mut bytes);
    }
    if bytes == 0 {
        return Err("cannot size child handle list".into());
    }
    let mut storage = vec![0usize; bytes.div_ceil(size_of::<usize>())];
    let list = LPPROC_THREAD_ATTRIBUTE_LIST(storage.as_mut_ptr().cast());
    // SAFETY: backing storage is pointer-aligned and sized as requested by Win32.
    unsafe { InitializeProcThreadAttributeList(Some(list), 1, None, &mut bytes) }
        .map_err(|e| format!("cannot initialize child handle list: {e}"))?;
    let owned = Attributes {
        list,
        _storage: storage,
    };
    // SAFETY: handles stays live through CreateProcessW; size describes the full slice.
    unsafe {
        UpdateProcThreadAttribute(
            list,
            0,
            PROC_THREAD_ATTRIBUTE_HANDLE_LIST as usize,
            Some(handles.as_ptr().cast()),
            std::mem::size_of_val(handles),
            None,
            None,
        )
    }
    .map_err(|e| format!("cannot restrict child handle inheritance: {e}"))?;
    Ok(owned)
}
fn pipe() -> Result<(OwnedHandle, OwnedHandle), String> {
    let (mut read, mut write) = (HANDLE::default(), HANDLE::default());
    // SAFETY: pointers receive two new non-inheritable pipe handles; ownership is transferred once.
    unsafe {
        CreatePipe(&mut read, &mut write, None, 4096)
            .map_err(|e| format!("cannot create child channel: {e}"))?;
        Ok((
            OwnedHandle::from_raw_handle(read.0),
            OwnedHandle::from_raw_handle(write.0),
        ))
    }
}
fn argument(out: &mut Vec<u16>, value: &OsStr) {
    out.push(b'"' as u16);
    let mut slashes = 0;
    for c in value.encode_wide() {
        if c == b'\\' as u16 {
            slashes += 1;
            continue;
        }
        out.extend(std::iter::repeat_n(
            b'\\' as u16,
            if c == b'"' as u16 {
                slashes * 2 + 1
            } else {
                slashes
            },
        ));
        slashes = 0;
        out.push(c);
    }
    out.extend(std::iter::repeat_n(b'\\' as u16, slashes * 2));
    out.push(b'"' as u16);
}

pub(crate) fn spawn(executable: &Path, args: &[&OsStr], transfer: bool) -> Result<Child, String> {
    let mut application: Vec<u16> = executable.as_os_str().encode_wide().collect();
    if application.contains(&0) || args.iter().any(|arg| arg.encode_wide().any(|c| c == 0)) {
        return Err("child invocation contains a null character".into());
    }
    application.push(0);
    let mut command = Vec::new();
    argument(&mut command, executable.as_os_str());
    for arg in args {
        command.push(b' ' as u16);
        argument(&mut command, arg);
    }
    command.push(0);
    let channels = if transfer {
        Some((pipe()?, pipe()?))
    } else {
        None
    };
    let handles: Vec<HANDLE> = channels
        .as_ref()
        .map(|((input, _), (_, output))| {
            vec![
                HANDLE(input.as_raw_handle()),
                HANDLE(output.as_raw_handle()),
            ]
        })
        .unwrap_or_default();
    let mut startup = STARTUPINFOEXW::default();
    let mut flags = DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP;
    let attrs = if transfer {
        for handle in &handles {
            // SAFETY: only child ends of private, newly created pipes become inheritable.
            unsafe { SetHandleInformation(*handle, HANDLE_FLAG_INHERIT.0, HANDLE_FLAG_INHERIT) }
                .map_err(|e| format!("cannot prepare child channel: {e}"))?;
        }
        let attrs = attributes(&handles)?;
        startup.StartupInfo.cb = size_of::<STARTUPINFOEXW>() as u32;
        startup.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
        startup.StartupInfo.hStdInput = handles[0];
        startup.StartupInfo.hStdOutput = handles[1];
        startup.lpAttributeList = attrs.list;
        flags |= EXTENDED_STARTUPINFO_PRESENT;
        Some(attrs)
    } else {
        startup.StartupInfo.cb = size_of::<STARTUPINFOW>() as u32;
        None
    };
    let mut info = PROCESS_INFORMATION::default();
    // SAFETY: application/command are null terminated, command is writable; startup,
    // attributes and explicitly inherited handles remain live through the call.
    unsafe {
        CreateProcessW(
            PCWSTR(application.as_ptr()),
            Some(PWSTR(command.as_mut_ptr())),
            None,
            None,
            transfer,
            flags,
            None,
            PCWSTR::null(),
            &startup.StartupInfo,
            &mut info,
        )
    }
    .map_err(|e| format!("cannot launch detached child: {e}"))?;
    drop(attrs);
    // SAFETY: successful CreateProcessW transfers two fresh handles to this caller.
    let process = unsafe { OwnedHandle::from_raw_handle(info.hProcess.0) };
    let _thread = unsafe { OwnedHandle::from_raw_handle(info.hThread.0) };
    let (stdin, stdout) = if let Some(((child_input, input), (output, child_output))) = channels {
        drop((child_input, child_output));
        (Some(File::from(input)), Some(File::from(output)))
    } else {
        (None, None)
    };
    Ok(Child {
        process,
        pid: info.dwProcessId,
        stdin,
        stdout,
    })
}
