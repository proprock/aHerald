#![cfg_attr(windows, windows_subsystem = "windows")]

fn main() {
    if std::env::args_os().nth(1).as_deref() == Some(std::ffi::OsStr::new("--help")) {
        println!("{}", aherald::help());
    }
}
