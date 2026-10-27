//! Deadline-bound unix-domain-socket exchange with the same frames as the Windows pipe.
//! All I/O is off the GUI thread and every wait has a bound.
use crate::{
    ipc::{self, Request, Response},
    local::{PipeServer, Scope},
};
use std::{
    io::{ErrorKind, Read, Write},
    os::unix::{io::AsRawFd, net::UnixStream},
    time::{Duration, Instant},
};

const IO_TIMEOUT: Duration = Duration::from_millis(250);
const ACCEPT_POLL: Duration = Duration::from_millis(50);

/// Socket failure; only the deadline is told apart because callers treat it as "nothing happened".
enum Failure {
    DeadlineExceeded,
    Other(String),
}
impl From<Failure> for String {
    fn from(failure: Failure) -> Self {
        match failure {
            Failure::DeadlineExceeded => "local socket deadline exceeded".into(),
            Failure::Other(message) => message,
        }
    }
}
impl From<String> for Failure {
    fn from(message: String) -> Self {
        Self::Other(message)
    }
}
impl From<std::io::Error> for Failure {
    fn from(error: std::io::Error) -> Self {
        match error.kind() {
            ErrorKind::WouldBlock | ErrorKind::TimedOut => Self::DeadlineExceeded,
            ErrorKind::UnexpectedEof | ErrorKind::BrokenPipe | ErrorKind::ConnectionReset => {
                Self::Other("local socket disconnected".into())
            }
            _ => Self::Other("local socket error".into()),
        }
    }
}

/// Runs `step` with the socket timeout set to what is left of `deadline`.
fn within<T>(
    stream: &UnixStream,
    deadline: Instant,
    write: bool,
    step: impl FnOnce(&UnixStream) -> std::io::Result<T>,
) -> Result<T, Failure> {
    let left = deadline.saturating_duration_since(Instant::now());
    if left.is_zero() {
        return Err(Failure::DeadlineExceeded);
    }
    if write {
        stream.set_write_timeout(Some(left))?;
    } else {
        stream.set_read_timeout(Some(left))?;
    }
    Ok(step(stream)?)
}

fn transfer(
    stream: &UnixStream,
    bytes: &mut [u8],
    write: bool,
    deadline: Instant,
) -> Result<(), Failure> {
    let mut offset = 0;
    while offset < bytes.len() {
        let n = within(stream, deadline, write, |mut s| {
            if write {
                s.write(&bytes[offset..])
            } else {
                s.read(&mut bytes[offset..])
            }
        })?;
        if n == 0 {
            return Err("local socket disconnected".to_owned().into());
        }
        offset += n;
    }
    Ok(())
}

fn receive<T: serde::de::DeserializeOwned>(stream: &UnixStream) -> Result<T, Failure> {
    let deadline = Instant::now() + IO_TIMEOUT;
    let mut header = [0; 6];
    transfer(stream, &mut header, false, deadline)?;
    let n = ipc::frame_length(&header)?;
    let mut frame = vec![0; n + 6];
    frame[..6].copy_from_slice(&header);
    transfer(stream, &mut frame[6..], false, deadline)?;
    Ok(ipc::decode(&frame)?)
}

fn send<T: serde::Serialize>(stream: &UnixStream, value: &T) -> Result<(), Failure> {
    let mut frame = ipc::encode(value)?;
    transfer(stream, &mut frame, true, Instant::now() + IO_TIMEOUT)
}

pub fn request(scope: &Scope, request: &Request, timeout: Duration) -> Result<Response, String> {
    let deadline = Instant::now() + timeout;
    let stream = loop {
        match UnixStream::connect(scope.socket_path()) {
            Ok(stream) => break stream,
            // No socket yet, a dead one, or a full backlog: the resident may be starting.
            Err(e)
                if Instant::now() < deadline
                    && matches!(
                        e.kind(),
                        ErrorKind::NotFound | ErrorKind::ConnectionRefused | ErrorKind::WouldBlock
                    ) =>
            {
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(_) => return Err("cannot reach the resident".into()),
        }
    };
    send(&stream, request)?;
    let response = receive(&stream)?;
    transfer(&stream, &mut [0x06], true, Instant::now() + IO_TIMEOUT)?;
    Ok(response)
}

/// Wakes a blocked `serve_until`: a socket pair whose read end is polled with the listener.
pub struct StopEvent {
    wake: UnixStream,
    sleep: UnixStream,
}
impl StopEvent {
    pub fn new() -> Result<Self, String> {
        let (wake, sleep) = UnixStream::pair().map_err(|_| "cannot create stop event")?;
        wake.set_nonblocking(true)
            .map_err(|_| "cannot create stop event")?;
        Ok(Self { wake, sleep })
    }
    pub fn signal(&self) {
        // A full buffer already holds an unread wake-up, so a failed write loses nothing.
        let _ = (&self.wake).write(&[1]);
    }
}

/// Waits until `descriptors` become readable (`true` per entry) or the timeout passes.
fn wait_readable(descriptors: &[i32], timeout: Option<Duration>) -> Result<Vec<bool>, String> {
    let mut polled: Vec<libc::pollfd> = descriptors
        .iter()
        .map(|&fd| libc::pollfd {
            fd,
            events: libc::POLLIN,
            revents: 0,
        })
        .collect();
    let deadline = timeout.map(|t| Instant::now() + t);
    loop {
        let milliseconds = deadline.map_or(-1, |d| {
            d.saturating_duration_since(Instant::now())
                .as_millis()
                .min(i32::MAX as u128) as i32
        });
        // SAFETY: `polled` is a live, correctly sized array of initialized `pollfd`s.
        let ready = unsafe {
            libc::poll(
                polled.as_mut_ptr(),
                polled.len() as libc::nfds_t,
                milliseconds,
            )
        };
        if ready >= 0 {
            return Ok(polled.iter().map(|p| p.revents != 0).collect());
        }
        if std::io::Error::last_os_error().kind() != ErrorKind::Interrupted {
            return Err("cannot wait for local clients".into());
        }
    }
}

/// The user id of the process on the other end of `stream`.
fn peer_uid(stream: &UnixStream) -> std::io::Result<u32> {
    let descriptor = stream.as_raw_fd();
    #[cfg(any(target_os = "linux", target_os = "android"))]
    {
        let mut credentials = libc::ucred {
            pid: 0,
            uid: 0,
            gid: 0,
        };
        let mut length = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
        // SAFETY: `credentials` and `length` are live and sized for SO_PEERCRED's `ucred`.
        let result = unsafe {
            libc::getsockopt(
                descriptor,
                libc::SOL_SOCKET,
                libc::SO_PEERCRED,
                (&mut credentials as *mut libc::ucred).cast(),
                &mut length,
            )
        };
        if result != 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(credentials.uid)
    }
    #[cfg(not(any(target_os = "linux", target_os = "android")))]
    {
        let (mut uid, mut gid) = (0, 0);
        // SAFETY: both out-pointers reference live integers.
        if unsafe { libc::getpeereid(descriptor, &mut uid, &mut gid) } != 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(uid)
    }
}

impl PipeServer {
    /// Idle accept poll lets the owner check shutdown; no client failed.
    pub fn serve_once(&self, handler: impl FnOnce(Request) -> Response) -> Result<(), String> {
        let ready = wait_readable(&[self.listener.as_raw_fd()], Some(ACCEPT_POLL))?;
        if ready[0] {
            self.accept_and_serve(handler)?;
        }
        Ok(())
    }

    /// Blocks without polling until a client connects (`Ok(true)` after serving it) or
    /// `stop` is signaled (`Ok(false)`).
    pub fn serve_until(
        &self,
        stop: &StopEvent,
        handler: impl FnOnce(Request) -> Response,
    ) -> Result<bool, String> {
        let ready = wait_readable(&[self.listener.as_raw_fd(), stop.sleep.as_raw_fd()], None)?;
        if ready[1] {
            return Ok(false);
        }
        self.accept_and_serve(handler).map(|()| true)
    }

    fn accept_and_serve(&self, handler: impl FnOnce(Request) -> Response) -> Result<(), String> {
        let stream = match self.listener.accept() {
            Ok((stream, _)) => stream,
            // The client left between the poll and the accept.
            Err(e) if e.kind() == ErrorKind::WouldBlock => return Ok(()),
            Err(_) => return Err("cannot accept local client".into()),
        };
        // Accepted sockets inherit the listener's non-blocking mode on BSD and macOS.
        stream
            .set_nonblocking(false)
            .map_err(|_| "cannot configure local client")?;
        match peer_uid(&stream) {
            Ok(uid) if uid == self.expected_uid => {}
            _ => return Err("rejected a local client of another user".into()),
        }
        let result = receive(&stream)
            .and_then(|request| send(&stream, &handler(request)))
            .and_then(|()| {
                let mut ack = [0];
                transfer(&stream, &mut ack, false, Instant::now() + IO_TIMEOUT)?;
                if ack == [0x06] {
                    Ok(())
                } else {
                    Err("invalid IPC acknowledgement".to_owned().into())
                }
            });
        result.map_err(String::from)
    }
}
