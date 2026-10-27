# Release acceptance (AH-023)

Automated part, run 2026-10-27 on Windows 11 (10.0.22621.4317), Rust 1.98.1 MSVC x64,
branch `feature/AH-075`, version 0.1.0.

| Check | Result |
| --- | --- |
| `cargo fmt --all -- --check` | pass |
| `cargo clippy --all-targets -- -D warnings` | pass |
| `cargo test --all-targets` (includes IPC, single-instance, lifecycle, detached-process and cleanup integration tests) | pass; interactive/live tests stay `#[ignore]` |
| `cargo build --release --target x86_64-pc-windows-msvc` | pass |
| `check-release.ps1` | x64 GUI executable, no separate C runtime |
| `package-release.ps1` | 1.29 MB zip; fresh extraction as a non-administrator runs `--version` |

Hook latency, release build in a clean folder, quiet config (no card, no sound), `Stop`
payload piped to `--hook`, 10 runs each, wall time including process start:

| Mode | min | median | max |
| --- | --- | --- | --- |
| `--transient` | 19 ms | 19-23 ms | 332-390 ms (first run) |
| `--resident` (first run starts the service) | 28-29 ms | 31-35 ms | 45-138 ms |

All runs exited 0 and printed `{}`. `--quit` returned in 22-53 ms and `--status` then
reported `{"running":false}`. The latencies are far below the three-second Codex hook timeout.

One anomaly: in the first measurement a resident that was already running (pid not started
by the script, queue empty) answered every hook with `aHerald: local pipe error (800700e9)`
on stderr. Hooks still returned in 29-45 ms with exit 0, so delivery degraded safely, but
the instance was quit by the script and the cause is unknown. It did not recur in three
later runs with fresh residents. Not investigated further.

## Not covered (still open for AH-023)

- Windows 10 smoke test: no Windows 10 machine was available.
- Visual checks: topmost, transparency, focus, tray menus, Explorer restart, mixed DPI and
  multiple monitors (AH-020, AH-076, AH-085, AH-086); they need a person at the screen.
- Long-lived waits (hours) and sound on real hardware.
- The GitHub release workflow has not run (branch not pushed).
- macOS and Linux are not part of this release: nothing there runs the service yet.
