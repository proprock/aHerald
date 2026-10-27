use aherald::{
    activation::{Agent, click},
    card::Card,
    config::Config,
    events::{Event, Source},
};
#[derive(Default)]
struct Fake {
    calls: Vec<String>,
    fail_open: bool,
    fail_window: bool,
}
impl Agent for Fake {
    fn open_thread(&mut self, url: &str) -> Result<(), String> {
        self.calls.push(url.into());
        if self.fail_open {
            Err("missing protocol".into())
        } else {
            Ok(())
        }
    }
    fn activate_window(&mut self) -> Result<(), String> {
        self.calls.push("window".into());
        if self.fail_window {
            Err("no window".into())
        } else {
            Ok(())
        }
    }
}
fn event(id: Option<&str>) -> Event {
    let mut e = Event::demo("permission_required").unwrap();
    e.source = Source::CodexHook;
    e.session_id = id.map(str::to_owned);
    e
}
#[test]
fn click_policy_encodes_one_component_and_falls_back_without_commands() {
    let mut a = Fake::default();
    click(&event(Some("a/b ?#%é")), true, true, &mut a).unwrap();
    assert_eq!(a.calls, ["codex://threads/a%2Fb%20%3F%23%25%C3%A9"]);
    a.calls.clear();
    a.fail_open = true;
    click(&event(Some("thread")), true, true, &mut a).unwrap();
    assert_eq!(a.calls, ["codex://threads/thread", "window"]);
    for id in [None, Some(""), Some("bad\nidentity")] {
        a.calls.clear();
        click(&event(id), true, true, &mut a).unwrap();
        assert_eq!(a.calls, ["window"]);
    }
    a.fail_window = true;
    assert!(click(&event(None), true, true, &mut a).is_err());
}
#[test]
fn option_demo_usage_and_controls_never_activate() {
    assert!(Config::default().notifications.activate_agent_on_click);
    assert!(
        !Config::parse(
            "[notifications]\nactivate_agent_on_click=false",
            Path::new(".")
        )
        .unwrap()
        .notifications
        .activate_agent_on_click
    );
    let mut a = Fake::default();
    for source in [Source::Demo, Source::Usage] {
        let mut e = event(Some("thread"));
        e.source = source;
        click(&e, true, true, &mut a).unwrap();
    }
    click(&event(Some("thread")), false, true, &mut a).unwrap();
    click(&event(Some("thread")), true, false, &mut a).unwrap();
    assert!(a.calls.is_empty());
}
use std::path::Path;
#[test]
fn body_activation_and_dismissal_are_independent_at_both_scales() {
    for dismiss in [true, false] {
        for scale in [1.0, 2.0] {
            let c = Card::new("Codex", 0, 2, dismiss);
            let (w, h) = ((320.0 * scale) as i32, (110.0 * scale) as i32);
            assert!(c.body_hit_test(w, h, scale, 10, 10));
            assert_eq!(c.hit_test(w, h, scale, 10, 10).is_some(), dismiss);
            for (_, r) in c.buttons(w, h, scale) {
                assert!(!c.body_hit_test(
                    w,
                    h,
                    scale,
                    (r.left + r.right) / 2,
                    (r.top + r.bottom) / 2
                ));
            }
            assert!(!c.body_hit_test(w, h, scale, -1, 10));
        }
    }
}

#[cfg(windows)]
#[test]
#[ignore = "interactive acceptance against installed Codex; requires AHERALD_TEST_THREAD"]
fn manual_agent_click_preview() {
    let id = std::env::var("AHERALD_TEST_THREAD").expect("set a real test thread ID");
    let mut c = Config::default();
    c.sounds.enabled = false;
    c.notifications.visual = aherald::config::Visual::Builtin;
    c.events.get_mut("permission_required").unwrap().duration_ms = 180_000;
    c.diagnostic_directory = std::env::temp_dir();
    aherald::notification::show(&event(Some(&id)), &c).unwrap();
}
