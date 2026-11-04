//! `--raw-output usage`: what a usage source last said is kept as it came, even when aHerald
//! cannot use it. One test only: the kept answers are process-wide.
use aherald::{
    agent::Agent,
    claude_usage::fetch_with,
    https::{HttpError, Response, Wiped},
    usage_service::{RAW_LIMIT, bounded_raw, last_raw},
};

const NOW: i64 = 1_000;

fn credentials() -> Result<Wiped<Vec<u8>>, aherald::app_server::Error> {
    Ok(Wiped(
        br#"{"claudeAiOauth":{"accessToken":"tok","expiresAt":9999999,"subscriptionType":"enterprise"}}"#
            .to_vec(),
    ))
}

fn answer(status: u16, body: &str) -> Result<Response, HttpError> {
    Ok(Response {
        status,
        body: body.as_bytes().to_vec(),
        retry_after: None,
    })
}

#[test]
fn the_last_answer_is_kept_whatever_it_was_and_never_longer_than_the_limit() {
    assert!(last_raw(Agent::Claude).is_none() && last_raw(Agent::Codex).is_none());

    // An answer that parses is kept as it came.
    let ok = r#"{"five_hour":{"utilization":1,"resets_at":null},"x":"extra field"}"#;
    assert!(fetch_with(credentials, |_, _, _, _| answer(200, ok), NOW).is_ok());
    let (note, _, body) = last_raw(Agent::Claude).unwrap();
    assert_eq!((note.as_str(), body.as_str()), ("HTTP 200", ok));

    // One aHerald cannot use is the very case `--raw-output` is for.
    let odd = r#"{"five_hour":null,"seven_day":null,"surprise":{"a":1}}"#;
    assert!(fetch_with(credentials, |_, _, _, _| answer(200, odd), NOW).is_err());
    assert_eq!(last_raw(Agent::Claude).unwrap().2, odd);

    // So is an error page.
    assert!(fetch_with(credentials, |_, _, _, _| answer(401, "denied"), NOW).is_err());
    let (note, _, body) = last_raw(Agent::Claude).unwrap();
    assert_eq!((note.as_str(), body.as_str()), ("HTTP 401", "denied"));

    // Codex has its own slot, untouched here.
    assert!(last_raw(Agent::Codex).is_none());

    // Long answers are cut on a character boundary and say so.
    let long = "é".repeat(RAW_LIMIT);
    let cut = bounded_raw(long.as_bytes());
    assert!(cut.len() <= RAW_LIMIT + "\n…truncated".len());
    assert!(cut.ends_with("\n…truncated") && cut.starts_with('é'));
    assert_eq!(bounded_raw(b"short"), "short");
}
