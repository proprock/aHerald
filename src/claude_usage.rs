//! Opt-in, read-only Claude Code quota (5-hour and 7-day windows).
//!
//! Claude Code hooks carry no quota, so this polls the undocumented
//! `GET https://api.anthropic.com/api/oauth/usage` with the access token Claude Code already stores
//! in `.credentials.json`. The token is read per request, sent only to that host, never logged, kept
//! in memory only for the call, and never refreshed or written back. The response shape is not a
//! documented contract: unknown or malformed data becomes an "unavailable" view.
use crate::{
    app_server::Error,
    usage::{Allowance, Bucket, Snapshot},
    usage_service::UsageSource,
};
use serde::Deserialize;
use serde_json::Value;
use std::time::Duration;

pub const HOST: &str = "api.anthropic.com";
pub const PATH: &str = "/api/oauth/usage";
const FIVE_HOUR_MINS: i64 = 300;
const SEVEN_DAY_MINS: i64 = 7 * 24 * 60;

/// Converts the usage response (and the optional plan label) into a one-bucket snapshot.
pub fn parse_response(body: &[u8], plan: Option<&str>) -> Result<Snapshot, Error> {
    #[derive(Deserialize)]
    struct Window {
        utilization: Option<f64>,
        resets_at: Option<String>,
    }
    #[derive(Deserialize)]
    struct Wire {
        five_hour: Option<Window>,
        seven_day: Option<Window>,
        extra_usage: Option<Value>,
        spend: Option<Value>,
    }
    let wire: Wire = serde_json::from_slice(body).map_err(|_| Error::Malformed)?;
    if wire.five_hour.is_none() && wire.seven_day.is_none() {
        return Err(Error::Malformed);
    }
    let window = |w: Option<Window>, mins: i64| {
        w.map(|w| crate::usage::Window {
            used_percent: w.utilization,
            window_duration_mins: Some(mins),
            resets_at: w.resets_at.as_deref().and_then(iso_to_unix),
        })
    };
    let bucket = Bucket {
        limit_id: Some("claude".into()),
        primary: window(wire.five_hour, FIVE_HOUR_MINS),
        secondary: window(wire.seven_day, SEVEN_DAY_MINS),
        plan_type: plan.map(str::to_owned),
        extra_usage: wire.extra_usage.as_ref().and_then(extra_usage),
        spend: wire.spend.as_ref().and_then(spend),
        ..Bucket::default()
    };
    Snapshot::single("claude", bucket).map_err(Error::Service)
}

/// Formats an amount in minor units (`1234`, exponent 2 -> `12.34`).
fn money(minor: i64, exponent: u32) -> String {
    let exponent = exponent.min(6);
    let scale = 10_i64.pow(exponent);
    let sign = if minor < 0 { "-" } else { "" };
    let abs = minor.unsigned_abs();
    if exponent == 0 {
        return format!("{sign}{abs}");
    }
    let scale = scale as u64;
    format!(
        "{sign}{}.{:0width$}",
        abs / scale,
        abs % scale,
        width = exponent as usize
    )
}

/// A bare number is minor units; a `{amount_minor, currency, exponent}` object carries its own.
fn amount(value: Option<&Value>, default_exponent: u32) -> Option<(String, Option<String>)> {
    let value = value?;
    let (minor, exponent, currency) = if let Some(minor) = value.as_i64() {
        (minor, default_exponent, None)
    } else {
        (
            value.get("amount_minor")?.as_i64()?,
            value
                .get("exponent")
                .and_then(Value::as_u64)
                .map_or(default_exponent, |e| e.min(6) as u32),
            value
                .get("currency")
                .and_then(Value::as_str)
                .map(str::to_owned),
        )
    };
    Some((money(minor, exponent), currency))
}

/// Keeps a currency code only when it is a short alphabetic string such as `USD`.
fn currency_code(currency: Option<&str>) -> Option<String> {
    currency
        .filter(|c| c.len() <= 8 && c.chars().all(|ch| ch.is_ascii_alphabetic()))
        .map(str::to_owned)
}

fn extra_usage(extra: &Value) -> Option<Allowance> {
    extra.as_object()?;
    let places = extra
        .get("decimal_places")
        .and_then(Value::as_u64)
        .map_or(2, |p| p.min(6) as u32);
    Some(Allowance {
        enabled: extra.get("is_enabled").and_then(Value::as_bool),
        used: amount(extra.get("used_credits"), places).map(|a| a.0),
        limit: amount(extra.get("monthly_limit"), places).map(|a| a.0),
        currency: currency_code(extra.get("currency").and_then(Value::as_str)),
    })
}

fn spend(spend: &Value) -> Option<Allowance> {
    spend.as_object()?;
    let used = amount(spend.get("used"), 2);
    let limit = amount(spend.get("limit"), 2);
    let currency = used
        .as_ref()
        .and_then(|a| a.1.as_deref())
        .or_else(|| limit.as_ref().and_then(|a| a.1.as_deref()));
    Some(Allowance {
        enabled: spend.get("enabled").and_then(Value::as_bool),
        currency: currency_code(currency),
        used: used.map(|a| a.0),
        limit: limit.map(|a| a.0),
    })
}

/// Reads only the non-secret `subscriptionType` label from the credentials file contents.
pub fn subscription_type(json: &[u8]) -> Option<String> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Oauth {
        subscription_type: Option<String>,
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct File {
        claude_ai_oauth: Option<Oauth>,
    }
    serde_json::from_slice::<File>(json)
        .ok()?
        .claude_ai_oauth?
        .subscription_type
        .filter(|p| !p.is_empty() && p.len() <= 32 && p.chars().all(|c| c.is_ascii_alphanumeric()))
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
pub fn status_error(status: u16, retry_after: Option<Duration>) -> Option<Error> {
    match status {
        200 => None,
        401 | 403 => Some(Error::Service(
            "Claude usage unauthorized (sign in to Claude Code again)",
        )),
        429 => Some(Error::RateLimited("Claude usage rate limited", retry_after)),
        // Redirects are never followed, so the token cannot leave the usage host.
        300..=399 => Some(Error::Service("Claude usage request redirected (refused)")),
        _ => Some(Error::Service("Claude usage service error")),
    }
}

/// One quota read from the configured source. `token` reads the stored login and asks the
/// endpoint; `statusline` is what Claude Code's statusLine last reported. With `Statusline` the
/// token function is never called, so the login is never touched.
pub fn read_with(
    source: crate::config::ClaudeUsageSource,
    token: impl FnOnce() -> Result<Snapshot, Error>,
    statusline: impl FnOnce() -> Option<Snapshot>,
) -> Result<Snapshot, Error> {
    use crate::config::ClaudeUsageSource as Source;
    const WAITING: Error = Error::Service("Waiting for a Claude Code statusLine report");
    match source {
        Source::Token => token(),
        Source::Statusline => statusline().ok_or(WAITING),
        Source::Auto => token().or_else(|error| statusline().ok_or(error)),
    }
}

/// Claude quota source: one HTTPS request per read, no connection and no pushed updates.
pub struct ClaudeSource<F> {
    fetch: F,
}
impl<F: FnMut() -> Result<Snapshot, Error>> ClaudeSource<F> {
    pub fn new(fetch: F) -> Self {
        Self { fetch }
    }
}
impl<F: FnMut() -> Result<Snapshot, Error>> UsageSource for ClaudeSource<F> {
    fn read(&mut self, _now: Duration) -> Result<Snapshot, Error> {
        let _timer = crate::perf::span(crate::perf::Span::UsagePoll);
        (self.fetch)()
    }
}

/// Reads the stored credentials and queries the live endpoint.
#[cfg(any(windows, unix))]
pub fn fetch_live() -> Result<Snapshot, Error> {
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    fetch_with(stored_credentials, crate::https::get, now_ms)
}

type Credentials = crate::https::Wiped<Vec<u8>>;

/// Claude Code's credentials JSON: the Keychain item on macOS, `.credentials.json` elsewhere.
/// The bytes are zeroed when dropped.
#[cfg(target_os = "macos")]
fn stored_credentials() -> Result<Credentials, Error> {
    use crate::mac::keychain::{Failure, read};
    read(KEYCHAIN_SERVICE).map_err(|failure| {
        Error::Service(match failure {
            Failure::Denied => {
                "Claude credentials access denied (allow aHerald in the Keychain prompt)"
            }
            Failure::Unavailable => "Claude credentials unavailable (the Keychain is locked)",
            Failure::NotFound | Failure::Other(_) => "Claude credentials unavailable",
        })
    })
}

#[cfg(all(unix, not(target_os = "macos")))]
fn stored_credentials() -> Result<Credentials, Error> {
    file_credentials()
}

#[cfg(windows)]
fn stored_credentials() -> Result<Credentials, Error> {
    file_credentials()
}

/// The Keychain service Claude Code stores its credentials under on macOS.
#[cfg(target_os = "macos")]
const KEYCHAIN_SERVICE: &str = "Claude Code-credentials";

#[cfg(any(windows, unix))]
#[cfg_attr(target_os = "macos", allow(dead_code))]
fn file_credentials() -> Result<Credentials, Error> {
    use std::io::Read;
    let dir = crate::paths::claude_config_dir()
        .ok_or(Error::Service("Claude credentials unavailable"))?;
    let mut bytes = crate::https::Wiped(Vec::new());
    std::fs::File::open(dir.join(".credentials.json"))
        .and_then(|f| f.take(65_536).read_to_end(&mut bytes.0))
        .map_err(|_| Error::Service("Claude credentials unavailable"))?;
    Ok(bytes)
}

/// One quota read. `credentials` supplies the stored JSON and `get` performs the HTTPS GET
/// (`host`, `path`, CRLF-separated headers, timeout); the token, the credentials and the request
/// headers are zeroed when dropped, and no error carries token material.
pub fn fetch_with(
    credentials: impl FnOnce() -> Result<Credentials, Error>,
    get: impl FnOnce(
        &str,
        &str,
        &str,
        Duration,
    ) -> Result<crate::https::Response, crate::https::HttpError>,
    now_ms: i64,
) -> Result<Snapshot, Error> {
    let bytes = credentials()?;
    let token = crate::https::Wiped(access_token(&bytes.0, now_ms).map_err(|e| {
        Error::Service(match e {
            CredentialError::Missing => "Claude credentials unavailable",
            CredentialError::Expired => "Claude token expired (open Claude Code to refresh)",
        })
    })?);
    let headers = crate::https::Wiped(format!(
        "Authorization: Bearer {}\r\nanthropic-beta: oauth-2025-04-20\r\nAccept: application/json\r\n",
        token.0
    ));
    let response = get(HOST, PATH, &headers.0, crate::app_server::REQUEST_TIMEOUT)
        .map_err(|_| Error::Service("Claude usage request failed"))?;
    if let Some(error) = status_error(response.status, response.retry_after) {
        return Err(error);
    }
    parse_response(&response.body, subscription_type(&bytes.0).as_deref())
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_configured_source_decides_which_reads_happen() {
        use crate::config::ClaudeUsageSource::{Auto, Statusline, Token};
        let snap = |id: &str| {
            Snapshot::single(
                "claude",
                Bucket {
                    limit_id: Some(id.into()),
                    primary: Some(crate::usage::Window {
                        used_percent: Some(1.0),
                        window_duration_mins: Some(300),
                        resets_at: None,
                    }),
                    ..Bucket::default()
                },
            )
            .unwrap()
        };
        let id =
            |r: Result<Snapshot, Error>| r.map(|s| s.buckets["claude"].limit_id.clone().unwrap());
        let boom = || -> Result<Snapshot, Error> { panic!("the token must not be read") };
        // statusline alone never touches the token and says so when it has nothing yet.
        assert_eq!(
            id(read_with(Statusline, boom, || Some(snap("s")))).unwrap(),
            "s"
        );
        assert!(matches!(
            read_with(Statusline, boom, || None),
            Err(Error::Service(_))
        ));
        // token alone never looks at the statusLine.
        let no_sl = || -> Option<Snapshot> { panic!("the statusLine must not be read") };
        assert_eq!(id(read_with(Token, || Ok(snap("t")), no_sl)).unwrap(), "t");
        assert!(read_with(Token, || Err(Error::Malformed), no_sl).is_err());
        // auto: the token wins, the statusLine only fills in when the token fails.
        assert_eq!(id(read_with(Auto, || Ok(snap("t")), no_sl)).unwrap(), "t");
        assert_eq!(
            id(read_with(
                Auto,
                || Err(Error::Malformed),
                || Some(snap("s"))
            ))
            .unwrap(),
            "s"
        );
        assert!(matches!(
            read_with(Auto, || Err(Error::Service("denied")), || None),
            Err(Error::Service("denied"))
        ));
    }

    use super::*;

    #[test]
    fn response_becomes_a_two_window_claude_snapshot() {
        let snapshot = parse_response(
            br#"{"five_hour":{"utilization":23.5,"resets_at":"2026-10-25T10:00:00.5+00:00"},
                 "seven_day":{"utilization":41,"resets_at":null},"extra":{"x":1}}"#,
            None,
        )
        .unwrap();
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
        let snapshot = parse_response(body, None).unwrap();
        let bucket = &snapshot.buckets["claude"];
        let five = bucket.primary.as_ref().unwrap();
        assert_eq!(five.used_percent, Some(10.0));
        assert_eq!(five.resets_at, iso_to_unix("2026-10-26T00:59:59Z"));
        assert_eq!(bucket.secondary.as_ref().unwrap().used_percent, Some(11.0));
    }

    #[test]
    fn details_card_follows_the_codex_template() {
        let body = br#"{"five_hour":{"utilization":8,"resets_at":"2026-10-25T17:50:00+00:00"},
            "seven_day":{"utilization":14,"resets_at":"2026-11-01T07:31:00+00:00"},
            "extra_usage":{"is_enabled":false,"monthly_limit":null}}"#;
        let snapshot = parse_response(body, Some("pro")).unwrap();
        let mut view = crate::usage_service::View::for_agent(
            crate::agent::Agent::Claude,
            true,
            Duration::from_secs(300),
        );
        view.snapshot = Some(snapshot);
        let unix = iso_to_unix("2026-10-25T15:25:00Z").unwrap();
        let text = view.card_text(Duration::ZERO, unix);
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines[0], "Claude plan: pro");
        assert!(lines[1].starts_with("5h: 8% used / 2:25"), "{}", lines[1]);
        assert!(lines[2].starts_with("Week: 14% used / "), "{}", lines[2]);
        assert_eq!(lines[3], "Extra usage: disabled, limit: —/—");
        assert!(!text.contains("Spend:"), "absent spend block is omitted");
    }

    #[test]
    fn extra_usage_and_spend_lines_show_amounts_with_currency() {
        let body = br#"{"five_hour":{"utilization":1,"resets_at":null},
            "extra_usage":{"is_enabled":true,"monthly_limit":5000,"used_credits":1234,
                           "currency":"USD","decimal_places":2},
            "spend":{"enabled":true,"used":{"amount_minor":250,"currency":"USD","exponent":2},
                     "limit":{"amount_minor":10000,"currency":"USD","exponent":2}}}"#;
        let text = card(body);
        assert!(
            text.contains("Extra usage: enabled, limit: 12.34/50.00 USD\n"),
            "{text}"
        );
        assert!(text.contains("Spend: enabled, 2.50/100.00 USD\n"), "{text}");
    }

    /// Card text a Claude view shows for a response body.
    fn card(body: &[u8]) -> String {
        let mut view = crate::usage_service::View::for_agent(
            crate::agent::Agent::Claude,
            true,
            Duration::from_secs(300),
        );
        view.snapshot = Some(parse_response(body, None).unwrap());
        view.card_text(Duration::ZERO, 0)
    }

    #[test]
    fn live_spend_with_zero_used_and_no_limit() {
        let body = br#"{"five_hour":{"utilization":1,"resets_at":null},
            "extra_usage":{"is_enabled":false,"monthly_limit":null,"used_credits":null,"currency":null},
            "spend":{"used":{"amount_minor":0,"currency":"USD","exponent":2},"limit":null,"enabled":false}}"#;
        let text = card(body);
        assert!(
            text.contains("Extra usage: disabled, limit: —/—\n"),
            "{text}"
        );
        assert!(text.contains("Spend: disabled, 0.00/— USD\n"), "{text}");
    }

    #[test]
    fn subscription_type_comes_from_credentials_without_the_token() {
        let file = |plan: &str| {
            format!(r#"{{"claudeAiOauth":{{"accessToken":"tok","subscriptionType":{plan}}}}}"#)
        };
        assert_eq!(
            subscription_type(file("\"max\"").as_bytes()).as_deref(),
            Some("max")
        );
        for bad in [file("null"), file("\"a b\""), file("\"\""), "{".into()] {
            assert_eq!(subscription_type(bad.as_bytes()), None, "{bad}");
        }
    }

    #[test]
    fn unusable_responses_are_malformed() {
        for body in [&b"{"[..], b"[]", b"{}", br#"{"five_hour":"x"}"#] {
            assert_eq!(parse_response(body, None).unwrap_err(), Error::Malformed);
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
        assert_eq!(status_error(200, None), None);
        for status in [301, 302, 307, 308, 401, 403, 429, 500] {
            let text = status_error(status, None).unwrap().to_string();
            assert!(text.starts_with("Claude"), "{text}");
        }
        assert!(
            status_error(302, None)
                .unwrap()
                .to_string()
                .contains("redirected")
        );
    }

    #[test]
    fn shared_driver_serves_claude_through_its_source() {
        use crate::{agent::Agent, usage_service::Driver};
        let source = ClaudeSource::new(|| {
            parse_response(
                br#"{"five_hour":{"utilization":10,"resets_at":null}}"#,
                None,
            )
        });
        let mut driver = Driver::with_source(Agent::Claude, Duration::from_secs(300), source);
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
        use crate::{agent::Agent, usage_service::Driver};
        let source = ClaudeSource::new(|| {
            Err(Error::Service(
                "Claude usage unauthorized (sign in to Claude Code again)",
            ))
        });
        let mut driver = Driver::with_source(Agent::Claude, Duration::from_secs(300), source);
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

    mod fetch {
        use super::*;
        use crate::https::{HttpError, Response, Wiped};
        use std::cell::RefCell;

        const TOKEN: &str = "sk-ant-secret-TOKEN-0123";
        const NOW: i64 = 1_800_000_000_000;

        fn credentials(expires_at: i64) -> Result<Wiped<Vec<u8>>, Error> {
            Ok(Wiped(
                format!(
                    r#"{{"claudeAiOauth":{{"accessToken":"{TOKEN}","expiresAt":{expires_at},"subscriptionType":"max"}}}}"#
                )
                .into_bytes(),
            ))
        }
        fn ok_body() -> Vec<u8> {
            br#"{"five_hour":{"utilization":12,"resets_at":"2026-10-26T00:59:59+00:00"},"seven_day":{"utilization":30,"resets_at":null}}"#.to_vec()
        }
        fn response(status: u16, body: Vec<u8>) -> Result<Response, HttpError> {
            Ok(Response {
                status,
                body,
                retry_after: None,
            })
        }

        #[test]
        fn a_valid_token_is_sent_to_the_usage_endpoint_only_and_the_plan_comes_from_the_item() {
            let seen = RefCell::new(None);
            let snapshot = fetch_with(
                || credentials(NOW + 60_000),
                |host, path, headers, _| {
                    *seen.borrow_mut() =
                        Some((host.to_owned(), path.to_owned(), headers.to_owned()));
                    response(200, ok_body())
                },
                NOW,
            )
            .unwrap();
            let (host, path, headers) = seen.into_inner().unwrap();
            assert_eq!((host.as_str(), path.as_str()), (HOST, PATH));
            assert!(headers.contains(&format!("Authorization: Bearer {TOKEN}\r\n")));
            assert!(headers.contains("anthropic-beta: oauth-2025-04-20"));
            assert_eq!(
                snapshot.buckets["claude"]
                    .primary
                    .as_ref()
                    .unwrap()
                    .used_percent,
                Some(12.0)
            );
            assert_eq!(snapshot.buckets["claude"].plan_type.as_deref(), Some("max"));
        }

        #[test]
        fn an_expired_token_is_refused_without_a_request() {
            let called = std::cell::Cell::new(false);
            let error = fetch_with(
                || credentials(NOW - 1),
                |_, _, _, _| {
                    called.set(true);
                    response(200, ok_body())
                },
                NOW,
            )
            .unwrap_err();
            assert!(!called.get());
            assert!(format!("{error:?}").contains("expired"), "{error:?}");
        }

        #[test]
        fn missing_credentials_and_unusable_items_make_no_request() {
            for stored in [
                Err(Error::Service("Claude credentials unavailable")),
                Ok(Wiped(b"not json".to_vec())),
                Ok(Wiped(br#"{"claudeAiOauth":{"accessToken":""}}"#.to_vec())),
            ] {
                let called = std::cell::Cell::new(false);
                let result = fetch_with(
                    || stored,
                    |_, _, _, _| {
                        called.set(true);
                        response(200, ok_body())
                    },
                    NOW,
                );
                assert!(result.is_err());
                assert!(!called.get());
            }
        }

        #[test]
        fn failures_never_carry_the_token() {
            let outcomes: Vec<Result<Response, HttpError>> = vec![
                response(401, TOKEN.as_bytes().to_vec()),
                response(429, Vec::new()),
                response(302, Vec::new()),
                response(500, TOKEN.as_bytes().to_vec()),
                response(200, TOKEN.as_bytes().to_vec()),
                Err(HttpError::Network),
                Err(HttpError::TooLarge),
            ];
            for outcome in outcomes {
                let error =
                    fetch_with(|| credentials(NOW + 1000), |_, _, _, _| outcome, NOW).unwrap_err();
                assert!(!format!("{error:?}").contains(TOKEN));
                assert!(!error.to_string().contains(TOKEN));
            }
        }

        #[test]
        fn an_unauthorized_answer_asks_to_sign_in_again() {
            let error = fetch_with(
                || credentials(NOW + 1000),
                |_, _, _, _| response(401, Vec::new()),
                NOW,
            )
            .unwrap_err();
            assert!(format!("{error:?}").contains("unauthorized"), "{error:?}");
        }
    }
}
