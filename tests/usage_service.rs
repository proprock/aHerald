use aherald::{
    agent::Agent,
    app_server::{Error, Transport},
    usage::Snapshot,
    usage_service::{Driver, UsageSource},
};
use serde_json::{Value, json};
use std::{
    collections::VecDeque,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
struct Mock {
    incoming: VecDeque<Result<Value, Error>>,
}
impl Transport for Mock {
    fn send(&mut self, _: &Value, _: Duration) -> Result<(), Error> {
        Ok(())
    }
    fn receive(&mut self, _: Duration) -> Result<Value, Error> {
        self.incoming.pop_front().unwrap_or(Err(Error::Timeout))
    }
}
#[test]
fn polling_manual_refresh_failure_and_recovery_are_bounded() {
    let attempts = Arc::new(AtomicUsize::new(0));
    let count = attempts.clone();
    let mut driver = Driver::new(Duration::from_secs(300), move || {
        let attempt = count.fetch_add(1, Ordering::SeqCst);
        Ok(Mock {
            incoming: if attempt == 0 {
                VecDeque::from([
                    Ok(json!({"id":1,"result":{}})),
                    Ok(json!({"id":2,"result":{"rateLimits":{"primary":{"usedPercent":25}}}})),
                    Ok(json!({"id":3,"error":{"code":-32000,"message":"SECRET"}})),
                ])
            } else {
                VecDeque::from([
                    Ok(json!({"id":1,"result":{}})),
                    Ok(json!({"id":2,"result":{"rateLimits":{"primary":{"usedPercent":40}}}})),
                ])
            },
        })
    });
    driver.step(Duration::ZERO, false);
    assert!(
        driver
            .view
            .card_text(Duration::ZERO, 0)
            .contains("25% used")
    );
    // The queued error response is deliberately consumed by the next request, not idle polling.
    driver.step(Duration::from_secs(5), true);
    let card = driver.view.card_text(Duration::from_secs(5), 0);
    assert!(!card.contains("Status:"), "{card}");
    assert!(!card.contains("SECRET"), "{card}");
    for t in 6..15 {
        driver.step(Duration::from_secs(t), true);
    }
    assert_eq!(attempts.load(Ordering::SeqCst), 1);
    driver.step(Duration::from_secs(15), true);
    assert_eq!(attempts.load(Ordering::SeqCst), 2);
    let card = driver.view.card_text(Duration::from_secs(15), 0);
    assert!(card.contains("40% used"), "{card}");
    assert!(!card.contains("stale"), "{card}");
    let later = driver.view.card_text(Duration::from_secs(316), 0);
    assert!(!later.contains("Status:"), "{later}");
}
#[test]
fn unavailable_polling_does_not_spin_or_expose_private_errors() {
    let count = Arc::new(AtomicUsize::new(0));
    let calls = count.clone();
    let mut driver = Driver::new(Duration::from_secs(300), move || -> Result<Mock, Error> {
        calls.fetch_add(1, Ordering::SeqCst);
        Err(Error::Unavailable)
    });
    for n in 0..1000 {
        driver.step(Duration::from_millis(n), true);
    }
    assert_eq!(count.load(Ordering::SeqCst), 1);
    assert!(
        driver
            .view
            .card_text(Duration::ZERO, 0)
            .contains("Status: unavailable")
    );
    driver.step(Duration::from_secs(10), false);
    assert_eq!(count.load(Ordering::SeqCst), 2);
    driver.step(Duration::from_secs(29), true);
    assert_eq!(count.load(Ordering::SeqCst), 2);
}
#[test]
fn rolling_updates_are_applied_and_account_changes_clear_previous_usage() {
    let mut driver = Driver::new(Duration::from_secs(300), || {
        Ok(Mock {
            incoming: VecDeque::from([
                Ok(json!({"id":1,"result":{}})),
                Ok(
                    json!({"id":2,"result":{"rateLimits":{"primary":{"usedPercent":25}},"rateLimitsByLimitId":{
                        "codex":{"primary":{"usedPercent":25}},"other":{"primary":{"usedPercent":90}}
                    }}}),
                ),
                Ok(
                    json!({"method":"account/rateLimits/updated","params":{"rateLimits":{"limitId":"codex","primary":{"usedPercent":28}}}}),
                ),
                Ok(json!({"method":"account/updated","params":{"accountId":"private-account"}})),
            ]),
        })
    });
    driver.step(Duration::ZERO, false);
    driver.step(Duration::from_secs(1), false);
    let card = driver.view.card_text(Duration::from_secs(1), 0);
    assert!(card.contains("28% used") && card.contains("90% used") && card.contains("other"));
    driver.step(Duration::from_secs(2), false);
    assert!(driver.view.snapshot.is_none());
    assert!(
        !driver
            .view
            .card_text(Duration::from_secs(2), 0)
            .contains("90%")
    );
}

#[test]
fn periodic_refresh_reuses_one_connection_and_disabled_service_never_connects() {
    struct Echo {
        response: Option<Value>,
        calls: Arc<AtomicUsize>,
    }
    impl Transport for Echo {
        fn send(&mut self, value: &Value, _: Duration) -> Result<(), Error> {
            if value["method"] == "account/rateLimits/read" {
                self.calls.fetch_add(1, Ordering::SeqCst);
            }
            if let Some(id) = value.get("id") {
                self.response =
                    Some(json!({"id":id,"result":{"rateLimits":{"primary":{"usedPercent":0}}}}));
            }
            Ok(())
        }
        fn receive(&mut self, _: Duration) -> Result<Value, Error> {
            self.response.take().ok_or(Error::Timeout)
        }
    }
    let reads = Arc::new(AtomicUsize::new(0));
    let starts = Arc::new(AtomicUsize::new(0));
    let calls = reads.clone();
    let connections = starts.clone();
    let mut driver = Driver::new(Duration::from_secs(300), move || {
        connections.fetch_add(1, Ordering::SeqCst);
        Ok(Echo {
            response: None,
            calls: calls.clone(),
        })
    });
    driver.step(Duration::ZERO, false);
    driver.step(Duration::from_secs(299), false);
    assert_eq!(reads.load(Ordering::SeqCst), 1);
    driver.step(Duration::from_secs(300), false);
    assert_eq!(reads.load(Ordering::SeqCst), 2);
    assert_eq!(starts.load(Ordering::SeqCst), 1);
    let service = aherald::usage_service::Service::spawn(
        false,
        Duration::from_secs(300),
        Arc::new(std::sync::atomic::AtomicBool::new(false)),
        || -> Result<Mock, Error> { panic!("disabled usage must not launch a child") },
    );
    service.refresh();
    assert!(service.card_text().contains("Status: disabled"));
    assert_eq!(
        aherald::config::Config::default()
            .codex
            .usage
            .refresh_interval_sec,
        300
    );
}

#[test]
fn idle_worker_sleeps_until_its_next_deadline() {
    let mut failing = Driver::new(Duration::from_secs(300), || {
        Err::<Mock, _>(Error::Unavailable)
    });
    failing.step(Duration::ZERO, false);
    assert_eq!(failing.wait(Duration::ZERO), Duration::from_secs(10));
    assert_eq!(failing.wait(Duration::from_secs(4)), Duration::from_secs(6));
    failing.step(Duration::from_secs(10), false);
    assert_eq!(
        failing.wait(Duration::from_secs(10)),
        Duration::from_secs(20),
        "backoff doubles"
    );
    // Never zero, so an overdue deadline cannot spin.
    assert!(failing.wait(Duration::from_secs(3600)) >= Duration::from_millis(1));
}

#[test]
fn live_connection_is_polled_for_updates_without_a_busy_loop() {
    let mut driver = Driver::new(Duration::from_secs(300), || {
        Ok(Mock {
            incoming: VecDeque::from([
                Ok(json!({"id":1,"result":{}})),
                Ok(json!({"id":2,"result":{"rateLimits":{"primary":{"usedPercent":25}}}})),
            ]),
        })
    });
    driver.step(Duration::ZERO, false);
    let wait = driver.wait(Duration::ZERO);
    assert!(wait >= Duration::from_millis(100) && wait <= Duration::from_secs(1));
}

/// A non-Codex source: no JSON-RPC, it returns a typed result per request.
struct Scripted {
    calls: Arc<AtomicUsize>,
    error: Error,
}
impl UsageSource for Scripted {
    fn read(&mut self, _now: Duration) -> Result<Snapshot, Error> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Err(self.error.clone())
    }
}
fn scripted(error: Error) -> (Driver<Scripted>, Arc<AtomicUsize>) {
    let calls = Arc::new(AtomicUsize::new(0));
    let source = Scripted {
        calls: calls.clone(),
        error,
    };
    (
        Driver::with_source(Agent::Claude, Duration::from_secs(300), source),
        calls,
    )
}
#[test]
fn rate_limited_source_waits_for_the_refresh_interval_even_for_manual_refresh() {
    let (mut driver, calls) = scripted(Error::RateLimited("Claude usage rate limited", None));
    driver.step(Duration::from_secs(1), false);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    for (second, manual) in [(11, false), (100, true), (299, false), (300, true)] {
        driver.step(Duration::from_secs(second), manual);
        assert_eq!(
            calls.load(Ordering::SeqCst),
            1,
            "early request at {second}s"
        );
    }
    assert_eq!(
        driver.view.error.as_deref(),
        Some("Claude usage rate limited")
    );
    driver.step(Duration::from_secs(301), false);
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    assert!(driver.wait(Duration::from_secs(301)) >= Duration::from_secs(299));
}
#[test]
fn retry_after_longer_than_the_interval_delays_every_request_until_it_elapses() {
    let (mut driver, calls) = scripted(Error::RateLimited(
        "Claude usage rate limited",
        Some(Duration::from_secs(600)),
    ));
    driver.step(Duration::from_secs(1), false);
    for (second, manual) in [(301, false), (400, true), (600, true), (600, false)] {
        driver.step(Duration::from_secs(second), manual);
        assert_eq!(
            calls.load(Ordering::SeqCst),
            1,
            "early request at {second}s"
        );
    }
    driver.step(Duration::from_secs(602), false);
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}
#[test]
fn retry_after_is_capped_at_fifteen_minutes() {
    let (mut driver, calls) = scripted(Error::RateLimited(
        "Claude usage rate limited",
        Some(Duration::from_secs(86_400)),
    ));
    driver.step(Duration::from_secs(1), false);
    driver.step(Duration::from_secs(900), true);
    assert_eq!(calls.load(Ordering::SeqCst), 1, "request before the cap");
    driver.step(Duration::from_secs(902), false);
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}
#[test]
fn a_short_retry_after_never_undercuts_the_refresh_interval() {
    let (mut driver, calls) = scripted(Error::RateLimited(
        "Claude usage rate limited",
        Some(Duration::from_secs(30)),
    ));
    driver.step(Duration::from_secs(1), false);
    driver.step(Duration::from_secs(60), true);
    driver.step(Duration::from_secs(299), false);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    driver.step(Duration::from_secs(302), false);
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}
#[test]
fn a_long_refresh_interval_still_wins_over_the_cap() {
    let calls = Arc::new(AtomicUsize::new(0));
    let source = Scripted {
        calls: calls.clone(),
        error: Error::RateLimited("Claude usage rate limited", None),
    };
    let mut driver = Driver::with_source(Agent::Claude, Duration::from_secs(3600), source);
    driver.step(Duration::from_secs(1), false);
    driver.step(Duration::from_secs(1800), true);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    driver.step(Duration::from_secs(3602), false);
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}
#[test]
fn ordinary_source_failures_keep_the_short_backoff() {
    let (mut driver, calls) = scripted(Error::Service("Claude usage service error"));
    driver.step(Duration::from_secs(1), false);
    driver.step(Duration::from_secs(5), false);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    driver.step(Duration::from_secs(11), false);
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}
#[test]
fn claude_view_uses_the_simple_card_selected_by_agent_not_label() {
    let (mut driver, _) = scripted(Error::Service("x"));
    driver.step(Duration::from_secs(1), false);
    let text = driver.view.card_text(Duration::from_secs(1), 0);
    assert!(text.starts_with("Claude usage"), "{text}");
}
