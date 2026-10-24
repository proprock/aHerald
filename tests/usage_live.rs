#![cfg(windows)]
#[test]
#[ignore = "manual read-only check requiring installed Codex; does not print account data"]
fn installed_codex_handshake_and_usage_read() {
    use aherald::{
        app_server::{Client, REQUEST_TIMEOUT, StdioTransport},
        usage::Snapshot,
    };
    let mut client = Client::initialize(StdioTransport::codex().unwrap(), REQUEST_TIMEOUT).unwrap();
    match client.read_limits() {
        Ok(value) => {
            let snapshot = Snapshot::parse(value).unwrap();
            eprintln!(
                "Official usage read succeeded; buckets={}",
                snapshot.buckets.len()
            );
        }
        Err(error) => eprintln!("Official handshake succeeded; usage state: {error}"),
    }
}
