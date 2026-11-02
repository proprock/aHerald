//! Start-up cost of the `--statusline` wrapper, which Claude Code runs after every response.
//!
//! The command is measured against `--version` of the same binary, interleaved in one run, so
//! machine speed and dynamic-loader cost (the executable links the whole GUI stack, see
//! docs/statusline-performance.md) cancel out and what remains is the wrapper's own work.
//! Medians are used: a loaded CI runner inflates means. Use --nocapture for the table.
#![cfg(unix)]
use std::{
    io::Write,
    path::PathBuf,
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
const FORMAT: &str = "[claude.statusline]\nformat = \"{model.display_name}{[·{worktree.branch}]}·+{cost.total_lines_added}·{context_window.used_percentage}%\"\n";

fn spawn(args: &[&str], stdin: Option<&str>) -> Duration {
    let start = Instant::now();
    let mut child = Command::new(env!("CARGO_BIN_EXE_aherald"))
        .args(args)
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
    let config: PathBuf = dir.join("config.toml");
    std::fs::write(&config, FORMAT).unwrap();
    let config = config.to_str().unwrap();
    let cases: [(&str, Vec<&str>); 4] = [
        ("reference --version", vec!["--version"]),
        ("--statusline", vec!["--statusline"]),
        (
            "--statusline + format",
            vec!["--statusline", "--config", config],
        ),
        (
            "--statusline --then true",
            vec!["--statusline", "--then", "true"],
        ),
    ];
    for (_, args) in &cases {
        spawn(args, Some(PAYLOAD)); // warm the file cache and the loader
    }
    let mut samples: Vec<Vec<Duration>> = vec![Vec::new(); cases.len()];
    for _ in 0..ROUNDS {
        for (i, (_, args)) in cases.iter().enumerate() {
            samples[i].push(spawn(args, Some(PAYLOAD)));
        }
    }
    let medians: Vec<f64> = samples.into_iter().map(median).collect();
    for ((name, _), ms) in cases.iter().zip(&medians) {
        println!(
            "{name:<28} median {ms:6.2} ms   (+{:5.2} vs reference)",
            ms - medians[0]
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
    for ((name, _), ms) in cases.iter().zip(&medians).skip(1) {
        assert!(
            ms - medians[0] <= OVERHEAD_BUDGET_MS,
            "{name}: {ms:.2} ms is more than {OVERHEAD_BUDGET_MS} ms over the {:.2} ms reference",
            medians[0]
        );
    }
    assert!(
        medians.iter().all(|ms| *ms <= ABSOLUTE_BUDGET_MS),
        "a median exceeded {ABSOLUTE_BUDGET_MS} ms: {medians:?}"
    );
}
