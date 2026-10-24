# aHerald

aHerald is a portable native Windows x64 notification companion. Milestone 1
provides detached visual and sound notifications, external PNG animation/WAV
resources, validated TOML settings, and bounded Codex hook input. It runs without
Codex installed. Resident/tray/IPC operation belongs to Milestone 2.

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

Explicit `--transient` overrides configured mode. `--resident`, `--status`, and
`--quit` report their later-milestone status. Monitoring and usage polling are not
implemented yet; enabling those settings does not stop transient notifications.

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
Stable real-session integration is a later milestone; all five event types can
already be exercised with `--event`.

See [asset formats and sound scheduling](docs/assets.md) and
[verification evidence and limitations](docs/milestone-1.md). Current automated and
manual evidence is Windows 11 build 22621, Rust 1.98.1 MSVC x64. Windows 10,
mixed-DPI/multiple monitors, display changes, and Explorer restart remain untested.
Explicit numbered-monitor selection belongs to AH-020. Each transient event owns
its own overlay; the single resident overlay/event navigation belongs to Milestone 2.

Original sample resources and source are MIT licensed; see [LICENSE](LICENSE)
and [asset provenance](assets/README.md). Regenerate the resources using
`cargo run --example generate_assets`. The ordinary test suite needs no audio device;
opt into a real audio-device check with:

```powershell
cargo test --lib native_device_completes_original_sample -- --ignored --nocapture
```
