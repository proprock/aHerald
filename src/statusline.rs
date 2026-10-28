//! The part of Claude Code's `statusLine` payload aHerald uses: the session's name and the
//! account's 5-hour and 7-day quota. Nothing else in the payload is read, kept or logged.
use crate::usage::{Bucket, Snapshot, Window};
use serde::{Deserialize, Serialize};

/// A statusLine payload is a few KiB; anything larger is not one (and `read_bounded` refuses it).
pub const MAX_PAYLOAD: usize = crate::events::MAX_INPUT;

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Limit {
    pub used_percent: f64,
    /// Unix seconds at which the window resets.
    pub resets_at: Option<i64>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Quota {
    pub five_hour: Option<Limit>,
    pub seven_day: Option<Limit>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Report {
    pub session_id: String,
    pub session_name: Option<String>,
    pub quota: Option<Quota>,
}

/// Reads the session id, name and quota from a statusLine payload.
pub fn parse(bytes: &[u8]) -> Result<Report, &'static str> {
    #[derive(Deserialize)]
    struct Window {
        used_percentage: Option<f64>,
        resets_at: Option<i64>,
    }
    #[derive(Deserialize)]
    struct Limits {
        five_hour: Option<Window>,
        seven_day: Option<Window>,
    }
    #[derive(Deserialize)]
    struct Wire {
        session_id: Option<String>,
        session_name: Option<String>,
        rate_limits: Option<Limits>,
    }
    if bytes.len() > MAX_PAYLOAD {
        return Err("statusLine payload is too large");
    }
    let wire: Wire = serde_json::from_slice(bytes).map_err(|_| "invalid statusLine payload")?;
    let session_id = wire
        .session_id
        .filter(|id| !id.is_empty() && id.len() <= 256 && !id.chars().any(char::is_control))
        .ok_or("statusLine payload has no valid session id")?;
    let limit = |window: Option<Window>| {
        let window = window?;
        let used = window
            .used_percentage
            .filter(|v| (0.0..=100.0).contains(v))?;
        Some(Limit {
            used_percent: used,
            resets_at: window.resets_at.filter(|t| *t > 0),
        })
    };
    let quota = wire.rate_limits.and_then(|l| {
        let quota = Quota {
            five_hour: limit(l.five_hour),
            seven_day: limit(l.seven_day),
        };
        (quota != Quota::default()).then_some(quota)
    });
    Ok(Report {
        session_id,
        session_name: wire
            .session_name
            .as_deref()
            .and_then(crate::session_source::clean),
        quota,
    })
}

const FIVE_HOUR_MINS: i64 = 5 * 60;
const SEVEN_DAY_MINS: i64 = 7 * 24 * 60;

impl Quota {
    /// The quota as a one-bucket Claude snapshot, like the one the usage endpoint yields.
    pub fn snapshot(&self, plan: Option<&str>) -> Option<Snapshot> {
        let window = |limit: Option<Limit>, mins: i64| {
            limit.map(|l| Window {
                used_percent: Some(l.used_percent),
                window_duration_mins: Some(mins),
                resets_at: l.resets_at,
            })
        };
        let bucket = Bucket {
            limit_id: Some("claude".into()),
            primary: window(self.five_hour, FIVE_HOUR_MINS),
            secondary: window(self.seven_day, SEVEN_DAY_MINS),
            plan_type: plan.map(str::to_owned),
            ..Bucket::default()
        };
        if bucket.primary.is_none() && bucket.secondary.is_none() {
            return None;
        }
        Snapshot::single("claude", bucket).ok()
    }
}

/// How many session names are remembered; the oldest is forgotten first.
const MAX_NAMES: usize = 64;
/// A quota not refreshed for this long is no longer shown.
const QUOTA_MAX_AGE_SECS: i64 = 60 * 60;

/// What the resident has learned from statusLine reports.
#[derive(Debug, Default)]
pub struct Store {
    names: std::collections::VecDeque<(String, String)>,
    quota: Option<(Quota, i64)>,
}

impl Store {
    /// Records one report received at `now` (Unix seconds).
    pub fn record(&mut self, report: Report, now: i64) {
        if let Some(name) = report.session_name {
            self.names.retain(|(id, _)| *id != report.session_id);
            if self.names.len() >= MAX_NAMES {
                self.names.pop_front();
            }
            self.names.push_back((report.session_id, name));
        }
        if let Some(quota) = report.quota {
            self.quota = Some((quota, now));
        }
    }

    pub fn name(&self, session_id: &str) -> Option<&str> {
        self.names
            .iter()
            .find(|(id, _)| id == session_id)
            .map(|(_, name)| name.as_str())
    }

    /// The latest quota as a snapshot: windows that have already reset are dropped, and a
    /// report older than an hour is not shown.
    pub fn quota_snapshot(&self, now: i64, plan: Option<&str>) -> Option<Snapshot> {
        let (quota, seen) = self.quota.as_ref()?;
        if now.saturating_sub(*seen) > QUOTA_MAX_AGE_SECS {
            return None;
        }
        let live = |limit: Option<Limit>| limit.filter(|l| l.resets_at.is_none_or(|t| t > now));
        Quota {
            five_hour: live(quota.five_hour),
            seven_day: live(quota.seven_day),
        }
        .snapshot(plan)
    }
}

/// What the `--statusline` wrapper hands back to Claude Code.
#[derive(Debug, PartialEq, Eq)]
pub struct Outcome {
    pub code: i32,
    /// The wrapped command's standard output, to be printed unchanged.
    pub stdout: Vec<u8>,
}

/// Runs the wrapper: `send` gets the parsed report (on its own thread, so a busy resident never
/// delays the status line), and `then`, when given, receives the same payload on its standard
/// input. The payload need not be valid for `then` to run.
pub fn run(input: &[u8], then: Option<&str>, send: impl FnOnce(Report) + Send) -> Outcome {
    std::thread::scope(|scope| {
        if let Ok(report) = parse(input) {
            scope.spawn(move || send(report));
        }
        match then {
            Some(command) => run_then(command, input),
            None => Outcome {
                code: 0,
                stdout: Vec::new(),
            },
        }
    })
}

fn run_then(command: &str, input: &[u8]) -> Outcome {
    use std::{
        io::Write,
        process::{Command, Stdio},
    };
    #[cfg(unix)]
    let mut child = Command::new("sh");
    #[cfg(unix)]
    child.arg("-c").arg(command);
    #[cfg(windows)]
    let mut child = Command::new("powershell.exe");
    #[cfg(windows)]
    child
        .args(["-NoProfile", "-NonInteractive", "-Command"])
        .arg(command);
    let spawned = child
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn();
    let Ok(mut spawned) = spawned else {
        return Outcome {
            code: 127,
            stdout: Vec::new(),
        };
    };
    if let Some(mut stdin) = spawned.stdin.take() {
        // A command that ignores its input closes the pipe early; that is not an error.
        let _ = stdin.write_all(input);
    }
    match spawned.wait_with_output() {
        Ok(output) => Outcome {
            code: output.status.code().unwrap_or(1),
            stdout: output.stdout,
        },
        Err(_) => Outcome {
            code: 1,
            stdout: Vec::new(),
        },
    }
}

static STORE: std::sync::Mutex<Option<Store>> = std::sync::Mutex::new(None);

/// The resident's store. Only a resident ever records into it.
pub fn with_store<R>(f: impl FnOnce(&mut Store) -> R) -> R {
    let mut guard = STORE.lock().unwrap_or_else(|e| e.into_inner());
    f(guard.get_or_insert_with(Store::default))
}

#[cfg(test)]
mod tests {
    use super::*;

    const FULL: &str = r#"{
      "session_id": "abc-123",
      "session_name": "refactor\u0007 the<RLO> parser",
      "cwd": "/Users/me/secret-project",
      "model": {"display_name": "Opus"},
      "transcript_path": "/Users/me/.claude/projects/x.jsonl",
      "cost": {"total_cost_usd": 1.5},
      "rate_limits": {
        "five_hour": {"used_percentage": 23.5, "resets_at": 1738425600},
        "seven_day": {"used_percentage": 41.2, "resets_at": 1738857600}
      }
    }"#;

    #[test]
    fn only_the_id_the_cleaned_name_and_the_quota_are_taken() {
        let payload = FULL.replace("<RLO>", "\u{202e}");
        let report = parse(payload.as_bytes()).unwrap();
        assert_eq!(report.session_id, "abc-123");
        assert_eq!(report.session_name.as_deref(), Some("refactor the parser"));
        let quota = report.quota.unwrap();
        assert_eq!(
            quota.five_hour,
            Some(Limit {
                used_percent: 23.5,
                resets_at: Some(1738425600)
            })
        );
        assert_eq!(quota.seven_day.unwrap().used_percent, 41.2);
        let encoded = serde_json::to_string(&report).unwrap();
        for secret in ["secret-project", "transcript", "Opus", "total_cost"] {
            assert!(!encoded.contains(secret), "{secret}");
        }
    }

    #[test]
    fn a_payload_without_a_name_or_limits_still_identifies_the_session() {
        let report = parse(br#"{"session_id":"s"}"#).unwrap();
        assert_eq!((report.session_name, report.quota), (None, None));
        let blank =
            parse(br#"{"session_id":"s","session_name":"  \n ","rate_limits":{}}"#).unwrap();
        assert_eq!(blank.session_name, None);
        assert_eq!(blank.quota, None);
    }

    #[test]
    fn one_window_alone_is_kept_and_out_of_range_values_are_dropped() {
        let report =
            parse(br#"{"session_id":"s","rate_limits":{"seven_day":{"used_percentage":12}}}"#)
                .unwrap();
        let quota = report.quota.unwrap();
        assert_eq!(quota.five_hour, None);
        assert_eq!(
            quota.seven_day,
            Some(Limit {
                used_percent: 12.0,
                resets_at: None
            })
        );
        let bad = parse(
            br#"{"session_id":"s","rate_limits":{"five_hour":{"used_percentage":250},"seven_day":{"used_percentage":-1}}}"#,
        )
        .unwrap();
        assert_eq!(bad.quota, None);
    }

    #[test]
    fn invalid_ids_malformed_json_and_oversized_payloads_are_refused() {
        for bad in [
            &br#"{"session_name":"x"}"#[..],
            br#"{"session_id":""}"#,
            br#"{"session_id":"a\u0000b"}"#,
            br#"[1]"#,
            b"not json",
        ] {
            assert!(parse(bad).is_err(), "{}", String::from_utf8_lossy(bad));
        }
        let long = format!(r#"{{"session_id":"{}"}}"#, "x".repeat(257));
        assert!(parse(long.as_bytes()).is_err());
        let big = format!(
            r#"{{"session_id":"s","pad":"{}"}}"#,
            "x".repeat(MAX_PAYLOAD)
        );
        assert!(parse(big.as_bytes()).is_err());
    }

    #[test]
    fn the_quota_becomes_a_claude_snapshot_with_both_windows() {
        let quota = parse(FULL.as_bytes()).unwrap().quota.unwrap();
        let snapshot = quota.snapshot(Some("max")).unwrap();
        let bucket = &snapshot.buckets["claude"];
        assert_eq!(bucket.plan_type.as_deref(), Some("max"));
        let five = bucket.primary.as_ref().unwrap();
        assert_eq!(
            (five.used_percent, five.window_duration_mins, five.resets_at),
            (Some(23.5), Some(300), Some(1738425600))
        );
        let seven = bucket.secondary.as_ref().unwrap();
        assert_eq!(
            (seven.used_percent, seven.window_duration_mins),
            (Some(41.2), Some(10_080))
        );
        assert!(Quota::default().snapshot(None).is_none());
    }

    fn report(id: &str, name: Option<&str>, quota: Option<Quota>) -> Report {
        Report {
            session_id: id.into(),
            session_name: name.map(str::to_owned),
            quota,
        }
    }
    fn limit(used: f64, resets_at: i64) -> Option<Limit> {
        Some(Limit {
            used_percent: used,
            resets_at: Some(resets_at),
        })
    }

    #[test]
    fn names_are_kept_per_session_and_the_oldest_is_forgotten() {
        let mut store = Store::default();
        store.record(report("a", Some("First"), None), 0);
        store.record(report("b", Some("Second"), None), 0);
        store.record(report("a", Some("Renamed"), None), 0);
        assert_eq!(
            (store.name("a"), store.name("b")),
            (Some("Renamed"), Some("Second"))
        );
        // A report without a name keeps what is known.
        store.record(report("a", None, None), 0);
        assert_eq!(store.name("a"), Some("Renamed"));
        for i in 0..MAX_NAMES {
            store.record(report(&format!("s{i}"), Some("x"), None), 0);
        }
        assert_eq!(store.name("b"), None);
        assert_eq!(store.name(&format!("s{}", MAX_NAMES - 1)), Some("x"));
    }

    #[test]
    fn the_quota_is_shown_while_fresh_and_per_window_until_it_resets() {
        let mut store = Store::default();
        let quota = Quota {
            five_hour: limit(10.0, 1_000),
            seven_day: limit(50.0, 100_000),
        };
        assert!(store.quota_snapshot(0, None).is_none());
        store.record(report("a", None, Some(quota)), 100);
        let both = store.quota_snapshot(200, Some("pro")).unwrap();
        let bucket = &both.buckets["claude"];
        assert!(bucket.primary.is_some() && bucket.secondary.is_some());
        // The 5 h window has reset by 1500; only the weekly one remains.
        let weekly = store.quota_snapshot(1_500, None).unwrap();
        let bucket = &weekly.buckets["claude"];
        assert!(bucket.primary.is_none() && bucket.secondary.is_some());
        // A report an hour old is not shown any more.
        assert!(
            store
                .quota_snapshot(100 + QUOTA_MAX_AGE_SECS + 1, None)
                .is_none()
        );
        // A newer report replaces it.
        store.record(
            report(
                "b",
                None,
                Some(Quota {
                    five_hour: limit(90.0, 9_000),
                    seven_day: None,
                }),
            ),
            3_000,
        );
        let newer = store.quota_snapshot(3_100, None).unwrap();
        assert_eq!(
            newer.buckets["claude"]
                .primary
                .as_ref()
                .unwrap()
                .used_percent,
            Some(90.0)
        );
    }

    #[test]
    fn the_shared_store_remembers_names_across_calls() {
        with_store(|s| s.record(report("global-store-test", Some("Kept"), None), 0));
        assert_eq!(
            with_store(|s| s.name("global-store-test").map(str::to_owned)),
            Some("Kept".into())
        );
    }

    #[cfg(unix)]
    mod wrapper {
        use super::super::*;
        use std::sync::{Arc, Mutex};

        fn capture() -> (Arc<Mutex<Vec<Report>>>, impl FnOnce(Report) + Send) {
            let sent = Arc::new(Mutex::new(Vec::new()));
            let sink = sent.clone();
            (sent, move |r| sink.lock().unwrap().push(r))
        }
        const PAYLOAD: &[u8] = br#"{"session_id":"w1","session_name":"N","rate_limits":{"five_hour":{"used_percentage":5,"resets_at":99}}}"#;

        #[test]
        fn without_a_wrapped_command_nothing_is_printed_and_the_report_is_sent() {
            let (sent, send) = capture();
            let out = run(PAYLOAD, None, send);
            assert_eq!(
                out,
                Outcome {
                    code: 0,
                    stdout: Vec::new()
                }
            );
            let sent = sent.lock().unwrap();
            assert_eq!(sent.len(), 1);
            assert_eq!(sent[0].session_id, "w1");
        }

        #[test]
        fn the_wrapped_command_gets_the_whole_payload_and_its_output_is_returned_unchanged() {
            let (sent, send) = capture();
            let out = run(PAYLOAD, Some("cat"), send);
            assert_eq!((out.code, out.stdout.as_slice()), (0, PAYLOAD));
            assert_eq!(sent.lock().unwrap().len(), 1);
        }

        #[test]
        fn the_wrapped_command_keeps_its_exit_code_and_may_ignore_its_input() {
            let (_, send) = capture();
            let out = run(&vec![b'x'; 200_000], Some("echo line; exit 3"), send);
            assert_eq!((out.code, out.stdout.as_slice()), (3, &b"line\n"[..]));
        }

        #[test]
        fn an_invalid_payload_sends_nothing_but_still_reaches_the_wrapped_command() {
            let (sent, send) = capture();
            let out = run(b"plain text", Some("cat"), send);
            assert_eq!(out.stdout, b"plain text");
            assert!(sent.lock().unwrap().is_empty());
        }

        #[test]
        fn a_command_that_cannot_run_reports_failure_without_panicking() {
            let (_, send) = capture();
            let out = run(PAYLOAD, Some("definitely-not-a-command-aherald"), send);
            assert_ne!(out.code, 0);
        }
    }
}
