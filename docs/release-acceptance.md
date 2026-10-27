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

One anomaly, explained: in the first measurement the owner's live resident (installed in
local appdata, built before the card redesign) answered every hook with `aHerald: local pipe
error (800700e9)` on stderr, and its log shows ten `ipc_failure` lines. Cause: the new hook
sends the `folder` event field (92f12fa) and `Event` had `deny_unknown_fields`, so the older
resident rejected the frame and closed the pipe. Hooks still returned in 29-45 ms with exit
0, but the events were lost, which is what happens when a hook is upgraded while an older
resident keeps running. Fixed: `Event` now ignores unknown fields (test
`an_event_frame_with_a_field_this_build_does_not_know_still_decodes`, red before the
change). Residents built before this fix still need a restart after an upgrade. The
measurement script's `--quit` stopped that live resident.

## Manual evidence and what is not covered (AH-023 closed 2026-10-27)

- Windows 10 is out of the acceptance scope (decided 2026-10-27: end of support, no
  machine). It was not tested; no support is claimed for it.
- Visual checks: topmost, transparency, focus, tray menus, Explorer restart, mixed DPI and
  multiple monitors (AH-020); they need a person at the screen. Card layout, the About card
  and native-DPI drawing at 100/150/200% were observed on 2026-10-27 (AH-076, AH-085,
  AH-086; `docs/milestone-5.md`).
- Long-lived waits: observed 2026-10-27 on the user's live resident, not in a dedicated
  test. `aherald.log` shows consecutive resident runs of up to about 2.7 hours
  without a crash, and the current run had been up for more than two hours. A single
  pending card kept for hours was not timed.
- Sound on real hardware: confirmed by the user on 2026-10-27.
- The GitHub release workflow ran on tag `v0.1.0` (run 40587823460): package, draft and
  publish succeeded; the downloaded archive matched its SHA-256 and `--version` printed
  `aHerald 0.1.0` from a clean extraction.
- A full Explorer process restart has not been performed (icon restoration is tested
  with the real `TaskbarCreated` message).
- macOS and Linux are not part of this release: nothing there runs the service yet.
