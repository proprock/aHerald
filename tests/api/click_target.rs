use aherald::events::{Event, Source};

#[cfg(windows)]
#[test]
fn ancestor_walk_follows_parents_and_survives_cycles_and_gaps() {
    use aherald::console_window::ancestors;
    let table = [(10, 9), (9, 8), (8, 7), (7, 1)];
    assert_eq!(ancestors(&table, 10), [9, 8, 7, 1]);
    assert_eq!(ancestors(&table, 99), Vec::<u32>::new());
    // pid reuse can create loops; the walk must stop.
    let looped = [(3, 2), (2, 3)];
    assert_eq!(ancestors(&looped, 3), [2, 3]);
    assert_eq!(ancestors(&[(5, 0)], 5), Vec::<u32>::new());
    let long: Vec<(u32, u32)> = (1..100).map(|p| (p, p + 1)).collect();
    assert!(ancestors(&long, 1).len() <= 16);
}

fn event() -> Event {
    let mut e = Event::demo("turn_complete").unwrap();
    e.source = Source::ClaudeHook;
    e.session_id = Some("s".into());
    e
}

#[test]
fn window_round_trips_and_is_omitted_when_absent() {
    let mut e = event();
    assert!(!serde_json::to_string(&e).unwrap().contains("window"));
    e.window = Some(0x1234);
    let json = serde_json::to_string(&e).unwrap();
    let back: Event = serde_json::from_str(&json).unwrap();
    assert_eq!(back.window, Some(0x1234));
    let old: Event = serde_json::from_str(
        r#"{"kind":"turn_complete","source":"claude_hook","session_id":"s","turn_id":null,"request_id":null}"#,
    )
    .unwrap();
    assert_eq!(old.window, None);
}

#[test]
fn a_zero_window_handle_is_invalid() {
    let mut e = event();
    e.window = Some(0);
    assert!(e.validate().is_err());
    e.window = Some(1);
    assert!(e.validate().is_ok());
}
