//! Quoting of one value for the shell a hook or statusLine command runs in.

/// `text` as one POSIX shell word: single quotes, each embedded `'` written as `'\''`.
pub(crate) fn posix(text: &str) -> String {
    format!("'{}'", text.replace('\'', "'\\''"))
}

/// `text` as a PowerShell single-quoted string: each embedded `'` is doubled.
pub(crate) fn powershell(text: &str) -> String {
    format!("'{}'", text.replace('\'', "''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn posix_quotes_everything_a_shell_would_expand() {
        assert_eq!(posix(""), "''");
        assert_eq!(posix("plain"), "'plain'");
        assert_eq!(posix("a b"), "'a b'");
        assert_eq!(posix("it's"), r"'it'\''s'");
        assert_eq!(posix("'"), r"''\'''");
        assert_eq!(posix("$HOME `id` \"x\" \\"), "'$HOME `id` \"x\" \\'");
        assert_eq!(posix("a\nb"), "'a\nb'");
    }

    #[test]
    fn powershell_doubles_single_quotes_and_leaves_the_rest() {
        assert_eq!(powershell(""), "''");
        assert_eq!(
            powershell("C:\\Program Files\\a.exe"),
            "'C:\\Program Files\\a.exe'"
        );
        assert_eq!(powershell("it's"), "'it''s'");
        assert_eq!(powershell("''"), "''''''");
        assert_eq!(powershell("$x `y` \"z\""), "'$x `y` \"z\"'");
        assert_eq!(powershell("a\nb"), "'a\nb'");
    }

    #[cfg(unix)]
    #[test]
    fn a_posix_word_reaches_the_program_unchanged() {
        for text in [
            "",
            "plain",
            "a b",
            "it's",
            "$HOME `id` \"x\" \\",
            "a\nb",
            "'",
        ] {
            let out = std::process::Command::new("sh")
                .arg("-c")
                .arg(format!("printf %s {}", posix(text)))
                .output()
                .unwrap();
            assert_eq!(String::from_utf8_lossy(&out.stdout), text);
        }
    }
}
