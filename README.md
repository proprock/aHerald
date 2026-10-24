# aHerald

aHerald is a portable native Windows x64 notification companion. It provides
detached or resident notifications, external animation/WAV resources, validated
TOML settings, and bounded Codex hook input. Resident mode adds one event card,
a priority queue, local IPC and a native tray. It runs without Codex installed.

## Build and verify

Use stable Rust with the MSVC toolchain and Windows SDK:

```powershell
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
cargo build --release --target x86_64-pc-windows-msvc
```

The executable is `target/x86_64-pc-windows-msvc/release/aherald.exe`. The target
configuration links the MSVC CRT statically; no redistributable, GUI framework,
or language runtime needs to accompany it. Windows system DLLs are still required.
Keep `assets/` external and copy it with the example configuration when desired.

## Run without Codex

```powershell
.\target\x86_64-pc-windows-msvc\release\aherald.exe --event turn_complete --config .\examples\resources.toml
.\target\x86_64-pc-windows-msvc\release\aherald.exe --event permission_required --config .\examples\resources.toml
```

Click the notification to dismiss it. `turn_complete`, `error`, and `interrupted`
expire after three seconds by default. `permission_required` and `input_required`
remain until clicked; dismissal never answers an agent request. A missing animation
uses a text card. A missing sound or audio device leaves visuals usable.

`examples/config.toml` lists settings; `examples/resources.toml` references the
original bundled animations and sounds. Paths are relative to the configuration,
not the working directory. The default config is `%APPDATA%\aHerald\config.toml`;
a missing default file uses built-in defaults. An explicitly missing/invalid file
is an error. No user configuration or startup entries are installed automatically.

Explicit `--transient` or `--resident` overrides configured mode. Input-wait
monitoring is unsupported for unrelated Codex sessions; usage polling awaits
Milestone 4. Enabling those settings reports limits without stopping notifications.

## Resident mode and tray

```powershell
.\target\x86_64-pc-windows-msvc\release\aherald.exe --resident --event permission_required --config .\examples\resources.toml
.\target\x86_64-pc-windows-msvc\release\aherald.exe --resident --event input_required --config .\examples\resources.toml
.\target\x86_64-pc-windows-msvc\release\aherald.exe --status
.\target\x86_64-pc-windows-msvc\release\aherald.exe --quit
```

An event starts a detached resident when needed. `--resident` without an event
runs the service directly. Set `[general] mode = "resident"` to use it for hooks
as well. The first resident's configuration remains in effect until it exits;
subsequent deliveries do not reload or replace it. No autostart is installed.
`--status` and `--quit` address the current user's current logon and do not start
an absent service. Status reports PID, queued count, pause, sound and visual state.

The single card shows the selected event number/count and Prev/Next/Dismiss.
Clicking its body dismisses only that event. Persistent waits can be switched
away from. The queue holds 64 entries, ordered by priority and then arrival.
Higher-priority overflow evicts the oldest lowest-priority entry; other overflow
is rejected. Matching event identity is deduplicated without merging unrelated
sessions; visible time accumulates only while an event is selected.

Click or right-click the aHerald tray icon (possibly under hidden icons) for usage
summary/details/refresh, sound, visual notifications, pause, configuration location
and exit. Enable the read-only Codex usage connection with `[usage] enabled = true`
in the resident configuration. The tray shows primary/secondary usage; Usage
details lists every available bucket, workspace/reset credits and reset times in
the Windows local timezone. `refresh_interval_sec` defaults to 300. Refresh usage
requests an update without waiting on the UI thread. Missing fields remain
unavailable; failed or overdue snapshots are marked stale. Errors back off from
10 to 300 seconds; manual refreshes coalesce and honor backoff (at least five
seconds between successful requests). Notifications continue during outages.
Codex must already be installed on PATH and signed in through its own supported
flow; aHerald never opens a login flow or reads tokens. Usage is resident-only,
disabled by default, and does not observe unrelated sessions. See
[Milestone 4 evidence and limits](docs/milestone-4.md).
Pause hides the card, cancels audio and freezes display time; incoming events
continue to be validated and queued. Matching verified Codex Stop/Interrupt
boundaries clear pending cards even while paused. Runtime switches are not saved
to TOML. Muted/skipped sounds are not
replayed on resume; at most four pending audio jobs are retained and playback is
serialized. Exit also closes open information windows and releases owned resources.
The icon is restored on `TaskbarCreated` and retried if Explorer is absent.

## Codex pet artwork

The card always shows the event source and signal text. Use `agents.codex.pet`
to point to a downloaded local `pet.json`; its PNG/WebP sprite sheet animates beside
the text. `notifications.pet` supplies an optional shared default. Standard v1/v2 packages are
supported. See [pet setup and format limits](docs/pets.md) and `examples/pet.toml`.
A per-event `animation` overrides agent and shared pets. No installer commands run from
configuration; runtime playback needs neither npm nor Codex.

## Hooks and limits

See [verified hook contracts](docs/codex-hooks.md) before manual integration.
`--hook` bounds stdin to 64 KiB and 250 ms, normalizes only verified event fields,
and returns independently of notification lifetime. `Stop` emits `{}`;
`PermissionRequest` and `Interrupt` emit no response. No permission decisions,
transcript parsing, token reading, or automatic Codex installation are performed.
See [manual Windows hook setup](docs/codex-hooks.md) and the
[version/coverage matrix](docs/codex-compatibility.md). Input-wait monitoring for
unrelated CLI/app sessions is [explicitly unsupported](docs/codex-input-waits.md).
All five event types can be exercised with `--event`.

See [asset formats and sound scheduling](docs/assets.md) and
[Milestone 1 evidence](docs/milestone-1.md) and [resident verification](docs/milestone-2.md). Current automated and
manual evidence is Windows 11 build 22621, Rust 1.98.1 MSVC x64. Windows 10,
mixed-DPI/multiple monitors and display changes remain untested. Icon restoration
is tested by deleting the test icon and sending the real `TaskbarCreated` message;
a complete Explorer process restart has not been performed. Explicit numbered-monitor
selection belongs to AH-020. Each transient event owns its own overlay.

Original sample resources and source are MIT licensed; see [LICENSE](LICENSE)
and [asset provenance](assets/README.md). Regenerate the resources using
`cargo run --example generate_assets`. The ordinary test suite needs no audio device;
opt into a real audio-device check with:

```powershell
cargo test --lib native_device_completes_original_sample -- --ignored --nocapture
```

The separate notification-area test requires an interactive Explorer desktop:

```powershell
cargo test --test tray explorer_notification -- --ignored --nocapture
```

It removes/restores only its isolated resident's icon and never restarts Explorer.
For read-only inspection of a manually started resident's window classes/titles:

```powershell
cargo test --test tray manual_resident -- --ignored --nocapture
```
