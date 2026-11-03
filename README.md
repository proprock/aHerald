<div align="center">

# aHerald

**A native desktop notification companion for AI coding agents.**
Know the moment your agent finishes, fails, or needs you, without watching the terminal.

[![CI](https://github.com/proprock/aHerald/actions/workflows/ci.yml/badge.svg)](https://github.com/proprock/aHerald/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/proprock/aHerald?sort=semver)](https://github.com/proprock/aHerald/releases/latest)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
![Platforms](https://img.shields.io/badge/platform-Windows%20%7C%20macOS%20%7C%20Linux-lightgrey)
![Rust](https://img.shields.io/badge/built%20with-Rust-orange?logo=rust)

</div>

aHerald is a single portable native program (`aherald` / `aherald.exe`): no Electron, no web view, no
runtime to install. Agents call it from their hooks; it shows a small animated card, plays a sound,
and keeps a queue so nothing is lost while you are away.

## Features

- **Codex and Claude Code** through their hooks, plus **OpenCode, Pi, Goose and Crush** through one
  generic input, `aherald --notify` ([other harnesses](docs/harnesses.md)).
- **Five signals:** turn complete, permission required, input required, error, interrupted. Waiting
  cards stay until you act; dismissing a card never answers the agent.
- **Resident mode:** one card with a priority queue (64 entries), `<` `>` navigation, event counter,
  and a native tray (pause, sound and pop-up switches, usage, edit config).
- **Pets and artwork:** builtin animations, Codex `pet.json` pets per harness (`[pets]`), or your own
  frames and WAV sounds. Everything is external and replaceable; missing files fall back to text.
- **Usage and warnings (opt-in):** Codex and Claude quota in the tray icon and a card, plus context-size
  and idle reminders per agent.
- **Click to activate:** a click can bring the originating terminal or Claude Desktop session forward;
  cards are suppressed while that session is focused.
- **Native everywhere:** Win32 on Windows 10/11 x64, AppKit on Apple Silicon macOS, and an X11/XWayland
  overlay with a StatusNotifierItem tray on Linux. Identical behaviour on all three.
- **Private by design:** no network listener, IPC limited to your user, payloads treated as untrusted
  and never logged, no tokens read except the two opt-in usage features. See [limits](#-limits).

## Install

| OS | Get it |
|---|---|
| **Windows** | Per-user setup EXE (Codex / Claude Code checkboxes) or the portable ZIP from [Releases](https://github.com/proprock/aHerald/releases/latest) |
| **macOS** (Apple Silicon) | `brew tap proprock/tap && brew install --cask aherald`, or the disk image (ad-hoc signed, see below) |
| **Linux** (x86-64, glibc) | `.deb` (`sudo apt install ./aherald-*.deb`) or the `.tar.gz` portable archive |

The macOS app is ad-hoc signed, not notarized, so Gatekeeper blocks its first start. Either open it once
and choose **Open Anyway** in System Settings, Privacy & Security (macOS 15 and later; right-click, Open no
longer works), or remove the quarantine mark yourself, which is quicker after `brew install`:

```sh
xattr -dr com.apple.quarantine /Applications/aHerald.app
```

Then connect your agent. The program writes hook entries only when you ask, with a dry run first:

```sh
aherald --install-hooks claude --apply            # or codex
aherald --setup --harness opencode --harness pi   # bridge files for other harnesses (dry run without --apply)
```

Details: [installation and removal](docs/installation.md) · [portable guide](docs/portable.md) ·
[macOS](docs/portable-macos.md) · [Linux](docs/portable-linux.md) · [Claude Code plugin](docs/claude-hooks.md).

## Try it

```sh
aherald --demo                                   # loops through every signal
aherald --resident --event permission_required   # starts the resident and shows one card
aherald --status                                 # PID, queue, switches
aherald --quit
```

Settings live in `config.toml` beside the executable ([`aHerald.example.toml`](aHerald.example.toml) lists every
option; see [configuration](docs/configuration.md)). Nothing is installed or started automatically.

## Support at a glance

| Harness | Mechanism | Signals |
|---|---|---|
| Codex | hooks (`--hook`) | complete, permission, interrupted, usage, warnings |
| Claude Code | hooks, status line, plugin | complete, permission, input, error, usage, warnings |
| OpenCode, Pi | bridge file (`--setup --harness`) | complete, error, interrupted, permission / prompts |
| Goose | hook file (`--setup --harness`) | complete |
| Crush | `crushrc` lines (manual) | complete, error, interrupted, permission |

Coverage and what was verified live per version: [harness compatibility](docs/harness-compatibility.md),
[Codex hooks](docs/codex-hooks.md).

## Limits

- Only sessions that report through a hook, bridge or `--notify` are visible; unrelated Codex CLI or app
  sessions are not observed.
- Native Wayland is not available (XWayland is used). On GNOME the tray needs an AppIndicator extension.
- macOS builds are ad-hoc signed, not notarized; Apple Silicon only.
- Mixed-DPI and multi-monitor setups have limited manual verification.

## Development

```sh
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
cargo build --release
```

Stable Rust; Windows builds use the MSVC toolchain. The [test runner guide](docs/ah-109-nextest.md), the
opt-in [live harness checks](tests/live/README.md), [packaging scripts](docs/portable.md) and the
[task list](TODO.md) cover the rest. Release history: [CHANGELOG](CHANGELOG.md).

## License

MIT, see [LICENSE](LICENSE). Sample artwork and sounds: [provenance](assets/README.md).
