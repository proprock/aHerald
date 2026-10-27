use aherald::app_server::{Client, Error, Transport};
use serde_json::{Value, json};
use std::{collections::VecDeque, time::Duration};

#[derive(Default)]
struct Mock {
    incoming: VecDeque<Result<Value, Error>>,
    sent: Vec<Value>,
}
impl Transport for Mock {
    fn send(&mut self, value: &Value, _: Duration) -> Result<(), Error> {
        self.sent.push(value.clone());
        Ok(())
    }
    fn receive(&mut self, _: Duration) -> Result<Value, Error> {
        self.incoming.pop_front().unwrap_or(Err(Error::Timeout))
    }
}
fn mock(values: Vec<Value>) -> Mock {
    Mock {
        incoming: values.into_iter().map(Ok).collect(),
        ..Mock::default()
    }
}
#[test]
fn handshake_precedes_reads_and_ids_are_correlated() {
    let mut client = Client::initialize(
        mock(vec![
            json!({"id":1,"result":{"userAgent":"mock"}}),
            json!({"method":"account/rateLimits/updated","params":{"rateLimits":{}}}),
            json!({"id":99,"result":{"wrong":true}}),
            json!({"id":2,"result":{"rateLimits":{}}}),
            json!({"id":3,"result":{"rateLimits":{"primary":null}}}),
        ]),
        Duration::from_secs(1),
    )
    .unwrap();
    assert_eq!(client.read_limits().unwrap(), json!({"rateLimits":{}}));
    assert_eq!(client.take_updates().len(), 1);
    assert_eq!(
        client.read_limits().unwrap(),
        json!({"rateLimits":{"primary":null}})
    );
    let sent = &client.transport().sent;
    assert_eq!(
        sent.iter()
            .map(|v| v["method"].as_str().unwrap())
            .collect::<Vec<_>>(),
        [
            "initialize",
            "initialized",
            "account/rateLimits/read",
            "account/rateLimits/read"
        ]
    );
    assert_eq!(sent[0]["params"]["clientInfo"]["name"], "aherald");
}
#[test]
fn session_name_is_read_without_loading_or_subscribing_to_the_thread() {
    let mut client = Client::initialize(
        mock(vec![
            json!({"id":1,"result":{}}),
            json!({"id":2,"result":{"thread":{"id":"session-a","name":"Fix printer","turns":[]}}}),
            json!({"id":3,"result":{"thread":{"id":"session-b","name":null,"turns":[]}}}),
            json!({"id":4,"result":{"thread":{"id":"wrong-session","name":"Wrong title"}}}),
        ]),
        Duration::from_secs(1),
    )
    .unwrap();
    assert_eq!(
        client.read_thread_name("session-a").unwrap().as_deref(),
        Some("Fix printer")
    );
    assert_eq!(client.read_thread_name("session-b").unwrap(), None);
    assert_eq!(client.read_thread_name("session-a"), Err(Error::Malformed));
    let sent = &client.transport().sent;
    assert_eq!(sent.len(), 5);
    for request in &sent[2..] {
        assert_eq!(request["method"], "thread/read");
        assert_eq!(request["params"]["includeTurns"], false);
    }
}
#[test]
fn timeout_disconnect_malformed_and_server_errors_are_distinct_and_sanitized() {
    for error in [Error::Timeout, Error::Disconnected, Error::Malformed] {
        let transport = Mock {
            incoming: VecDeque::from([Err(error.clone())]),
            ..Mock::default()
        };
        assert!(
            matches!(Client::initialize(transport, Duration::from_millis(10)), Err(e) if e == error)
        );
    }
    let mut client = Client::initialize(
        mock(vec![
            json!({"id":1,"result":{}}),
            json!({"id":2,"error":{"code":-32601,"message":"SECRET"}}),
            json!({"id":3,"error":{"code":-32000,"message":"SECRET"}}),
        ]),
        Duration::from_secs(1),
    )
    .unwrap();
    assert_eq!(client.read_limits(), Err(Error::Unsupported));
    let error = client.read_limits().unwrap_err();
    assert_eq!(error, Error::Remote(-32000));
    assert!(!format!("{error:?}").contains("SECRET"));
}
#[test]
fn unsolicited_requests_never_receive_permission_responses() {
    let mut client = Client::initialize(
        mock(vec![
            json!({"id":1,"result":{}}),
            json!({"id":7,"method":"item/commandExecution/requestApproval","params":{}}),
            json!({"id":2,"result":{}}),
        ]),
        Duration::from_secs(1),
    )
    .unwrap();
    assert_eq!(client.read_limits(), Err(Error::UnexpectedRequest));
    assert_eq!(client.transport().sent.len(), 3);
}

#[test]
fn reconnect_obeys_backoff_and_reinitializes_before_reading() {
    use aherald::app_server::Connection;
    let mut attempts = 0;
    let mut connection = Connection::new(|| {
        attempts += 1;
        if attempts == 1 {
            return Err(Error::Unavailable);
        }
        Ok(mock(vec![
            json!({"id":1,"result":{}}),
            json!({"id":2,"result":{"ok":true}}),
        ]))
    });
    assert_eq!(connection.read(Duration::ZERO), Err(Error::Unavailable));
    assert_eq!(
        connection.read(Duration::from_millis(999)),
        Err(Error::Backoff)
    );
    assert_eq!(
        connection.read(Duration::from_secs(1)).unwrap(),
        json!({"ok":true})
    );
    assert_eq!(
        connection.client.as_ref().unwrap().transport().sent.len(),
        3
    );
}

#[cfg(windows)]
#[test]
fn owned_stdio_process_exchanges_json_bounds_input_and_is_reaped() {
    use aherald::app_server::StdioTransport;
    use std::process::Command;
    use windows::Win32::Foundation::*;
    use windows::Win32::System::Threading::*;
    const STARTUP_BOUND: Duration = Duration::from_secs(30);
    fn start(script: &str) -> StdioTransport {
        let mut command = Command::new("powershell.exe");
        command.args(["-NoProfile", "-NonInteractive", "-Command", script]);
        StdioTransport::spawn(command).unwrap()
    }
    let transport = start(
        r#"while ($line = [Console]::ReadLine()) { $v = $line | ConvertFrom-Json; if ($null -ne $v.id) { [Console]::WriteLine(('{"id":' + $v.id + ',"result":{}}')) } }"#,
    );
    let process = unsafe {
        OpenProcess(
            PROCESS_SYNCHRONIZE | PROCESS_QUERY_LIMITED_INFORMATION,
            false,
            transport.pid(),
        )
        .unwrap()
    };
    // Cold PowerShell start-up on a loaded CI runner can exceed 5 s; the bound only caps a hang.
    let mut client = Client::initialize(transport, STARTUP_BOUND).unwrap();
    assert_eq!(client.read_limits().unwrap(), json!({}));
    drop(client);
    unsafe {
        assert_eq!(WaitForSingleObject(process, 1000), WAIT_OBJECT_0);
        CloseHandle(process).unwrap();
    }
    for script in [
        "[Console]::WriteLine('invalid')",
        "[Console]::WriteLine(('x' * 262145))",
    ] {
        let mut transport = start(script);
        assert_eq!(transport.receive(STARTUP_BOUND), Err(Error::Malformed));
    }
    let mut transport = start("exit");
    assert_eq!(transport.receive(STARTUP_BOUND), Err(Error::Disconnected));
    let mut transport = start("[Console]::ReadLine() | Out-Null");
    assert_eq!(
        transport.receive(Duration::from_millis(20)),
        Err(Error::Timeout)
    );
}
