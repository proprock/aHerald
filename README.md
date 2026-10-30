# aHerald

aHerald is a portable native Windows x64 notification companion. It provides
detached or resident notifications, external animation/WAV resources, validated
TOML settings, and bounded Codex hook input. Resident mode adds one event card,
a priority queue, local IPC and a native tray. It runs without Codex installed.

Platforms: Windows 10/11 x64 is the supported runtime. macOS has a native port; Linux
has an X11/XWayland notification overlay, a StatusNotifierItem tray, PulseAudio-compatible
sound and click-to-activate. Linux requires a compositing X11 session and the
Cairo, Pango, PangoCairo, GObject and libpulse-simple runtime libraries. PipeWire users need
its PulseAudio-compatible server. A native Wayland overlay is not yet available.

For a Linux preview, build with `cargo build`, run `target/debug/aherald --resident`,
and send `target/debug/aherald --event input_required --resident` from another terminal.
Click the card to dismiss it and use `target/debug/aherald --quit` to stop the service.
On GNOME, the tray requires the AppIndicator extension (or another StatusNotifierItem host).
The tray menu offers sound, pop-up and pause switches, configuration, restart and exit.
For terminal hooks, Linux records a window only when its X11 process is in the hook's
ancestor chain; if several windows belong to that process, the active one is selected.
Some terminal launchers do not expose a matching process or X11 window, so their window
cannot be identified automatically. Activation raises the window; it does not restore a
previously selected Warp tab or pane.

On macOS (Apple Silicon) see the [macOS guide](docs/portable-macos.md); build the app with
`./package-macos.sh` after `cargo build --release --target aarch64-apple-darwin`.
On Linux see the [Linux guide](docs/portable-linux.md); build the archive with
`./package-linux.sh` after `cargo build --release`.

New to aHerald? Start with the [portable guide](docs/portable.md): archive contents,
first run, modes, command line, replacing artwork, manual hooks and autostart, uninstall.

## Claude Code plugin (macOS)

This repository is also a Claude Code plugin marketplace. With `aHerald.app` installed:

```bash
claude plugin marketplace add proprock/aHerald
claude plugin install aherald@proprock
```

The plugin registers the hooks and finds the app itself (`/Applications`, `~/Applications`, Spotlight,
or `AHERALD_APP`); it exits quietly when the app is missing. Alternatively
`aherald --install-hooks claude --apply` writes the hooks into `~/.claude/settings.json`; use one of the
two, not both. Codex uses `aherald --install-hooks codex`. See [the macOS guide](docs/portable-macos.md).

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

Developer utilities live in the repository root:

- `build.rs` automatically copies bundled assets beside the executable during Cargo builds.
- `generate_assets.rs` regenerates bundled PNG/WAV assets and animation settings with
  `cargo run --example generate_assets`, overwriting the corresponding files in `assets/`.
- `check-release.ps1` inspects the release executable's architecture, GUI subsystem,
  and CRT imports. Run `.\check-release.ps1` after building; it requires `llvm-readobj`.
- `package-release.ps1` builds `dist/aherald-vX.Y.Z-windows-x64.zip` (with `.sha256`) and
  proves that a fresh extraction runs; the release workflow uses the same script.

## Run without Codex

```powershell
.\target\x86_64-pc-windows-msvc\release\aherald.exe --event turn_complete --config .\examples\resources.toml
.\target\x86_64-pc-windows-msvc\release\aherald.exe --event permission_required --config .\examples\resources.toml
```

Click the notification to dismiss it. `turn_complete`, `error`, and `interrupted`
expire after three seconds by default. `permission_required` and `input_required`
remain until clicked; dismissal never answers an agent request. Standard animations
load automatically from `assets` beside `aherald.exe`; Cargo stages that directory
when building. Keep it beside the executable when distributing or moving the app.
Failed pet/custom artwork falls back to the event's standard animation, then text
if that resource is also unavailable. A missing sound/audio device leaves visuals usable.

[`aHerald.example.toml`](aHerald.example.toml) is the complete configuration reference,
with built-in defaults, comments for every setting, accepted values and optional
artwork/state overrides. Copy it and edit it for your setup. `examples/resources.toml`
is a separate demo preset with scale 3 and configured sounds, not the defaults.
See [the examples directory guide](examples/README.md) for the remaining files.
See [artwork selection](docs/visuals.md) for builtin, full-pet and hybrid setups,
including migration of existing custom animation paths.
Paths are relative to the configuration,
not the working directory. Without `--config`, aHerald uses `config.toml` beside
`aherald.exe`, regardless of the working directory. An absent default file is
created with the standard settings; an existing file is preserved. An explicit
missing/invalid `--config PATH` is an error. No APPDATA lookup or automatic migration
is performed. See [configuration and manual migration](docs/configuration.md).
No hooks or startup entries are installed automatically.

Explicit `--transient` or `--resident` overrides configured mode. Input-wait
monitoring is unsupported for unrelated Codex sessions; usage polling awaits
Milestone 4. Enabling those settings reports limits without stopping notifications.

Cards show `<harness>, <folder>` on the first row (the folder is the last component of
the hook's working directory) and the Codex thread name on the second row.
Names are read in the background through the official metadata-only `thread/read`
API; hooks and the window do not wait for that request. While loading, or when no
stored name is available, the name row stays empty; a session ID is never shown.
The automatic sidebar caption is not always a stored thread name.
Titles refresh after 60 seconds while displayed;
the notification timer and animation continue unchanged. Long names are ellipsized.
See [session source labels](docs/session-source.md) for protocol verification and limits.

## Configuration demo

```powershell
.\target\x86_64-pc-windows-msvc\release\aherald.exe --demo
.\target\x86_64-pc-windows-msvc\release\aherald.exe --demo 3 --config .\examples\resources.toml
```

`--demo [SECONDS]` loops through all enabled signals, then a group of signals for
trying navigation and the event count. The pause **between** scenarios is
three seconds by default; whole seconds from 1 to 86400 are accepted. Each scenario
lasts at least three seconds, or the longest configured event duration plus fade-out.
Persistent waits automatically clear at the end of their scenario. Finite events
still expire at their configured duration; short events may leave the card hidden
for the rest of that scenario. Click dismissal does not end the loop.

The demo loads the default configuration or `--config`, retaining event enablement,
artwork overrides, sounds/volume, fades, placement and scale. Cards are labelled
`aHerald demo`; artwork uses `agents.demo`, then `agents.codex` when available,
then the shared pet. Disabled events are skipped; disabling all events is an error.
Disabled visuals or sounds stay disabled. Use the tray to try pause, sound/visual
switches and usage details (real usage is queried only when enabled in the config).
Exit from this demo's tray to stop the cycle. Pause freezes the cycle as well.

The demo runs independently of the normal resident, without resident IPC or hook
installation, and does not save runtime switches. `--status`/`--quit` address the
normal resident; stop the demo through its tray. `--demo` cannot be combined with
other operations or `--resident`/`--transient`. Restart it after editing configuration
or replacing assets to preview the changes.
See [demo verification and limitations](docs/demo.md).

## Resident mode and tray

```powershell
.\target\x86_64-pc-windows-msvc\release\aherald.exe --resident --event permission_required --config .\examples\resources.toml
.\target\x86_64-pc-windows-msvc\release\aherald.exe --resident --event input_required --config .\examples\resources.toml
.\target\x86_64-pc-windows-msvc\release\aherald.exe --status
.\target\x86_64-pc-windows-msvc\release\aherald.exe --quit
.\target\x86_64-pc-windows-msvc\release\aherald.exe --version
```

`--version` prints `aHerald <version>` and needs no configuration. The tray's
**aHerald v<version>** item (above Sound) opens a card in the usage-card style with the same version;
choosing it again closes the card.

An event starts a detached resident when needed. `--resident` without an event
runs the service directly. Set `[general] mode = "resident"` to use it for hooks
as well. The first resident's configuration remains in effect until it exits;
subsequent deliveries do not reload or replace it. No autostart is installed.
`--status` and `--quit` address the current user's current logon and do not start
an absent service. Status reports PID, queued count, pause, sound and visual state.

With multiple events, the card shows the selected number/count and bottom-right
`<` / `>` buttons. These controls disappear for a single event. Clicking anywhere
else dismisses only the selected event; navigation clicks keep it queued.
With `click_to_dismiss = false`, only the explicit Dismiss button closes it by mouse.
Cards are informational: dismissal never answers an agent request. Finite events
show remaining whole seconds at the top right; persistent waits have no countdown.
Set `[events.NAME] color = "#RRGGBB"` to change an event's left accent stripe.
Set `[notifications] background_color = "#RRGGBB"` for all card backgrounds
(default `#1C232D`). Optional `[events.NAME] background_color` and `[codex.usage]
background_color` override it for individual card types. `border_color` (same three
places, default absent) outlines the card and frames its buttons; the stripe colour is
`color` in each `[events.NAME]`.
See [pet state overrides](docs/pets.md#per-event-state-overrides) for animation settings.
Persistent waits can be switched away from. The queue holds 64 entries, ordered by priority and then arrival.
Higher-priority overflow evicts the oldest lowest-priority entry; other overflow
is rejected. Matching event identity is deduplicated without merging unrelated
sessions; visible time accumulates only while an event is selected.

Click or right-click the aHerald tray icon (possibly under hidden icons) for usage
summary, sound, visual notifications, pause, configuration editing, restart and
exit. Clicking the usage summary toggles a persistent card in the notification queue. Enable the read-only Codex usage connection with `[codex.usage] enabled = true`
in the resident configuration. The tray icon has two columns: primary/5h on the
left and secondary/week on the right. Fill follows `[codex.usage] display` (`remain` or
`used`); colors always describe remaining quota: above 60% green, above 20% yellow,
otherwise red. Missing values are gray. The usage card lists every available
bucket, workspace/reset credits and reset times in the Windows local timezone.
`refresh_interval_sec` defaults to 300. Last received values stay current through
delayed or failed refreshes; update age and errors remain in the card details.
Errors back off from 10 to 300 seconds. Notifications continue during outages.
See [tray usage behavior and verification](docs/tray-usage.md).
Codex must already be installed on PATH and signed in through its own supported
flow. aHerald first searches absolute PATH directories for `codex.exe`; if absent,
it finds the native x64 binary inside an existing npm `@openai/codex` installation
beside a `codex.cmd` shim on PATH. Both nested and hoisted platform packages and
the bundled vendor layout are supported. No Codex copy, installation, Node process,
or shim execution is needed. Restart aHerald after changing its inherited PATH.
aHerald never opens a login flow or reads tokens. Usage is resident-only,
disabled by default, and does not observe unrelated sessions. See
[Milestone 4 evidence and limits](docs/milestone-4.md).
Pause hides the card, cancels audio and freezes display time; incoming events
continue to be validated and queued. Matching verified Codex Stop/Interrupt
boundaries clear pending cards even while paused. Runtime switches are not saved
to TOML. Muted/skipped sounds are not
replayed on resume; at most four pending audio jobs are retained and playback is
serialized. Exit also closes open information windows and releases owned resources.
The icon is restored on `TaskbarCreated` and retried if Explorer is absent.

Resident artwork is cached by agent/signal in a 16-entry, 32 MiB LRU, including
builtin/text recovery for invalid resources. Restart the resident after replacing assets.
The IPC worker writes fixed operational codes to `aherald.log` beside the config,
controlled by `general.logging` (default `warn`; `off` disables it). It keeps one
64 KiB file and one 64 KiB backup, `aherald.previous.log` (512 KiB each at `debug`).
Lines look like `2026-10-27T14:03:22.481+03:00 WARN  ipc.failure`: local time,
severity, a fixed event name and numbers, without payloads, paths, credentials or
session IDs. At `debug` the log also gets aggregated timings of the hot paths
(`perf frame.render n=120 avg=0.41ms … p95=0.52ms max=1.90ms`), cache counters and a
once-a-minute resource line; `aherald --perf-report` ranks them by total time.
An unwritable log directory leaves notification delivery working.
See [resource limits and recovery evidence](docs/milestone-5.md).

Placement supports `center`, `top_center`, `bottom_right` and `near_cursor` in the
work area. `notifications.monitor` accepts `primary` or a zero-based numeric
string (primary first, then Windows device-name order); an unavailable index
falls back to primary. Numeric selection takes precedence over the cursor;
`primary` plus `near_cursor` uses the cursor's monitor. Cards fit the work area
and recalculate on display/DPI/work-area changes. Mixed-DPI and physical monitor
removal remain unverified; see the Milestone 5 evidence.

## Codex pet artwork

The card always shows the event source and signal text. Use `agents.codex.pet`
to point to a downloaded local `pet.json`; its PNG/WebP sprite sheet animates beside
the text. `notifications.pet` supplies an optional shared default. Standard v1/v2 packages are
supported. See [pet setup and format limits](docs/pets.md) and `examples/pet.toml`.
In auto mode, configuring a pet replaces builtin artwork for all inherited events.
Use an event's `visual` for explicit builtin/pet/custom exceptions; `animation` is
used only with `visual = 'custom'`. No installer commands run from
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
a complete Explorer process restart has not been performed. Numbered-monitor
selection is implemented, with physical multi-monitor acceptance still pending.
Each transient event owns its own overlay.

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
