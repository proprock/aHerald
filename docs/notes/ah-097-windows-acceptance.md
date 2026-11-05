# AH-097 Windows acceptance

Status: partially verified on 2026-11-01. Keep AH-097 open until the remaining
acceptance checks below pass. Work branch: `feature/ah-097-windows-completion`.

## Implementation and fix

The native setup transaction, receipt-based removal, per-user Inno installer,
usage/warning selections, Claude statusLine wrapping, graceful resident shutdown
and Windows CI packaging were present before this work.

An executable-level regression reproduced setup replacing hooks owned by another
aHerald installation. Recognition used the executable filename rather than its full
path. Setup now recognizes legacy commands only for the current executable and
uses saved receipt bindings to remove old commands when the executable moves.
Foreign hooks survive initial setup, repeated setup and uninstall. Tests also cover
legacy shell upgrades without replacing another installation's commands, and
receipt-based replacement after a move.

The regression
`setup_and_repeated_setup_preserve_another_installations_bindings` failed before
the fix: Codex had one Stop group instead of the expected two. It passes afterward
for both Codex and Claude. Tests execute the binary and inspect resulting JSON;
they do not assert Rust source text.

## Environment

- Windows 11, OS version reported by .NET: `10.0.22621.0`.
- Rust `1.98.1 (48a229cea 2026-09-01)`, MSVC x64 target.
- aHerald `0.8.0-beta`.
- Codex CLI `0.166.0-alpha.5.1`, exercised by the optional launch probe.
- Claude Code `2.1.316` installed; version queried only, no live Claude delivery
  acceptance in this run.
- Inno Setup `7.1.1`.

## Automated results

| Command | Observed result |
| --- | --- |
| `cargo test --test setup` | 26 passed, one ignored Git Bash check |
| `cargo fmt --all -- --check` | Passed |
| `cargo clippy --all-targets -- -D warnings` | Passed |
| `cargo build --release --target x86_64-pc-windows-msvc` | Passed |
| `cargo test --all-targets` | Failed at `native_window_loads_builtin_recovers_custom_failures_and_keeps_text_without_assets` in `tests/native_assets.rs:76`: `notification startup deadline exceeded` |
| `check-release.ps1` | x64 Windows GUI executable, system DLL imports |
| `package-release.ps1` | ZIP built; clean extraction ran `--version` as a non-administrator |
| `build-installer.ps1` | Normal setup EXE and SHA-256 built |
| `build-installer.ps1 -InstallerAppId aHerald.Native.InstallerTest` | Separate test setup EXE built |
| `tests/installer_windows.ps1 -Installer dist/aherald-v0.8.0-beta-windows-x64-setup-test.exe` | Passed isolated installer lifecycle |

The full Cargo run failed at the same native startup timeout both inside and
outside the sandbox. Changing execution context did not resolve it. No cause or
baseline classification is established, and tests after the failing integration
binary were not executed by that command. Logs are in
`target/ah097-cargo-test.log` and `target/ah097-cargo-test-native.log`.

The installer lifecycle verified malformed-input preflight, hook/settings
installation, backups, statusLine wrapping/restoration, live configuration
preservation, warning thresholds, selection changes, update of a locked EXE after
graceful shutdown, refusal to replace files when shutdown does not finish,
preservation of another resident, blocked uninstall with an invalid receipt,
saved-path cleanup after environment changes, and uninstall shutdown. Its
resident shutdown boundary uses a native fixture, not a live harness session.
Evidence directory:
`%TEMP%/aherald-installer-test-a7959a54cd514090bbe8d97dfc310f67`.

## Optional Codex launch probe

```powershell
./tests/live/codex_hook_launch.ps1 `
  -CodexPath (Get-Command codex.exe).Source `
  -AheraldPath "$PWD/target/x86_64-pc-windows-msvc/release/aherald.exe"
```

Passed with a private CODEX_HOME, generated setup binding and closed loopback model
endpoint; no account or billable model request was needed. The recorder received
294 input bytes and exactly `--hook --resident`. Immediate ancestry was
`aherald.exe <- pwsh.exe <- codex.exe`. Real aHerald responses were `{}` with exit
zero and empty stderr in both PowerShell runtimes. Delivery was disabled for this
probe, so these results do not prove notification delivery or dismissal.

Two warmups and nine measured calls per case, alternating direct/old launch order:

| Case | Median ms | Mean ms | Maximum ms |
| --- | ---: | ---: | ---: |
| Native EXE | 18.7 | 20.7 | 34.8 |
| Windows PowerShell 5.1, generated direct command | 390.9 | 430.5 | 704.2 |
| Windows PowerShell 5.1, extra PowerShell process | 746.9 | 769.5 | 1155.5 |
| PowerShell 7, generated direct command | 539.2 | 654.7 | 1020.9 |
| PowerShell 7, extra PowerShell process | 861.9 | 984.0 | 1573.3 |

These are diagnostic command timings including the outer shell startup, not a
portable performance budget or full Codex pipeline measurement. Evidence:
`%TEMP%/aherald-hook-launch-281b0559361e47db80fbf2c90247707a`, and
`target/ah097-codex-hook-launch.log`.

Current official documentation was refreshed: Codex documents the Windows
[`commandWindows` override](https://learn.chatgpt.com/docs/hooks), and Claude
documents direct executable-plus-args
[`exec` hooks](https://code.claude.com/docs/en/hooks#exec-form-and-shell-form).
The pinned Codex release-source links in installation.md could not be fetched by
the web tool in this run; source/schema compatibility was not newly established.
The launch behavior above was observed against the installed target version.

## Remaining acceptance

- Resolve the native startup timeout and complete the required full Cargo check.
- Verify the installer tasks page and upgrade state visually: harness choices,
  usage/warning selections and explanatory text, directory changes, custom
  thresholds, and high-DPI layout.
- Verify the checked launch-after-install option, its unchecked behavior, no launch
  on failed setup, and no launch during silent installation.
- Exercise the installed generated hooks in real Codex and Claude sessions;
  observe notification delivery, independent hook completion and dismissal.
- Run installation, hooks, update and uninstall on a clean supported Windows
  machine without WSL/Bash or development runtimes. The maintainer explicitly
  deferred this check on 2026-11-01 because no machine is available.

Only the license page was visually inspected. The test wizard's input did not
reliably advance it through Computer Use, so full GUI acceptance is unverified.
The owned test wizard was stopped without installing through its GUI. A first test
installer rebuild encountered that wizard's locked EXE; after closing it, the
rebuild and isolated lifecycle passed. No production installation, normal user
hooks, autostart or release publication was changed by this acceptance run.
