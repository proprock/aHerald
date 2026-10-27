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
