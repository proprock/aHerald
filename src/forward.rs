//! Builds the configured sinks: resolves each secret from the environment or a file and turns a
//! sink that cannot be built into a notice, never into a stopped resident.
//!
//! A notice names the variable or the file, never a value, so a misconfigured token cannot leak
//! through a card or a log.
use crate::{
    config::{Forward, SinkConfig, SinkType},
    sink::Route,
    telegram::Telegram,
};
use std::path::Path;

pub struct Built {
    pub routes: Vec<Route>,
    /// One line for each sink left out, to show as a card at startup.
    pub notices: Vec<String>,
}

/// Builds every sink in `forward`. `env` looks a variable up; `read` returns a token file's text.
pub fn build(
    forward: &Forward,
    env: impl Fn(&str) -> Option<String>,
    read: impl Fn(&Path) -> std::io::Result<String>,
) -> Built {
    let mut built = Built {
        routes: Vec::new(),
        notices: Vec::new(),
    };
    for (index, sink) in forward.sinks.iter().enumerate() {
        match route(sink, &env, &read) {
            Ok(route) => built.routes.push(route),
            Err(why) => built
                .notices
                .push(format!("Messenger forwarding, sink {}: {why}", index + 1)),
        }
    }
    built
}

fn route(
    sink: &SinkConfig,
    env: &impl Fn(&str) -> Option<String>,
    read: &impl Fn(&Path) -> std::io::Result<String>,
) -> Result<Route, String> {
    let variable = |name: &str| {
        env(name)
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                format!(
                    "environment variable {name} is not set or is empty (a resident started \
                     from the Run key or the installer sees only your user environment)"
                )
            })
    };
    let token = match (&sink.token_env, &sink.token_file) {
        (Some(name), _) => variable(name)?,
        (None, Some(path)) => {
            let shown = path.file_name().map_or_else(
                || "the token file".to_owned(),
                |name| name.to_string_lossy().into_owned(),
            );
            read(path)
                .map(|text| text.trim().to_owned())
                .map_err(|_| format!("cannot read the token file {shown}"))
                .and_then(|token| {
                    if token.is_empty() {
                        Err(format!("the token file {shown} is empty"))
                    } else {
                        Ok(token)
                    }
                })?
        }
        (None, None) => return Err("no token_env or token_file".to_owned()),
    };
    let chat_id = variable(sink.chat_id_env.as_deref().ok_or("no chat_id_env")?)?;
    let SinkType::Telegram = sink.kind;
    Ok(Route {
        sink: Box::new(Telegram::new(token.clone(), chat_id)),
        kinds: sink.kinds.clone(),
        secrets: vec![token],
        include_folder: sink.include_folder,
    })
}

/// Reads a token file. Where permissions can be checked (Unix) a file that other users can read is
/// refused; Windows relies on the file living in the user's own profile.
pub fn read_token_file(path: &Path) -> std::io::Result<String> {
    use std::io::Read;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if std::fs::metadata(path)?.permissions().mode() & 0o077 != 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "readable by other users",
            ));
        }
    }
    // A token is short; anything larger is not a token file.
    let mut text = String::new();
    std::io::Read::take(std::fs::File::open(path)?, 4096).read_to_string(&mut text)?;
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use std::{collections::HashMap, io::ErrorKind, path::PathBuf};

    fn forward(text: &str) -> Forward {
        Config::parse(text, &PathBuf::from(".")).unwrap().forward
    }

    const BY_ENV: &str =
        "[[forward.sinks]]\ntype = \"telegram\"\ntoken_env = \"TOKEN\"\nchat_id_env = \"CHAT\"\n";
    const BY_FILE: &str = "[[forward.sinks]]\ntype = \"telegram\"\ntoken_file = \"token.txt\"\nchat_id_env = \"CHAT\"\n";

    fn env(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let map: HashMap<String, String> = pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect();
        move |name| map.get(name).cloned()
    }

    fn no_files(_: &Path) -> std::io::Result<String> {
        Err(ErrorKind::NotFound.into())
    }

    #[test]
    fn a_sink_with_its_variables_set_is_built_and_its_token_is_a_secret_to_scrub() {
        let built = build(
            &forward(BY_ENV),
            env(&[("TOKEN", " 12:abc\n"), ("CHAT", "42")]),
            no_files,
        );
        assert!(built.notices.is_empty(), "{:?}", built.notices);
        assert_eq!(built.routes.len(), 1);
        assert_eq!(built.routes[0].secrets, ["12:abc"]);
        assert_eq!(built.routes[0].sink.name(), "telegram");
        assert!(!built.routes[0].include_folder);
    }

    #[test]
    fn a_missing_or_empty_variable_is_a_notice_naming_the_variable_not_a_value() {
        for pairs in [
            vec![("CHAT", "42")],
            vec![("TOKEN", "  "), ("CHAT", "42")],
            vec![("TOKEN", "12:secret-value")],
        ] {
            let built = build(&forward(BY_ENV), env(&pairs), no_files);
            assert!(built.routes.is_empty());
            assert_eq!(built.notices.len(), 1);
            let notice = &built.notices[0];
            assert!(
                notice.contains("TOKEN") || notice.contains("CHAT"),
                "{notice}"
            );
            assert!(notice.contains("user environment"), "{notice}");
            assert!(!notice.contains("secret-value"), "{notice}");
        }
    }

    #[test]
    fn the_token_comes_from_a_file_with_whitespace_trimmed() {
        let read = |path: &Path| {
            assert_eq!(path, Path::new("./token.txt"));
            Ok("  12:from-file\r\n".to_owned())
        };
        let built = build(&forward(BY_FILE), env(&[("CHAT", "42")]), read);
        assert_eq!(built.routes[0].secrets, ["12:from-file"]);
    }

    #[test]
    fn an_unreadable_or_empty_token_file_is_a_notice_with_the_file_name_only() {
        let missing = build(&forward(BY_FILE), env(&[("CHAT", "42")]), no_files);
        assert_eq!(missing.routes.len(), 0);
        assert!(
            missing.notices[0].contains("token.txt"),
            "{:?}",
            missing.notices
        );
        let empty = build(&forward(BY_FILE), env(&[("CHAT", "42")]), |_: &Path| {
            Ok(" \n".to_owned())
        });
        assert!(empty.notices[0].contains("empty"), "{:?}", empty.notices);
    }

    #[test]
    fn one_broken_sink_does_not_stop_the_next() {
        let text = format!("{BY_FILE}{}", BY_ENV.replace("TOKEN", "OTHER"));
        let built = build(
            &forward(&text),
            env(&[("OTHER", "9:ok"), ("CHAT", "42")]),
            no_files,
        );
        assert_eq!((built.routes.len(), built.notices.len()), (1, 1));
        assert!(built.notices[0].contains("sink 1"), "{:?}", built.notices);
    }

    #[cfg(unix)]
    #[test]
    fn a_token_file_other_users_can_read_is_refused() {
        use std::os::unix::fs::PermissionsExt;
        let path = std::env::temp_dir().join(format!("aherald-token-{}", std::process::id()));
        std::fs::write(&path, "12:abc").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert!(read_token_file(&path).is_err());
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        assert_eq!(read_token_file(&path).unwrap(), "12:abc");
        std::fs::remove_file(&path).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn a_token_file_is_read_and_a_long_one_is_cut() {
        let path = std::env::temp_dir().join(format!("aherald-token-{}", std::process::id()));
        std::fs::write(&path, "x".repeat(10_000)).unwrap();
        assert_eq!(read_token_file(&path).unwrap().len(), 4096);
        std::fs::remove_file(&path).unwrap();
    }
}
