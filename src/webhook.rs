//! The generic sink: POSTs a body template to a URL, which serves ntfy, Slack, Telegram and any
//! other service that takes an HTTP request.
//!
//! Its placeholders (`${NAME}` once at build time, `{text}` and friends at each send) are in
//! `crate::template`.
use crate::{
    http_common::Zero,
    https::{self, HttpError, Wiped},
    sink::{Message, SendError, Sink},
    template::{Escape, render},
};
use std::time::Duration;

const TIMEOUT: Duration = Duration::from_secs(10);

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

    fn send(&mut self, message: &Message) -> Result<(), SendError> {
        let mode = Escape::for_content_type(&self.content_type);
        let body = Wiped(render(&self.body.0, message, mode));
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
            // Too many requests or unavailable: the server may say when to come back.
            Ok(response) if matches!(response.status, 429 | 503) => Err(SendError {
                message: format!("the server answered HTTP {}", response.status),
                retry_after: response.retry_after,
            }),
            Ok(response) => Err(format!("the server answered HTTP {}", response.status).into()),
            Err(HttpError::Network) => Err("cannot reach the server".into()),
            Err(HttpError::TooLarge) => Err("the server reply was too large".into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
