#![cfg_attr(windows, windows_subsystem = "windows")]

mod commands;

use aherald::cli::{Cli, Operation};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    // A hook-like command never fails its harness: its problems go to stderr only.
    let hook = args.iter().any(|arg| arg == "--hook" || arg == "--notify");
    if let Err(error) = run(args) {
        eprintln!("aHerald: {error}");
        if !hook {
            std::process::exit(2);
        }
    }
}

fn run(args: Vec<String>) -> Result<(), String> {
    let cli = Cli::parse(args)?;
    if let Some(parent) = cli.restart_parent {
        aherald::restart::wait_for_parent(parent)?;
    }
    #[cfg(target_os = "macos")]
    if cli.no_arguments
        && let Ok(exe) = std::env::current_exe()
        && aherald::setup_selection::launch_opens_setup(&[], &exe)
    {
        return aherald::mac::setup_window::run();
    }
    use commands::{child, control, deliver, setup, statusline};
    match cli.operation {
        Some(Operation::Help) => {
            println!("{}", aherald::cli::help());
            Ok(())
        }
        Some(Operation::Version) => {
            println!("{}", aherald::version_line());
            Ok(())
        }
        Some(Operation::ResidentChild) => child::resident(&cli),
        Some(Operation::DisplayChild) => child::display(&cli),
        Some(Operation::ConsoleSetup) => setup::console(),
        Some(Operation::SetupWindow) => setup::window(),
        Some(Operation::SetupSettings) => setup::settings(&cli),
        Some(Operation::Setup | Operation::Unsetup) => setup::native(&cli),
        Some(Operation::Statusline) => statusline::run(&cli),
        Some(Operation::InstallStatusline | Operation::UninstallStatusline) => {
            setup::statusline_file(&cli)
        }
        Some(Operation::InstallHooks | Operation::UninstallHooks) => setup::hooks(&cli),
        Some(Operation::RemoteInfo) => control::remote_info(&cli),
        Some(Operation::PerfReport) => control::perf_report(&cli),
        Some(Operation::Status | Operation::Quit) => control::status_or_quit(&cli),
        Some(Operation::RawOutput) => control::raw_output(&cli),
        Some(Operation::Hook) => deliver::hook(&cli),
        Some(Operation::Notify) => deliver::notify(&cli),
        Some(Operation::Event) => deliver::event(&cli),
        Some(Operation::Demo) => deliver::demo(&cli),
        None => deliver::start(&cli),
    }
}
