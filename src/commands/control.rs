//! Questions to, and orders for, the running resident, and the reports read from disk.
use super::{config_path, executable};
use aherald::cli::Cli;

/// `--remote-info`: the endpoint and token a guest needs.
pub fn remote_info(cli: &Cli) -> Result<(), String> {
    let path = config_path(cli)?;
    let config = aherald::config::Config::load(&path, cli.config.is_some())?;
    print!("{}", aherald::remote::info(&config, &path)?);
    Ok(())
}

/// `--perf-report`: the timings of the debug log.
pub fn perf_report(cli: &Cli) -> Result<(), String> {
    let path = config_path(cli)?;
    let exe = executable()?;
    let directory = aherald::paths::diagnostic_dir_with(&exe, &path, |key| std::env::var_os(key))?;
    print!("{}", aherald::perf_report::report(&directory)?);
    Ok(())
}

/// `--status` and `--quit`: ask the resident, or say that none runs.
pub fn status_or_quit(cli: &Cli) -> Result<(), String> {
    // The configuration is not read, but a bad `--config` path is still reported first.
    config_path(cli)?;
    let scope = aherald::local::Scope::current("resident")?;
    let request = if cli.operation == Some(aherald::cli::Operation::Quit) {
        aherald::ipc::Request::Quit
    } else {
        aherald::ipc::Request::Status
    };
    match aherald::ipc::pipe::request(&scope, &request, std::time::Duration::from_millis(100)) {
        Ok(response) => println!(
            "{}",
            serde_json::to_string(&response).map_err(|_| "cannot encode status")?
        ),
        Err(_) => println!("{{\"running\":false}}"),
    }
    Ok(())
}

/// `--raw-output`: prints the last unparsed answer of a source exactly as the resident kept it.
/// The text goes to stdout, where it came from and how old it is to stderr.
pub fn raw_output(cli: &Cli) -> Result<(), String> {
    use aherald::ipc::{Request, Response};
    use std::io::Write;
    config_path(cli)?;
    let source = cli
        .raw_source
        .ok_or("--raw-output expects usage or statusline")?;
    let scope = aherald::local::Scope::current("resident")?;
    let response = aherald::ipc::pipe::request(
        &scope,
        &Request::RawOutput { source },
        std::time::Duration::from_millis(1000),
    )
    .map_err(|_| {
        "no running aHerald of this version answered (not running, or started before an upgrade)"
    })?;
    match response {
        Response::RawOutput {
            found: true,
            note,
            age_secs,
            body,
        } => {
            let mut out = std::io::stdout();
            let _ = out.write_all(body.as_bytes());
            if !body.ends_with('\n') {
                let _ = out.write_all(b"\n");
            }
            let _ = out.flush();
            eprintln!(
                "aHerald: {note}, received {}, {} bytes",
                aherald::card::age_text(age_secs.saturating_mul(1000)),
                body.len()
            );
            Ok(())
        }
        Response::RawOutput { note, .. } => Err(note),
        _ => Err("the running aHerald does not support --raw-output (restart it)".into()),
    }
}
