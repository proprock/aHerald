//! Deadline-bound overlapped named-pipe operations. All I/O is off the GUI thread.
use crate::{
    ipc::{self, Request, Response},
    local::{Handle, PipeServer, Scope, wide},
};
use std::time::{Duration, Instant};
use windows::{
    Win32::{
        Foundation::*,
        Storage::FileSystem::*,
        System::{IO::*, Pipes::*, Threading::*},
    },
    core::PCWSTR,
};
const IO_TIMEOUT: Duration = Duration::from_millis(250);
fn error(e: windows::core::Error) -> String {
    format!("local pipe error ({:08x})", e.code().0)
}
fn operation(
    handle: HANDLE,
    deadline: Instant,
    call: impl FnOnce(*mut OVERLAPPED, &mut u32) -> windows::core::Result<()>,
) -> Result<u32, String> {
    // SAFETY: the event, OVERLAPPED and caller's buffer stay live until completion,
    // including cancellation. No pending operation can reference released stack data.
    unsafe {
        let event = Handle(CreateEventW(None, true, false, None).map_err(error)?);
        let mut pending = OVERLAPPED {
            hEvent: event.0,
            ..Default::default()
        };
        let mut count = 0;
        match call(&mut pending, &mut count) {
            Ok(()) => Ok(count),
            Err(e) if e.code() == windows::core::HRESULT::from_win32(ERROR_PIPE_CONNECTED.0) => {
                Ok(0)
            }
            Err(e) if e.code() == windows::core::HRESULT::from_win32(ERROR_IO_PENDING.0) => {
                let timeout = deadline
                    .saturating_duration_since(Instant::now())
                    .as_millis()
                    .min(u32::MAX as u128) as u32;
                if WaitForSingleObject(event.0, timeout) != WAIT_OBJECT_0 {
                    let _ = CancelIoEx(handle, Some(&pending));
                    // Drain canceled named-pipe I/O before freeing its borrowed buffers.
                    let _ = GetOverlappedResult(handle, &pending, &mut count, true);
                    return Err("local pipe deadline exceeded".into());
                }
                GetOverlappedResult(handle, &pending, &mut count, false).map_err(error)?;
                Ok(count)
            }
            Err(e) => Err(error(e)),
        }
    }
}
fn transfer(
    handle: HANDLE,
    bytes: &mut [u8],
    write: bool,
    deadline: Instant,
) -> Result<(), String> {
    let mut offset = 0;
    while offset < bytes.len() {
        if Instant::now() >= deadline {
            return Err("local pipe deadline exceeded".into());
        }
        let n = operation(handle, deadline, |ov, count| unsafe {
            if write {
                WriteFile(handle, Some(&bytes[offset..]), Some(count), Some(ov))
            } else {
                ReadFile(handle, Some(&mut bytes[offset..]), Some(count), Some(ov))
            }
        })? as usize;
        if n == 0 || n > bytes.len() - offset {
            return Err("local pipe disconnected".into());
        }
        offset += n;
    }
    Ok(())
}
fn receive<T: serde::de::DeserializeOwned>(handle: HANDLE) -> Result<T, String> {
    let deadline = Instant::now() + IO_TIMEOUT;
    let mut header = [0; 6];
    transfer(handle, &mut header, false, deadline)?;
    let n = ipc::frame_length(&header)?;
    let mut frame = vec![0; n + 6];
    frame[..6].copy_from_slice(&header);
    transfer(handle, &mut frame[6..], false, deadline)?;
    ipc::decode(&frame)
}
fn send<T: serde::Serialize>(handle: HANDLE, value: &T) -> Result<(), String> {
    let mut frame = ipc::encode(value)?;
    transfer(handle, &mut frame, true, Instant::now() + IO_TIMEOUT)
}
pub fn request(scope: &Scope, request: &Request, timeout: Duration) -> Result<Response, String> {
    let deadline = Instant::now() + timeout;
    let name = wide(&scope.pipe_name);
    let handle = loop {
        // Identification SQOS prevents a server from impersonating the calling client.
        let result = unsafe {
            CreateFileW(
                PCWSTR(name.as_ptr()),
                GENERIC_READ.0 | GENERIC_WRITE.0,
                FILE_SHARE_MODE(0),
                None,
                OPEN_EXISTING,
                FILE_FLAG_OVERLAPPED | SECURITY_SQOS_PRESENT | SECURITY_IDENTIFICATION,
                None,
            )
        };
        match result {
            Ok(handle) => break Handle(handle),
            Err(e)
                if Instant::now() < deadline
                    && [ERROR_PIPE_BUSY, ERROR_FILE_NOT_FOUND]
                        .iter()
                        .any(|code| e.code() == windows::core::HRESULT::from_win32(code.0)) =>
            {
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(e) => return Err(error(e)),
        }
    };
    send(handle.0, request)?;
    let response = receive(handle.0)?;
    transfer(handle.0, &mut [0x06], true, Instant::now() + IO_TIMEOUT)?;
    Ok(response)
}
impl PipeServer {
    pub fn serve_once(&self, handler: impl FnOnce(Request) -> Response) -> Result<(), String> {
        operation(
            self.handle.0,
            Instant::now() + Duration::from_millis(50),
            |ov, _| unsafe { ConnectNamedPipe(self.handle.0, Some(ov)) },
        )?;
        let result = receive(self.handle.0)
            .and_then(|request| send(self.handle.0, &handler(request)))
            .and_then(|()| {
                let mut ack = [0];
                transfer(self.handle.0, &mut ack, false, Instant::now() + IO_TIMEOUT)?;
                if ack == [0x06] {
                    Ok(())
                } else {
                    Err("invalid IPC acknowledgement".into())
                }
            });
        unsafe {
            let _ = DisconnectNamedPipe(self.handle.0);
        }
        result
    }
}
