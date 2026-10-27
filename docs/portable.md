# aHerald portable guide

aHerald shows a small card when a coding agent (Codex, Claude Code) finishes a turn,
needs permission or input, fails or is interrupted. It is one native Windows 10/11 x64
program: `aherald.exe`. Nothing is installed and no administrator rights are needed.
No .NET, Node, Python, browser engine or C++ redistributable is required; the C runtime
is linked in and only Windows system DLLs are used.

In the release archive this file is also `START-HERE.md`.

## What is in the archive

| Path | Purpose |
| --- | --- |
| `aherald.exe` | The program. |
| `aHerald.example.toml` | Complete configuration reference with the built-in defaults. |
| `assets/` | Default animations (`sheet.png` + `animation.toml`) and sounds (`sound.wav`), one folder per event. Keep it beside `aherald.exe`. |
| `examples/` | Hook examples for Codex and Claude Code and demo presets. |
| `docs/` | Detailed documentation; `docs/configuration.md` is the best next read. |
| `MANIFEST.txt` | Version and SHA-256 of every file. |
| `LICENSE`, `README.md`, `CHANGELOG.md` | MIT license, overview, release notes. |

The archive also carries a `.sha256` file next to it; compare it with
`Get-FileHash aherald-*.zip -Algorithm SHA256`.

## First run

1. Extract the archive to any writable folder, for example `C:\Tools\aherald`. A folder
   such as `C:\Program Files` is read-only for normal users, so aHerald could not create
   its configuration or log there.
2. Try it without any agent:

   ```powershell
   .\aherald.exe --demo
   ```

   Cards for every event loop on screen. Exit through the tray icon (it may be under the
   hidden icons).
3. Show one real card: `.\aherald.exe --event turn_complete`.

The first start creates `config.toml` beside `aherald.exe` with the defaults. aHerald never
looks in `%APPDATA%`. If you move the program, move `config.toml` and `assets\` with it.

## Modes

- **Transient** (default): every event starts a short-lived process that shows one card and
  exits. Simple, nothing stays running. Several simultaneous cards are stacked.
- **Resident**: one background process keeps a queue of events, a tray icon, `< n/m >`
  navigation between cards, and the optional quota display. The first event starts it
  automatically; `--resident` starts it without an event.

Pick the mode with `[general] mode = "transient"` or `"resident"` in `config.toml`, or per
run with `--transient` / `--resident`. A running resident keeps the configuration it
started with; restart it from the tray (or `--quit`, then start again) after editing.

## Command line

```text
aherald [--hook | --event EVENT | --demo [SECONDS] | --status | --quit | --perf-report | --version]
        [--resident | --transient] [--config PATH]
```

| Option | Meaning |
| --- | --- |
| `--hook` | Read an agent hook payload from stdin (used by hook configuration). |
| `--event EVENT` | Show a card by hand. Events: `turn_complete`, `permission_required`, `input_required`, `error`, `interrupted`. |
| `--demo [SECONDS]` | Loop through all events to preview your configuration. |
| `--resident` / `--transient` | Override the configured mode. `--resident` alone runs the service. |
| `--status` / `--quit` | Report on or stop the running resident of the current user. They never start one. |
| `--version` | Print `aHerald <version>`. |
| `--perf-report` | Rank timings that a resident with `logging = "debug"` wrote to `aherald.log`. |
| `--config PATH` | Use another configuration file; relative paths inside it resolve from its folder. |

`aherald.exe` is a GUI-subsystem program: no console window opens, and in PowerShell its
output appears only when piped, for example `.\aherald.exe --status | Out-String`.

## Configuration

Edit `config.toml` (tray: **Edit configuration**). Every setting, accepted value and default
is commented in `aHerald.example.toml`; see also [configuration](configuration.md). Unknown or
invalid fields are reported per field and the card still appears with defaults where possible.
Useful settings: `[general] mode`, `[notifications] position`, `monitor`, `scale`,
`background_color`, per-event `[events.NAME]` sections (`duration_ms`, `sound_enabled`,
colour), `[sounds] volume`, `[codex.usage]` and `[claude.usage]` (quota display, both off by
default).

## Replacing artwork and sounds

Each event folder under `assets\` holds `animation.toml`, a PNG sprite sheet or frame
sequence, and a WAV. Edit the files in place or point an event at your own folder:

```toml
[events.turn_complete]
visual = "custom"
animation = "my-assets/done"      # folder with animation.toml + PNG, relative to config.toml
sound = "my-assets/done.wav"      # PCM 16-bit WAV, at most 5 s and 2 MiB
```

Formats and limits: [assets](assets.md); Codex pet packages: [pets](pets.md); source
selection: [visuals](visuals.md). A missing or broken file falls back to the built-in
animation, then to text; a missing sound leaves the card silent. Restart a resident after
changing files.

## Connecting an agent (manual)

aHerald never edits agent configuration, installs hooks or adds itself to startup.

- **Codex**: [docs/codex-hooks.md](codex-hooks.md) and `examples/hooks.windows.json`. Replace the
  executable path, then review and trust the hooks in Codex (`/hooks`).
- **Claude Code**: [docs/claude-hooks.md](claude-hooks.md) and `examples/claude-settings.json`.

## Start with Windows (manual)

Press `Win+R`, run `shell:startup`, and create a shortcut there with the target
`C:\Tools\aherald\aherald.exe --resident` and "Start in" `C:\Tools\aherald`. Delete the
shortcut to undo it. Hooks start a resident on demand, so this is optional.

## Uninstall

1. `aherald.exe --quit` (or tray **Exit**).
2. Remove the hook entries you added to Codex / Claude Code, and the Startup shortcut if any.
3. Delete the extracted folder. aHerald stores nothing elsewhere: `config.toml`,
   `aherald.log` and `aherald.previous.log` live beside the executable.

## Build from source

Stable Rust, MSVC toolchain and the Windows SDK:

```powershell
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
cargo build --release --target x86_64-pc-windows-msvc
.\check-release.ps1      # needs llvm-readobj: x64, GUI subsystem, no separate C runtime
.\package-release.ps1    # builds dist\aherald-vX.Y.Z-windows-x64.zip and proves a clean extraction runs
```

## Limits to know

- Windows 10/11 x64 only. macOS and Linux build but show no cards yet.
- Events come from agent hooks. Codex sessions that run no hook (unrelated CLI or app
  sessions) are not visible: [input waits](codex-input-waits.md).
- Dismissing a card never answers an agent request, and aHerald never approves anything.
- Tested on Windows 11 (build 22621). Windows 10, mixed-DPI and multi-monitor setups are
  not yet verified; see the release notes and [CHANGELOG](../CHANGELOG.md).
- Claude quota display reads the Claude Code sign-in token from its credentials file only
  when `[claude.usage] enabled = true`, and sends it only to `api.anthropic.com`.
