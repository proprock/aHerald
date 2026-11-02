//! Text template for the status line `--statusline` prints when no `--then` command is given.
//!
//! `{a.b.c}` inserts the JSON field at that path of Claude Code's payload; a missing, null,
//! object or array value is empty. `{{` and `}}` are literal braces. `{[ ... ]}` is an optional
//! group: it is dropped entirely when any field written directly inside it is empty, so
//! separators vanish with the value they introduce (`{[ · {worktree.branch}]}`).
//! A field named `resets_at` (Unix epoch seconds) is shown as the time left, in the same form as
//! the usage card and tray (`3d 5h`, `4:05`, `15 min`).
//! Token counts (fields whose name ends in `tokens`, and `context_window_size`) are shortened to
//! `950`, `18k`, `18.6k`, `1.2M` so they fit a narrow line even when a session reaches millions.
//! Field values come from outside, so control characters in them are replaced by spaces; the
//! template itself may contain escape sequences such as ANSI colours.
use serde_json::Value;

/// Longest template accepted from configuration, in bytes.
pub const MAX_TEMPLATE: usize = 1024;
/// Longest value inserted by one field, in characters.
const MAX_VALUE: usize = 256;
/// Longest rendered line, in bytes.
const MAX_OUTPUT: usize = 4096;

#[derive(Debug, PartialEq, Eq)]
enum Node {
    Text(String),
    Field(Vec<String>),
    Group(Vec<Node>),
}

/// Checks the template's syntax; field names cannot be checked because the payload is Claude's.
pub fn validate(template: &str) -> Result<(), String> {
    if template.len() > MAX_TEMPLATE {
        return Err(format!("longer than {MAX_TEMPLATE} bytes"));
    }
    parse(template).map(|_| ())
}

/// Renders `template` against the payload. Returns nothing for an invalid template or payload.
pub fn render(template: &str, payload: &[u8]) -> String {
    render_at(template, payload, crate::usage_service::unix_now())
}

fn render_at(template: &str, payload: &[u8], now: i64) -> String {
    let (Ok(nodes), Ok(value)) = (parse(template), serde_json::from_slice::<Value>(payload)) else {
        return String::new();
    };
    let mut out = eval(&nodes, &value, now).0;
    if out.len() > MAX_OUTPUT {
        let mut end = MAX_OUTPUT;
        while !out.is_char_boundary(end) {
            end -= 1;
        }
        out.truncate(end);
    }
    out
}

fn parse(template: &str) -> Result<Vec<Node>, String> {
    let chars: Vec<char> = template.chars().collect();
    let mut stack: Vec<Vec<Node>> = vec![Vec::new()];
    let mut text = String::new();
    let mut i = 0;
    let flush = |text: &mut String, stack: &mut Vec<Vec<Node>>| {
        if !text.is_empty() {
            stack
                .last_mut()
                .expect("stack is never empty")
                .push(Node::Text(std::mem::take(text)));
        }
    };
    while i < chars.len() {
        match (chars[i], chars.get(i + 1)) {
            ('{', Some('{')) | ('}', Some('}')) => {
                text.push(chars[i]);
                i += 2;
            }
            ('{', Some('[')) => {
                flush(&mut text, &mut stack);
                stack.push(Vec::new());
                i += 2;
            }
            (']', Some('}')) if stack.len() > 1 => {
                flush(&mut text, &mut stack);
                let group = stack.pop().expect("checked above");
                stack
                    .last_mut()
                    .expect("stack is never empty")
                    .push(Node::Group(group));
                i += 2;
            }
            ('{', _) => {
                let end = chars[i + 1..]
                    .iter()
                    .position(|&c| c == '}')
                    .ok_or("unclosed '{'; write '{{' for a literal brace")?;
                let name: String = chars[i + 1..i + 1 + end].iter().collect();
                let valid = !name.is_empty()
                    && name.split('.').all(|part| {
                        !part.is_empty()
                            && part
                                .chars()
                                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
                    });
                if !valid {
                    return Err(format!(
                        "invalid field name '{{{name}}}'; use letters, digits, '_' and '-' separated by '.'"
                    ));
                }
                flush(&mut text, &mut stack);
                stack
                    .last_mut()
                    .expect("stack is never empty")
                    .push(Node::Field(name.split('.').map(str::to_owned).collect()));
                i += end + 2;
            }
            ('}', _) => return Err("unmatched '}'; write '}}' for a literal brace".into()),
            (c, _) => {
                text.push(c);
                i += 1;
            }
        }
    }
    flush(&mut text, &mut stack);
    if stack.len() > 1 {
        return Err("unclosed '{[' group; close it with ']}'".into());
    }
    Ok(stack.pop().expect("stack is never empty"))
}

/// Returns the text and whether every field written directly in `nodes` was non-empty.
fn eval(nodes: &[Node], payload: &Value, now: i64) -> (String, bool) {
    let mut out = String::new();
    let mut complete = true;
    for node in nodes {
        match node {
            Node::Text(text) => out.push_str(text),
            Node::Field(path) => {
                let value = lookup(payload, path, now);
                complete &= !value.is_empty();
                out.push_str(&value);
            }
            Node::Group(inner) => {
                let (text, whole) = eval(inner, payload, now);
                if whole {
                    out.push_str(&text);
                }
            }
        }
    }
    (out, complete)
}

fn lookup(payload: &Value, path: &[String], now: i64) -> String {
    let mut value = payload;
    for part in path {
        match value.get(part) {
            Some(next) => value = next,
            None => return String::new(),
        }
    }
    let text = match value {
        Value::String(text) => text.clone(),
        Value::Number(number) if path.last().is_some_and(|name| name == "resets_at") => {
            match number
                .as_i64()
                .or_else(|| number.as_f64().map(|n| n as i64))
            {
                Some(reset) => crate::usage_service::countdown(Some(reset), now),
                None => return String::new(),
            }
        }
        Value::Number(number)
            if path
                .last()
                .is_some_and(|name| name.ends_with("tokens") || name == "context_window_size") =>
        {
            match number
                .as_u64()
                .or_else(|| number.as_f64().map(|n| n.max(0.0) as u64))
            {
                Some(count) => compact_count(count),
                None => return String::new(),
            }
        }
        Value::Number(number) => number.to_string(),
        Value::Bool(flag) => flag.to_string(),
        _ => return String::new(),
    };
    text.chars()
        .take(MAX_VALUE)
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect()
}

/// `950`, `18k`, `18.6k`, `999.9k`, `1M`, `18.6M`: one decimal at most, rounded half up.
fn compact_count(count: u64) -> String {
    let wide = u128::from(count);
    let scaled = |unit: u128| (wide * 10 + unit / 2) / unit;
    let tenths = |value: u128, suffix: &str| {
        if value % 10 == 0 {
            format!("{}{suffix}", value / 10)
        } else {
            format!("{}.{}{suffix}", value / 10, value % 10)
        }
    };
    if count < 1_000 {
        count.to_string()
    } else if scaled(1_000) < 10_000 {
        tenths(scaled(1_000), "k")
    } else {
        tenths(scaled(1_000_000), "M")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAYLOAD: &str = r#"{"model":{"display_name":"Opus"},"workspace":{"repo":"aHerald"},
        "cost":{"total_lines_added":12,"total_lines_removed":3},
        "context_window":{"total_input_tokens":4500,"used_percentage":23.5},
        "rate_limits":{"five_hour":{"used_percentage":8,"resets_at":1790000000}},
        "branch":"a\u001b[31mb\nc","flag":true,"nothing":null,"list":[1]}"#;

    fn show(template: &str) -> String {
        render(template, PAYLOAD.as_bytes())
    }

    #[test]
    fn fields_render_strings_numbers_and_booleans() {
        assert_eq!(
            show("{model.display_name} +{cost.total_lines_added}, -{cost.total_lines_removed}"),
            "Opus +12, -3"
        );
        assert_eq!(show("{context_window.used_percentage}%"), "23.5%");
        assert_eq!(show("{flag}"), "true");
    }

    #[test]
    fn token_counts_are_shortened_to_k_and_m() {
        for (count, text) in [
            (0, "0"),
            (950, "950"),
            (1_000, "1k"),
            (18_000, "18k"),
            (18_600, "18.6k"),
            (18_649, "18.6k"),
            (18_650, "18.7k"),
            (200_000, "200k"),
            (999_949, "999.9k"),
            (999_950, "1M"),
            (1_000_000, "1M"),
            (1_234_567, "1.2M"),
            (18_600_000, "18.6M"),
            (2_000_000_000, "2000M"),
            (u64::MAX, "18446744073709.6M"),
        ] {
            assert_eq!(compact_count(count), text, "{count}");
        }
        let payload = r#"{"context_window":{"total_input_tokens":18600000,"total_output_tokens":900,"context_window_size":1000000,"used_percentage":18600000},"cost":{"total_lines_added":18000},"tokens_note":18000}"#;
        let shown = |template| render(template, payload.as_bytes());
        assert_eq!(
            shown(
                "{context_window.total_input_tokens}/{context_window.total_output_tokens}/{context_window.context_window_size}"
            ),
            "18.6M/900/1M"
        );
        // Only token fields are shortened; other numbers keep their value.
        assert_eq!(shown("{context_window.used_percentage}"), "18600000");
        assert_eq!(shown("{cost.total_lines_added}{tokens_note}"), "1800018000");
    }

    #[test]
    fn resets_at_shows_the_time_left_like_the_usage_card() {
        let template = "{rate_limits.five_hour.used_percentage}%/{rate_limits.five_hour.resets_at}";
        let at = |now| render_at(template, PAYLOAD.as_bytes(), now);
        let reset = 1_790_000_000;
        assert_eq!(at(reset - 3 * 86_400 - 5 * 3_600 - 59), "8%/3d 5h");
        assert_eq!(at(reset - 4 * 3_600 - 5 * 60), "8%/4:05");
        assert_eq!(at(reset - 15 * 60), "8%/15 min");
        assert_eq!(at(reset + 100), "8%/0 min");
        let text = r#"{"rate_limits":{"five_hour":{"resets_at":"soon"}}}"#;
        assert_eq!(
            render_at("[{rate_limits.five_hour.resets_at}]", text.as_bytes(), 0),
            "[soon]"
        );
    }

    #[test]
    fn missing_null_and_structured_values_are_empty() {
        assert_eq!(show("[{worktree.branch}]"), "[]");
        assert_eq!(show("[{nothing}{list}{model}{model.display_name.x}]"), "[]");
        assert_eq!(show("{}x"), "");
    }

    #[test]
    fn braces_can_be_escaped() {
        assert_eq!(show("{{x}} {model.display_name}"), "{x} Opus");
    }

    #[test]
    fn optional_groups_vanish_with_their_missing_field() {
        let template = "{workspace.repo}{[·{worktree.branch}]}{[·+{cost.total_lines_added}]}";
        assert_eq!(show(template), "aHerald·+12");
        assert_eq!(show("a{[ {missing}{model.display_name}]}b"), "ab");
        assert_eq!(show("{[x{[y{missing}]}z]}"), "xz");
    }

    #[test]
    fn substituted_values_lose_control_characters_but_the_template_keeps_them() {
        assert_eq!(show("\u{1b}[1m{branch}"), "\u{1b}[1ma [31mb c");
    }

    #[test]
    fn values_and_output_are_bounded() {
        let long = format!(r#"{{"v":"{}"}}"#, "é".repeat(1000));
        assert_eq!(render("{v}", long.as_bytes()).chars().count(), MAX_VALUE);
        let template = "{v}".repeat(100);
        assert!(render(&template, long.as_bytes()).len() <= MAX_OUTPUT);
    }

    #[test]
    fn invalid_payload_or_template_renders_nothing() {
        assert_eq!(render("{a}", b"not json"), "");
        assert_eq!(render("{a", PAYLOAD.as_bytes()), "");
    }

    #[test]
    fn validate_rejects_syntax_errors_with_a_reason() {
        for bad in ["{a", "a}", "{a b}", "{.a}", "{a..b}", "{[x", "x]}", "{a.}"] {
            assert!(validate(bad).is_err(), "{bad}");
        }
        for good in ["", "plain", "{a.b-c_d}", "{{}}", "{[x{a}]}", "x]"] {
            assert!(validate(good).is_ok(), "{good}");
        }
        assert!(validate(&"x".repeat(MAX_TEMPLATE + 1)).is_err());
    }
}
