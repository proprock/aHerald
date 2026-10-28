# Changelog

All notable changes to aHerald are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and versions follow
[Semantic Versioning](https://semver.org/). Ticket IDs refer to `DONE.md`.

Before tagging a release, rename `Unreleased` to the version and date, for example
`## [0.1.0] - 2026-11-01`; the release workflow publishes that section as the notes.

## [Unreleased]

## [0.4.0] - 2026-10-29

First release for macOS (Apple Silicon) next to Windows 10/11 x64. The macOS build is
ad-hoc signed and not notarized.

### Added
- macOS support (AH-062–AH-066, AH-061): a non-activating floating card on `NSPanel` with fades,
  countdown, artwork, mouse navigation and `--demo`; a menu-bar status item with the tray menu, usage
  and about cards; sounds through AudioQueue; click-to-activate for terminals, Claude Desktop and
  Codex.app; placement that follows display changes (checked with a second display on an M1).
- Claude quota on macOS from the Claude Code login in the Keychain (`[claude.usage]`, off by default;
  the system prompt is never bypassed and the token is never logged, copied or stored).
- `aHerald.app` for Apple Silicon (AH-067): `package-macos.sh`, `check-release-macos.sh`, a bundle
  without a Dock icon whose configuration and log live in `~/Library/Application Support/aHerald`,
  hook scripts and `docs/portable-macos.md`; the release workflow builds the archive.
- `aherald --install-hooks|--uninstall-hooks claude|codex` (AH-091): adds or removes only aHerald's
  hook entries in the agent's own file; a dry run unless `--apply`, with a backup.
- A Claude Code plugin and marketplace in this repository: `claude plugin marketplace add
  proprock/aHerald`, then `claude plugin install aherald@proprock`.
- Claude session names and a token-free quota (AH-059): `aherald --statusline [--then COMMAND]`,
  `--install-statusline` / `--uninstall-statusline`, and `[claude.usage] source = auto|token|statusline`.
- `[tray] style = "bw"` draws the macOS menu-bar quota icon in the system's black or white.
- aHerald notice cards (AH-090): problems the user can fix (an unusable sound or picture, a missing
  monitor, a sound that cannot be played) show once as a card headed "aHerald notice" and are logged,
  instead of failing silently. Refused hook input and dropped events are logged with fixed codes.
- Log timestamps carry the local UTC offset on Unix.
- `https-client` Cargo feature (default on) for the Unix HTTPS client.

### Changed
- The colour stripe on the card is 14 px wide (was 8) and the built-in turn-complete tick is centred
  in its circle (Windows and macOS).
- The resident request handler, the card text, the activation worker, the card surface and the
  artwork composition are shared between Windows and macOS (AH-088); no intended behaviour change.

### Known limitations
- Windows: the wider stripe and the notice card were checked only by tests and CI in this release,
  not by eye, and the missing-monitor notice exists on macOS only; `--install-statusline` and the plugin launcher are macOS-only for now (Windows reports
  "not supported yet"); Windows 10 is still untested (AH-023).
- macOS: not signed with a Developer ID and not notarized, so Gatekeeper warns on a downloaded
  copy; Intel Macs are unsupported; not yet checked in iTerm2 or other terminals, nor with a
  full-screen app on the main display.
- Claude Desktop does not run `statusLine`, so session names and the statusLine quota reach cards
  only for terminal and IDE sessions; the token quota works for Desktop.
- Codex input waits are observed only through hooks; unrelated CLI/app sessions are not visible.
- Linux builds and has IPC, spawn and HTTPS but no overlay, tray or audio yet (AH-068 onward).

## [0.1.0] - 2026-10-27

First release candidate. Windows 10/11 x64 is the supported runtime; the crate also
builds on macOS and Linux behind stub platform modules, where only hook parsing and
configuration work.

### Added
- Detached transient and resident notification modes with a native layered overlay,
  external PNG animations and WAV sounds, and a looping `--demo` preview (AH-001–AH-008, AH-026).
- Resident service: single instance, bounded versioned IPC, event queue with
  deduplication, card switching, tray icon with usage graph, graceful shutdown (AH-009–AH-012, AH-034).
- Codex hooks for completion, permission request and interruption, with manual setup
  examples; lifecycle correlation of pending and terminal states (AH-013–AH-016).
- Read-only Codex quota display through the official `app-server` (AH-017–AH-019).
- Claude Code hooks, optional Claude 5h/7d quota (`[claude.usage]`, off by default)
  and click-to-activate for Claude terminals and Claude Desktop (AH-036, AH-037, AH-055, AH-056).
- Click a card to switch to the agent application (AH-032); local Codex pet packages (AH-024).
- Card redesign: wider event-colour stripe, `<harness>, <folder>` heading, session
  name row, larger event kind, event age, framed `< n/m >` navigation, Dismiss button
  and a fixed card height; configurable `background_color` and `border_color` (AH-035, AH-085).
- `[codex] transient_names` to look up Codex session names in transient mode on request (AH-058).
- Timestamped, levelled operational log with debug performance spans, GUI stall
  detector, animation frame-budget report, resource snapshots and `--perf-report` (AH-077–AH-082, AH-084).
- Placement options (centre, top centre, bottom right, near cursor, selected monitor)
  and stacked simultaneous transient cards; implemented, manual multi-monitor acceptance is pending (AH-020).
- `--version` and a tray **About aHerald** item that shows the version on a usage-style card (AH-086).
- Portable archive: `package-release.ps1` adds `START-HERE.md`, `MANIFEST.txt` with file
  hashes and a clean-extraction check; new `docs/portable.md` guide covering modes, CLI,
  replacing assets, manual hooks and autostart, uninstall and limits (AH-022).
- `src/unsupported.rs` stubs, `src/paths.rs` and a three-OS CI matrix (AH-060).
- Unix common layer for macOS and Linux: a unix-domain-socket IPC with the Windows
  frames (private per-user directory, `flock` single instance, peer user-id check),
  detached child spawn, restart, `open`/`xdg-open`, and an HTTPS GET on `ureq` with
  rustls; none of it is reachable until a Unix overlay exists (AH-061).

### Changed
- The card's shapes and text layout moved into the platform-independent
  `card_paint` module that Windows draws from and macOS will (AH-062).
- `config.toml` is read from beside `aherald.exe`; there is no APPDATA lookup (AH-033).
- Usage sections are nested under their agent (`[codex.usage]`, `[claude.usage]`).
- Cards are drawn at the target monitor's effective DPI (AH-048, AH-076).
- A resident card is composed once instead of about three times, and only the changed
  text is redrawn when the queue changes (AH-083, AH-042, AH-051).
- Animation memory is bounded and decoded sounds and app-server clients are reused (AH-048, AH-049).

### Fixed
- The resident survives card render failures and hook bursts (AH-041, AH-046).
- Stale Claude pending cards are resolved when the session continues (AH-040).
- The Claude `idle_prompt` notification no longer raises a card; the "input required"
  card appears for an idle terminal session (AH-074).
- Configuration errors are reported per field and the tray Restart is guarded (AH-043).

### Security
- The Claude quota request refuses redirects so the bearer token reaches only
  `api.anthropic.com`; the token and headers are zeroed after use (AH-045).
- Hook and IPC input is bounded and treated as untrusted; IPC is limited to the
  current Windows user and logon session.
- Only the last component of the hook's working directory is shown on a card, with
  control and bidirectional characters removed; it is never logged (AH-085).

### Known limitations
- Codex input waits are observed only through hooks; unrelated CLI/app sessions are
  not visible (accepted release limitation).
- Tested on Windows 11 (build 22621) only; this release was **not tested on Windows 10**.
- Mixed-DPI multi-monitor behaviour and the system-proxy check for the Claude quota
  request are not yet verified (AH-020, AH-045).
- macOS and Linux build and have IPC, spawn and HTTPS, but no overlay, tray or audio, so
  the notification service does not run there yet (AH-062 onward). Unix IPC isolates
  by user id only; there is no logon-session equivalent.
