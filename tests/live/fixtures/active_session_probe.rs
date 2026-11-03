//! Opt-in native UI probe. Synthetic events, a real terminal, and a private resident.
#[cfg(windows)]
fn main() -> Result<(), String> {
    use aherald::{
        config::Config,
        events::{Event, Source},
        ipc::{Request, pipe},
        local::Scope,
    };
    use std::{
        io::{BufRead, Write},
        path::PathBuf,
        time::Duration,
    };
    let mut args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.is_empty() {
        // No shell is needed for native UI automation: launch the built probe directly.
        let current = std::env::current_exe().map_err(|e| e.to_string())?;
        let debug = current
            .parent()
            .and_then(|p| p.parent())
            .ok_or("missing build directory")?;
        let target = debug
            .parent()
            .ok_or("missing target directory")?
            .join("ah-098");
        args = vec![
            debug.join("aherald.exe").into(),
            target.join("preview.toml").into(),
            target.join("evidence.jsonl").into(),
        ];
    }
    if args.len() != 3 {
        return Err("usage: active_session_probe AHERALD_EXE CONFIG EVIDENCE_JSONL".into());
    }
    let exe = PathBuf::from(&args[0]);
    let path = PathBuf::from(&args[1]);
    let config = Config::load(&path, true)?;
    let scope = Scope::current(&format!("ah098-probe-{}", std::process::id()))?;
    struct Cleanup(Scope);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = pipe::request(&self.0, &Request::Quit, Duration::from_secs(1));
        }
    }
    let _cleanup = Cleanup(scope.clone());
    let window = aherald::console_window::current();
    aherald::resident::ensure(&exe, &path, &scope)?;
    let mut evidence = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&args[2])
        .map_err(|e| e.to_string())?;
    println!(
        "AH-098: window={window:?}. Commands: claude, codex, other, delayed, quit. Delayed waits 8 seconds for a focus switch."
    );
    for (sequence, line) in std::io::stdin().lock().lines().enumerate() {
        let command = line.map_err(|e| e.to_string())?;
        if command == "quit" {
            break;
        }
        if !matches!(command.as_str(), "claude" | "codex" | "other" | "delayed") {
            continue;
        }
        if command == "delayed" {
            std::thread::sleep(Duration::from_secs(8));
        }
        let mut event = Event::demo("turn_complete")?;
        event.source = if command == "codex" {
            Source::CodexHook
        } else {
            Source::ClaudeHook
        };
        event.session_id = Some(format!("ah098-synthetic-{sequence}"));
        event.turn_id = Some(format!("turn-{sequence}"));
        event.window = if command == "other" { None } else { window };
        event.folder = Some("AH-098 synthetic".into());
        let observation = aherald::focus::observe(&event, &config);
        let outputs = aherald::focus::outputs(&config, observation);
        let response = pipe::request(&scope, &Request::Event { event }, Duration::from_secs(1))?;
        let record = serde_json::json!({"command":command,"focused":observation.window_focused,"session":format!("{:?}", observation.session),"window":outputs.window,"sound":outputs.sound,"accepted":matches!(response, aherald::ipc::Response::Accepted)});
        writeln!(evidence, "{record}").map_err(|e| e.to_string())?;
        println!("{record}");
    }
    Ok(())
}

#[cfg(not(windows))]
fn main() {
    eprintln!("This native acceptance probe requires Windows.");
}
