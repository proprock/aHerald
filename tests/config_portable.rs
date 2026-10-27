use aherald::{
    config::{Config, Mode},
    events::Kind,
};
use std::{
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{Arc, Barrier},
};
struct Root(PathBuf);
impl Root {
    fn new(name: &str) -> Self {
        let p = std::env::temp_dir().join(format!(
            "aherald portable {name} {} \u{043f}\u{0443}\u{0442}\u{044c}",
            std::process::id()
        ));
        std::fs::create_dir_all(&p).unwrap();
        Self(p)
    }
}
impl Drop for Root {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
#[test]
fn default_is_beside_current_executable_and_help_explains_it() {
    assert_eq!(
        Config::default_path().unwrap(),
        std::env::current_exe()
            .unwrap()
            .parent()
            .unwrap()
            .join("config.toml")
    );
    assert!(aherald::help().contains("config.toml beside"));
}
#[test]
fn first_load_creates_defaults_without_replacing_existing_custom_resources() {
    let r = Root::new("create");
    let path = r.0.join("config.toml");
    let c = Config::load(&path, false).unwrap();
    assert!(path.is_file());
    assert_eq!(c.general.mode, Mode::Transient);
    assert!(!c.codex.usage.enabled);
    assert!(c.notifications.activate_agent_on_click);
    assert_eq!(c.events[Kind::PermissionRequired].duration_ms, 0);
    assert_eq!(c.events[Kind::TurnComplete].duration_ms, 3000);
    let text =
        "[notifications]\npet='pets/my pet.json'\n[events.error]\nsound='sounds/my sound.wav'";
    std::fs::write(&path, text).unwrap();
    let c = Config::load(&path, false).unwrap();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), text);
    assert_eq!(c.notifications.pet, Some(r.0.join("pets/my pet.json")));
    assert_eq!(
        c.events[Kind::Error].sound,
        Some(r.0.join("sounds/my sound.wav"))
    );
    assert_eq!(c.diagnostic_directory, r.0);
    std::fs::write(&path, "invalid = [").unwrap();
    assert!(Config::load(&path, false).is_err());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "invalid = [");
}
#[test]
fn explicit_missing_file_and_write_failure_report_target_and_reason() {
    let r = Root::new("failure");
    let missing = r.0.join("custom.toml");
    let e = Config::load(&missing, true).unwrap_err();
    assert!(e.contains(missing.to_str().unwrap()), "{e}");
    assert!(!missing.exists());
    let blocked = r.0.join("file");
    std::fs::write(&blocked, "occupied").unwrap();
    let target = blocked.join("config.toml");
    let e = Config::load(&target, false).unwrap_err();
    assert!(e.contains(target.to_str().unwrap()), "{e}");
    assert!(e.contains(':'));
    assert_eq!(std::fs::read_to_string(blocked).unwrap(), "occupied");
}
#[test]
fn concurrent_first_loads_publish_one_complete_configuration() {
    let r = Root::new("race");
    let path = r.0.join("config.toml");
    let barrier = Arc::new(Barrier::new(8));
    let workers: Vec<_> = (0..8)
        .map(|_| {
            let p = path.clone();
            let b = barrier.clone();
            std::thread::spawn(move || {
                b.wait();
                let c = Config::load(&p, false).unwrap();
                assert_eq!(c.general.mode, Mode::Transient);
                std::fs::read_to_string(&p).unwrap()
            })
        })
        .collect();
    let texts: Vec<_> = workers.into_iter().map(|t| t.join().unwrap()).collect();
    assert!(!texts[0].is_empty());
    assert!(texts.iter().all(|s| s == &texts[0]));
    assert_eq!(std::fs::read_dir(&r.0).unwrap().count(), 1);
}
fn hook(exe: &Path, cwd: &Path, appdata: &Path, args: &[&std::ffi::OsStr]) -> std::process::Output {
    let mut child = Command::new(exe)
        .arg("--hook")
        .args(args)
        .current_dir(cwd)
        .env("APPDATA", appdata)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(br#"{"hook_event_name":"Unknown"}"#)
        .unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    while child.try_wait().unwrap().is_none() {
        if std::time::Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("hook startup blocked");
        }
        std::thread::yield_now();
    }
    child.wait_with_output().unwrap()
}
#[test]
fn process_uses_installation_config_ignores_cwd_and_appdata_and_honors_explicit_path() {
    let r = Root::new("process");
    let install = r.0.join("install \u{00e9}");
    let cwd = r.0.join("other folder");
    let appdata = r.0.join("old appdata");
    for p in [&install, &cwd, &appdata.join("aHerald")] {
        std::fs::create_dir_all(p).unwrap();
    }
    std::fs::write(cwd.join("config.toml"), "invalid = [").unwrap();
    let legacy = appdata.join("aHerald/config.toml");
    std::fs::write(&legacy, "invalid = [").unwrap();
    let exe = install.join("aherald.exe");
    std::fs::copy(env!("CARGO_BIN_EXE_aherald"), &exe).unwrap();
    let result = hook(&exe, &cwd, &appdata, &[]);
    assert!(result.status.success());
    assert!(result.stderr.is_empty(), "{:?}", result.stderr);
    let config = install.join("config.toml");
    assert!(config.is_file());
    assert_eq!(std::fs::read_to_string(&legacy).unwrap(), "invalid = [");
    std::fs::remove_file(&config).unwrap();
    let custom = cwd.join("my config.toml");
    std::fs::write(&custom, "[sounds]\nenabled=false").unwrap();
    let result = hook(
        &exe,
        &cwd,
        &appdata,
        &[
            std::ffi::OsStr::new("--config"),
            std::ffi::OsStr::new("my config.toml"),
        ],
    );
    assert!(result.stderr.is_empty());
    assert!(!config.exists());
    std::fs::remove_file(&custom).unwrap();
    let result = hook(
        &exe,
        &cwd,
        &appdata,
        &[std::ffi::OsStr::new("--config"), custom.as_os_str()],
    );
    assert!(String::from_utf8_lossy(&result.stderr).contains(custom.to_str().unwrap()));
    assert!(!custom.exists());
    assert!(!config.exists());
}
