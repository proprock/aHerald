//! Without the `https-client` feature the Unix client is a stand-in that reports a network
//! error; it exists so the crate checks where `ring` cannot be built.
#![cfg(all(unix, not(feature = "https-client")))]
use aherald::https::{HttpError, get};
use std::time::Duration;

#[test]
fn every_request_is_a_network_error() {
    assert!(matches!(
        get("example.invalid", "/", "", Duration::from_secs(1)),
        Err(HttpError::Network)
    ));
}
