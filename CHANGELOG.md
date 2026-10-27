# Changelog

All notable changes to aHerald are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and versions follow
[Semantic Versioning](https://semver.org/). Ticket IDs refer to `DONE.md`.

Before tagging a release, rename `Unreleased` to the version and date, for example
`## [0.1.0] - 2026-11-01`; the release workflow publishes that section as the notes.

## [Unreleased]

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
- `src/unsupported.rs` stubs, `src/paths.rs` and a three-OS CI matrix (AH-060).

### Changed
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
- Mixed-DPI multi-monitor behaviour, the Windows 10 smoke matrix and the system-proxy
  check for the Claude quota request are not yet verified (AH-020, AH-023, AH-045).
- macOS and Linux build but do not run the notification service yet (AH-061 onward).
