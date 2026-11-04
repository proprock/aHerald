//! Console companion for `aherald.exe` (AH-211).
//!
//! `aherald.exe` is a GUI-subsystem program. Started from PowerShell or cmd, it writes nothing
//! to the console it inherits, and the shell does not wait for it. This program is a
//! console-subsystem program that starts the sibling `aherald.exe` with the same arguments,
//! relays its stdin, stdout and stderr through pipes, waits, and exits with its exit code.
//! Hooks keep calling `aherald.exe` directly: a parent without a console must not get a window.
use std::io::Write;
use std::process::{Command, Stdio};

#[cfg(windows)]
fn main() {
    let own = match std::env::current_exe() {
        Ok(path) => path,
        Err(error) => std::process::exit(fail(&format!("cannot locate itself: {error}"))),
    };
    let target = own.with_file_name("aherald.exe");
    let mut child = match Command::new(&target)
        .args(std::env::args_os().skip(1))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(error) => {
            std::process::exit(fail(&format!("cannot start {}: {error}", target.display())))
        }
    };
    // Stdin is relayed without waiting for it: a child that ignores its input must not hang on
    // a console read. The thread ends with this process, which exits once the child has.
    if let Some(mut to_child) = child.stdin.take() {
        std::thread::spawn(move || {
            let _ = std::io::copy(&mut std::io::stdin(), &mut to_child);
        });
    }
    let stdout = child.stdout.take().map(|mut from_child| {
        std::thread::spawn(move || {
            let _ = std::io::copy(&mut from_child, &mut std::io::stdout());
        })
    });
    let stderr = child.stderr.take().map(|mut from_child| {
        std::thread::spawn(move || {
            let _ = std::io::copy(&mut from_child, &mut std::io::stderr());
        })
    });
    let status = child.wait();
    for relay in [stdout, stderr].into_iter().flatten() {
        let _ = relay.join();
    }
    let _ = std::io::stdout().flush();
    let _ = std::io::stderr().flush();
    let code = match status {
        // Windows always reports a code; the fallback only keeps the type total.
        Ok(status) => status.code().unwrap_or(1),
        Err(error) => fail(&format!("cannot wait for {}: {error}", target.display())),
    };
    // The full i32 is kept: a crash code such as 0xC0000005 must not be truncated.
    std::process::exit(code);
}

#[cfg(not(windows))]
fn main() {
    std::process::exit(fail("this companion runs on Windows only"));
}

/// Reports on stderr without panicking when stderr is closed, and returns the usage code.
fn fail(message: &str) -> i32 {
    let _ = writeln!(std::io::stderr(), "aherald-cli: {message}");
    2
}
