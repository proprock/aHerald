//! Minimal bounded HTTPS GET over WinHTTP (TLS and proxy settings come from the OS).
use std::{ffi::c_void, time::Duration};
use windows::{
    Win32::Networking::WinHttp::*,
    core::{HSTRING, PCWSTR},
};

pub const MAX_BODY: usize = 64 * 1024;

#[derive(Debug, PartialEq, Eq)]
pub enum HttpError {
    Network,
    TooLarge,
}

/// Closes a WinHTTP handle on drop.
struct Handle(*mut c_void);
impl Handle {
    fn new(raw: *mut c_void) -> Result<Self, HttpError> {
        if raw.is_null() {
            Err(HttpError::Network)
        } else {
            Ok(Self(raw))
        }
    }
}
impl Drop for Handle {
    fn drop(&mut self) {
        // SAFETY: the handle came from a successful WinHTTP open call and is closed once.
        unsafe {
            let _ = WinHttpCloseHandle(self.0);
        }
    }
}

/// Blocking GET to `https://host/path`; `headers` is CRLF-separated `Name: value` lines.
/// Returns the status code and at most `MAX_BODY` bytes of body.
pub fn get(
    host: &str,
    path: &str,
    headers: &str,
    timeout: Duration,
) -> Result<(u16, Vec<u8>), HttpError> {
    let ms = timeout.as_millis().min(i32::MAX as u128) as i32;
    // SAFETY: every pointer passed below references live, initialized data for the duration of
    // each synchronous call, and every returned handle is owned by a `Handle` guard that is
    // dropped (closing it) in reverse creation order.
    unsafe {
        let session = Handle::new(WinHttpOpen(
            &HSTRING::from("aHerald"),
            WINHTTP_ACCESS_TYPE_DEFAULT_PROXY,
            PCWSTR::null(),
            PCWSTR::null(),
            0,
        ))?;
        WinHttpSetTimeouts(session.0, ms, ms, ms, ms).map_err(|_| HttpError::Network)?;
        let connection = Handle::new(WinHttpConnect(session.0, &HSTRING::from(host), 443, 0))?;
        let request = Handle::new(WinHttpOpenRequest(
            connection.0,
            &HSTRING::from("GET"),
            &HSTRING::from(path),
            PCWSTR::null(),
            PCWSTR::null(),
            std::ptr::null(),
            WINHTTP_OPEN_REQUEST_FLAGS(0x0080_0000), // WINHTTP_FLAG_SECURE
        ))?;
        let wide: Vec<u16> = headers.encode_utf16().collect();
        WinHttpSendRequest(request.0, Some(&wide), None, 0, 0, 0)
            .map_err(|_| HttpError::Network)?;
        WinHttpReceiveResponse(request.0, std::ptr::null_mut()).map_err(|_| HttpError::Network)?;
        let mut status = 0u32;
        let mut size = std::mem::size_of::<u32>() as u32;
        // WINHTTP_QUERY_STATUS_CODE (19) | WINHTTP_QUERY_FLAG_NUMBER (0x20000000)
        WinHttpQueryHeaders(
            request.0,
            19 | 0x2000_0000,
            PCWSTR::null(),
            Some(&mut status as *mut u32 as *mut c_void),
            &mut size,
            std::ptr::null_mut(),
        )
        .map_err(|_| HttpError::Network)?;
        let mut body = Vec::new();
        let mut chunk = [0u8; 4096];
        loop {
            let mut read = 0u32;
            WinHttpReadData(
                request.0,
                chunk.as_mut_ptr() as *mut c_void,
                chunk.len() as u32,
                &mut read,
            )
            .map_err(|_| HttpError::Network)?;
            if read == 0 {
                break;
            }
            if body.len() + read as usize > MAX_BODY {
                return Err(HttpError::TooLarge);
            }
            body.extend_from_slice(&chunk[..read as usize]);
        }
        Ok((status as u16, body))
    }
}
