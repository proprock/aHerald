//! Setup operations: the native `--setup`/`--unsetup`, the settings report, the interactive and
//! windowed front ends, and the older hook and status-line file editors.
use super::executable;
use aherald::cli::{Cli, Operation};

/// `--setup-settings`: report the usage and warning flags without writing.
pub fn settings(cli: &Cli) -> Result<(), String> {
    let exe = executable()?;
    print!(
        "{}",
        aherald::setup::settings_state(&exe, cli.usage.file.as_deref())?
    );
    Ok(())
}

/// `--setup` and `--unsetup`.
pub fn native(cli: &Cli) -> Result<(), String> {
    let exe = executable()?;
    print!(
        "{}",
        aherald::setup::run_with_options(
            &exe,
            &cli.harnesses,
            cli.operation == Some(Operation::Unsetup),
            cli.apply,
            &cli.usage
        )?
    );
    Ok(())
}

/// `--setup-window`: the macOS setup window.
pub fn window() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    return aherald::mac::setup_window::run();
    #[cfg(not(target_os = "macos"))]
    Err("--setup-window is available on macOS only".into())
}

/// A path from the command line as the setup receipt needs it: absolute.
fn absolute(path: &std::path::Path) -> Result<std::path::PathBuf, String> {
    std::path::absolute(path).map_err(|e| format!("cannot resolve {}: {e}", path.display()))
}

/// `--install-statusline` and `--uninstall-statusline`.
pub fn statusline_file(cli: &Cli) -> Result<(), String> {
    let file = cli.hooks_file.as_deref().map(absolute).transpose()?;
    print!(
        "{}",
        aherald::setup::run_statusline_only(
            &executable()?,
            file.as_deref(),
            cli.operation == Some(Operation::UninstallStatusline),
            cli.apply,
        )?
    );
    Ok(())
}

/// `--install-hooks` and `--uninstall-hooks`.
pub fn hooks(cli: &Cli) -> Result<(), String> {
    let agent = cli.hooks_agent.ok_or("hook setup needs an agent")?;
    let file = cli.hooks_file.as_deref().map(absolute).transpose()?;
    print!(
        "{}",
        aherald::setup::run_hooks_only(
            &executable()?,
            agent,
            file.as_deref(),
            cli.operation == Some(Operation::UninstallHooks),
            cli.apply,
        )?
    );
    Ok(())
}

pub fn console() -> Result<(), String> {
    use std::io::{IsTerminal, Write};
    if !cfg!(target_os = "linux") {
        return Err("--console-setup is available on Linux only".into());
    }
    if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
        return Err("--console-setup requires an interactive terminal".into());
    }
    fn choice(prompt: &str, default: bool) -> Result<bool, String> {
        loop {
            print!("{prompt} {} ", if default { "[Y/n]" } else { "[y/N]" });
            std::io::stdout().flush().map_err(|e| e.to_string())?;
            let mut line = String::new();
            if std::io::stdin()
                .read_line(&mut line)
                .map_err(|e| e.to_string())?
                == 0
            {
                return Err("setup cancelled: input closed".into());
            }
            match line.trim().to_ascii_lowercase().as_str() {
                "" => return Ok(default),
                "y" | "yes" => return Ok(true),
                "n" | "no" => return Ok(false),
                _ => eprintln!("Enter y or n."),
            }
        }
    }
    let exe = super::executable()?;
    let previous = aherald::setup::selected_harnesses(&exe)?;
    let settings = aherald::setup::settings_state(&exe, None)?;
    let defaults = aherald::setup_selection::Selection::initial(&previous, &settings);
    println!("Select harness integrations. Existing user settings and assets are preserved.");
    let mut selection = defaults;
    selection.codex = choice("Configure Codex hooks?", defaults.codex)?;
    selection.claude = choice("Configure Claude Code hooks?", defaults.claude)?;
    let names: Vec<&str> = aherald::bridge_setup::Bridge::ALL
        .iter()
        .map(|bridge| bridge.label())
        .collect();
    println!(
        "{} use one small bridge file each. Choosing no is removing an installed one.",
        match names.split_last() {
            Some((last, rest)) if !rest.is_empty() => format!("{} and {last}", rest.join(", ")),
            _ => names.join(""),
        }
    );
    for bridge in aherald::bridge_setup::Bridge::ALL {
        let selected = choice(
            &format!("Install the {} bridge?", bridge.label()),
            defaults.bridge(bridge),
        )?;
        selection.set_bridge(bridge, selected);
    }
    println!(
        "Usage monitoring is optional. Claude auto uses fresh statusLine quota when available and falls back to the login token. Existing source settings are preserved."
    );
    selection.codex_usage = choice("Enable Codex usage monitoring?", defaults.codex_usage)?;
    selection.claude_usage = choice("Enable Claude usage monitoring?", defaults.claude_usage)?;
    // Warning choices apply to a selected agent only; an unselected agent keeps its settings.
    println!(
        "Session warnings are optional. A context warning needs a resident and, for Claude Code, the statusLine wrapper: an existing status line is kept and still shown, and without one Claude shows the [claude.statusline] format (empty by default). Existing thresholds are preserved."
    );
    if selection.codex {
        selection.codex_context = choice("Enable Codex context warning?", defaults.codex_context)?;
        selection.codex_idle = choice("Enable Codex idle warning?", defaults.codex_idle)?;
    }
    if selection.claude {
        selection.claude_context =
            choice("Enable Claude context warning?", defaults.claude_context)?;
        selection.claude_idle = choice("Enable Claude idle warning?", defaults.claude_idle)?;
        println!(
            "The status line wrapper reports the session name, quota and context size to the resident; the Claude context warning needs it."
        );
        selection.claude_statusline = choice(
            "Install the Claude Code status line wrapper?",
            defaults.claude_statusline || selection.claude_context,
        )?;
    }
    // Only the answers just given count: a warning saved for an agent whose hooks were declined
    // must not bring the hooks back. The Claude context warning needs the wrapper; say so
    // instead of leaving it silently off.
    if selection.claude && selection.claude_context && !selection.claude_statusline {
        println!("The Claude context warning needs the status line wrapper; it is installed too.");
        selection.claude_statusline = true;
    }
    let harnesses = selection.harnesses();
    let options = selection.options();
    let remove = selection.remove();
    print!(
        "{}",
        aherald::setup::run_with_options(&exe, &harnesses, remove, false, &options)?
    );
    if !choice("Apply these changes?", false)? {
        println!("No changes applied.");
        return Ok(());
    }
    print!(
        "{}",
        aherald::setup::run_with_options(&exe, &harnesses, remove, true, &options)?
    );
    if choice("Launch aHerald now?", true)? {
        std::process::Command::new(exe)
            .arg("--resident")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map_err(|e| format!("cannot launch aHerald: {e}"))?;
    }
    Ok(())
}
