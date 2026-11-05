//! `aherald-ping`: the hook of an agent that runs in a container or virtual machine, where the
//! resident's same-user pipe cannot be reached. It reads the hook payload from stdin, keeps only
//! turn ends and waits, strips the payload down to the fields aHerald reads, and posts it to the
//! `[remote]` listener of aHerald on the host (`POST /v1/hook?agent=...`).
//!
//! A hook must never fail or delay its agent: every error ends in exit 0, and the whole exchange
//! is abandoned after one second. Only `std` and `serde_json`; no TLS, no proxy support.
use serde_json::{Map, Value};
use std::{
    io::{Read, Write},
    net::{TcpStream, ToSocketAddrs},
    path::PathBuf,
    sync::mpsc,
    time::{Duration, Instant},
};

/// The listener refuses more; so do the local hooks.
pub const MAX_INPUT: usize = 64 * 1024;
/// A hook is abandoned after this, whatever the network does.
pub const DEADLINE: Duration = Duration::from_secs(1);
const CONNECT: Duration = Duration::from_millis(500);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Agent {
    Codex,
    Claude,
}

impl Agent {
    pub fn name(self) -> &'static str {
        match self {
            Self::Codex => "codex",
            Self::Claude => "claude",
        }
    }
}

/// What the command line asked for.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Cli {
    pub agent: Option<Agent>,
    pub endpoint: Option<String>,
    pub token_file: Option<PathBuf>,
    pub self_test: bool,
}

/// Accepts what `aherald --hook` accepts (`--hook`, `--agent`, `--resident`, `--transient`) so a
/// guest's hook line only changes the program, plus `--endpoint`, `--token-file`, `--self-test`.
pub fn parse_args(args: impl IntoIterator<Item = String>) -> Result<Cli, String> {
    let mut cli = Cli::default();
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--hook" | "--resident" | "--transient" => {}
            "--self-test" => cli.self_test = true,
            "--agent" => {
                cli.agent = Some(match args.next().as_deref() {
                    Some("claude") => Agent::Claude,
                    Some("codex") => Agent::Codex,
                    _ => return Err("--agent needs claude or codex".into()),
                });
            }
            "--endpoint" => cli.endpoint = Some(args.next().ok_or("--endpoint needs a value")?),
            "--token-file" => {
                cli.token_file = Some(args.next().ok_or("--token-file needs a path")?.into());
            }
            _ => return Err("unknown argument".into()),
        }
    }
    Ok(cli)
}

/// Whether the host takes this event; everything else (tool and prompt activity, subagents,
/// idle reminders) never leaves the guest. Public only so that `tests/parser_contract.rs` can hold
/// it to what the host's hook parser accepts.
#[doc(hidden)]
pub fn wanted(agent: Agent, value: &Value) -> bool {
    let name = value.get("hook_event_name").and_then(Value::as_str);
    match (agent, name) {
        (Agent::Claude, Some("Stop" | "StopFailure" | "PermissionRequest" | "SessionEnd")) => true,
        (Agent::Claude, Some("Notification")) => matches!(
            value.get("notification_type").and_then(Value::as_str),
            Some("permission_prompt" | "elicitation_dialog" | "agent_needs_input")
        ),
        (Agent::Codex, Some("Stop" | "PermissionRequest" | "Interrupt" | "SessionEnd")) => true,
        _ => false,
    }
}

/// The last component of a path, which is all the host keeps of it (the folder name).
fn folder(path: &str) -> Option<&str> {
    path.rsplit(['/', '\\']).find(|part| !part.is_empty())
}

/// The payload reduced to the fields the host reads. Tool input, transcript paths, prompts and
/// the contents of background tasks are never copied: the full working directory becomes its
/// last component, a task becomes its type, a wakeup becomes whether it recurs.
pub fn redact(value: &Value) -> Value {
    let mut out = Map::new();
    for key in [
        "hook_event_name",
        "session_id",
        "turn_id",
        "agent_id",
        "notification_type",
    ] {
        if let Some(text) = value.get(key).filter(|v| v.is_string()) {
            out.insert(key.into(), text.clone());
        }
    }
    if let Some(name) = value.get("cwd").and_then(Value::as_str).and_then(folder) {
        out.insert("cwd".into(), name.into());
    }
    if let Some(tasks) = value.get("background_tasks").and_then(Value::as_array) {
        let tasks = tasks
            .iter()
            .map(|task| match task.get("type").filter(|t| t.is_string()) {
                Some(kind) => serde_json::json!({ "type": kind }),
                None => serde_json::json!({}),
            })
            .collect();
        out.insert("background_tasks".into(), Value::Array(tasks));
    }
    if let Some(crons) = value.get("session_crons").and_then(Value::as_array) {
        let crons = crons
            .iter()
            .map(
                |cron| match cron.get("recurring").filter(|r| r.is_boolean()) {
                    Some(recurring) => serde_json::json!({ "recurring": recurring }),
                    None => serde_json::json!({}),
                },
            )
            .collect();
        out.insert("session_crons".into(), Value::Array(crons));
    }
    Value::Object(out)
}

/// The body to post for `input`, or `None` when the host takes nothing from it.
pub fn prepare(agent: Agent, input: &[u8]) -> Option<Vec<u8>> {
    let value: Value = serde_json::from_slice(input).ok()?;
    wanted(agent, &value).then(|| redact(&value).to_string().into_bytes())
}

/// `http://host:port` split into the address to connect to and the `Host` header value.
pub fn parse_endpoint(endpoint: &str) -> Result<String, String> {
    let rest = endpoint
        .strip_prefix("http://")
        .ok_or("the endpoint must start with http://")?;
    let authority = rest.trim_end_matches('/');
    if authority.is_empty() || authority.contains(['/', '?', '#', '@', ' ']) {
        return Err("the endpoint must be http://host:port".into());
    }
    if !authority
        .rsplit_once(':')
        .is_some_and(|(_, port)| port.parse::<u16>().is_ok())
    {
        return Err("the endpoint needs a port".into());
    }
    Ok(authority.to_owned())
}

/// One request and its answer, bounded by `deadline`. Returns the status and the body.
pub fn request(
    authority: &str,
    token: &str,
    target: &str,
    body: &[u8],
    deadline: Instant,
) -> Result<(u16, String), String> {
    let left = |what: &str| {
        deadline
            .checked_duration_since(Instant::now())
            .filter(|d| !d.is_zero())
            .ok_or_else(|| format!("{what}: out of time"))
    };
    let addr = authority
        .to_socket_addrs()
        .map_err(|_| "cannot resolve the host")?
        .next()
        .ok_or("cannot resolve the host")?;
    let mut stream = TcpStream::connect_timeout(&addr, CONNECT.min(left("connect")?))
        .map_err(|_| "cannot connect to the host")?;
    let (write, read) = (left("write")?, left("read")?);
    stream
        .set_write_timeout(Some(write))
        .and_then(|()| stream.set_read_timeout(Some(read)))
        .map_err(|_| "cannot set timeouts")?;
    let head = format!(
        "POST {target} HTTP/1.1\r\nHost: {authority}\r\nAuthorization: Bearer {token}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream
        .write_all(head.as_bytes())
        .and_then(|()| stream.write_all(body))
        .map_err(|_| "cannot send the request")?;
    let mut text = Vec::new();
    // The listener closes after one answer; a few KiB is far more than it ever sends.
    stream
        .take(8 * 1024)
        .read_to_end(&mut text)
        .map_err(|_| "no answer from the host")?;
    let text = String::from_utf8_lossy(&text);
    let status = text
        .split(' ')
        .nth(1)
        .and_then(|s| s.parse().ok())
        .ok_or("the host's answer is not HTTP")?;
    let body = text.split_once("\r\n\r\n").map_or("", |(_, b)| b);
    Ok((status, body.to_owned()))
}

/// Where the endpoint and token come from: flags first, then the environment.
pub struct Settings {
    pub endpoint: String,
    pub token: String,
}

pub fn settings(cli: &Cli, env: impl Fn(&str) -> Option<String>) -> Result<Settings, String> {
    let endpoint = cli
        .endpoint
        .clone()
        .or_else(|| env("AHERALD_ENDPOINT"))
        .filter(|v| !v.is_empty())
        .ok_or("no endpoint: set AHERALD_ENDPOINT or pass --endpoint")?;
    let token = match &cli.token_file {
        Some(path) => std::fs::read_to_string(path).map_err(|_| "cannot read the token file")?,
        None => env("AHERALD_TOKEN").unwrap_or_default(),
    };
    let token = token.trim().to_owned();
    if token.is_empty() {
        return Err("no token: set AHERALD_TOKEN or pass --token-file".into());
    }
    Ok(Settings { endpoint, token })
}

/// The hook: what to print on stdout. `None` means print nothing. Never fails; every problem
/// is a quiet `None`, because the agent must not notice.
pub fn hook(cli: &Cli, input: &[u8], env: impl Fn(&str) -> Option<String>) -> Option<String> {
    let agent = cli.agent.unwrap_or(Agent::Codex);
    let body = prepare(agent, input)?;
    let settings = settings(cli, env).ok()?;
    let authority = parse_endpoint(&settings.endpoint).ok()?;
    let target = format!("/v1/hook?agent={}", agent.name());
    let deadline = Instant::now() + DEADLINE;
    let (status, reply) = request(&authority, &settings.token, &target, &body, deadline).ok()?;
    // As the local hook and the scripts: only the Codex Stop acknowledgement is passed on.
    (status == 200 && agent == Agent::Codex && reply == "{}").then_some(reply)
}

/// Runs `work` and gives up after `limit`: a name lookup can outlast every socket timeout.
pub fn within<T: Send + 'static>(
    limit: Duration,
    work: impl FnOnce() -> T + Send + 'static,
) -> Option<T> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(work());
    });
    rx.recv_timeout(limit).ok()
}

/// `--self-test`: whether the endpoint and token reach an aHerald that accepts them.
pub fn self_test(cli: &Cli, env: impl Fn(&str) -> Option<String>) -> Result<String, String> {
    let settings = settings(cli, env)?;
    let authority = parse_endpoint(&settings.endpoint)?;
    let (status, body) = request(
        &authority,
        &settings.token,
        "/v1/ping",
        b"",
        Instant::now() + Duration::from_secs(3),
    )?;
    match (status, body.as_str()) {
        (200, "aherald") => Ok(format!("ok: aHerald answered at {authority}")),
        (401, _) => Err("the host refused the token".into()),
        (200, _) => Err("something else answered; is this the aHerald [remote] port?".into()),
        (status, _) => Err(format!("the host answered {status}")),
    }
}

/// Reads stdin up to the limit; a payload over it is cut and so no longer parses.
pub fn read_input(mut reader: impl Read) -> Vec<u8> {
    let mut bytes = Vec::new();
    let _ = (&mut reader)
        .take(MAX_INPUT as u64 + 1)
        .read_to_end(&mut bytes);
    // The rest is drained by the caller's exit; an over-long payload is dropped as invalid.
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn args(list: &[&str]) -> Result<Cli, String> {
        parse_args(list.iter().map(|s| (*s).to_owned()))
    }

    #[test]
    fn the_local_hook_line_is_accepted_unchanged() {
        let cli = args(&["--hook", "--agent", "claude", "--resident"]).unwrap();
        assert_eq!(cli.agent, Some(Agent::Claude));
        assert_eq!(args(&["--hook", "--transient"]).unwrap().agent, None);
        assert!(args(&["--agent", "gemini"]).is_err());
        assert!(args(&["--frobnicate"]).is_err());
        assert!(args(&["--endpoint"]).is_err());
        let cli = args(&[
            "--endpoint",
            "http://h:1",
            "--token-file",
            "/t",
            "--self-test",
        ])
        .unwrap();
        assert!(cli.self_test);
        assert_eq!(cli.endpoint.as_deref(), Some("http://h:1"));
    }

    #[test]
    fn only_turn_ends_and_waits_are_wanted() {
        for name in ["Stop", "StopFailure", "PermissionRequest", "SessionEnd"] {
            let value = json!({"hook_event_name": name});
            assert!(wanted(Agent::Claude, &value), "{name}");
        }
        for name in [
            "PreToolUse",
            "PostToolUse",
            "UserPromptSubmit",
            "SubagentStop",
            "Notification",
        ] {
            assert!(
                !wanted(Agent::Claude, &json!({"hook_event_name": name})),
                "{name}"
            );
        }
        for kind in [
            "permission_prompt",
            "elicitation_dialog",
            "agent_needs_input",
        ] {
            let value = json!({"hook_event_name": "Notification", "notification_type": kind});
            assert!(wanted(Agent::Claude, &value), "{kind}");
        }
        let idle = json!({"hook_event_name": "Notification", "notification_type": "idle_prompt"});
        assert!(!wanted(Agent::Claude, &idle));
        for name in ["Stop", "PermissionRequest", "Interrupt", "SessionEnd"] {
            assert!(
                wanted(Agent::Codex, &json!({"hook_event_name": name})),
                "{name}"
            );
        }
        // Claude's events that Codex does not have are not Codex's.
        assert!(!wanted(
            Agent::Codex,
            &json!({"hook_event_name": "StopFailure"})
        ));
        assert!(!wanted(
            Agent::Codex,
            &json!({"hook_event_name": "PostModelRequest"})
        ));
        assert!(!wanted(Agent::Codex, &json!({})));
    }

    #[test]
    fn nothing_but_the_fields_the_host_reads_survives() {
        let payload = json!({
            "hook_event_name": "PermissionRequest",
            "session_id": "s-1",
            "turn_id": "t-1",
            "agent_id": "a-1",
            "notification_type": "permission_prompt",
            "cwd": "/home/me/secret-project/",
            "transcript_path": "/home/me/.claude/projects/x.jsonl",
            "prompt": "my private prompt",
            "tool_name": "Bash",
            "tool_input": {"command": "cat ~/.ssh/id_rsa"},
            "last_assistant_message": "private",
            "background_tasks": [{"type": "shell", "command": "make secret", "id": "1"}],
            "session_crons": [{"recurring": false, "prompt": "private"}, {"schedule": "x"}],
        });
        let out = redact(&payload);
        assert_eq!(
            out,
            json!({
                "hook_event_name": "PermissionRequest",
                "session_id": "s-1",
                "turn_id": "t-1",
                "agent_id": "a-1",
                "notification_type": "permission_prompt",
                "cwd": "secret-project",
                "background_tasks": [{"type": "shell"}],
                "session_crons": [{"recurring": false}, {}],
            })
        );
        let text = out.to_string();
        for secret in ["id_rsa", "private", "/home/me", "transcript", "make secret"] {
            assert!(!text.contains(secret), "{secret}");
        }
    }

    #[test]
    fn a_working_directory_keeps_only_its_last_component() {
        assert_eq!(folder("/a/b/c"), Some("c"));
        assert_eq!(folder("C:\\Users\\me\\proj\\"), Some("proj"));
        assert_eq!(folder("proj"), Some("proj"));
        assert_eq!(folder("/"), None);
        assert_eq!(folder(""), None);
    }

    #[test]
    fn wrongly_typed_fields_are_dropped_not_forwarded() {
        let out = redact(
            &json!({"session_id": 5, "cwd": ["/x"], "background_tasks": "x", "agent_id": {"a": 1}}),
        );
        assert_eq!(out, json!({}));
    }

    #[test]
    fn prepare_drops_unwanted_and_unparseable_input() {
        assert!(prepare(Agent::Claude, br#"{"hook_event_name":"PreToolUse"}"#).is_none());
        assert!(prepare(Agent::Claude, b"{nope").is_none());
        assert!(prepare(Agent::Claude, &vec![b' '; MAX_INPUT * 2]).is_none());
        let body = prepare(
            Agent::Claude,
            br#"{"hook_event_name":"Stop","session_id":"s"}"#,
        )
        .unwrap();
        assert_eq!(
            serde_json::from_slice::<Value>(&body).unwrap()["session_id"],
            "s"
        );
    }

    #[test]
    fn an_endpoint_is_http_host_and_port() {
        assert_eq!(
            parse_endpoint("http://172.17.0.1:47821").unwrap(),
            "172.17.0.1:47821"
        );
        assert_eq!(
            parse_endpoint("http://host.docker.internal:47821/").unwrap(),
            "host.docker.internal:47821"
        );
        for bad in [
            "https://h:1",
            "h:1",
            "http://",
            "http://h",
            "http://h:x",
            "http://u@h:1",
            "http://h:1/path",
            "http://h :1",
        ] {
            assert!(parse_endpoint(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn flags_beat_the_environment_and_a_token_file_is_trimmed() {
        let dir = std::env::temp_dir().join(format!("ping-token-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("t"), "  filetoken\n").unwrap();
        let env = |name: &str| match name {
            "AHERALD_ENDPOINT" => Some("http://env:1".to_owned()),
            "AHERALD_TOKEN" => Some("envtoken".to_owned()),
            _ => None,
        };
        let found = settings(&Cli::default(), env).unwrap();
        assert_eq!(
            (found.endpoint.as_str(), found.token.as_str()),
            ("http://env:1", "envtoken")
        );
        let cli = Cli {
            endpoint: Some("http://flag:2".into()),
            token_file: Some(dir.join("t")),
            ..Cli::default()
        };
        let found = settings(&cli, env).unwrap();
        assert_eq!(
            (found.endpoint.as_str(), found.token.as_str()),
            ("http://flag:2", "filetoken")
        );
        assert!(settings(&Cli::default(), |_| None).is_err());
        let no_token = |name: &str| (name == "AHERALD_ENDPOINT").then(|| "http://e:1".to_owned());
        assert!(settings(&Cli::default(), no_token).is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_slow_lookup_is_abandoned() {
        let started = Instant::now();
        let got = within(Duration::from_millis(100), || {
            std::thread::sleep(Duration::from_secs(2));
            1
        });
        assert_eq!(got, None);
        assert!(started.elapsed() < Duration::from_secs(1));
        assert_eq!(within(Duration::from_secs(1), || 7), Some(7));
    }
}
