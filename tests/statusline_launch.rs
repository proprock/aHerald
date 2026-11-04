//! The Windows `statusLine` command must read the same in Git Bash and in PowerShell (AH-211).
use aherald::statusline_launch::{companion_command, is_wrapper, script_command};
use std::path::Path;

#[test]
fn a_plain_path_to_the_companion_is_a_bare_command_with_forward_slashes() {
    assert_eq!(
        companion_command(Path::new(r"C:\Users\me\aHerald\aherald-cli.exe")).as_deref(),
        Some("C:/Users/me/aHerald/aherald-cli.exe --statusline")
    );
}

#[test]
fn a_path_either_shell_would_read_differently_cannot_be_a_bare_command() {
    for path in [
        r"C:\Program Files\aHerald\aherald-cli.exe",
        r"C:\Users\me's\aherald-cli.exe",
        r"C:\a&b\aherald-cli.exe",
        r"C:\a$HOME\aherald-cli.exe",
        r"C:\a`b\aherald-cli.exe",
        r"C:\a,b\aherald-cli.exe",
        r"C:\a(b)\aherald-cli.exe",
    ] {
        assert_eq!(companion_command(Path::new(path)), None, "{path}");
    }
}

#[test]
fn every_form_aherald_writes_is_recognized_as_its_own_wrapper() {
    for command in [
        "C:/Users/me/aHerald/aherald-cli.exe --statusline",
        "'/Applications/aHerald.app/Contents/MacOS/aherald' --statusline --then 'echo mine'",
        "powershell.exe -NoProfile -NonInteractive -Command \"& ([scriptblock]::Create((-join [char[]](36,115)))); # aherald --statusline\"",
        "powershell -NoProfile -NonInteractive -ExecutionPolicy Bypass -File \"C:/tools/statusline.ps1\"",
    ] {
        assert!(is_wrapper(command), "{command}");
    }
    for command in [
        "echo mine",
        "npx ccusage statusline",
        "powershell -File \"C:/tools/other.ps1\"",
    ] {
        assert!(!is_wrapper(command), "{command}");
    }
}

#[test]
fn a_short_8_3_name_is_still_a_bare_command() {
    // Windows CI reports its temporary directory as `C:\Users\RUNNER~1\...`.
    assert_eq!(
        companion_command(Path::new(r"C:\Users\RUNNER~1\aherald-cli.exe")).as_deref(),
        Some("C:/Users/RUNNER~1/aherald-cli.exe --statusline")
    );
}

#[test]
fn a_non_ascii_letter_in_the_path_is_still_a_bare_command() {
    assert_eq!(
        companion_command(Path::new(r"C:\Users\Тони\aherald-cli.exe")).as_deref(),
        Some("C:/Users/Тони/aherald-cli.exe --statusline")
    );
}

#[test]
fn a_path_with_spaces_is_quoted_as_an_argument_of_powershell() {
    assert_eq!(
        script_command(Path::new(r"C:\Program Files\aHerald\statusline.ps1")).as_deref(),
        Some(
            "powershell -NoProfile -NonInteractive -ExecutionPolicy Bypass -File \"C:/Program Files/aHerald/statusline.ps1\""
        )
    );
    // An apostrophe and an ampersand stay literal inside double quotes in both shells.
    assert!(script_command(Path::new(r"C:\it's & mine\statusline.ps1")).is_some());
}

#[test]
fn a_path_that_either_shell_expands_inside_double_quotes_is_refused() {
    for path in [
        r"C:\a$HOME\statusline.ps1",
        r"C:\a`tick\statusline.ps1",
        "C:\\a\"b\\statusline.ps1",
    ] {
        assert_eq!(script_command(Path::new(path)), None, "{path}");
    }
}
