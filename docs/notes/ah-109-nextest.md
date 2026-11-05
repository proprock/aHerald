# AH-109: Windows runner evaluation

## Reproduce

Run from an interactive Windows checkout with stable Rust/MSVC and the Windows SDK.
The evaluated runner is cargo-nextest **0.9.150** (d225b1daf, 2026-10-26), the
[official prebuilt Windows binary](https://nexte.st/docs/installation/pre-built-binaries/).
Download `https://get.nexte.st/0.9.150/windows`, extract the archive, and verify
`Get-AuthenticodeSignature` reports `Valid` with the SignPath Foundation signer.
Copy `cargo-nextest.exe` into the normal `$HOME/.cargo/bin` directory; confirm
`cargo nextest --version` reports exactly 0.9.150 before execution. This is a developer tool, not a dependency of the portable application.

```powershell
./tests/runner_probe.ps1
./tests/compare_runners.ps1 -Runs 3 -RebuildPackage
# Resume after fixing an inventory/reporting error without repeating cold builds:
./tests/compare_runners.ps1 -Runs 3 -Resume
```

The comparison uses the normal target and Cargo caches. Each package-cold build
runs `cargo clean -p aherald --profile test` first; dependency caches remain warm
for both. Cleaning is outside the build measurement. No other build/test workload
should run during measurement. Execution rounds alternate Cargo/nextest order;
they include warm Cargo discovery/no-op build overhead. This is not a measurement
of clean dependency compilation or hosted CI throughput. Build and execution wall
times are recorded separately. Do not infer a speedup from one cold/one warm build.

Artifacts in `target/ah109/`: environment/version record, build and run logs,
measurements.json, exact test inventory/coverage.json, one JUnit report per nextest
round, and the separate doctest log. Each nonzero command stops the comparison;
investigate before rerunning. No automatic retries are enabled.

## Scheduling and resource inventory

`.config/nextest.toml` starts from a conservative common `desktop` group with one
slot. This includes the mixed library unit tests and every integration binary
except six explicitly reviewed pure/local-fixture suites. Consequently new or
unreviewed tests serialize by default, including unknown platform tests.

| Resource / examples | Scheduling |
| --- | --- |
| Desktop, placement mutex, foreground windows: native_overlay, native_notice, window_wait, activation, click_target, presenter, resident/render fixtures | Common desktop group, one test process |
| Tray / Explorer: tray, tray_about, tray_restart, tray_usage, usage_ui | Same desktop group |
| Audio device and playback: audio, audio_log, mixed library tests | Same desktop group |
| Process trees, IPC, logger/environment state, setup, platform integrations, all unreviewed suites | Same desktop group; retain existing unique IPC scopes and owned fixture cleanup |
| animation (unique per-process image fixtures), queue, queue_outcome, card_folder, usage_compact, warning_messages | isolated group, up to four processes; no shared native GUI/audio resources |
| perf*, resident_perf_log | desktop group AND all four runner slots; no concurrent isolated tests |

The last catch-all override is deliberate: a process-local Rust mutex does not
serialize independent nextest test processes. These groups coordinate one runner
invocation only. Never run another GUI suite or benchmark concurrently. Ordinary
Cargo retains its existing supported behavior; use `-- --test-threads=1` when
investigating intra-binary native contention.

Default per-test deadline is 120 seconds (30-second slow warnings, termination on
the fourth period), failure output appears immediately and at the end, fail-fast
is disabled, retries are zero. The `ci` profile writes
`target/nextest/ci/junit.xml`. On Windows nextest terminates the test's job including
owned descendants ([timeout contract](https://nexte.st/docs/features/slow-tests/)).
Explicit diagnostic probes use a five-second deadline and their own JUnit path.

## Selections

```powershell
# Fast deterministic subset (not a substitute for full native acceptance):
cargo nextest run --all-targets -E 'binary(=animation) | binary(=queue) | binary(=queue_outcome) | binary(=card_folder) | binary(=usage_compact) | binary(=warning_messages)'
# Native integration: includes conservative non-fast suites; ignored tests stay ignored.
cargo nextest run --all-targets -E 'not (binary(=animation) | binary(=queue) | binary(=queue_outcome) | binary(=card_folder) | binary(=usage_compact) | binary(=warning_messages) | binary(/^perf/) | binary(=resident_perf_log))'
# Required performance acceptance; close competing GUI/build/benchmark workloads first.
cargo nextest run --all-targets -E 'binary(/^perf/) | binary(=resident_perf_log)'
# Equivalent complete ordinary selection and machine-readable failure report:
cargo nextest run --all-targets --locked --profile ci
# Nextest does not replace doctests:
cargo test --doc --locked
# Explicit manual Explorer check, only on an interactive desktop:
cargo test --test tray explorer_notification_recreates_deleted_icon_and_quit_removes_it -- --ignored --exact --nocapture
# Ordinary Cargo remains supported:
cargo test --all-targets --locked
```

Do not use a blanket `--run-ignored all`. Select an exact ignored test only after
reading its prerequisites. `tests/live/README.md` retains the separate installed
harness/account/network checks; they are never part of this evaluation or CI.
README's explicit audio, Explorer and manual resident commands remain opt-in.
Interactive previews and the intentional hanging runner probe are also ignored.

## Behavioral evidence and limits

The opt-in `runner_contract` tests execute real resource effects, not source-text
assertions. Without scheduling, two test processes contend for the same
`create_new` file: the observed red run passed one and failed one with AlreadyExists.
With the committed configuration, both pass. A separate performance/background
pair fails when the all-slots rule is removed and passes with that rule present.

`tests/runner_probe.ps1` then launches an intentionally hung test which starts a
real detached resident in a unique IPC scope and holds an exclusive Windows file.
It retains exact handles to both processes before the deadline. The observed
runner exit is 100, console classification TIMEOUT, and JUnit contains a timeout
failure. Both handles signal exit within bounded waits. A second process acquires
the released resident mutex, observes the pipe unavailable, and opens/removes the
exclusive file. The script owns cleanup, validates its private temporary deletion
path, restores its environment and never targets an installed resident.

These are scheduling/lifecycle checks, not visual acceptance of transparency,
DPI, multiple monitors, or sound quality. Windows 11 build 22621 is the local
execution environment; macOS/Linux nextest compatibility is not claimed. Existing
platform-specific Cargo, Linux isolated display/D-Bus, installer and packaging CI
checks must remain intact. No integration binaries are consolidated: separate
logger/environment/process fixtures still provide intentional isolation.

## Measurements and decision

Package-cold build wall time, one observation per runner: Cargo **158.448 s**,
nextest **168.737 s**. Dependencies were warm in both cases. Nextest includes
test discovery; there is no demonstrated compilation saving. The second clean
removed 1.3 GiB of package artifacts. Integration-binary consolidation is not
justified by these two observations.

Inventory comparison executed each binary with `--list` and `--list --ignored`
and matched exact names/ignored flags against nextest JSON: **93 binaries,
693 ordinary tests, 23 ignored, 716 total**. Six new ignored tests are runner
diagnostics. All ordinary performance guards remain selected.

First paired warm execution: Cargo **112.177 s**, nextest **196.730 s**
(nextest execution summary 191.255 s; its wall measurement also includes discovery).
Both passed all 693 intended tests with 23 ignored and no failures.

Cargo log summary sums show 694 passes because a nested subprocess test prints its
own one-test summary. The independently matched inventory contains 693 tests, not
694. Nextest JUnit contains the 693 executed cases and omits ignored cases; its
zero JUnit skips do not mean that the 23 ignored tests ran. Use the inventory and
console summary for ignored counts.

| Warm execution round | Cargo wall, s | nextest wall, s | Intended outcome, each runner |
| --- | ---: | ---: | --- |
| 1 (Cargo then nextest) | 112.177 | 196.730 | 693 passed, 0 failed, 23 ignored |
| 2 (nextest then Cargo) | 119.959 | 195.050 | 693 passed, 0 failed, 23 ignored |
| 3 (Cargo then nextest) | 117.652 | 196.335 | 693 passed, 0 failed, 23 ignored |
| Median | **117.652** | **196.335** | Equivalent intended selection |

Decision: **retain Cargo as the mandatory CI runner**. At this conservative,
verified resource schedule, nextest's median execution wall time is **66.9% higher**.
Its bounded termination and JUnit diagnostics work, but they do not justify the
observed throughput regression and additional CI installation/platform maintenance
in this slice. Keep the pinned nextest configuration and opt-in diagnostic script
available; do not relax native serialization merely to manufacture a speedup.
This result applies to this Windows machine and schedule, not to nextest generally.

No CI workflow was changed: Windows/macOS/Linux Cargo checks, Linux isolated
Xvfb/D-Bus tests, installer diagnostics/lifecycle and release packaging remain
covered by their existing jobs. They were not all executed locally. No integration
binaries or performance fixtures/budgets were changed.

Environment: Windows 11 build 22621, four logical processors, rustc 1.98.1
(48a229cea, 2026-09-01), Cargo 1.98.1, base commit 2e58cb3 plus the AH-109 working
tree. Normal interactive desktop, sequential benchmark invocations, no competing
test/build/benchmark command started by this evaluation; OS/file-cache background
activity was not controlled. Three runs are observations, not a statistical
guarantee. Both runners had zero ordinary failures in all rounds; there are no
baseline failures to subtract. Expected fault-injection/timeout failures are
separate from ordinary suite results.

Verification:

- `cargo fmt --all -- --check`: passed.
- `cargo clippy --all-targets -- -D warnings`: passed.
- `cargo test --all-targets --locked`: three full successful rounds above.
- `cargo nextest run --all-targets --locked --profile ci`: three successful rounds.
- `cargo test --doc --locked`: passed, zero doctests currently present.
- `./tests/runner_probe.ps1`: passed; each removed scheduling rule independently
  produced one expected AlreadyExists failure, each configured pair passed, and
  the five-second timeout produced exit 100/TIMEOUT/JUnit failure with exact owned
  process exit and released native resources.
- `cargo build --release --target x86_64-pc-windows-msvc`: passed.
