//! Opt-in, read-only Claude Code quota (5-hour and 7-day windows).
//!
//! Claude Code hooks carry no quota, so this polls the undocumented
//! `GET https://api.anthropic.com/api/oauth/usage` with the access token Claude Code already stores
//! in `.credentials.json`. The token is read per request, sent only to that host, never logged, kept
//! in memory only for the call, and never refreshed or written back. The response shape is not a
//! documented contract: unknown or malformed data becomes an "unavailable" view.
use crate::app_server::{Error, Transport};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{collections::VecDeque, time::Duration};

pub const HOST: &str = "api.anthropic.com";
pub const PATH: &str = "/api/oauth/usage";
const FIVE_HOUR_MINS: i64 = 300;
const SEVEN_DAY_MINS: i64 = 7 * 24 * 60;

/// Converts the usage response into the Codex-shaped snapshot `usage::Snapshot` understands.
pub fn snapshot_from_response(body: &[u8]) -> Result<Value, Error> {
    #[derive(Deserialize)]
    struct Window {
        utilization: Option<f64>,
        resets_at: Option<String>,
    }
    #[derive(Deserialize)]
    struct Wire {
        five_hour: Option<Window>,
        seven_day: Option<Window>,
    }
    let wire: Wire = serde_json::from_slice(body).map_err(|_| Error::Malformed)?;
    if wire.five_hour.is_none() && wire.seven_day.is_none() {
        return Err(Error::Malformed);
    }
    let window = |w: Option<Window>, mins: i64| {
        w.map(|w| {
            json!({
                "usedPercent": w.utilization,
                "windowDurationMins": mins,
                "resetsAt": w.resets_at.as_deref().and_then(iso_to_unix),
            })
        })
    };
    Ok(json!({"rateLimits": {
        "limitId": "claude",
        "primary": window(wire.five_hour, FIVE_HOUR_MINS),
        "secondary": window(wire.seven_day, SEVEN_DAY_MINS),
    }}))
}

/// Parses `YYYY-MM-DDTHH:MM:SS[.fraction](Z|+HH:MM|-HH:MM)` into Unix seconds.
pub fn iso_to_unix(text: &str) -> Option<i64> {
    let b = text.as_bytes();
    let num = |from: usize, to: usize| -> Option<i64> {
        let part = text.get(from..to)?;
        part.bytes()
            .all(|c| c.is_ascii_digit())
            .then(|| part.parse().ok())?
    };
    if b.len() < 20 || b[4] != b'-' || b[7] != b'-' || !matches!(b[10], b'T' | b' ') {
        return None;
    }
    let (year, month, day) = (num(0, 4)?, num(5, 7)?, num(8, 10)?);
    let (hour, minute, second) = (num(11, 13)?, num(14, 16)?, num(17, 19)?);
    if !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
        || hour > 23
        || minute > 59
        || second > 60
    {
        return None;
    }
    let mut i = 19;
    if b[i] == b'.' {
        i += 1;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
    }
    let offset = match b.get(i)? {
        b'Z' if i + 1 == b.len() => 0,
        sign @ (b'+' | b'-') if i + 6 == b.len() && b[i + 3] == b':' => {
            let secs = num(i + 1, i + 3)? * 3600 + num(i + 4, i + 6)? * 60;
            if *sign == b'+' { secs } else { -secs }
        }
        _ => return None,
    };
    // Days from civil date (proleptic Gregorian), Howard Hinnant's algorithm.
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    Some(days * 86_400 + hour * 3600 + minute * 60 + second - offset)
}

/// Why no usable token was found; messages never contain token material.
#[derive(Debug, PartialEq, Eq)]
pub enum CredentialError {
    Missing,
    Expired,
}

/// Extracts the access token from Claude Code's credentials file contents.
pub fn access_token(json: &[u8], now_ms: i64) -> Result<String, CredentialError> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Oauth {
        access_token: Option<String>,
        expires_at: Option<i64>,
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct File {
        claude_ai_oauth: Option<Oauth>,
    }
    let oauth = serde_json::from_slice::<File>(json)
        .ok()
        .and_then(|f| f.claude_ai_oauth)
        .ok_or(CredentialError::Missing)?;
    let token = oauth
        .access_token
        .filter(|t| !t.is_empty() && t.len() <= 4096 && !t.chars().any(char::is_control))
        .ok_or(CredentialError::Missing)?;
    if oauth.expires_at.is_some_and(|at| at <= now_ms) {
        return Err(CredentialError::Expired);
    }
    Ok(token)
}

/// Maps an HTTP status to the message shown in the usage card.
pub fn status_error(status: u16) -> Option<Error> {
    match status {
        200 => None,
        401 | 403 => Some(Error::Service(
            "Claude usage unauthorized (sign in to Claude Code again)",
        )),
        429 => Some(Error::Service("Claude usage rate limited")),
        _ => Some(Error::Service("Claude usage service error")),
    }
}

/// Speaks just enough of the app-server JSON-RPC shape for `Client`/`Connection`, answering
/// `account/rateLimits/read` from `fetch` so the shared usage driver serves Claude unchanged.
pub struct ClaudeTransport<F> {
    fetch: F,
    replies: VecDeque<Value>,
}
impl<F: FnMut() -> Result<Value, Error>> ClaudeTransport<F> {
    pub fn new(fetch: F) -> Self {
        Self {
            fetch,
            replies: VecDeque::new(),
        }
    }
}
impl<F: FnMut() -> Result<Value, Error>> Transport for ClaudeTransport<F> {
    fn send(&mut self, value: &Value, _timeout: Duration) -> Result<(), Error> {
        let Some(id) = value.get("id") else {
            return Ok(()); // notifications such as `initialized`
        };
        let reply = match value["method"].as_str() {
            Some("initialize") => json!({"id": id, "result": {}}),
            Some("account/rateLimits/read") => json!({"id": id, "result": (self.fetch)()?}),
            _ => json!({"id": id, "error": {"code": -32601}}),
        };
        self.replies.push_back(reply);
        Ok(())
    }
    fn receive(&mut self, _timeout: Duration) -> Result<Value, Error> {
        self.replies.pop_front().ok_or(Error::Timeout)
    }
}

/// Reads the stored token and queries the live endpoint.
#[cfg(windows)]
pub fn fetch_live() -> Result<Value, Error> {
    let dir = std::env::var_os("CLAUDE_CONFIG_DIR")
        .map(std::path::PathBuf::from)
        .or_else(|| {
            std::env::var_os("USERPROFILE").map(|p| std::path::Path::new(&p).join(".claude"))
        })
        .ok_or(Error::Service("Claude credentials unavailable"))?;
    let mut bytes = Vec::new();
    {
        use std::io::Read;
        std::fs::File::open(dir.join(".credentials.json"))
            .and_then(|f| f.take(65_536).read_to_end(&mut bytes))
            .map_err(|_| Error::Service("Claude credentials unavailable"))?;
    }
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    let token = access_token(&bytes, now_ms).map_err(|e| {
        Error::Service(match e {
            CredentialError::Missing => "Claude credentials unavailable",
            CredentialError::Expired => "Claude token expired (open Claude Code to refresh)",
        })
    })?;
    let headers = format!(
        "Authorization: Bearer {token}\r\nanthropic-beta: oauth-2025-04-20\r\nAccept: application/json\r\n"
    );
    let (status, body) =
        crate::https::get(HOST, PATH, &headers, crate::app_server::REQUEST_TIMEOUT)
            .map_err(|_| Error::Service("Claude usage request failed"))?;
    if let Some(error) = status_error(status) {
        return Err(error);
    }
    snapshot_from_response(&body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::usage::Snapshot;

    #[test]
    fn response_becomes_a_two_window_claude_snapshot() {
        let value = snapshot_from_response(
            br#"{"five_hour":{"utilization":23.5,"resets_at":"2026-10-25T10:00:00.5+00:00"},
                 "seven_day":{"utilization":41,"resets_at":null},"extra":{"x":1}}"#,
        )
        .unwrap();
        let snapshot = Snapshot::parse(value).unwrap();
        let bucket = &snapshot.buckets["claude"];
        let (five, seven) = (
            bucket.primary.as_ref().unwrap(),
            bucket.secondary.as_ref().unwrap(),
        );
        assert_eq!(five.used_percent, Some(23.5));
        assert_eq!(five.window_duration_mins, Some(300));
        assert_eq!(five.resets_at, Some(1_792_922_400));
        assert_eq!(seven.window_duration_mins, Some(10_080));
        assert_eq!(seven.resets_at, None);
    }

    #[test]
    fn live_response_shape_observed_2026_10_25_parses() {
        // Sanitized from a real 200 response; extra windows and objects must be ignored.
        let body = br#"{"five_hour":{"utilization":10.0,"resets_at":"2026-10-26T00:59:59.931298+00:00","limit_dollars":null,"used_dollars":null,"remaining_dollars":null,"locked_reason":null},
            "seven_day":{"utilization":11.0,"resets_at":"2026-10-31T09:59:59.931317+00:00","limit_dollars":null,"used_dollars":null,"remaining_dollars":null,"locked_reason":null},
            "seven_day_opus":null,"extra_usage":{"is_enabled":false},"limits":[{"kind":"session","percent":10}]}"#;
        let snapshot = Snapshot::parse(snapshot_from_response(body).unwrap()).unwrap();
        let bucket = &snapshot.buckets["claude"];
        let five = bucket.primary.as_ref().unwrap();
        assert_eq!(five.used_percent, Some(10.0));
        assert_eq!(five.resets_at, iso_to_unix("2026-10-26T00:59:59Z"));
        assert_eq!(bucket.secondary.as_ref().unwrap().used_percent, Some(11.0));
    }

    #[test]
    fn unusable_responses_are_malformed() {
        for body in [&b"{"[..], b"[]", b"{}", br#"{"five_hour":"x"}"#] {
            assert_eq!(snapshot_from_response(body), Err(Error::Malformed));
        }
    }

    #[test]
    fn iso_timestamps_cover_offsets_and_reject_garbage() {
        assert_eq!(iso_to_unix("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(iso_to_unix("2024-02-29T12:00:00Z"), Some(1_709_208_000));
        assert_eq!(
            iso_to_unix("2024-02-29T14:30:00+02:30"),
            Some(1_709_208_000)
        );
        assert_eq!(
            iso_to_unix("2024-02-29T09:00:00.123456-03:00"),
            Some(1_709_208_000)
        );
        for bad in [
            "",
            "yesterday",
            "2024-13-01T00:00:00Z",
            "2024-01-01T00:00:00",
            "2024-01-01T00:00:00+0200",
        ] {
            assert_eq!(iso_to_unix(bad), None, "{bad}");
        }
    }

    #[test]
    fn credentials_yield_a_token_only_when_present_and_unexpired() {
        let file = |extra: &str| format!(r#"{{"claudeAiOauth":{{"accessToken":"tok"{extra}}}}}"#);
        assert_eq!(access_token(file("").as_bytes(), 10).as_deref(), Ok("tok"));
        assert_eq!(
            access_token(file(r#","expiresAt":11"#).as_bytes(), 10).as_deref(),
            Ok("tok")
        );
        assert_eq!(
            access_token(file(r#","expiresAt":10"#).as_bytes(), 10),
            Err(CredentialError::Expired)
        );
        for bad in [&b"{"[..], b"{}", br#"{"claudeAiOauth":{"accessToken":""}}"#] {
            assert_eq!(access_token(bad, 0), Err(CredentialError::Missing));
        }
    }

    #[test]
    fn http_statuses_map_to_sanitized_errors() {
        assert_eq!(status_error(200), None);
        for status in [401, 403, 429, 500] {
            let text = status_error(status).unwrap().to_string();
            assert!(text.starts_with("Claude"), "{text}");
        }
    }

    #[test]
    fn shared_driver_serves_claude_through_the_transport() {
        use crate::usage_service::Driver;
        let mut driver = Driver::new(Duration::from_secs(300), || {
            Ok(ClaudeTransport::new(|| {
                snapshot_from_response(br#"{"five_hour":{"utilization":10,"resets_at":null}}"#)
            }))
        });
        assert!(driver.step(Duration::from_secs(1), true));
        assert!(driver.view.error.is_none());
        assert_eq!(
            driver.view.snapshot.unwrap().buckets["claude"]
                .primary
                .as_ref()
                .unwrap()
                .used_percent,
            Some(10.0)
        );
    }

    #[test]
    fn fetch_failure_becomes_a_view_error_not_a_panic() {
        use crate::usage_service::Driver;
        let mut driver = Driver::new(Duration::from_secs(300), || {
            Ok(ClaudeTransport::new(|| {
                Err(Error::Service(
                    "Claude usage unauthorized (sign in to Claude Code again)",
                ))
            }))
        });
        assert!(driver.step(Duration::from_secs(1), true));
        assert!(driver.view.snapshot.is_none());
        assert!(
            driver
                .view
                .error
                .as_deref()
                .unwrap()
                .contains("unauthorized")
        );
    }
}
