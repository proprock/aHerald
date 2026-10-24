use aherald::{cli::Cli, config::Config};
use std::{fs, path::PathBuf, process::Command};

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
