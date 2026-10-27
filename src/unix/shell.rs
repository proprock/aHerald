//! Hands a URL or a file to the desktop's default handler: `open` on macOS, `xdg-open` elsewhere.
use std::{
    ffi::OsStr,
    path::Path,
    process::{Command, Stdio},
};

const OPENER: &str = if cfg!(target_os = "macos") {
    "open"
} else {
    "xdg-open"
};

/// The opener invocation for `target`, or `None` when the target could be read as an option.
pub fn open_command(target: &OsStr) -> Option<Command> {
    if target.is_empty() || target.as_encoded_bytes().starts_with(b"-") {
        return None;
    }
    let mut command = Command::new(OPENER);
    command
        .arg(target)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    Some(command)
}

pub fn open(target: &OsStr) -> Result<(), String> {
    let mut command = open_command(target).ok_or("invalid open target")?;
    let mut child = command
        .spawn()
        .map_err(|_| format!("cannot start {OPENER}"))?;
    // The opener returns quickly; reaping it off-thread avoids a zombie without blocking the GUI.
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

/// Opens the configuration file in the default editor.
pub fn edit_configuration(path: &Path) -> Result<(), String> {
    open(path.as_os_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_target_is_passed_as_a_single_argument_to_the_platform_opener() {
        let command = open_command(OsStr::new("https://example.com/a b")).unwrap();
        assert_eq!(
            command.get_program(),
            if cfg!(target_os = "macos") {
                "open"
            } else {
                "xdg-open"
            }
        );
        let args: Vec<_> = command.get_args().collect();
        assert_eq!(args, [OsStr::new("https://example.com/a b")]);
    }

    #[test]
    fn a_target_that_looks_like_an_option_or_is_empty_is_refused() {
        assert!(open_command(OsStr::new("--help")).is_none());
        assert!(open_command(OsStr::new("-e")).is_none());
        assert!(open_command(OsStr::new("")).is_none());
        assert!(open(OsStr::new("--help")).is_err());
    }
}
