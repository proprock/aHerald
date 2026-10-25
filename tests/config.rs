use aherald::{cli::Cli, config::Config};
use std::{fs, path::PathBuf, process::Command};

#[test]
fn background_colors_accept_all_three_scopes_and_reject_invalid_hex() {
    let dir = PathBuf::from(".");
    assert!(Config::parse("[notifications]\nbackground_color='#102030'\n[events.error]\nbackground_color='#304050'\n[usage]\nbackground_color='#506070'", &dir).is_ok());
    for scope in ["notifications", "events.error", "usage"] {
        for invalid in ["red", "#12345", "#gg0000", "#11223344"] {
            let text = format!("[{scope}]\nbackground_color='{invalid}'");
            assert!(Config::parse(&text, &dir).is_err(), "{text}");
        }
    }
}

#[test]
fn claude_sections_default_safely_and_validate() {
    let dir = PathBuf::from(".");
    let defaults = Config::parse("", &dir).unwrap();
    assert!(defaults.claude.hooks_enabled);
    assert!(!defaults.claude_usage.enabled, "token use is opt-in");
    assert_eq!(defaults.claude_usage.refresh_interval_sec, 300);
    assert!(
        Config::parse(
            "[claude_usage]\nenabled=true\nrefresh_interval_sec=600",
            &dir
        )
        .is_ok()
    );
    for bad in [
        "[claude_usage]\nrefresh_interval_sec=299",
        "[claude_usage]\nunknown=1",
        "[claude]\nunknown=1",
    ] {
        assert!(Config::parse(bad, &dir).is_err(), "{bad}");
    }
}

#[test]
fn background_colors_inherit_global_unless_overridden() {
    use aherald::events::Kind;
    let defaults = Config::default();
    assert_eq!(defaults.event_background_color(Kind::Error).0, [28, 35, 45]);
    assert_eq!(defaults.usage_background_color().0, [28, 35, 45]);
    let config = Config::parse("[notifications]\nbackground_color='#102030'\n[events.error]\nbackground_color='#304050'\n[usage]\nbackground_color='#506070'", &PathBuf::from(".")).unwrap();
    assert_eq!(config.event_background_color(Kind::Error).0, [48, 64, 80]);
    assert_eq!(
        config.event_background_color(Kind::TurnComplete).0,
        [16, 32, 48]
    );
    assert_eq!(config.usage_background_color().0, [80, 96, 112]);
    let inherited = Config::parse(
        "[notifications]\nbackground_color='#102030'",
        &PathBuf::from("."),
    )
    .unwrap();
    assert_eq!(inherited.usage_background_color().0, [16, 32, 48]);
    assert_eq!(config.event_color(Kind::Error).0, [245, 85, 100]);
}

#[test]
fn visual_selection_validates_scope_and_custom_paths() {
    for text in [
        "[notifications]\nvisual='auto'",
        "[notifications]\nvisual='builtin'",
        "[notifications]\nvisual='pet'",
        "[events.error]\nvisual='inherit'",
        "[events.error]\nvisual='builtin'",
        "[events.error]\nvisual='pet'",
        "[events.error]\nvisual='custom'\nanimation='custom/error'",
    ] {
        assert!(Config::parse(text, &PathBuf::from(".")).is_ok(), "{text}");
    }
    for text in [
        "[notifications]\nvisual='inherit'",
        "[notifications]\nvisual='custom'",
        "[events.error]\nvisual='auto'",
        "[events.error]\nvisual='unknown'",
        "[events.error]\nvisual='custom'",
    ] {
        assert!(Config::parse(text, &PathBuf::from(".")).is_err(), "{text}");
    }
}

#[test]
fn event_accent_colors_and_pet_state_maps_are_validated() {
    let dir = PathBuf::from(".");
    let config = Config::parse("[events.error]\ncolor='#102030'\n[notifications.pet_animations]\nerror='waiting'\n[agents.codex.pet_animations]\nturn_complete='review'", &dir).unwrap();
    assert_eq!(
        config.event_color(aherald::events::Kind::Error).0,
        [16, 32, 48]
    );
    assert_eq!(
        config.event_color(aherald::events::Kind::TurnComplete).0,
        [65, 210, 140]
    );
    for text in [
        "[events.error]\ncolor='red'",
        "[events.error]\ncolor='#12345'",
        "[events.error]\ncolor='#gg0000'",
        "[events.error]\ncolor='#11223344'",
        "[notifications.pet_animations]\nunknown='idle'",
        "[notifications.pet_animations]\nerror='unknown'",
        "[agents.codex.pet_animations]\nunknown='idle'",
    ] {
        assert!(Config::parse(text, &dir).is_err(), "{text}");
    }
}

#[test]
fn defaults_paths_and_mode_override() {
    let dir = PathBuf::from(r"C:\demo");
    let config = Config::parse(
        "[general]\nmode='resident'\n[events.error]\nsound='sound.wav'",
        &dir,
    )
    .unwrap();
    assert_eq!(config.events["error"].sound, Some(dir.join("sound.wav")));
    assert_eq!(config.events["permission_required"].duration_ms, 0);
    assert!(config.events["turn_complete"].enabled);
    assert_eq!(Cli::default().selected_mode(&config), "resident");
    assert_eq!(
        Cli::parse(args(&["--transient"]))
            .unwrap()
            .selected_mode(&config),
        "transient"
    );
    assert_eq!(
        Config::parse("", &dir).unwrap().usage.refresh_interval_sec,
        300
    );
}

#[test]
fn invalid_configuration_is_rejected() {
    for text in [
        "[",
        "[sounds]\nvolume=1.1",
        "[notifications]\nscale=nan",
        "[notifications]\nscale=0.0",
        "[events.error]\nduration_ms=-1",
        "[usage]\nrefresh_interval_sec=0",
        "[general]\nmode='other'",
        "[events.unknown]\nenabled=true",
        "[resident]\nauto_start=true",
        "[sounds]\nvolum=0.5",
    ] {
        assert!(Config::parse(text, &PathBuf::from(".")).is_err(), "{text}");
    }
}

fn args(values: &[&str]) -> Vec<String> {
    values.iter().map(|s| s.to_string()).collect()
}

#[test]
fn explicit_operations_are_unambiguous() {
    for input in [
        vec!["--hook", "--status"],
        vec!["--resident", "--transient"],
        vec!["--event", "error", "--quit"],
        vec!["--event", "unknown"],
        vec!["--config"],
        vec!["--unknown"],
    ] {
        assert!(Cli::parse(args(&input)).is_err(), "{input:?}");
    }
    let cli = Cli::parse(args(&["--hook", "--transient", "--config", "demo.toml"])).unwrap();
    assert_eq!(cli.operation.as_deref(), Some("hook"));
    assert_eq!(cli.mode.as_deref(), Some("transient"));
    assert_eq!(cli.config, Some(PathBuf::from("demo.toml")));
}

#[test]
fn missing_explicit_configuration_is_an_error() {
    let path = std::env::temp_dir().join(format!("aherald-missing-{}.toml", std::process::id()));
    assert!(Config::load(&path, true).is_err());
    assert!(Config::load(&path, false).is_ok());
    assert!(path.is_file());
    fs::remove_file(path).unwrap();
}

#[test]
fn hook_configuration_error_is_silent_and_successful() {
    let path = std::env::temp_dir().join(format!("aherald-invalid-{}.toml", std::process::id()));
    fs::write(&path, "[sounds]\nvolume = 9.0").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_aherald"))
        .args(["--hook", "--config"])
        .arg(&path)
        .output()
        .unwrap();
    fs::remove_file(path).unwrap();
    assert!(output.status.success());
    assert!(output.stdout.is_empty());
    assert!(!output.stderr.is_empty());
}

#[test]
fn partial_wait_configuration_keeps_indefinite_duration() {
    let config = Config::parse(
        "[events.permission_required]\nenabled=false",
        &PathBuf::from("."),
    )
    .unwrap();
    assert_eq!(config.events["permission_required"].duration_ms, 0);
}
