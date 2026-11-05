//! The `[remote]` listener: hook events from agents inside containers and virtual machines.
//!
//! A guest cannot reach the resident's same-user pipe, so it POSTs the raw hook payload here
//! (`POST /v1/hook?agent=claude|codex`, `Authorization: Bearer <token>`). The payload is parsed
//! by the same `Agent::parse_hook` the local hook uses, so a sender never needs to know an
//! agent's format and can never supply a window or terminal of this machine. Only events that
//! end or wait for a turn are taken; per-tool activity is dropped. Plain `std`, one short thread
//! per connection, no TLS: the token is a guard against other local clients, not a secret that
//! survives a hostile network, so bind to loopback or a private bridge.
use crate::{
    agent::Agent,
    config::Config,
    events::Event,
    ipc::{Request, Response},
    operational_log::{Record, log},
};
use std::{
    io::{Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};

/// The largest body accepted; the local hook reads the same 64 KiB.
pub const MAX_BODY: usize = crate::events::MAX_INPUT;
/// Connections handled at once; further ones are told to come back.
pub const MAX_CONNECTIONS: usize = 8;
/// A whole request, headers and body, must arrive within this time.
const REQUEST_DEADLINE: Duration = Duration::from_secs(1);
const MAX_HEADER_BYTES: usize = 8 * 1024;
const MAX_HEADERS: usize = 32;
const RETRY_BIND: Duration = Duration::from_secs(30);
const POLL: Duration = Duration::from_millis(20);

/// How the listener hands an event to the resident: the same decision the local pipe makes.
pub type Deliver = Arc<dyn Fn(Request) -> Response + Send + Sync>;

/// What the listener needs to serve; built once by the resident.
pub struct Context {
    pub config: Config,
    /// The shared secret, 64 hexadecimal characters.
    pub token: String,
    pub deliver: Deliver,
}

/// The answer to one request.
#[derive(Debug, PartialEq, Eq)]
pub struct Reply {
    pub status: u16,
    pub body: String,
}

impl Reply {
    fn status(status: u16) -> Self {
        Self {
            status,
            body: String::new(),
        }
    }
    fn reason(&self) -> &'static str {
        match self.status {
            200 => "OK",
            400 => "Bad Request",
            401 => "Unauthorized",
            404 => "Not Found",
            405 => "Method Not Allowed",
            411 => "Length Required",
            413 => "Payload Too Large",
            503 => "Service Unavailable",
            _ => "Error",
        }
    }
}

/// The word logged for a refusal; never a value from the request.
pub fn reject_code(status: u16) -> &'static str {
    match status {
        400 => "invalid",
        401 => "unauthorized",
        404 | 405 => "route",
        411 | 413 => "size",
        503 => "busy",
        _ => "error",
    }
}

/// Equal strings take the same time, whatever the first difference is.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    let mut diff = a.len() ^ b.len();
    for i in 0..a.len().max(b.len()) {
        diff |= usize::from(a.get(i).copied().unwrap_or(0) ^ b.get(i).copied().unwrap_or(0));
    }
    diff == 0
}

#[derive(Debug, PartialEq, Eq)]
struct Head {
    method: String,
    target: String,
    authorization: Option<String>,
    content_length: Option<usize>,
    chunked: bool,
}

fn parse_head(text: &str) -> Result<Head, Reply> {
    let mut lines = text.split("\r\n");
    let request_line = lines.next().unwrap_or_default();
    let mut parts = request_line.split(' ');
    let (Some(method), Some(target), Some(version), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return Err(Reply::status(400));
    };
    if !version.starts_with("HTTP/1.") {
        return Err(Reply::status(400));
    }
    let mut head = Head {
        method: method.into(),
        target: target.into(),
        authorization: None,
        content_length: None,
        chunked: false,
    };
    let mut count = 0;
    for line in lines.filter(|line| !line.is_empty()) {
        count += 1;
        if count > MAX_HEADERS {
            return Err(Reply::status(400));
        }
        let (name, value) = line.split_once(':').ok_or(Reply::status(400))?;
        let value = value.trim();
        match name.trim().to_ascii_lowercase().as_str() {
            "authorization" => head.authorization = Some(value.into()),
            "content-length" => {
                // A repeated or non-numeric length is ambiguous: refuse it.
                if head.content_length.is_some() || !value.bytes().all(|b| b.is_ascii_digit()) {
                    return Err(Reply::status(400));
                }
                head.content_length = Some(value.parse().map_err(|_| Reply::status(413))?);
            }
            "transfer-encoding" => head.chunked = true,
            _ => {}
        }
    }
    Ok(head)
}

/// The route: `POST /v1/hook?agent=claude|codex`, and nothing else.
fn route(head: &Head) -> Result<Agent, Reply> {
    let (path, query) = head.target.split_once('?').unwrap_or((&head.target, ""));
    if path != "/v1/hook" {
        return Err(Reply::status(404));
    }
    if head.method != "POST" {
        return Err(Reply::status(405));
    }
    let mut agent = None;
    for pair in query.split('&').filter(|pair| !pair.is_empty()) {
        match pair.split_once('=') {
            Some(("agent", name)) if agent.is_none() => agent = Agent::from_name(name),
            _ => return Err(Reply::status(400)),
        }
    }
    agent.ok_or(Reply::status(400))
}

fn authorized(head: &Head, token: &str) -> bool {
    head.authorization
        .as_deref()
        .and_then(|value| value.strip_prefix("Bearer "))
        .is_some_and(|given| constant_time_eq(given.trim().as_bytes(), token.as_bytes()))
}

/// Turns one payload into the answer: parse it like the local hook, drop what a guest may not
/// say, deliver the rest.
pub fn process(context: &Context, agent: Agent, body: &[u8]) -> Reply {
    if !agent.hooks_enabled(&context.config) {
        return Reply::status(200);
    }
    let event = match agent.parse_hook(body) {
        Ok(Some(event)) => event,
        // Valid but not a turn end or wait (for example a SubagentStop): nothing to do.
        Ok(None) => return Reply::status(200),
        Err(_) => return Reply::status(400),
    };
    let busy = crate::agent::is_claude_stop(&event)
        && crate::agent::claude_has_background_work(body)
        && context.config.claude.suppress_while_busy;
    let mut event: Event = event;
    if busy {
        // As the local hook does: a Stop with background work only settles the session's waits.
        event.activity = true;
    } else if event.activity {
        // Per-tool and per-prompt activity is not forwarded from a guest.
        return Reply::status(200);
    }
    // Never a window or terminal of this machine: a guest's id means nothing here.
    event.window = None;
    event.tty = None;
    let reply = agent.hook_reply(&event).unwrap_or_default().to_owned();
    match (context.deliver)(Request::Event { event }) {
        Response::Accepted => Reply {
            status: 200,
            body: reply,
        },
        _ => Reply::status(503),
    }
}

/// Reads one request from `stream` and answers it. `deadline` bounds the whole exchange.
fn serve_connection(context: &Context, mut stream: TcpStream) {
    let deadline = Instant::now() + REQUEST_DEADLINE;
    let reply = read_and_process(context, &mut stream, deadline);
    if reply.status != 200 {
        log(Record::RemoteRejected(reject_code(reply.status)));
    }
    let text = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        reply.status,
        reply.reason(),
        reply.body.len(),
        reply.body
    );
    let _ = stream.set_write_timeout(Some(REQUEST_DEADLINE));
    let _ = stream.write_all(text.as_bytes());
    let _ = stream.flush();
}

fn read_some(stream: &mut TcpStream, buf: &mut [u8], deadline: Instant) -> Result<usize, Reply> {
    let left = deadline
        .checked_duration_since(Instant::now())
        .filter(|left| !left.is_zero())
        .ok_or(Reply::status(400))?;
    stream
        .set_read_timeout(Some(left))
        .map_err(|_| Reply::status(400))?;
    stream.read(buf).map_err(|_| Reply::status(400))
}

fn read_and_process(context: &Context, stream: &mut TcpStream, deadline: Instant) -> Reply {
    let mut buffer = Vec::with_capacity(1024);
    let mut chunk = [0u8; 1024];
    let end = loop {
        if let Some(at) = buffer.windows(4).position(|w| w == b"\r\n\r\n") {
            break at;
        }
        if buffer.len() > MAX_HEADER_BYTES {
            return Reply::status(400);
        }
        match read_some(stream, &mut chunk, deadline) {
            Ok(0) | Err(_) => return Reply::status(400),
            Ok(n) => buffer.extend_from_slice(&chunk[..n]),
        }
    };
    let Ok(text) = std::str::from_utf8(&buffer[..end]) else {
        return Reply::status(400);
    };
    let head = match parse_head(text) {
        Ok(head) => head,
        Err(reply) => return reply,
    };
    // Before anything else is said about the route or the body: no token, no answer.
    if !authorized(&head, &context.token) {
        return Reply::status(401);
    }
    // `aherald-ping --self-test`: proves the address and the token without making a card.
    if head.target == "/v1/ping" {
        return Reply {
            status: 200,
            body: "aherald".into(),
        };
    }
    let agent = match route(&head) {
        Ok(agent) => agent,
        Err(reply) => return reply,
    };
    if head.chunked {
        return Reply::status(400);
    }
    let Some(length) = head.content_length else {
        return Reply::status(411);
    };
    if length > MAX_BODY {
        return Reply::status(413);
    }
    let mut body = buffer[end + 4..].to_vec();
    if body.len() > length {
        return Reply::status(400);
    }
    while body.len() < length {
        match read_some(stream, &mut chunk, deadline) {
            Ok(0) | Err(_) => return Reply::status(400),
            Ok(n) => body.extend_from_slice(&chunk[..n.min(length + 1 - body.len())]),
        }
        if body.len() > length {
            return Reply::status(400);
        }
    }
    process(context, agent, &body)
}

/// Starts the listener when `[remote]` is enabled. The token lives beside the configuration.
/// A token that cannot be had leaves the listener off and says so in the log.
pub fn start_if_enabled(
    config: &Config,
    config_path: &std::path::Path,
    deliver: Deliver,
) -> Option<Remote> {
    if !config.remote.enabled {
        return None;
    }
    let directory = config_path
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."));
    let (bind, token) = match (config.remote.socket_addr(), load_or_create_token(directory)) {
        (Ok(bind), Ok(token)) => (bind, token),
        _ => {
            log(Record::RemoteBindFailed);
            return None;
        }
    };
    let context = Arc::new(Context {
        config: config.clone(),
        token,
        deliver,
    });
    Some(Remote::start(bind, context))
}

/// What `aherald --remote-info` prints: the two lines a guest needs, in the form `docker
/// --env-file` and a shell `export` accept, with comments about what to check on this side. The
/// token file is created on first use.
pub fn info(config: &Config, config_path: &std::path::Path) -> Result<String, String> {
    let directory = config_path
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."));
    let token = load_or_create_token(directory)?;
    let mut text = format!(
        "AHERALD_ENDPOINT=http://{}\nAHERALD_TOKEN={token}\n",
        config.remote.bind
    );
    text.push_str(
        "# In the guest, use the address that reaches this machine instead of the host above.\n",
    );
    if !config.remote.enabled {
        text.push_str(
            "# [remote] is off: set enabled = true in config.toml and restart aHerald.\n",
        );
    } else if !config
        .remote
        .socket_addr()
        .is_ok_and(|addr| addr.ip().is_loopback())
    {
        text.push_str(
            "# The listener is not on loopback: limit who can reach it with a firewall rule.\n",
        );
    }
    Ok(text)
}

/// A running listener; dropping it stops it.
pub struct Remote {
    stop: Arc<AtomicBool>,
    bound: Arc<std::sync::Mutex<Option<SocketAddr>>>,
    thread: Option<JoinHandle<()>>,
}

impl Remote {
    /// Starts listening on `bind`. A failed bind is logged and tried again every 30 seconds, so
    /// an address that appears later (a container bridge) starts working without a restart.
    pub fn start(bind: SocketAddr, context: Arc<Context>) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let bound = Arc::new(std::sync::Mutex::new(None));
        let thread = {
            let (stop, bound) = (stop.clone(), bound.clone());
            std::thread::spawn(move || run(bind, &context, &stop, &bound))
        };
        Self {
            stop,
            bound,
            thread: Some(thread),
        }
    }

    /// The address actually bound, once the listener is up.
    pub fn local_addr(&self) -> Option<SocketAddr> {
        self.bound.lock().ok().and_then(|bound| *bound)
    }
}

impl Drop for Remote {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn run(
    bind: SocketAddr,
    context: &Arc<Context>,
    stop: &AtomicBool,
    bound: &std::sync::Mutex<Option<SocketAddr>>,
) {
    let active = Arc::new(AtomicUsize::new(0));
    let mut failed = false;
    while !stop.load(Ordering::Acquire) {
        let listener =
            match TcpListener::bind(bind).and_then(|l| l.set_nonblocking(true).map(|()| l)) {
                Ok(listener) => listener,
                Err(_) => {
                    if !failed {
                        log(Record::RemoteBindFailed);
                        failed = true;
                    }
                    let until = Instant::now() + RETRY_BIND;
                    while Instant::now() < until && !stop.load(Ordering::Acquire) {
                        std::thread::sleep(POLL);
                    }
                    continue;
                }
            };
        if let Ok(mut slot) = bound.lock() {
            *slot = listener.local_addr().ok();
        }
        log(Record::RemoteListening);
        while !stop.load(Ordering::Acquire) {
            match listener.accept() {
                Ok((stream, _)) => {
                    let _ = stream.set_nonblocking(false);
                    if active.fetch_add(1, Ordering::AcqRel) >= MAX_CONNECTIONS {
                        active.fetch_sub(1, Ordering::AcqRel);
                        let _ = busy(stream);
                        continue;
                    }
                    let (context, active) = (context.clone(), active.clone());
                    std::thread::spawn(move || {
                        serve_connection(&context, stream);
                        active.fetch_sub(1, Ordering::AcqRel);
                    });
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => std::thread::sleep(POLL),
                Err(_) => std::thread::sleep(POLL),
            }
        }
        return;
    }
}

fn busy(mut stream: TcpStream) -> std::io::Result<()> {
    log(Record::RemoteRejected("busy"));
    stream.set_write_timeout(Some(Duration::from_millis(200)))?;
    stream.write_all(
        b"HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
    )
}

/// 256 random bits as 64 hexadecimal characters, from the operating system.
#[cfg(unix)]
pub fn generate_token() -> Result<String, String> {
    let mut bytes = [0u8; 32];
    std::fs::File::open("/dev/urandom")
        .and_then(|mut file| file.read_exact(&mut bytes))
        .map_err(|_| "cannot read random bytes from the system")?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

#[cfg(windows)]
pub fn generate_token() -> Result<String, String> {
    use windows::Win32::Security::Cryptography::{
        BCRYPT_USE_SYSTEM_PREFERRED_RNG, BCryptGenRandom,
    };
    let mut bytes = [0u8; 32];
    // SAFETY: the buffer is valid for its length and no algorithm handle is passed.
    let status = unsafe { BCryptGenRandom(None, &mut bytes, BCRYPT_USE_SYSTEM_PREFERRED_RNG) };
    if status.is_err() {
        return Err("cannot read random bytes from the system".into());
    }
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

/// The token in `directory/remote-token`, created (readable by this user only) when absent.
pub fn load_or_create_token(directory: &std::path::Path) -> Result<String, String> {
    let path = directory.join("remote-token");
    if let Ok(text) = std::fs::read_to_string(&path) {
        let token = text.trim();
        if token.len() == 64 && token.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Ok(token.to_owned());
        }
        return Err("the remote token file is damaged; delete it to create a new one".into());
    }
    let token = generate_token()?;
    write_private(&path, &token)?;
    Ok(token)
}

#[cfg(unix)]
fn write_private(path: &std::path::Path, text: &str) -> Result<(), String> {
    use std::os::unix::fs::OpenOptionsExt;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .map_err(|e| format!("cannot create the remote token file: {e}"))?;
    file.write_all(text.as_bytes())
        .map_err(|e| format!("cannot write the remote token file: {e}"))
}

#[cfg(windows)]
fn write_private(path: &std::path::Path, text: &str) -> Result<(), String> {
    // The file sits beside the portable configuration, which only this user can reach.
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .and_then(|mut file| file.write_all(text.as_bytes()))
        .map_err(|e| format!("cannot create the remote token file: {e}"))
}

#[cfg(test)]
mod info_tests {
    use super::*;

    #[test]
    fn the_info_names_the_endpoint_and_the_stored_token() {
        let dir = std::env::temp_dir().join(format!("aherald-info-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let config = Config::parse(
            "[remote]\nenabled = true\nbind = \"172.17.0.1:5000\"\n",
            &dir,
        )
        .unwrap();
        let text = info(&config, &dir.join("config.toml")).unwrap();
        let token = load_or_create_token(&dir).unwrap();
        assert!(text.starts_with(&format!(
            "AHERALD_ENDPOINT=http://172.17.0.1:5000\nAHERALD_TOKEN={token}\n"
        )));
        assert!(text.contains("firewall"));
        let off = info(&Config::parse("", &dir).unwrap(), &dir.join("config.toml")).unwrap();
        assert!(off.contains("[remote] is off"));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    const TOKEN: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    fn context(config: &str) -> (Arc<Context>, Arc<Mutex<Vec<Request>>>) {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let deliver: Deliver = {
            let seen = seen.clone();
            Arc::new(move |request| {
                seen.lock().unwrap().push(request);
                Response::Accepted
            })
        };
        let config = Config::parse(config, std::path::Path::new(".")).unwrap();
        (
            Arc::new(Context {
                config,
                token: TOKEN.into(),
                deliver,
            }),
            seen,
        )
    }

    fn post(addr: SocketAddr, token: Option<&str>, target: &str, body: &[u8]) -> (u16, String) {
        let mut stream = TcpStream::connect(addr).unwrap();
        let auth = token.map_or(String::new(), |t| format!("Authorization: Bearer {t}\r\n"));
        let head = format!(
            "POST {target} HTTP/1.1\r\nHost: x\r\n{auth}Content-Length: {}\r\n\r\n",
            body.len()
        );
        stream.write_all(head.as_bytes()).unwrap();
        stream.write_all(body).unwrap();
        response(stream)
    }

    fn response(mut stream: TcpStream) -> (u16, String) {
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut text = String::new();
        let _ = stream.read_to_string(&mut text);
        let status = text
            .split(' ')
            .nth(1)
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        let body = text
            .split_once("\r\n\r\n")
            .map_or("", |(_, b)| b)
            .to_owned();
        (status, body)
    }

    fn listen(config: &str) -> (Remote, SocketAddr, Arc<Mutex<Vec<Request>>>) {
        let (context, seen) = context(config);
        let remote = Remote::start("127.0.0.1:0".parse().unwrap(), context);
        let deadline = Instant::now() + Duration::from_secs(5);
        while remote.local_addr().is_none() {
            assert!(Instant::now() < deadline, "the listener did not start");
            std::thread::sleep(Duration::from_millis(5));
        }
        let addr = remote.local_addr().unwrap();
        (remote, addr, seen)
    }

    const STOP: &[u8] = br#"{"hook_event_name":"Stop","session_id":"s-1","cwd":"/work/project"}"#;
    const CODEX_STOP: &[u8] =
        br#"{"hook_event_name":"Stop","session_id":"c-1","turn_id":"t-1","cwd":"/w/p"}"#;

    #[test]
    fn a_claude_stop_becomes_an_event_without_a_window() {
        let (_remote, addr, seen) = listen("");
        let (status, body) = post(addr, Some(TOKEN), "/v1/hook?agent=claude", STOP);
        assert_eq!((status, body.as_str()), (200, ""));
        let seen = seen.lock().unwrap();
        let [Request::Event { event }] = seen.as_slice() else {
            panic!("{seen:?}");
        };
        assert_eq!(event.session_id.as_deref(), Some("s-1"));
        assert_eq!(event.window, None);
        assert_eq!(event.tty, None);
    }

    #[test]
    fn a_codex_stop_is_answered_with_the_hook_reply() {
        let (_remote, addr, seen) = listen("");
        let (status, body) = post(addr, Some(TOKEN), "/v1/hook?agent=codex", CODEX_STOP);
        assert_eq!((status, body.as_str()), (200, "{}"));
        assert_eq!(seen.lock().unwrap().len(), 1);
    }

    #[test]
    fn a_window_in_the_payload_is_ignored() {
        let (_remote, addr, seen) = listen("");
        let body =
            br#"{"hook_event_name":"Stop","session_id":"s-1","window":4242,"tty":"/dev/pts/9"}"#;
        post(addr, Some(TOKEN), "/v1/hook?agent=claude", body);
        let seen = seen.lock().unwrap();
        let [Request::Event { event }] = seen.as_slice() else {
            panic!("{seen:?}");
        };
        assert_eq!((event.window, event.tty.as_deref()), (None, None));
    }

    #[test]
    fn tool_and_prompt_activity_is_not_delivered() {
        let (_remote, addr, seen) = listen("");
        for name in ["PreToolUse", "PostToolUse", "UserPromptSubmit"] {
            let body = format!(r#"{{"hook_event_name":"{name}","session_id":"s-1"}}"#);
            let (status, _) = post(addr, Some(TOKEN), "/v1/hook?agent=claude", body.as_bytes());
            assert_eq!(status, 200, "{name}");
        }
        assert!(seen.lock().unwrap().is_empty());
    }

    #[test]
    fn a_stop_with_background_work_only_settles_waits() {
        let (_remote, addr, seen) = listen("");
        let body = br#"{"hook_event_name":"Stop","session_id":"s-1","background_tasks":[{"type":"bash"}]}"#;
        post(addr, Some(TOKEN), "/v1/hook?agent=claude", body);
        let seen = seen.lock().unwrap();
        let [Request::Event { event }] = seen.as_slice() else {
            panic!("{seen:?}");
        };
        assert!(event.activity);
    }

    #[test]
    fn a_session_end_and_a_permission_request_are_delivered() {
        let (_remote, addr, seen) = listen("");
        for body in [
            r#"{"hook_event_name":"SessionEnd","session_id":"s-1"}"#,
            r#"{"hook_event_name":"PermissionRequest","session_id":"s-1","tool_name":"Bash","tool_input":{"command":"ls"}}"#,
        ] {
            let (status, _) = post(addr, Some(TOKEN), "/v1/hook?agent=claude", body.as_bytes());
            assert_eq!(status, 200);
        }
        assert_eq!(seen.lock().unwrap().len(), 2);
    }

    #[test]
    fn disabled_hooks_of_an_agent_deliver_nothing() {
        let (_remote, addr, seen) = listen("[claude]\nhooks_enabled = false\n");
        let (status, _) = post(addr, Some(TOKEN), "/v1/hook?agent=claude", STOP);
        assert_eq!(status, 200);
        assert!(seen.lock().unwrap().is_empty());
    }

    #[test]
    fn a_wrong_or_missing_token_is_refused_before_the_route_is_checked() {
        let (_remote, addr, seen) = listen("");
        let wrong = TOKEN.replace('0', "1");
        for token in [None, Some(wrong.as_str()), Some(""), Some("short")] {
            for target in ["/v1/hook?agent=claude", "/nothing"] {
                assert_eq!(post(addr, token, target, STOP).0, 401, "{token:?} {target}");
            }
        }
        assert!(seen.lock().unwrap().is_empty());
    }

    #[test]
    fn the_ping_route_needs_the_token_and_makes_no_event() {
        let (_remote, addr, seen) = listen("");
        assert_eq!(post(addr, None, "/v1/ping", b"").0, 401);
        let (status, body) = post(addr, Some(TOKEN), "/v1/ping", b"");
        assert_eq!((status, body.as_str()), (200, "aherald"));
        assert!(seen.lock().unwrap().is_empty());
    }

    #[test]
    fn other_routes_methods_and_agents_are_refused() {
        let (_remote, addr, _seen) = listen("");
        assert_eq!(post(addr, Some(TOKEN), "/nothing", STOP).0, 404);
        assert_eq!(post(addr, Some(TOKEN), "/v1/hook", STOP).0, 400);
        assert_eq!(
            post(addr, Some(TOKEN), "/v1/hook?agent=opencode", STOP).0,
            400
        );
        assert_eq!(
            post(addr, Some(TOKEN), "/v1/hook?agent=claude&x=1", STOP).0,
            400
        );
        let mut stream = TcpStream::connect(addr).unwrap();
        write!(
            stream,
            "GET /v1/hook?agent=claude HTTP/1.1\r\nAuthorization: Bearer {TOKEN}\r\n\r\n"
        )
        .unwrap();
        assert_eq!(response(stream).0, 405);
    }

    #[test]
    fn a_malformed_or_oversized_body_is_refused() {
        let (_remote, addr, seen) = listen("");
        assert_eq!(
            post(addr, Some(TOKEN), "/v1/hook?agent=claude", b"{nope").0,
            400
        );
        let big = vec![b' '; MAX_BODY + 1];
        assert_eq!(
            post(addr, Some(TOKEN), "/v1/hook?agent=claude", &big).0,
            413
        );
        let mut stream = TcpStream::connect(addr).unwrap();
        write!(
            stream,
            "POST /v1/hook?agent=claude HTTP/1.1\r\nAuthorization: Bearer {TOKEN}\r\nTransfer-Encoding: chunked\r\n\r\n"
        )
        .unwrap();
        assert_eq!(response(stream).0, 400);
        let mut stream = TcpStream::connect(addr).unwrap();
        write!(
            stream,
            "POST /v1/hook?agent=claude HTTP/1.1\r\nAuthorization: Bearer {TOKEN}\r\n\r\n"
        )
        .unwrap();
        assert_eq!(response(stream).0, 411);
        assert!(seen.lock().unwrap().is_empty());
    }

    #[test]
    fn a_client_that_stalls_is_cut_off_at_the_deadline() {
        let (_remote, addr, _seen) = listen("");
        let started = Instant::now();
        let mut stream = TcpStream::connect(addr).unwrap();
        stream
            .write_all(b"POST /v1/hook?agent=claude HTTP/1.1\r\n")
            .unwrap();
        let (status, _) = response(stream);
        assert_eq!(status, 400);
        assert!(
            started.elapsed() < Duration::from_secs(3),
            "{:?}",
            started.elapsed()
        );
    }

    #[test]
    fn connections_beyond_the_cap_are_told_to_retry() {
        let (_remote, addr, _seen) = listen("");
        // Idle connections hold the slots until the deadline.
        let held: Vec<_> = (0..MAX_CONNECTIONS)
            .map(|_| TcpStream::connect(addr).unwrap())
            .collect();
        std::thread::sleep(Duration::from_millis(200));
        let (status, _) = response(TcpStream::connect(addr).unwrap());
        assert_eq!(status, 503);
        drop(held);
    }

    #[test]
    fn a_taken_address_is_retried_and_the_listener_stops_on_drop() {
        let taken = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = taken.local_addr().unwrap();
        let (context, _seen) = context("");
        let remote = Remote::start(addr, context);
        std::thread::sleep(Duration::from_millis(200));
        assert_eq!(remote.local_addr(), None);
        let started = Instant::now();
        drop(remote);
        assert!(started.elapsed() < Duration::from_secs(2));
    }

    #[test]
    fn the_token_is_created_once_and_kept_private() {
        let dir = std::env::temp_dir().join(format!("aherald-token-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let first = load_or_create_token(&dir).unwrap();
        assert_eq!(first.len(), 64);
        assert_eq!(load_or_create_token(&dir).unwrap(), first);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(dir.join("remote-token"))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600);
        }
        std::fs::write(dir.join("remote-token"), "nonsense").unwrap();
        assert!(load_or_create_token(&dir).is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn comparison_does_not_depend_on_where_strings_differ() {
        assert!(constant_time_eq(b"abc", b"abc"));
        assert!(!constant_time_eq(b"abc", b"abd"));
        assert!(!constant_time_eq(b"abc", b"abcd"));
        assert!(!constant_time_eq(b"", b"a"));
    }
}
