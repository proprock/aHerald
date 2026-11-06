use aherald::{agent::Agent, events::folder_name};

#[test]
fn folder_is_the_last_path_component_with_either_separator() {
    assert_eq!(
        folder_name(r"C:\Users\tonym\Repos\aHerald").as_deref(),
        Some("aHerald")
    );
    assert_eq!(folder_name("/home/tony/work/api").as_deref(), Some("api"));
    assert_eq!(
        folder_name(r"C:\Users\tonym\Repos\aHerald\\").as_deref(),
        Some("aHerald")
    );
    assert_eq!(folder_name("/home/tony/work/api/").as_deref(), Some("api"));
    assert_eq!(
        folder_name(r"C:\mixed/separators\here").as_deref(),
        Some("here")
    );
    assert_eq!(folder_name("project").as_deref(), Some("project"));
}

#[test]
fn roots_and_empty_paths_have_no_folder() {
    for cwd in [
        "",
        " ",
        "/",
        r"\",
        r"C:\",
        "C:",
        r"\\",
        r"\\server\share",
        ".",
    ] {
        assert_eq!(folder_name(cwd), None, "{cwd:?}");
    }
}

#[test]
fn folder_text_is_safe_to_draw() {
    // Control and bidirectional override characters are dropped, spaces collapse.
    assert_eq!(
        folder_name("/w/my\u{202e}evil\tname\n dir").as_deref(),
        Some("myevil name dir")
    );
    assert_eq!(folder_name("/w/\u{7}\u{8}"), None);
    let long = format!("/w/{}", "x".repeat(200));
    assert_eq!(folder_name(&long).unwrap().chars().count(), 40);
    assert_eq!(
        folder_name("/w/日本語のプロジェクト").as_deref(),
        Some("日本語のプロジェクト")
    );
}

#[test]
fn codex_and_claude_hooks_carry_only_the_folder_name() {
    let codex = br#"{"hook_event_name":"Stop","session_id":"s","turn_id":"t","cwd":"C:\\Users\\tonym\\Repos\\aHerald"}"#;
    let event = Agent::Codex.parse_hook(codex).unwrap().unwrap();
    assert_eq!(event.folder.as_deref(), Some("aHerald"));
    let claude = br#"{"hook_event_name":"Stop","session_id":"s","cwd":"/home/tony/work/api"}"#;
    let event = Agent::Claude.parse_hook(claude).unwrap().unwrap();
    assert_eq!(event.folder.as_deref(), Some("api"));
    // The full path never reaches the event, so it cannot reach IPC or logs.
    assert!(!serde_json::to_string(&event).unwrap().contains("home"));
}

#[test]
fn a_hook_without_cwd_still_produces_an_event() {
    let claude = br#"{"hook_event_name":"Stop","session_id":"s"}"#;
    let event = Agent::Claude.parse_hook(claude).unwrap().unwrap();
    assert_eq!(event.folder, None);
    let codex = br#"{"hook_event_name":"Stop","session_id":"s","cwd":42}"#;
    // A malformed cwd costs only the folder, never the notification.
    let event = Agent::Codex.parse_hook(codex).unwrap().unwrap();
    assert_eq!(event.folder, None);
}

#[test]
fn an_old_serialized_event_without_folder_still_loads() {
    let old = r#"{"kind":"turn_complete","source":"claude_hook","session_id":"s","turn_id":null,"request_id":null}"#;
    let event: aherald::events::Event = serde_json::from_str(old).unwrap();
    assert_eq!(event.folder, None);
}
