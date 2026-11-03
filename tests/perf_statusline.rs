//! Start-up cost of the `--statusline` wrapper, which Claude Code runs after every response.
//!
//! The command is measured against `--version` of the same binary, interleaved in one run, so
//! machine speed and dynamic-loader cost (the executable links the whole GUI stack, see
//! docs/statusline-performance.md) cancel out and what remains is the wrapper's own work.
//! No resident runs in the private runtime directory, which is the case that once cost
//! 150 ms per call (the IPC client waited for a resident to start). Medians are used: a loaded CI runner inflates means. Use --nocapture for the table.
#![cfg(unix)]
use std::{
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

const ROUNDS: usize = 80;
/// Extra median milliseconds a wrapper invocation may cost over `--version`. Measured on the
/// unoptimized test build (Ubuntu 22.04, two runs of 80 rounds): bare +1.6..1.7, with a format
/// +0.8..0.9, with `--then true` +1.2..1.3 (a release build starts in 3.5..4.5 ms in all cases).
/// The budget is about 3.5x the largest observation, so only a structural regression, such as
/// reading the whole configuration on the `--then` path, fails it.
const OVERHEAD_BUDGET_MS: f64 = 6.0;
/// Ceiling for any single median, for runners where even the reference is slow.
const ABSOLUTE_BUDGET_MS: f64 = 50.0;
const PAYLOAD: &str = r#"{"session_id":"perf-sl","session_name":"N","model":{"display_name":"Opus"},"cost":{"total_lines_added":4},"context_window":{"used_percentage":12.5,"current_usage":{"input_tokens":1000,"cache_creation_input_tokens":0,"cache_read_input_tokens":0}}}"#;
const FORMAT: &str = "[claude.statusline]\nformat = \"{model.display_name}{[·{worktree.branch}]}·+{cost.total_lines_added}·{context_window}{[ · {rate_limits.five_hour}]}\"\n";

fn spawn(program: &str, args: &[&str], stdin: Option<&str>, runtime: &Path) -> Duration {
    let start = Instant::now();
    let mut child = Command::new(program)
        .args(args)
        .env("XDG_RUNTIME_DIR", runtime)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut pipe = child.stdin.take().unwrap();
    if let Some(text) = stdin {
        let _ = pipe.write_all(text.as_bytes());
    }
    drop(pipe);
    child.wait().unwrap();
    start.elapsed()
}

fn median(mut samples: Vec<Duration>) -> f64 {
    samples.sort();
    samples[samples.len() / 2].as_secs_f64() * 1000.0
}

#[test]
fn the_wrapper_costs_little_beyond_starting_the_executable() {
    let dir = std::env::temp_dir().join(format!("aherald-perf-sl-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    // A private, empty runtime directory: no resident is listening, whatever runs on this
    // machine. Without one the wrapper must not wait for a resident to start.
    let runtime = dir.join("runtime");
    std::fs::create_dir_all(&runtime).unwrap();
    std::fs::set_permissions(
        &runtime,
        std::os::unix::fs::PermissionsExt::from_mode(0o700),
    )
    .unwrap();
    let config: PathBuf = dir.join("config.toml");
    std::fs::write(&config, FORMAT).unwrap();
    let config = config.to_str().unwrap();
    let exe = env!("CARGO_BIN_EXE_aherald");
    // The `--then` command is the user's own, run after the wrapper's own start by `sh -c`,
    // so the cost of starting `sh` itself (about 1 ms on Linux, 3 ms on an M1 Mac, 8 ms on a
    // macOS runner) is added to the budget for that case. It is the absolute median of
    // `sh -c true` alone: measured against `--version` it would be negative when `sh` starts
    // faster than the GUI-linked executable, and the allowance would vanish.
    let cases: [(&str, &str, Vec<&str>); 5] = [
        ("reference --version", exe, vec!["--version"]),
        ("--statusline", exe, vec!["--statusline"]),
        (
            "--statusline + format",
            exe,
            vec!["--statusline", "--config", config],
        ),
        (
            "--statusline --then true",
            exe,
            vec!["--statusline", "--then", "true"],
        ),
        ("sh -c true (alone)", "sh", vec!["-c", "true"]),
    ];
    for (_, program, args) in &cases {
        spawn(program, args, Some(PAYLOAD), &runtime); // warm the file cache and the loader
    }
    let mut samples: Vec<Vec<Duration>> = vec![Vec::new(); cases.len()];
    for _ in 0..ROUNDS {
        for (i, (_, program, args)) in cases.iter().enumerate() {
            samples[i].push(spawn(program, args, Some(PAYLOAD), &runtime));
        }
    }
    let medians: Vec<f64> = samples.into_iter().map(median).collect();
    for ((name, _, _), ms) in cases.iter().zip(&medians) {
        println!(
            "{name:<28} median {ms:6.2} ms   (+{:5.2} vs reference)",
            ms - medians[0]
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
    let shell_start = medians[4];
    for (i, ((name, _, _), ms)) in cases.iter().zip(&medians).enumerate().take(4).skip(1) {
        let allowed = OVERHEAD_BUDGET_MS
            + if i == 3 { shell_start } else { 0.0 };
        assert!(
            ms - medians[0] <= allowed,
            "{name}: {ms:.2} ms is more than {allowed:.1} ms over the {:.2} ms reference",
            medians[0]
        );
    }
    assert!(
        medians.iter().all(|ms| *ms <= ABSOLUTE_BUDGET_MS),
        "a median exceeded {ABSOLUTE_BUDGET_MS} ms: {medians:?}"
    );
}
