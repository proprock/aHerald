//! The two placeholder kinds of the sinks, with separate namespaces to keep secrets out of
//! `config.toml` (used by the webhook and the command sink):
//! - `${NAME}` is replaced once, when the sink is built, by the environment variable `NAME`. Every
//!   value substituted this way is a secret to scrub from error text.
//! - `{text}`, `{kind}`, `{harness}`, `{folder}` and `{host}` are replaced at each send. For a JSON content
//!   type the value is escaped to sit inside a JSON string, so the template writes its own quotes:
//!   `"text": "{text}"`; a form body is percent-encoded.
use crate::sink::Message;

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

/// How a substituted value is made safe for the body it lands in. It follows the content type, so
/// a value (a folder name can hold any character) cannot add a JSON field or a form parameter.
/// Any other type gets the value as written.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Escape {
    Raw,
    Json,
    Form,
}

impl Escape {
    pub fn for_content_type(content_type: &str) -> Self {
        let media = content_type
            .split(';')
            .next()
            .unwrap_or("")
            .trim()
            .to_ascii_lowercase();
        if media == "application/x-www-form-urlencoded" {
            Self::Form
        } else if media.ends_with("json") {
            Self::Json
        } else {
            Self::Raw
        }
    }
}

/// Percent-encodes every byte except the unreserved characters of RFC 3986.
fn percent_encode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            out.push(char::from(byte));
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

/// Fills `{text}`, `{kind}`, `{harness}`, `{folder}` and `{host}` in one pass, so a value that itself
/// contains a placeholder is not expanded again. Other braces (JSON) are left alone.
pub fn render(template: &str, message: &Message, mode: Escape) -> String {
    let escape = |value: &str| match mode {
        Escape::Json => {
            let quoted = serde_json::Value::from(value).to_string();
            quoted[1..quoted.len() - 1].to_owned()
        }
        Escape::Form => percent_encode(value),
        Escape::Raw => value.to_owned(),
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
            ("{host}", message.host.as_deref().unwrap_or("")),
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
            host: None,
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
        let body = render(template, &message("hi {kind}"), Escape::Raw);
        assert_eq!(
            body,
            r#"{"chat_id": "42", "text": "hi {kind}", "who": "Claude/input_required/project"}"#
        );
    }

    #[test]
    fn a_json_body_escapes_the_values_so_the_result_always_parses() {
        let nasty = "say \"hi\"\nback\\slash\t{text}";
        let body = render(r#"{"text": "{text}"}"#, &message(nasty), Escape::Json);
        let parsed: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(parsed["text"], nasty);
    }

    #[test]
    fn a_form_body_percent_encodes_the_values_so_none_can_add_a_field() {
        let mut hostile = message("done & more=1");
        hostile.folder = Some("x&chat_id=666 é".into());
        let body = render("text={text}&folder={folder}", &hostile, Escape::Form);
        assert_eq!(
            body,
            "text=done%20%26%20more%3D1&folder=x%26chat_id%3D666%20%C3%A9"
        );
    }

    #[test]
    fn the_escaping_follows_the_content_type() {
        for (content_type, expected) in [
            ("application/json", Escape::Json),
            ("Application/JSON; charset=utf-8", Escape::Json),
            ("application/vnd.api+json", Escape::Json),
            ("application/x-www-form-urlencoded", Escape::Form),
            (
                "Application/X-WWW-Form-Urlencoded; charset=utf-8",
                Escape::Form,
            ),
            ("text/plain; charset=utf-8", Escape::Raw),
        ] {
            assert_eq!(
                Escape::for_content_type(content_type),
                expected,
                "{content_type}"
            );
        }
    }

    #[test]
    fn an_absent_folder_renders_empty() {
        let mut message = message("t");
        message.folder = None;
        assert_eq!(render("[{folder}]", &message, Escape::Raw), "[]");
    }

    #[test]
    fn the_host_is_a_placeholder_and_empty_when_absent() {
        let mut message = message("t");
        assert_eq!(render("[{host}]", &message, Escape::Raw), "[]");
        message.host = Some("work station&1".into());
        assert_eq!(
            render("[{host}]", &message, Escape::Raw),
            "[work station&1]"
        );
        assert_eq!(
            render("h={host}", &message, Escape::Form),
            "h=work%20station%261"
        );
    }

    #[test]
    fn the_two_line_text_is_escaped_for_each_body_type() {
        let text = "workstation-1/project\n\nClaude: turn complete";
        let message = message(text);
        let json = render(r#"{"text": "{text}"}"#, &message, Escape::Json);
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["text"], text);
        let form = render("text={text}", &message, Escape::Form);
        assert!(form.contains("%0A%0A") && !form.contains('\n'), "{form}");
        assert_eq!(render("{text}", &message, Escape::Raw), text);
    }
}
