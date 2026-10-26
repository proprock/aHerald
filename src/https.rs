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

/// Memory that may hold a credential; it is overwritten with zeros when dropped.
///
/// This clears only buffers this crate owns. Copies made earlier by the standard
/// library, the JSON parser or WinHTTP itself are out of reach, so it narrows the
/// exposure window rather than guaranteeing erasure.
pub struct Wiped<T: Zero>(pub T);
impl<T: Zero> Drop for Wiped<T> {
    fn drop(&mut self) {
        self.0.zero();
    }
}
pub trait Zero {
    fn zero(&mut self);
}
impl Zero for Vec<u16> {
    fn zero(&mut self) {
        for unit in self.iter_mut() {
            // SAFETY: `unit` is a valid, aligned, exclusive reference; the volatile write
            // keeps the compiler from removing the store as dead.
            unsafe { std::ptr::write_volatile(unit, 0) };
        }
    }
}
impl Zero for Vec<u8> {
    fn zero(&mut self) {
        for byte in self.iter_mut() {
            // SAFETY: as above.
            unsafe { std::ptr::write_volatile(byte, 0) };
        }
    }
}
impl Zero for String {
    fn zero(&mut self) {
        // SAFETY: NUL bytes are valid UTF-8, so the string stays well formed.
        let bytes = unsafe { self.as_mut_vec() };
        bytes.zero();
    }
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
) -> Result<(u16, Vec<u8>), HttpError> {
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
        WinHttpSendRequest(request.0, Some(&wide.0), None, 0, 0, 0)
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{Read, Write},
        net::TcpListener,
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
    };

    /// Serves `responses` in order on loopback and counts the requests it receives.
    fn serve(responses: Vec<String>) -> (u16, Arc<AtomicUsize>, std::thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        listener.set_nonblocking(true).unwrap();
        let requests = Arc::new(AtomicUsize::new(0));
        let seen = requests.clone();
        let thread = std::thread::spawn(move || {
            let deadline = std::time::Instant::now() + Duration::from_secs(3);
            for response in responses {
                let mut stream = loop {
                    match listener.accept() {
                        Ok((stream, _)) => break stream,
                        Err(_) if std::time::Instant::now() < deadline => {
                            std::thread::sleep(Duration::from_millis(5))
                        }
                        Err(_) => return,
                    }
                };
                stream.set_nonblocking(false).unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(1)))
                    .unwrap();
                let mut buffer = [0u8; 2048];
                let _ = stream.read(&mut buffer);
                seen.fetch_add(1, Ordering::SeqCst);
                let _ = stream.write_all(response.as_bytes());
            }
        });
        (port, requests, thread)
    }

    #[test]
    fn zeroing_overwrites_every_unit_and_keeps_the_length() {
        let mut text = String::from("Bearer secret");
        text.zero();
        assert_eq!(text.len(), 13);
        assert!(text.bytes().all(|b| b == 0));
        let mut wide: Vec<u16> = "secret".encode_utf16().collect();
        wide.zero();
        assert_eq!(wide, [0u16; 6]);
    }

    #[test]
    fn redirect_is_reported_and_never_followed() {
        let (port, requests, server) = serve(vec![
            "HTTP/1.1 302 Found\r\nLocation: /next\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                .into(),
            "HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok".into(),
        ]);
        let (status, _) = get_at(
            false,
            "127.0.0.1",
            port,
            "/start",
            "Authorization: Bearer test\r\n",
            Duration::from_secs(2),
        )
        .unwrap();
        server.join().unwrap();
        assert_eq!(status, 302);
        assert_eq!(requests.load(Ordering::SeqCst), 1, "redirect was followed");
    }
}
