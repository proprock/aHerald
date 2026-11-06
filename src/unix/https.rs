//! Minimal bounded HTTPS GET over `ureq` with rustls (no system TLS library). It needs the
//! `https-client` feature (on by default); without it every request is a network error.
//!
//! Redirects are never followed, so a bearer token reaches only the requested host. Proxy
//! settings come from the standard environment variables (`HTTPS_PROXY`, `ALL_PROXY`).
use std::time::Duration;

pub use crate::http_common::{HttpError, MAX_BODY, Response, Wiped, Zero};

pub use crate::http_common::{get, get_at, post, post_at};

#[cfg(not(feature = "https-client"))]
pub(crate) fn request_at(
    _body: Option<&[u8]>,
    _secure: bool,
    _host: &str,
    _port: u16,
    _path: &str,
    _headers: &str,
    _timeout: Duration,
) -> Result<Response, HttpError> {
    Err(HttpError::Network)
}

/// A GET when `body` is `None`, otherwise a POST of that body.
#[cfg(feature = "https-client")]
pub(crate) fn request_at(
    body: Option<&[u8]>,
    secure: bool,
    host: &str,
    port: u16,
    path: &str,
    headers: &str,
    timeout: Duration,
) -> Result<Response, HttpError> {
    use std::io::Read;
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .max_redirects(0)
        // 3xx and 4xx are results the caller interprets, not transport failures.
        .http_status_as_error(false)
        .timeout_global(Some(timeout))
        .user_agent("aHerald")
        .build()
        .into();
    let scheme = if secure { "https" } else { "http" };
    let url = format!("{scheme}://{host}:{port}{path}");
    // The lines may carry a credential, so the zeroing wrapper owns them while they are parsed.
    let headers = Wiped(headers.to_owned());
    let mut lines = Vec::new();
    for line in headers.0.split("\r\n").filter(|line| !line.is_empty()) {
        let (name, value) = line.split_once(':').ok_or(HttpError::Network)?;
        lines.push((name.trim(), value.trim()));
    }
    let sent = match body {
        None => {
            let mut request = agent.get(url);
            for (name, value) in &lines {
                request = request.header(*name, *value);
            }
            request.call()
        }
        Some(payload) => {
            let mut request = agent.post(url);
            for (name, value) in &lines {
                request = request.header(*name, *value);
            }
            request.send(payload)
        }
    };
    let mut response = sent.map_err(|_| HttpError::Network)?;
    let retry_after = response
        .headers()
        .get("retry-after")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.trim().parse::<u32>().ok())
        .map(|seconds| Duration::from_secs(u64::from(seconds)));
    let status = response.status().as_u16();
    let mut reply = Vec::new();
    // One byte past the limit tells an exact-size body from an oversized one.
    response
        .body_mut()
        .as_reader()
        .take(MAX_BODY as u64 + 1)
        .read_to_end(&mut reply)
        .map_err(|_| HttpError::Network)?;
    if reply.len() > MAX_BODY {
        return Err(HttpError::TooLarge);
    }
    Ok(Response {
        status,
        body: reply,
        retry_after,
    })
}
