mod support;
use aherald::{
    cli::Cli,
    config::{Config, Interpolation, LogLevel, Mode, Monitor, Position, TrayStyle},
    events::Kind,
};
use rstest::rstest;
use std::{fs, path::PathBuf, process::Command};

#[test]
fn focused_output_options_default_on_and_parse_independently() {
    let dir = PathBuf::from(".");
    let defaults = Config::parse("", &dir).unwrap();
    assert!(defaults.notifications.suppress_when_focused);
    assert!(defaults.sounds.suppress_when_focused);
    // Parsing the keys verifies that old configurations can opt out of either output.
    for visual in [false, true] {
        for sound in [false, true] {
            let text = format!(
                "[notifications]\nsuppress_when_focused={visual}\n[sounds]\nsuppress_when_focused={sound}"
            );
            let config = Config::parse(&text, &dir).unwrap();
            assert_eq!(config.notifications.suppress_when_focused, visual);
            assert_eq!(config.sounds.suppress_when_focused, sound);
        }
    }
}

#[rstest]
#[case::zero(0, 0)]
#[case::one_second(1, 1000)]
#[case::twelve_seconds(12, 12_000)]
#[case::one_day_maximum(86_400, 86_400_000)]
fn event_duration_seconds_are_converted_to_internal_milliseconds(
    #[case] seconds: u64,
    #[case] milliseconds: u64,
) {
    let dir = PathBuf::from(".");
    let text = format!("[events.turn_complete]\nduration_sec={seconds}");
    let config = Config::parse(&text, &dir).unwrap();
    assert_eq!(config.events[Kind::TurnComplete].duration_ms, milliseconds);
}

#[test]
fn event_duration_outside_the_supported_form_is_rejected() {
    let dir = PathBuf::from(".");
    assert!(Config::parse("[events.turn_complete]\nduration_sec=86401", &dir).is_err());
    assert!(Config::parse("[events.turn_complete]\nduration_ms=3000", &dir).is_err());
}

#[test]
fn enumerated_settings_parse_every_documented_value() {
    let dir = PathBuf::from(".");
    for (name, mode) in [("resident", Mode::Resident), ("transient", Mode::Transient)] {
        let text = format!("[general]\nmode='{name}'");
        assert_eq!(Config::parse(&text, &dir).unwrap().general.mode, mode);
    }
    for (name, level) in [
        ("off", LogLevel::Off),
        ("error", LogLevel::Error),
        ("warn", LogLevel::Warn),
        ("info", LogLevel::Info),
        ("debug", LogLevel::Debug),
        ("trace", LogLevel::Trace),
    ] {
        let text = format!("[general]\nlogging='{name}'");
        assert_eq!(Config::parse(&text, &dir).unwrap().general.logging, level);
    }
    for (name, position) in [
        ("center", Position::Center),
        ("top_center", Position::TopCenter),
        ("bottom_right", Position::BottomRight),
        ("near_cursor", Position::NearCursor),
    ] {
        let text = format!("[notifications]\nposition='{name}'");
        let parsed = Config::parse(&text, &dir).unwrap();
        assert_eq!(parsed.notifications.position, position);
    }
    for (name, monitor) in [("primary", Monitor::Primary), ("2", Monitor::Index(2))] {
        let text = format!("[notifications]\nmonitor='{name}'");
        let parsed = Config::parse(&text, &dir).unwrap();
        assert_eq!(parsed.notifications.monitor, monitor);
    }
    for (name, mode) in [
        ("nearest", Interpolation::Nearest),
        ("bilinear", Interpolation::Bilinear),
    ] {
        let text = format!("[notifications]\ninterpolation='{name}'");
        let parsed = Config::parse(&text, &dir).unwrap();
        assert_eq!(parsed.notifications.interpolation, mode);
    }
}

#[test]
fn enumerated_settings_reject_unknown_values_without_echoing_them() {
    let dir = PathBuf::from(".");
    for (text, field) in [
        ("[general]\nmode='SECRET'", "general.mode"),
        ("[general]\nlogging='SECRET'", "general.logging"),
        (
            "[notifications]\nposition='SECRET'",
            "notifications.position",
        ),
        ("[notifications]\nmonitor='SECRET'", "notifications.monitor"),
        ("[notifications]\nmonitor='-1'", "notifications.monitor"),
        (
            "[notifications]\ninterpolation='SECRET'",
            "notifications.interpolation",
        ),
        ("[general]\nmode=1", "general.mode"),
    ] {
        let error = Config::parse(text, &dir).unwrap_err();
        assert!(error.contains(field), "{text}: {error}");
        assert!(!error.contains("SECRET"), "{text} echoed a value: {error}");
    }
}

#[test]
fn tray_graph_defaults_and_validates_modes() {
    use aherald::config::TrayGraph;
    let dir = PathBuf::from(".");
    for text in ["", "[tray]"] {
        assert_eq!(
            Config::parse(text, &dir).unwrap().tray.graph,
            TrayGraph::Auto
        );
    }
    for (name, mode) in [
        ("auto", TrayGraph::Auto),
        ("both", TrayGraph::Both),
        ("claude", TrayGraph::Claude),
        ("codex", TrayGraph::Codex),
    ] {
        let text = format!("[tray]\ngraph='{name}'");
        assert_eq!(Config::parse(&text, &dir).unwrap().tray.graph, mode);
    }
    for text in [
        "[tray]\ngraph='unknown'",
        "[tray]\ngraph=1",
        "[tray]\nunknown=true",
    ] {
        assert!(Config::parse(text, &dir).is_err(), "{text}");
    }
}

#[test]
fn background_colors_accept_all_three_scopes_and_reject_invalid_hex() {
    let dir = PathBuf::from(".");
    assert!(Config::parse("[notifications]\nbackground_color='#102030'\n[events.error]\nbackground_color='#304050'\n[codex.usage]\nbackground_color='#506070'", &dir).is_ok());
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
    assert!(defaults.claude.suppress_while_busy);
    assert!(!defaults.claude.usage.enabled, "token use is opt-in");
    assert_eq!(defaults.claude.usage.refresh_interval_sec, 300);
    assert!(
        Config::parse(
            "[claude.usage]\nenabled=true\nrefresh_interval_sec=600",
            &dir
        )
        .is_ok()
    );
    for bad in [
        "[claude.usage]\nrefresh_interval_sec=299",
        "[claude.usage]\nunknown=1",
        "[claude]\nunknown=1",
        // Former top-level names are rejected, not migrated.
        "[claude_usage]\nenabled=true",
        "[usage]\nenabled=true",
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
    let config = Config::parse("[notifications]\nbackground_color='#102030'\n[events.error]\nbackground_color='#304050'\n[codex.usage]\nbackground_color='#506070'", &PathBuf::from(".")).unwrap();
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
    let config = Config::parse("[events.error]\ncolor='#102030'\n[pet_animations]\nerror='waiting'\n[pet_animations.codex]\nturn_complete='review'", &dir).unwrap();
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
        "[pet_animations]\nunknown='idle'",
        "[pet_animations]\nerror='unknown'",
        "[pet_animations.codex]\nunknown='idle'",
        "[pet_animations.Bad_Key]\nerror='idle'",
        "[pets]\nBad_Key='pet.json'",
        "[notifications]\npet='pet.json'",
        "[notifications.pet_animations]\nerror='idle'",
        "[agents.codex]\npet='pet.json'",
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
    assert_eq!(
        config.events[Kind::Error].sound,
        Some(dir.join("sound.wav"))
    );
    assert_eq!(config.events[Kind::PermissionRequired].duration_ms, 0);
    assert!(config.events[Kind::TurnComplete].enabled);
    assert_eq!(Cli::default().selected_mode(&config), Mode::Resident);
    assert_eq!(
        Cli::parse(args(&["--transient"]))
            .unwrap()
            .selected_mode(&config),
        Mode::Transient
    );
    assert_eq!(
        Config::parse("", &dir)
            .unwrap()
            .codex
            .usage
            .refresh_interval_sec,
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
        "[events.error]\nduration_sec=-1",
        "[codex.usage]\nrefresh_interval_sec=0",
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
    assert_eq!(cli.mode, Some(Mode::Transient));
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
fn the_tray_style_is_color_by_default_and_accepts_bw() {
    let dir = PathBuf::from(".");
    assert_eq!(
        Config::parse("", &dir).unwrap().tray.style,
        TrayStyle::Color
    );
    for (name, style) in [("color", TrayStyle::Color), ("bw", TrayStyle::Bw)] {
        let text = format!("[tray]\nstyle='{name}'");
        assert_eq!(Config::parse(&text, &dir).unwrap().tray.style, style);
    }
    let error = Config::parse("[tray]\nstyle='gray'", &dir).unwrap_err();
    assert!(error.contains("tray.style"), "{error}");
}

#[test]
fn a_missing_default_configuration_creates_its_folder() {
    let root = std::env::temp_dir().join(format!("aherald-fresh-{}", std::process::id()));
    let path = root.join("Library/Application Support/aHerald/config.toml");
    assert!(Config::load(&path, true).is_err());
    assert!(!root.exists());
    assert!(Config::load(&path, false).is_ok());
    assert!(path.is_file());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn hook_setup_operations_take_an_agent_and_an_optional_file_and_apply() {
    use aherald::agent::Agent;
    let cli = Cli::parse(args(&["--install-hooks", "claude"])).unwrap();
    assert_eq!(cli.operation.as_deref(), Some("install-hooks"));
    assert_eq!(
        (cli.hooks_agent, cli.apply, cli.hooks_file),
        (Some(Agent::Claude), false, None)
    );
    let cli = Cli::parse(args(&[
        "--uninstall-hooks",
        "codex",
        "--apply",
        "--hooks-file",
        "h.json",
    ]))
    .unwrap();
    assert_eq!(cli.operation.as_deref(), Some("uninstall-hooks"));
    assert_eq!(
        (cli.hooks_agent, cli.apply, cli.hooks_file),
        (Some(Agent::Codex), true, Some(PathBuf::from("h.json")))
    );
    for bad in [
        vec!["--install-hooks"],
        vec!["--install-hooks", "gemini"],
        vec!["--install-hooks", "claude", "--resident"],
        vec!["--apply"],
        vec!["--hooks-file", "x.json"],
        vec!["--install-hooks", "claude", "--uninstall-hooks", "claude"],
        vec!["--hook", "--apply"],
    ] {
        assert!(Cli::parse(args(&bad)).is_err(), "{bad:?}");
    }
}

#[test]
fn the_claude_quota_source_defaults_to_auto_and_accepts_the_three_values() {
    use aherald::config::ClaudeUsageSource as S;
    let dir = PathBuf::from(".");
    assert_eq!(
        Config::parse("", &dir).unwrap().claude.usage.source,
        S::Auto
    );
    for (name, source) in [
        ("auto", S::Auto),
        ("token", S::Token),
        ("statusline", S::Statusline),
    ] {
        let text = format!("[claude.usage]\nsource='{name}'");
        assert_eq!(
            Config::parse(&text, &dir).unwrap().claude.usage.source,
            source
        );
    }
    let error = Config::parse("[claude.usage]\nsource='both'", &dir).unwrap_err();
    assert!(error.contains("claude.usage.source"), "{error}");
}

#[test]
fn statusline_takes_an_optional_command_and_no_mode() {
    let cli = Cli::parse(args(&["--statusline"])).unwrap();
    assert_eq!(
        (cli.operation.as_deref(), cli.then),
        (Some("statusline"), None)
    );
    let cli = Cli::parse(args(&["--statusline", "--then", "~/bar.sh --x"])).unwrap();
    assert_eq!(cli.then.as_deref(), Some("~/bar.sh --x"));
    for bad in [
        vec!["--then", "x"],
        vec!["--statusline", "--then"],
        vec!["--statusline", "--then", "a", "--then", "b"],
        vec!["--statusline", "--resident"],
        vec!["--statusline", "--agent", "claude"],
        vec!["--hook", "--then", "x"],
    ] {
        assert!(Cli::parse(args(&bad)).is_err(), "{bad:?}");
    }
}

#[test]
fn statusline_setup_operations_share_apply_and_file_options() {
    for op in ["--install-statusline", "--uninstall-statusline"] {
        let cli = Cli::parse(args(&[op, "--apply", "--hooks-file", "s.json"])).unwrap();
        assert_eq!(cli.operation.as_deref(), Some(&op[2..]));
        assert!(cli.apply && cli.hooks_file.is_some());
        assert!(Cli::parse(args(&[op, "--resident"])).is_err());
        assert!(Cli::parse(args(&[op, "--then", "x"])).is_err());
    }
}

#[cfg(unix)]
#[test]
fn statusline_setup_wraps_an_existing_command_through_the_binary_and_restores_it() {
    let dir = std::env::temp_dir().join(format!("aherald-sl-bin-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let file = dir.join("settings.json");
    let original = r#"{"statusLine":{"type":"command","command":"my-bar --x"},"theme":"dark"}"#;
    fs::write(&file, original).unwrap();
    let run = |op: &str, extra: &[&str]| {
        support::output(
            Command::new(env!("CARGO_BIN_EXE_aherald"))
                .args([op, "--hooks-file"])
                .arg(&file)
                .args(extra),
        )
    };
    let dry = run("--install-statusline", &[]);
    assert!(String::from_utf8_lossy(&dry.stdout).contains("Dry run"));
    assert_eq!(fs::read_to_string(&file).unwrap(), original);
    assert!(run("--install-statusline", &["--apply"]).status.success());
    let text = fs::read_to_string(&file).unwrap();
    assert!(text.contains("--statusline --then 'my-bar --x'"), "{text}");
    assert!(run("--uninstall-statusline", &["--apply"]).status.success());
    let restored: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&file).unwrap()).unwrap();
    assert_eq!(
        restored,
        serde_json::from_str::<serde_json::Value>(original).unwrap()
    );
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn hook_setup_dry_run_and_apply_work_through_the_binary() {
    let dir = std::env::temp_dir().join(format!("aherald-setup-bin-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let file = dir.join("settings.json");
    fs::write(&file, r#"{"theme":"dark"}"#).unwrap();
    let run = |extra: &[&str]| {
        support::output(
            Command::new(env!("CARGO_BIN_EXE_aherald"))
                .args(["--install-hooks", "claude", "--hooks-file"])
                .arg(&file)
                .args(extra),
        )
    };
    let dry = run(&[]);
    assert!(dry.status.success());
    assert!(String::from_utf8_lossy(&dry.stdout).contains("Dry run"));
    assert_eq!(fs::read_to_string(&file).unwrap(), r#"{"theme":"dark"}"#);
    let applied = run(&["--apply"]);
    assert!(applied.status.success());
    let text = fs::read_to_string(&file).unwrap();
    assert!(text.contains("--hook --agent claude --resident") && text.contains("\"theme\""));
    let again = run(&["--apply"]);
    assert!(String::from_utf8_lossy(&again.stdout).contains("Nothing to change"));
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn hook_configuration_error_is_silent_and_successful() {
    let path = std::env::temp_dir().join(format!("aherald-invalid-{}.toml", std::process::id()));
    fs::write(&path, "[sounds]\nvolume = 9.0").unwrap();
    let output = support::output(
        Command::new(env!("CARGO_BIN_EXE_aherald"))
            .args(["--hook", "--config"])
            .arg(&path),
    );
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
    assert_eq!(config.events[Kind::PermissionRequired].duration_ms, 0);
}

#[test]
fn invalid_configuration_names_the_field_and_reason_without_echoing_values() {
    let dir = PathBuf::from(".");
    let cases = [
        ("[notifications]\nscale = 99", "notifications.scale", "99"),
        (
            "[notifications]\nscale = \"SECRETVALUE\"",
            "notifications.scale",
            "SECRETVALUE",
        ),
        ("[general]\nbogus = true", "general.bogus", "true"),
        (
            "[general]\nmode = \"SECRETMODE\"",
            "general.mode",
            "SECRETMODE",
        ),
        (
            "[events.turn_complete]\nduration_sec = 987654321987",
            "events.turn_complete.duration_sec",
            "987654321987",
        ),
        (
            "[resident]\nauto_start = true",
            "resident.auto_start",
            "true",
        ),
        ("[events.nope]\nenabled = true", "events.nope", "nope\""),
        (
            "[claude.usage]\nrefresh_interval_sec = 60",
            "claude.usage.refresh_interval_sec",
            "= 60",
        ),
        (
            "[tray]\ngraph = \"SECRETGRAPH\"",
            "tray.graph",
            "SECRETGRAPH",
        ),
    ];
    for (text, field, secret) in cases {
        let error = Config::parse(text, &dir).unwrap_err();
        assert!(error.contains(field), "{text}: {error}");
        assert!(!error.contains(secret), "{text} echoed a value: {error}");
    }
    let error = Config::parse("[notifications\nSECRETLINE", &dir).unwrap_err();
    assert!(error.contains("line 1"), "{error}");
    assert!(!error.contains("SECRETLINE"), "{error}");
}
#[test]
fn session_warning_thresholds_accept_tokens_and_kilotokens() {
    for value in ["120000", "\"120000\"", "\"120k\""] {
        let text = format!(
            "[claude.warnings]\ncontext_tokens = {value}\nidle_minutes = 5\n[codex.warnings]\nidle_minutes = 10"
        );
        let config = Config::parse(&text, &PathBuf::from("."));
        assert!(config.is_ok(), "{config:?}");
        let config = config.unwrap();
        assert_eq!(config.claude.warnings.context_tokens.unwrap().0, 120000);
        assert_eq!(config.claude.warnings.idle_minutes, Some(5));
        assert_eq!(config.codex.warnings.idle_minutes, Some(10));
    }
}

#[test]
fn session_warning_defaults_are_per_agent_and_invalid_values_are_private() {
    for config in [
        Config::default(),
        Config::parse("", &PathBuf::from(".")).unwrap(),
        Config::parse(include_str!("../config.example.toml"), &PathBuf::from(".")).unwrap(),
        Config::parse(
            "[claude.warnings]\ncontext_tokens=100\n[codex.warnings]",
            &PathBuf::from("."),
        )
        .unwrap(),
    ] {
        assert_eq!(config.claude.warnings.idle_minutes, Some(55));
        assert_eq!(config.codex.warnings.idle_minutes, Some(25));
    }
    assert!(Config::default().claude.warnings.context_tokens.is_none());
    for setting in [
        "context_tokens = 0",
        "context_tokens = -1",
        "context_tokens = 1.5",
        "context_tokens = \"1.5k\"",
        "context_tokens = \"18446744073709551615k\"",
        "context_tokens = \"SECRET\"",
        "idle_minutes = -1",
        "idle_minutes = 525601",
    ] {
        let error = Config::parse(
            &format!("[claude.warnings]\n{setting}"),
            &PathBuf::from("."),
        )
        .unwrap_err();
        assert!(!error.contains("SECRET"));
    }
    assert_eq!(
        Config::parse(
            "[codex.warnings]\ncontext_tokens = '1k'",
            &PathBuf::from(".")
        )
        .unwrap()
        .codex
        .warnings
        .context_tokens
        .unwrap()
        .0,
        1000
    );
}

#[test]
fn session_warning_zero_disables_only_the_selected_agents_idle_timer() {
    let config = Config::parse("[claude.warnings]\nidle_minutes=0", &PathBuf::from(".")).unwrap();
    assert!(config.claude.warnings.idle_minutes.is_none());
    assert_eq!(config.codex.warnings.idle_minutes, Some(25));
    let config = Config::parse("[codex.warnings]\nidle_minutes=0", &PathBuf::from(".")).unwrap();
    assert!(config.codex.warnings.idle_minutes.is_none());
    assert_eq!(config.claude.warnings.idle_minutes, Some(55));
}

#[test]
fn the_status_line_format_defaults_to_empty_and_syntax_errors_name_the_setting() {
    let dir = PathBuf::from(".");
    assert_eq!(Config::default().claude.statusline.format, "");
    let ok = "[claude.statusline]\nformat = \"{model.display_name}{[ · {a.b}]}\"\n";
    assert!(Config::parse(ok, &dir).is_ok());
    let error = Config::parse("[claude.statusline]\nformat = \"{oops\"\n", &dir).unwrap_err();
    assert!(error.contains("claude.statusline.format"), "{error}");
    assert!(Config::parse("[claude.statusline]\nformt = \"x\"\n", &dir).is_err());
}

#[test]
fn the_status_line_shows_the_context_used_and_the_quotas_remaining_unless_told_otherwise() {
    use aherald::config::UsageDisplay::{Remain, Used};
    let dir = PathBuf::from(".");
    let defaults = Config::default().claude.statusline;
    assert_eq!(
        (defaults.display_ctx, defaults.display_usage),
        (Used, Remain)
    );
    let set = "[claude.statusline]\ndisplay_ctx = \"remain\"\ndisplay_usage = \"used\"\n";
    let chosen = Config::parse(set, &dir).unwrap().claude.statusline;
    assert_eq!((chosen.display_ctx, chosen.display_usage), (Remain, Used));
    assert!(Config::parse("[claude.statusline]\ndisplay_ctx = \"left\"\n", &dir).is_err());
}

const TELEGRAM: &str = "[[forward.sinks]]\ntype = \"telegram\"\ntoken_env = \"AHERALD_TELEGRAM_TOKEN\"\nchat_id_env = \"AHERALD_TELEGRAM_CHAT\"\n";

#[test]
fn forwarding_defaults_to_on_with_no_sinks() {
    let config = Config::parse("", &PathBuf::from(".")).unwrap();
    assert!(config.forward.enabled);
    assert!(config.forward.sinks.is_empty());
}

#[test]
fn a_telegram_sink_defaults_to_the_four_attention_kinds_and_no_folder() {
    let config = Config::parse(TELEGRAM, &PathBuf::from(".")).unwrap();
    let sink = &config.forward.sinks[0];
    assert_eq!(
        sink.kinds,
        [
            Kind::TurnComplete,
            Kind::PermissionRequired,
            Kind::InputRequired,
            Kind::Error
        ]
    );
    assert!(!sink.include_folder);
    assert_eq!(sink.token_env.as_deref(), Some("AHERALD_TELEGRAM_TOKEN"));
}

#[test]
fn the_kinds_and_the_folder_switch_are_configurable() {
    let text = format!("{TELEGRAM}kinds = [\"error\"]\ninclude_folder = true\n");
    let sink = &Config::parse(&text, &PathBuf::from("."))
        .unwrap()
        .forward
        .sinks[0];
    assert_eq!(sink.kinds, [Kind::Error]);
    assert!(sink.include_folder);
}

#[rstest]
#[case::literal_token("token = \"123456:LITERAL-SECRET\"\n")]
#[case::token_value_in_the_variable_name("token_env = \"123456:LITERAL-SECRET\"\n")]
#[case::both_token_forms("token_file = \"token.txt\"\n")]
#[case::unknown_kind("kinds = [\"turn_complet\"]\n")]
fn an_unsafe_or_ambiguous_sink_is_refused_without_echoing_the_value(#[case] extra: &str) {
    let text = if extra.starts_with("token_env") {
        format!(
            "[[forward.sinks]]\ntype = \"telegram\"\n{extra}chat_id_env = \"AHERALD_TELEGRAM_CHAT\"\n"
        )
    } else {
        format!("{TELEGRAM}{extra}")
    };
    let error = Config::parse(&text, &PathBuf::from(".")).unwrap_err();
    assert!(
        error.starts_with("invalid configuration: forward.sinks"),
        "{error}"
    );
    assert!(!error.contains("LITERAL-SECRET"), "{error}");
}

#[rstest]
#[case::no_token("[[forward.sinks]]\ntype = \"telegram\"\nchat_id_env = \"C\"\n")]
#[case::no_chat("[[forward.sinks]]\ntype = \"telegram\"\ntoken_env = \"T\"\n")]
#[case::unknown_type("[[forward.sinks]]\ntype = \"carrier-pigeon\"\n")]
fn an_incomplete_sink_is_refused(#[case] text: &str) {
    assert!(Config::parse(text, &PathBuf::from(".")).is_err());
}

#[test]
fn a_token_file_beside_the_configuration_resolves_to_an_absolute_path() {
    let text = "[[forward.sinks]]\ntype = \"telegram\"\ntoken_file = \"secrets/token.txt\"\nchat_id_env = \"C\"\n";
    let dir = PathBuf::from("base");
    let sink = &Config::parse(text, &dir).unwrap().forward.sinks[0];
    assert_eq!(
        sink.token_file.as_deref(),
        Some(dir.join("secrets/token.txt").as_path())
    );
}

const WEBHOOK: &str =
    "[[forward.sinks]]\ntype = \"webhook\"\nurl = \"https://api.example/bot${TOKEN}/send\"\n";

#[test]
fn a_webhook_sink_defaults_to_a_plain_text_body_of_the_message() {
    let config = Config::parse(WEBHOOK, &PathBuf::from(".")).unwrap();
    let sink = &config.forward.sinks[0];
    assert_eq!(
        sink.url.as_deref(),
        Some("https://api.example/bot${TOKEN}/send")
    );
    assert_eq!(sink.body.as_deref(), None);
    assert_eq!(sink.content_type.as_deref(), None);
    assert_eq!(sink.kinds.len(), 4);
}

#[test]
fn a_webhook_sink_takes_a_body_and_a_content_type() {
    let text = format!(
        "{WEBHOOK}body = '{{\"text\": \"{{text}}\"}}'\ncontent_type = \"application/json\"\n"
    );
    let sink = &Config::parse(&text, &PathBuf::from("."))
        .unwrap()
        .forward
        .sinks[0];
    assert_eq!(sink.body.as_deref(), Some("{\"text\": \"{text}\"}"));
    assert_eq!(sink.content_type.as_deref(), Some("application/json"));
}

#[rstest]
#[case::no_url("[[forward.sinks]]\ntype = \"webhook\"\n")]
#[case::token_field(
    "[[forward.sinks]]\ntype = \"webhook\"\nurl = \"https://h/\"\ntoken_env = \"T\"\n"
)]
#[case::telegram_with_url(
    "[[forward.sinks]]\ntype = \"telegram\"\ntoken_env = \"T\"\nchat_id_env = \"C\"\nurl = \"https://h/\"\n"
)]
fn a_sink_with_fields_of_another_type_or_no_url_is_refused(#[case] text: &str) {
    let error = Config::parse(text, &PathBuf::from(".")).unwrap_err();
    assert!(
        error.starts_with("invalid configuration: forward.sinks"),
        "{error}"
    );
}
