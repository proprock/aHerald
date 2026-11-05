//! The generic sink: POSTs a body template to a URL, which serves ntfy, Slack, Telegram and any
//! other service that takes an HTTP request.
//!
//! Two placeholder kinds with separate namespaces keep secrets out of `config.toml`:
//! - `${NAME}` is replaced once, when the sink is built, by the environment variable `NAME`, in the
//!   URL and the body. Every value substituted this way is a secret to scrub from error text.
//! - `{text}`, `{kind}`, `{harness}` and `{folder}` are replaced at each send, in the body only.
//!   For a JSON content type the value is escaped to sit inside a JSON string, so the template
//!   writes its own quotes: `"text": "{text}"`.
use crate::{
    http_common::Zero,
    https::{self, HttpError, Wiped},
    sink::{Message, Sink},
};
use std::time::Duration;

const TIMEOUT: Duration = Duration::from_secs(10);

/// Replaces each `${NAME}` in `template` by the value of that variable. Returns the text and the
/// values used. An error names the variable, never a value.
pub fn expand_env(
    template: &str,
    env: &impl Fn(&str) -> Option<String>,
) -> Result<(String, Vec<String>), String> {
    let mut out = String::with_capacity(template.len());
    let mut secrets = Vec::new();
    let mut rest = template;
    while let Some(start) = rest.find("${") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        let end = after
            .find('}')
            .ok_or("a \"${\" is not closed with \"}\"".to_owned())?;
        let name = &after[..end];
        if !crate::config::is_variable_name(name) {
            return Err("\"${…}\" must hold an environment variable name".to_owned());
        }
        let value = env(name)
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                format!(
                    "environment variable {name} is not set or is empty (a resident started \
                     from the Run key or the installer sees only your user environment)"
                )
            })?;
        out.push_str(&value);
        secrets.push(value);
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    Ok((out, secrets))
}

/// Fills `{text}`, `{kind}`, `{harness}` and `{folder}` in one pass, so a value that itself
/// contains a placeholder is not expanded again. Other braces (JSON) are left alone.
pub fn render(template: &str, message: &Message, json: bool) -> String {
    let escape = |value: &str| {
        if json {
            let quoted = serde_json::Value::from(value).to_string();
            quoted[1..quoted.len() - 1].to_owned()
        } else {
            value.to_owned()
        }
    };
    let mut out = String::with_capacity(template.len() + message.text.len());
    let mut rest = template;
    while let Some(start) = rest.find('{') {
        out.push_str(&rest[..start]);
        let tail = &rest[start..];
        let value = [
            ("{text}", message.text.as_str()),
            ("{kind}", message.kind.as_str()),
            ("{harness}", message.harness.as_str()),
            ("{folder}", message.folder.as_deref().unwrap_or("")),
        ]
        .into_iter()
        .find(|(name, _)| tail.starts_with(name));
        match value {
            Some((name, value)) => {
                out.push_str(&escape(value));
                rest = &tail[name.len()..];
            }
            None => {
                out.push('{');
                rest = &tail[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// A parsed `http(s)://host[:port]/path`.
struct Target {
    secure: bool,
    host: String,
    port: u16,
    path: String,
}

fn parse_url(url: &str) -> Result<Target, String> {
    let (secure, rest) = if let Some(rest) = url.strip_prefix("https://") {
        (true, rest)
    } else if let Some(rest) = url.strip_prefix("http://") {
        (false, rest)
    } else {
        return Err("the url must start with https:// or http://".to_owned());
    };
    let (authority, path) = match rest.find(['/', '?']) {
        Some(at) if rest.as_bytes()[at] == b'/' => (&rest[..at], rest[at..].to_owned()),
        Some(at) => (&rest[..at], format!("/{}", &rest[at..])),
        None => (rest, "/".to_owned()),
    };
    let (host, port) = match authority.rsplit_once(':') {
        Some((host, port)) => (
            host,
            port.parse::<u16>()
                .map_err(|_| "the url has an invalid port".to_owned())?,
        ),
        None => (authority, if secure { 443 } else { 80 }),
    };
    if host.is_empty() || host.contains('@') {
        return Err("the url must name a host and carry no user name".to_owned());
    }
    Ok(Target {
        secure,
        host: host.to_owned(),
        port,
        path,
    })
}

pub struct Webhook {
    target: Wiped<Target>,
    content_type: String,
    body: Wiped<String>,
}

impl Webhook {
    /// `url` and `body` have their `${NAME}` placeholders already expanded.
    pub fn new(url: &str, body: String, content_type: String) -> Result<Self, String> {
        Ok(Self {
            target: Wiped(parse_url(url)?),
            content_type,
            body: Wiped(body),
        })
    }
}

impl Zero for Target {
    fn zero(&mut self) {
        self.host.zero();
        self.path.zero();
    }
}

impl Sink for Webhook {
    fn name(&self) -> &str {
        "webhook"
    }

    fn send(&mut self, message: &Message) -> Result<(), String> {
        let json = self.content_type.to_ascii_lowercase().contains("json");
        let body = Wiped(render(&self.body.0, message, json));
        let target = &self.target.0;
        let headers = format!("Content-Type: {}\r\n", self.content_type);
        let response = https::post_at(
            target.secure,
            &target.host,
            target.port,
            &target.path,
            &headers,
            body.0.as_bytes(),
            TIMEOUT,
        );
        match response {
            Ok(response) if (200..300).contains(&response.status) => Ok(()),
            Ok(response) => Err(format!("the server answered HTTP {}", response.status)),
            Err(HttpError::Network) => Err("cannot reach the server".to_owned()),
            Err(HttpError::TooLarge) => Err("the server reply was too large".to_owned()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env<'a>(pairs: &'a [(&str, &str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |name| {
            pairs
                .iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| (*value).to_owned())
        }
    }

    fn message(text: &str) -> Message {
        Message {
            kind: "input_required".into(),
            harness: "Claude".into(),
            folder: Some("project".into()),
            text: text.into(),
        }
    }

    #[test]
    fn environment_placeholders_are_replaced_and_their_values_reported_as_secrets() {
        let (text, secrets) = expand_env(
            "https://api.example/bot${TOKEN}/send?c=${CHAT}",
            &env(&[("TOKEN", " 12:abc\n"), ("CHAT", "42")]),
        )
        .unwrap();
        assert_eq!(text, "https://api.example/bot12:abc/send?c=42");
        assert_eq!(secrets, ["12:abc", "42"]);
        let (plain, none) = expand_env("no placeholders {text}", &env(&[])).unwrap();
        assert_eq!((plain.as_str(), none.len()), ("no placeholders {text}", 0));
    }

    #[test]
    fn a_missing_empty_or_malformed_placeholder_is_an_error_without_values() {
        let vars = env(&[("EMPTY", "  "), ("SET", "s3cret-value")]);
        let missing = expand_env("a${NOPE}b", &vars).unwrap_err();
        assert!(missing.contains("NOPE") && missing.contains("user environment"));
        assert!(expand_env("${EMPTY}", &vars).unwrap_err().contains("EMPTY"));
        for bad in ["${SET", "${}", "${a b}", "${1X}", "${s3cret-value}"] {
            let error = expand_env(bad, &vars).unwrap_err();
            assert!(!error.contains("s3cret-value"), "{bad}: {error}");
        }
    }

    #[test]
    fn message_placeholders_are_filled_once_and_json_braces_survive() {
        let template = r#"{"chat_id": "42", "text": "{text}", "who": "{harness}/{kind}/{folder}"}"#;
        let body = render(template, &message("hi {kind}"), false);
        assert_eq!(
            body,
            r#"{"chat_id": "42", "text": "hi {kind}", "who": "Claude/input_required/project"}"#
        );
    }

    #[test]
    fn a_json_body_escapes_the_values_so_the_result_always_parses() {
        let nasty = "say \"hi\"\nback\\slash\t{text}";
        let body = render(r#"{"text": "{text}"}"#, &message(nasty), true);
        let parsed: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(parsed["text"], nasty);
    }

    #[test]
    fn an_absent_folder_renders_empty() {
        let mut message = message("t");
        message.folder = None;
        assert_eq!(render("[{folder}]", &message, false), "[]");
    }

    #[test]
    fn a_url_is_split_into_scheme_host_port_and_path() {
        let cases = [
            (
                "https://api.example/bot1/send",
                true,
                "api.example",
                443,
                "/bot1/send",
            ),
            (
                "http://127.0.0.1:8080/x?y=1",
                false,
                "127.0.0.1",
                8080,
                "/x?y=1",
            ),
            ("https://ntfy.sh", true, "ntfy.sh", 443, "/"),
            ("https://host?q=1", true, "host", 443, "/?q=1"),
        ];
        for (url, secure, host, port, path) in cases {
            let target = parse_url(url).unwrap();
            assert_eq!(
                (
                    target.secure,
                    target.host.as_str(),
                    target.port,
                    target.path.as_str()
                ),
                (secure, host, port, path),
                "{url}"
            );
        }
    }

    #[test]
    fn an_unusable_url_is_refused_without_echoing_it() {
        for url in [
            "ftp://secret-host/x",
            "secret-host/x",
            "https://u:p@secret-host/",
            "https://secret-host:99999/",
        ] {
            let error = parse_url(url).err().unwrap();
            assert!(!error.contains("secret-host"), "{url}: {error}");
        }
    }
}
