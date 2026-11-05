//! How long the Linux resident's loop may block. It wakes for an X11 event or a signal that
//! something else changed (an IPC request, a tray command, new usage numbers, a stop), and
//! otherwise for the next frame while something is shown or a slow housekeeping beat (AH-231).
use crate::ipc::pipe::{StopEvent, wait_readable};
use std::time::Duration;

/// One animation frame, about 60 per second.
pub const FRAME: Duration = Duration::from_millis(16);
/// How often enabled usage polling or a session-warning deadline refreshes the tray.
pub const HOUSEKEEPING: Duration = Duration::from_secs(1);

/// The longest the loop may block: a frame while `busy`, the housekeeping beat while only
/// `housekeeping` is due (see `resident_request::idle_poll_needed`), else until an event.
pub fn timeout(busy: bool, housekeeping: bool) -> Option<Duration> {
    if busy {
        Some(FRAME)
    } else if housekeeping {
        Some(HOUSEKEEPING)
    } else {
        None
    }
}

/// Blocks until the X connection `descriptor` is readable, `event` is signalled, or `timeout`
/// passes, and forgets the signals so the next call blocks again.
pub fn wait(event: &StopEvent, descriptor: i32, timeout: Option<Duration>) -> Result<(), String> {
    wait_readable(&[descriptor, event.descriptor()], timeout)
        .map_err(|_| "cannot wait for work")?;
    event.clear();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::Write,
        os::{fd::AsRawFd, unix::net::UnixStream},
        time::Instant,
    };

    #[test]
    fn the_loop_blocks_only_as_long_as_something_needs_it() {
        assert_eq!(timeout(true, true), Some(FRAME));
        assert_eq!(timeout(true, false), Some(FRAME));
        assert_eq!(timeout(false, true), Some(HOUSEKEEPING));
        assert_eq!(timeout(false, false), None);
    }

    fn elapsed(run: impl FnOnce()) -> Duration {
        let started = Instant::now();
        run();
        started.elapsed()
    }

    #[test]
    fn a_quiet_wait_lasts_the_whole_timeout() {
        let event = StopEvent::new().unwrap();
        let (_keep, idle) = UnixStream::pair().unwrap();
        let took =
            elapsed(|| wait(&event, idle.as_raw_fd(), Some(Duration::from_millis(150))).unwrap());
        assert!(took >= Duration::from_millis(140), "{took:?}");
    }

    #[test]
    fn a_signal_wakes_a_wait_at_once_even_when_sent_before_it() {
        let event = StopEvent::new().unwrap();
        let (_keep, idle) = UnixStream::pair().unwrap();
        event.signal();
        let took =
            elapsed(|| wait(&event, idle.as_raw_fd(), Some(Duration::from_secs(5))).unwrap());
        assert!(took < Duration::from_secs(1), "{took:?}");
    }

    #[test]
    fn a_signal_from_another_thread_wakes_a_wait_without_a_timeout_elapsing() {
        let event = std::sync::Arc::new(StopEvent::new().unwrap());
        let (_keep, idle) = UnixStream::pair().unwrap();
        let sender = event.clone();
        let thread = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(100));
            sender.signal();
        });
        let took =
            elapsed(|| wait(&event, idle.as_raw_fd(), Some(Duration::from_secs(5))).unwrap());
        thread.join().unwrap();
        assert!(took >= Duration::from_millis(90), "{took:?}");
        assert!(took < Duration::from_secs(2), "{took:?}");
    }

    #[test]
    fn a_wait_forgets_the_signals_it_woke_for_and_many_signals_count_once() {
        let event = StopEvent::new().unwrap();
        let (_keep, idle) = UnixStream::pair().unwrap();
        for _ in 0..1000 {
            event.signal();
        }
        wait(&event, idle.as_raw_fd(), Some(Duration::from_secs(5))).unwrap();
        let took =
            elapsed(|| wait(&event, idle.as_raw_fd(), Some(Duration::from_millis(100))).unwrap());
        assert!(
            took >= Duration::from_millis(90),
            "stale signal woke the wait: {took:?}"
        );
    }

    #[test]
    fn readable_x_data_wakes_a_wait_and_stays_for_the_caller_to_read() {
        let event = StopEvent::new().unwrap();
        let (mut server, client) = UnixStream::pair().unwrap();
        server.write_all(b"x").unwrap();
        let took =
            elapsed(|| wait(&event, client.as_raw_fd(), Some(Duration::from_secs(5))).unwrap());
        assert!(took < Duration::from_secs(1), "{took:?}");
        // The wait did not consume the connection's bytes: it returns at once again.
        let again =
            elapsed(|| wait(&event, client.as_raw_fd(), Some(Duration::from_secs(5))).unwrap());
        assert!(again < Duration::from_secs(1), "{again:?}");
    }
}
