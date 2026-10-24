//! Read-only Codex app-server connection. No thread, login or approval operations.
use serde_json::{Value, json};
use std::{
    io::{BufRead, BufReader, Read, Write},
    process::{Child, Command, Stdio},
    sync::mpsc::{self, Receiver, SyncSender},
    thread::JoinHandle,
    time::{Duration, Instant},
};

pub const MESSAGE_LIMIT: usize = 256 * 1024;
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    Unavailable,
    Timeout,
    Disconnected,
    Malformed,
    Unsupported,
    Remote(i64),
    UnexpectedRequest,
    Backoff,
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Unavailable => "Codex executable unavailable",
            Self::Timeout => "Codex request timed out",
            Self::Disconnected => "Codex connection closed",
            Self::Malformed => "Invalid Codex protocol message",
            Self::Unsupported => "Codex usage method unsupported",
            Self::Remote(_) => "Codex usage unavailable (authentication or service error)",
            Self::UnexpectedRequest => "Unexpected Codex server request; connection closed",
            Self::Backoff => "Codex reconnect delayed",
        })
    }
}
pub trait Transport {
    fn send(&mut self, value: &Value, timeout: Duration) -> Result<(), Error>;
    fn receive(&mut self, timeout: Duration) -> Result<Value, Error>;
}
pub struct Client<T> {
    transport: T,
    next_id: u64,
    timeout: Duration,
    updates: Vec<Value>,
}
impl<T: Transport> Client<T> {
    pub fn initialize(transport: T, timeout: Duration) -> Result<Self, Error> {
        let mut client = Self {
            transport,
            next_id: 1,
            timeout,
            updates: Vec::new(),
        };
        client.request(
            "initialize",
            json!({"clientInfo": {
                "name":"aherald", "title":"aHerald", "version":env!("CARGO_PKG_VERSION")
            }}),
        )?;
        client
            .transport
            .send(&json!({"method":"initialized","params":{}}), timeout)?;
        Ok(client)
    }
    pub fn transport(&self) -> &T {
        &self.transport
    }
    pub fn take_updates(&mut self) -> Vec<Value> {
        std::mem::take(&mut self.updates)
    }
    pub fn read_limits(&mut self) -> Result<Value, Error> {
        self.request("account/rateLimits/read", Value::Null)
    }
    fn notification(&mut self, value: &Value) -> Result<(), Error> {
        if value.get("id").is_some() {
            return Err(Error::UnexpectedRequest);
        }
        match value["method"].as_str() {
            Some("account/rateLimits/updated" | "account/updated") => {
                if self.updates.len() == 32 {
                    return Err(Error::Malformed);
                }
                // Account updates only invalidate a snapshot. Never retain account details.
                self.updates.push(if value["method"] == "account/updated" {
                    json!({"method":"account/updated"})
                } else {
                    value.clone()
                });
            }
            Some(_) => (),
            None => return Err(Error::Malformed),
        }
        Ok(())
    }
    pub fn poll(&mut self, timeout: Duration) -> Result<(), Error> {
        let value = self.transport.receive(timeout)?;
        if value.get("method").is_some() {
            self.notification(&value)?;
        } else if value.get("id").is_none() {
            return Err(Error::Malformed);
        }
        Ok(())
    }
    fn request(&mut self, method: &str, params: Value) -> Result<Value, Error> {
        let id = self.next_id;
        self.next_id = self.next_id.checked_add(1).ok_or(Error::Malformed)?;
        let deadline = Instant::now() + self.timeout;
        self.transport.send(
            &json!({"id":id,"method":method,"params":params}),
            self.timeout,
        )?;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(Error::Timeout);
            }
            let value = self.transport.receive(remaining)?;
            if value.get("method").is_some() {
                self.notification(&value)?;
                continue;
            }
            if value["id"].as_u64().is_none() {
                return Err(Error::Malformed);
            }
            if value["id"] != id {
                continue;
            }
            if let Some(error) = value.get("error") {
                let code = error["code"].as_i64().ok_or(Error::Malformed)?;
                return Err(if code == -32601 {
                    Error::Unsupported
                } else {
                    Error::Remote(code)
                });
            }
            return value.get("result").cloned().ok_or(Error::Malformed);
        }
    }
}

/// One owned child at a time. Failures defer the next connection by 1..60 seconds.
pub struct Connection<T, F> {
    pub client: Option<Client<T>>,
    factory: F,
    retry_at: Duration,
    delay: Duration,
}
impl<T: Transport, F: FnMut() -> Result<T, Error>> Connection<T, F> {
    pub fn new(factory: F) -> Self {
        Self {
            client: None,
            factory,
            retry_at: Duration::ZERO,
            delay: Duration::from_secs(1),
        }
    }
    pub fn read(&mut self, now: Duration) -> Result<Value, Error> {
        if now < self.retry_at {
            return Err(Error::Backoff);
        }
        let result = (|| {
            if self.client.is_none() {
                self.client = Some(Client::initialize((self.factory)()?, REQUEST_TIMEOUT)?);
            }
            self.client
                .as_mut()
                .ok_or(Error::Disconnected)?
                .read_limits()
        })();
        if result.is_err() {
            self.failed(now);
        } else {
            self.delay = Duration::from_secs(1);
        }
        result
    }
    pub fn failed(&mut self, now: Duration) {
        self.client.take();
        self.retry_at = now.saturating_add(self.delay);
        self.delay = (self.delay * 2).min(Duration::from_secs(60));
    }
}

type WriteJob = (Vec<u8>, SyncSender<Result<(), Error>>);
pub struct StdioTransport {
    child: Child,
    incoming: Receiver<Result<Value, Error>>,
    outgoing: Option<SyncSender<WriteJob>>,
    reader: Option<JoinHandle<()>>,
    writer: Option<JoinHandle<()>>,
}
impl StdioTransport {
    pub fn codex() -> Result<Self, Error> {
        let mut command = Command::new("codex.exe");
        command.args(["app-server", "--listen", "stdio://"]);
        Self::spawn(command)
    }
    pub fn spawn(mut command: Command) -> Result<Self, Error> {
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000); // CREATE_NO_WINDOW; explicit stdio stays redirected.
        }
        // stderr is intentionally discarded: upstream diagnostics can contain private data.
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|_| Error::Unavailable)?;
        let (Some(stdout), Some(mut stdin)) = (child.stdout.take(), child.stdin.take()) else {
            let _ = child.kill();
            let _ = child.wait();
            return Err(Error::Unavailable);
        };
        let (tx, incoming) = mpsc::sync_channel(32);
        let reader = std::thread::spawn(move || {
            let mut input = BufReader::new(stdout);
            loop {
                let mut line = Vec::new();
                let result = (&mut input)
                    .take((MESSAGE_LIMIT + 1) as u64)
                    .read_until(b'\n', &mut line);
                let value = match result {
                    Ok(0) | Err(_) => Err(Error::Disconnected),
                    Ok(_) if line.len() > MESSAGE_LIMIT || !line.ends_with(b"\n") => {
                        Err(Error::Malformed)
                    }
                    Ok(_) => serde_json::from_slice::<Value>(&line)
                        .map_err(|_| Error::Malformed)
                        .and_then(|v| {
                            if v.is_object() {
                                Ok(v)
                            } else {
                                Err(Error::Malformed)
                            }
                        }),
                };
                let failed = value.is_err();
                // Overflow closes the stream rather than blocking cleanup or growing memory.
                if tx.try_send(value).is_err() || failed {
                    break;
                }
            }
        });
        let (outgoing, rx) = mpsc::sync_channel::<WriteJob>(1);
        let writer = std::thread::spawn(move || {
            while let Ok((bytes, ack)) = rx.recv() {
                let result = stdin
                    .write_all(&bytes)
                    .and_then(|_| stdin.flush())
                    .map_err(|_| Error::Disconnected);
                let failed = result.is_err();
                let _ = ack.try_send(result);
                if failed {
                    break;
                }
            }
        });
        Ok(Self {
            child,
            incoming,
            outgoing: Some(outgoing),
            reader: Some(reader),
            writer: Some(writer),
        })
    }
    pub fn pid(&self) -> u32 {
        self.child.id()
    }
}
impl Transport for StdioTransport {
    fn send(&mut self, value: &Value, timeout: Duration) -> Result<(), Error> {
        let mut bytes = serde_json::to_vec(value).map_err(|_| Error::Malformed)?;
        if bytes.len() >= MESSAGE_LIMIT {
            return Err(Error::Malformed);
        }
        bytes.push(b'\n');
        let (tx, rx) = mpsc::sync_channel(1);
        self.outgoing
            .as_ref()
            .ok_or(Error::Disconnected)?
            .try_send((bytes, tx))
            .map_err(|_| Error::Disconnected)?;
        rx.recv_timeout(timeout).map_err(|e| match e {
            mpsc::RecvTimeoutError::Timeout => Error::Timeout,
            _ => Error::Disconnected,
        })?
    }
    fn receive(&mut self, timeout: Duration) -> Result<Value, Error> {
        self.incoming.recv_timeout(timeout).map_err(|e| match e {
            mpsc::RecvTimeoutError::Timeout => Error::Timeout,
            _ => Error::Disconnected,
        })?
    }
}
impl Drop for StdioTransport {
    fn drop(&mut self) {
        // This child creates no agent turns or tools. Kill/reap only our own process;
        // closing its pipe ends releases any blocked reader/writer before joining.
        let _ = self.child.kill();
        let _ = self.child.wait();
        self.outgoing.take();
        if let Some(job) = self.reader.take() {
            let _ = job.join();
        }
        if let Some(job) = self.writer.take() {
            let _ = job.join();
        }
    }
}
