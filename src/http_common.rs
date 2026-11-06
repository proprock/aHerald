//! Platform-independent pieces of the bounded HTTPS GET: its result types and the
//! zeroing wrapper for credential-bearing buffers.
use std::time::Duration;

pub const MAX_BODY: usize = 64 * 1024;

#[derive(Debug, PartialEq, Eq)]
pub enum HttpError {
    Network,
    TooLarge,
}

/// Memory that may hold a credential; it is overwritten with zeros when dropped.
///
/// This clears only buffers this crate owns. Copies made earlier by the standard
/// library, the JSON parser or the HTTP stack itself are out of reach, so it narrows the
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

/// A completed response: status, at most `MAX_BODY` bytes of body, and `Retry-After`.
pub struct Response {
    pub status: u16,
    pub body: Vec<u8>,
    /// Only the delay-seconds form; an HTTP-date or a missing header is `None`.
    pub retry_after: Option<Duration>,
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

/// Blocking POST to `https://host/path` with `body`; `headers` as in `get`.
pub fn post(
    host: &str,
    path: &str,
    headers: &str,
    body: &[u8],
    timeout: Duration,
) -> Result<Response, HttpError> {
    post_at(true, host, 443, path, headers, body, timeout)
}

/// `post` with an explicit scheme and port; plain HTTP exists only for loopback tests.
pub fn post_at(
    secure: bool,
    host: &str,
    port: u16,
    path: &str,
    headers: &str,
    body: &[u8],
    timeout: Duration,
) -> Result<Response, HttpError> {
    crate::https::request_at(Some(body), secure, host, port, path, headers, timeout)
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
    crate::https::request_at(None, secure, host, port, path, headers, timeout)
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
