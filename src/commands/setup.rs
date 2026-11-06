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

/// `--install-statusline` and `--uninstall-statusline`.
pub fn statusline_file(cli: &Cli) -> Result<(), String> {
    let action = match cli.operation {
        Some(Operation::InstallStatusline) => aherald::hook_setup::Action::Install,
        _ => aherald::hook_setup::Action::Uninstall,
    };
    let exe = executable()?;
    // Windows keeps the wrapper in the setup receipt, as `--setup` does (AH-211).
    #[cfg(windows)]
    let report = aherald::setup::run_statusline_only(
        &exe,
        cli.hooks_file.as_deref(),
        action == aherald::hook_setup::Action::Uninstall,
        cli.apply,
    )?;
    #[cfg(not(windows))]
    let report = {
        let file = match cli.hooks_file.clone() {
            Some(file) => file,
            None => aherald::paths::agent_hook_file(aherald::agent::Agent::Claude)
                .ok_or("cannot locate the configuration directory; use --hooks-file")?,
        };
        aherald::hook_setup::run_statusline(
            action,
            &file,
            &aherald::hook_setup::statusline_command(&exe),
            cli.apply,
        )?
    };
    print!("{report}");
    Ok(())
}

/// `--install-hooks` and `--uninstall-hooks`.
pub fn hooks(cli: &Cli) -> Result<(), String> {
    let action = match cli.operation {
        Some(Operation::InstallHooks) => aherald::hook_setup::Action::Install,
        _ => aherald::hook_setup::Action::Uninstall,
    };
    let agent = cli.hooks_agent.ok_or("hook setup needs an agent")?;
    let exe = executable()?;
    let file = match cli.hooks_file.clone() {
        Some(file) => file,
        None => aherald::paths::agent_hook_file(agent)
            .ok_or("cannot locate the configuration directory; use --hooks-file")?,
    };
    let command = aherald::hook_setup::command_for(&exe, agent, cfg!(windows));
    print!(
        "{}",
        aherald::hook_setup::run(agent, action, &file, &command, cli.apply)?
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
        "Usage monitoring is optional. Claude auto tries the login token, then the last statusLine report; existing source settings are preserved."
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
