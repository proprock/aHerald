//! Minimal bounded HTTPS GET over WinHTTP (TLS and proxy settings come from the OS).
use std::{ffi::c_void, time::Duration};
use windows::{
    Win32::Networking::WinHttp::*,
    core::{HSTRING, PCWSTR},
};

pub use crate::http_common::{HttpError, MAX_BODY, Response, Wiped, Zero};

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
pub fn get(
    host: &str,
    path: &str,
    headers: &str,
    timeout: Duration,
) -> Result<Response, HttpError> {
    get_at(true, host, 443, path, headers, timeout)
}

/// `get` with an explicit scheme and port; plain HTTP exists only so tests can use a
/// loopback endpoint.
pub fn get_at(
    secure: bool,
    host: &str,
    port: u16,
    path: &str,
    headers: &str,
    timeout: Duration,
) -> Result<Response, HttpError> {
    let ms = timeout.as_millis().min(i32::MAX as u128) as i32;
    // SAFETY: every pointer passed below references live, initialized data for the duration of
    // each synchronous call, and every returned handle is owned by a `Handle` guard that is
    // dropped (closing it) in reverse creation order.
    unsafe {
        let session = Handle::new(WinHttpOpen(
            &HSTRING::from("aHerald"),
            // Honors the system and PAC/WPAD proxy settings, unlike the default `netsh` ones.
            WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY,
            PCWSTR::null(),
            PCWSTR::null(),
            0,
        ))?;
        WinHttpSetTimeouts(session.0, ms, ms, ms, ms).map_err(|_| HttpError::Network)?;
        let connection = Handle::new(WinHttpConnect(session.0, &HSTRING::from(host), port, 0))?;
        let request = Handle::new(WinHttpOpenRequest(
            connection.0,
            &HSTRING::from("GET"),
            &HSTRING::from(path),
            PCWSTR::null(),
            PCWSTR::null(),
            std::ptr::null(),
            WINHTTP_OPEN_REQUEST_FLAGS(if secure { 0x0080_0000 } else { 0 }), // WINHTTP_FLAG_SECURE
        ))?;
        // The bearer token must reach only the requested host: never follow a redirect,
        // the caller maps any 3xx to an error.
        WinHttpSetOption(
            Some(request.0),
            WINHTTP_OPTION_REDIRECT_POLICY,
            Some(&WINHTTP_OPTION_REDIRECT_POLICY_NEVER.to_ne_bytes()),
        )
        .map_err(|_| HttpError::Network)?;
        let wide = Wiped(headers.encode_utf16().collect::<Vec<u16>>());
        // An empty slice would reach WinHTTP as a dangling pointer it reads as a C string.
        let header_units = Some(wide.0.as_slice()).filter(|h| !h.is_empty());
        WinHttpSendRequest(request.0, header_units, None, 0, 0, 0)
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
        // WINHTTP_QUERY_RETRY_AFTER (36) as text; `size` is in bytes. An absent header or a
        // buffer too small for a delay-seconds value leaves `retry_after` empty.
        let mut text = [0u16; 24];
        let mut size = std::mem::size_of_val(&text) as u32;
        let retry_after = WinHttpQueryHeaders(
            request.0,
            36,
            PCWSTR::null(),
            Some(text.as_mut_ptr() as *mut c_void),
            &mut size,
            std::ptr::null_mut(),
        )
        .ok()
        .and_then(|()| text.get(..size as usize / 2))
        .and_then(|units| String::from_utf16(units).ok())
        .and_then(|value| value.trim().parse::<u32>().ok())
        .map(|seconds| Duration::from_secs(u64::from(seconds)));
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
        Ok(Response {
            status: status as u16,
            body,
            retry_after,
        })
    }
}
