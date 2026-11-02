mod support;
use aherald::{
    agent::Agent,
    app_server::Error,
    config::LogLevel,
    operational_log,
    usage::Snapshot,
    usage_service::{Driver, UsageSource},
};
use std::{collections::VecDeque, time::Duration};

struct Source(VecDeque<Result<Snapshot, Error>>);
impl UsageSource for Source {
    fn read(&mut self, _: Duration) -> Result<Snapshot, Error> {
        self.0.pop_front().unwrap()
    }
}

#[test]
fn usage_failures_are_logged_once_per_episode_without_private_diagnostics() {
    let directory = support::TempDir::new("usage-failure-log");
    let log = directory.join("aherald.log");
    let _ = std::fs::remove_file(&log);
    operational_log::init(&directory, LogLevel::Warn);
    for agent in [Agent::Codex, Agent::Claude] {
        let source = Source(VecDeque::from([
            Err(Error::Service("PRIVATE_TOKEN_PATH")),
            Err(Error::Timeout),
            Ok(Snapshot::default()),
            Err(Error::Service("PRIVATE_PAYLOAD")),
        ]));
        let mut driver = Driver::with_source(agent, Duration::from_secs(300), source);
        for second in [0, 10, 30, 330] {
            assert!(driver.step(Duration::from_secs(second), false));
        }
    }
    let text = std::fs::read_to_string(&log).unwrap_or_default();
    assert_eq!(text.matches("usage.failed").count(), 4, "{text}");
    assert!(!text.contains("PRIVATE"));
    std::fs::remove_file(log).unwrap();
}
