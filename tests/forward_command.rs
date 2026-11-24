//! The command sink from configuration to a real program: what it receives, that no shell reads
//! the values, and how a failure is reported. Unix only: the recording program is `sh`.
#![cfg(unix)]
use aherald::{
    config::Config,
    events::{Event, Kind, Source},
    forward::build,
    sink::{Dispatcher, Policy},
};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

fn event(folder: Option<&str>) -> Event {
    let mut event = Event::notice("ignored");
    event.kind = Kind::InputRequired;
    event.source = Source::ClaudeHook;
    event.folder = folder.map(str::to_owned);
    event
}

fn dispatcher(
    config: &str,
    vars: &'static [(&str, &str)],
) -> (Dispatcher, Arc<Mutex<Vec<String>>>) {
    let forward = Config::parse(config, &PathBuf::from(".")).unwrap().forward;
    let built = build(&forward, |name| {
        vars.iter()
            .find(|(key, _)| *key == name)
            .map(|(_, value)| (*value).to_owned())
    });
    assert!(built.notices.is_empty(), "{:?}", built.notices);
    let posted = Arc::new(Mutex::new(Vec::new()));
    let record = posted.clone();
    let dispatcher = Dispatcher::start(
        built.routes,
        Policy {
            queue: 4,
            attempts: 1,
            backoff: Duration::ZERO,
        },
        true,
        move |text| record.lock().unwrap().push(text.to_owned()),
    );
    (dispatcher, posted)
}

fn wait_for<T>(mut ready: impl FnMut() -> Option<T>) -> T {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(value) = ready() {
            return value;
        }
        assert!(Instant::now() < deadline, "timed out");
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "aherald-forward-command-{}-{name}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn forwarding_distinguishes_named_sessions_in_one_folder_and_falls_back_when_unnamed() {
    let dir = scratch("session-name");
    let out = dir.join("out");
    let config = format!(
        r#"
[[forward.sinks]]
type = "command"
kinds = ["input_required"]
include_folder = true
include_session_name = true
command = ["sh", "-c", "cat >> \"$1\"", "sh", "{}"]
stdin = "{{text}}\nname=[{{session_name}}]\n"
"#,
        out.display()
    );
    let (dispatcher, posted) = dispatcher(&config, &[]);
    for (id, name) in [
        ("forward-command-alpha", Some("First session")),
        ("forward-command-beta", Some("Second session")),
        ("forward-command-unnamed", None),
    ] {
        if let Some(name) = name {
            let payload = serde_json::json!({"session_id": id, "session_name": name});
            let report = aherald::statusline::parse(payload.to_string().as_bytes()).unwrap();
            aherald::statusline::with_store(|store| store.record(report, 0));
        }
        let mut event = event(Some("project"));
        event.session_id = Some(id.into());
        dispatcher.submit_event(&event);
    }
    let delivered = wait_for(|| {
        let contents = std::fs::read_to_string(&out).ok()?;
        contents.ends_with("name=[]\n").then_some(contents)
    });
    assert_eq!(
        delivered,
        "Claude (project): First session\ninput required\nname=[First session]\n\
         Claude (project): Second session\ninput required\nname=[Second session]\n\
         project\n\nClaude: input required\nname=[]\n"
    );
    assert!(posted.lock().unwrap().is_empty());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_card_reaches_the_program_as_arguments_and_standard_input_without_a_shell() {
    let dir = scratch("deliver");
    let out = dir.join("out");
    let marker = dir.join("marker");
    let folder = format!("proj; touch {}", marker.display());
    let config = format!(
        "[[forward.sinks]]\ntype = \"command\"\nkinds = [\"input_required\"]\ninclude_folder = true\n\
         command = [\"sh\", \"-c\", \"printf '%s\\\\n' \\\"$1\\\" > \\\"$2\\\"; cat >> \\\"$2\\\"\", \"sh\", \"{{kind}}\", \"{}\"]\n\
         stdin = \"To: me\\n\\n{{text}}\\n\"\n",
        out.display()
    );
    let (dispatcher, posted) = dispatcher(&config, &[]);
    dispatcher.submit_event(&event(Some(&folder)));
    wait_for(|| {
        std::fs::read_to_string(&out)
            .ok()
            .filter(|text| text.contains("To: me"))
    });
    assert_eq!(
        std::fs::read_to_string(&out).unwrap(),
        format!("input_required\nTo: me\n\n{folder}\n\nClaude: input required\n")
    );
    assert!(!marker.exists(), "a shell parsed the folder name");
    assert!(posted.lock().unwrap().is_empty());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_failing_command_is_one_card_with_its_exit_status_and_never_the_secret() {
    let config = "[[forward.sinks]]\ntype = \"command\"\nkinds = [\"input_required\"]\n\
                  command = [\"sh\", \"-c\", \"echo $1 >&2; exit 3\", \"sh\", \"${PASS}\"]\n";
    let (dispatcher, posted) = dispatcher(config, &[("PASS", "hunter2-SECRET")]);
    dispatcher.submit_event(&event(None));
    dispatcher.submit_event(&event(None));
    wait_for(|| (!posted.lock().unwrap().is_empty()).then_some(()));
    std::thread::sleep(Duration::from_millis(200));
    let posted = posted.lock().unwrap();
    assert_eq!(posted.len(), 1, "{posted:?}");
    assert!(posted[0].contains("exit status 3"), "{posted:?}");
    assert!(!posted[0].contains("SECRET"), "{posted:?}");
}
