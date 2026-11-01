//! Opt-in live-test bridge: real adapters and IPC, with a private resident scope.
//! Keeps ordinary installed hooks from contaminating an account acceptance test.
#[cfg(windows)]
fn main() -> Result<(), String> {
    use aherald::{
        cli::Cli,
        events,
        ipc::{Request, Response, pipe},
        local::Scope,
    };
    use std::time::{Duration, Instant};
    let cli = Cli::parse(std::env::args().skip(1))?;
    let namespace = std::env::var("AH101_SCOPE").map_err(|_| "missing private test scope")?;
    if !namespace.starts_with("ah101-") {
        return Err("a private ah101- scope is required".into());
    }
    let scope = Scope::current(&namespace)?;
    let request = match cli.operation.as_deref() {
        Some("status") => Request::Status,
        Some("quit") => Request::Quit,
        Some("hook") => {
            let started = Instant::now();
            let bytes = events::read_bounded(std::io::stdin(), events::INPUT_TIMEOUT)?;
            if let Some(report) = aherald::codex_context::parse(&bytes)? {
                return match pipe::request(
                    &scope,
                    &Request::CodexContext { report },
                    Duration::from_millis(500),
                ) {
                    Ok(Response::Accepted) => Ok(()),
                    _ => Err("private resident rejected context evidence".into()),
                };
            }
            let Some(event) = cli.hook_agent().parse_hook(&bytes)? else {
                return Ok(());
            };
            if let Some(reply) = cli.hook_agent().hook_reply(&event) {
                println!("{reply}");
            }
            let result = pipe::request(
                &scope,
                &Request::Event {
                    event: event.clone(),
                },
                Duration::from_millis(500),
            );
            // Persist only identity and the transport result, never raw input.
            let input: serde_json::Value =
                serde_json::from_slice(&bytes).map_err(|_| "invalid hook JSON")?;
            let name = input
                .get("hook_event_name")
                .and_then(|v| v.as_str())
                .filter(|name| {
                    matches!(
                        *name,
                        "UserPromptSubmit" | "PreToolUse" | "PostToolUse" | "Stop" | "SessionEnd"
                    )
                })
                .ok_or("unexpected hook event")?;
            let root = std::env::var_os("AH101_ROOT").ok_or("missing evidence directory")?;
            let record = serde_json::json!({
                "event": name,
                "session": event.session_id,
                "stage": "completed",
                "code": if matches!(result, Ok(Response::Accepted)) { 0 } else { 1 },
                "milliseconds": started.elapsed().as_secs_f64() * 1000.0,
                "reply": cli.hook_agent().hook_reply(&event).unwrap_or_default(),
            });
            std::fs::write(
                std::path::Path::new(&root).join(format!(
                    "{}-{name}-{}.json",
                    std::env::var("AH101_AGENT").map_err(|_| "missing agent")?,
                    std::process::id()
                )),
                serde_json::to_vec(&record).map_err(|_| "evidence encoding failed")?,
            )
            .map_err(|_| "evidence write failed")?;
            return match result {
                Ok(Response::Accepted) => Ok(()),
                _ => Err("private resident rejected live hook evidence".into()),
            };
        }
        Some("statusline") => {
            let bytes = events::read_bounded(std::io::stdin(), events::INPUT_TIMEOUT)?;
            let report = aherald::statusline::parse(&bytes)?;
            if let Some(root) = std::env::var_os("AH100_CONTEXT_ROOT") {
                let record = serde_json::json!({ "session": report.session_id, "context_tokens": report.context_tokens });
                std::fs::write(
                    std::path::Path::new(&root)
                        .join(format!("claude-context-{}.json", std::process::id())),
                    serde_json::to_vec(&record).map_err(|_| "evidence encoding failed")?,
                )
                .map_err(|_| "evidence write failed")?;
            }
            return match pipe::request(
                &scope,
                &Request::StatusLine { report },
                Duration::from_millis(500),
            ) {
                Ok(Response::Accepted) => Ok(()),
                _ => Err("private resident rejected statusLine evidence".into()),
            };
        }
        _ => {
            return Err(
                "only hook/statusline/status/quit are supported by this test bridge".into(),
            );
        }
    };
    match pipe::request(&scope, &request, Duration::from_millis(500)) {
        Ok(response) => match request {
            Request::Event { .. } if !matches!(response, Response::Accepted) => {
                Err("private resident rejected live hook evidence".into())
            }
            Request::Event { .. } => Ok(()),
            _ => {
                println!(
                    "{}",
                    serde_json::to_string(&response).map_err(|_| "status encoding failed")?
                );
                Ok(())
            }
        },
        Err(_) if matches!(request, Request::Status) => {
            println!("{{\"running\":false}}");
            Ok(())
        }
        Err(error) => Err(error),
    }
}

#[cfg(not(windows))]
fn main() {
    eprintln!("This optional live acceptance bridge requires Windows.");
    std::process::exit(2);
}
