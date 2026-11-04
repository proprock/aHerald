//! The Telegram Bot API as a sink: `sendMessage` with a chat id and plain text.
//!
//! The bot token travels in the request path, as the API requires, and nowhere else: it is held in
//! zeroing memory and no error text, log line or card ever contains it.
use crate::{
    https::{self, HttpError, Wiped},
    sink::{Message, Sink},
};
use std::time::Duration;

const HOST: &str = "api.telegram.org";
const TIMEOUT: Duration = Duration::from_secs(10);

pub struct Telegram {
    token: Wiped<String>,
    chat_id: String,
    secure: bool,
    host: String,
    port: u16,
}

impl Telegram {
    pub fn new(token: String, chat_id: String) -> Self {
        Self {
            token: Wiped(token),
            chat_id,
            secure: true,
            host: HOST.to_owned(),
            port: 443,
        }
    }

    /// Points the sink at another endpoint; plain HTTP exists only for loopback tests.
    pub fn at(mut self, secure: bool, host: &str, port: u16) -> Self {
        self.secure = secure;
        self.host = host.to_owned();
        self.port = port;
        self
    }
}

impl Sink for Telegram {
    fn name(&self) -> &str {
        "telegram"
    }

    fn send(&mut self, message: &Message) -> Result<(), String> {
        // No parse mode, so the text needs no escaping.
        let body = serde_json::json!({"chat_id": self.chat_id, "text": message.text}).to_string();
        let path = Wiped(format!("/bot{}/sendMessage", self.token.0));
        let response = https::post_at(
            self.secure,
            &self.host,
            self.port,
            &path.0,
            "Content-Type: application/json\r\n",
            body.as_bytes(),
            TIMEOUT,
        );
        match response {
            Ok(response) if response.status == 200 => Ok(()),
            Ok(response) => Err(format!("the Bot API answered HTTP {}", response.status)),
            Err(HttpError::Network) => Err("cannot reach the Bot API".to_owned()),
            Err(HttpError::TooLarge) => Err("the Bot API reply was too large".to_owned()),
        }
    }
}
