//! The local time zone on Unix, through the C library.

/// The broken-down local time of `timestamp` (Unix seconds), with the zone's offset in force at
/// that instant, or `None` when the platform cannot represent it.
pub fn local_time(timestamp: i64) -> Option<libc::tm> {
    unsafe extern "C" {
        // POSIX; the `libc` crate does not bind it on every target.
        fn tzset();
    }
    let time = libc::time_t::try_from(timestamp).ok()?;
    // SAFETY: `tm` is plain data that `localtime_r` fills in; `tzset` re-reads the zone so a
    // changed `TZ` applies.
    unsafe {
        tzset();
        let mut tm: libc::tm = std::mem::zeroed();
        if libc::localtime_r(&time, &mut tm).is_null() {
            return None;
        }
        Some(tm)
    }
}
