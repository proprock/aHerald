# Changelog

All notable changes to aHerald are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and versions follow
[Semantic Versioning](https://semver.org/). Ticket IDs refer to `DONE.md`.

Before tagging a release, rename `Unreleased` to the version and date, for example
`## [0.1.0] - 2026-11-01`; the release workflow publishes that section as the notes.

## [Unreleased]

## [0.8.0-beta] - 2026-11-01

### Added
- Codex input-context warnings (AH-100), using the completed main-session
  `PostModelRequest` input-token report. Both agents support independent thresholds
  in tokens or decimal kilotokens, warn once strictly above the threshold, and rearm
  at or below it. Unknown readings preserve the crossing state.
- Windows installer controls for each agent's context and idle warnings. Existing
  thresholds are preserved; first context enablement uses Codex 250k / Claude 400k
  when no threshold exists. Idle defaults remain Codex 25 / Claude 55 minutes.
  Separate persisted enablement flags retain thresholds while warnings are disabled.
- Transactional Claude statusLine wrapper setup for context warnings, preserving
  the original command/options and restoring only unchanged owned bindings.
- Windows tray Context warnings / Idle warnings runtime switches, each controlling
  both agents without changing saved thresholds; switches reset on resident restart.
- Windows tray Apply and Apply + Test commands. Both validate the saved configuration
  before restarting; an invalid file leaves the current resident running. Apply + Test
  previews all seven card formats in the shared window, marked [Test], with navigation
  and configured sounds. Dismissal or a real notification ends the preview.

### Changed
- Context warning text includes the measured input context and configured threshold;
  idle warning text includes elapsed time and its limit. Values are captured when
  the warning is created, and the agent name appears only in the heading.
- Development and test profiles generate less debug information.
- Artwork performance checks now include a fixed-scale check with frozen fixtures.
  The resident baseline retains native monitor DPI; its artwork and composition
  limits are calibrated within 10% of observed maxima. See
  docs/perf-baseline-investigation.md for measurements and sensitivity limits.
- Native demo checks share the GUI-test serialization guard to avoid interfering
  with transient placement checks.

### Fixed
- The generated Windows Claude statusLine wrapper preserves input bytes when the
  launching console uses UTF-8 with a BOM, keeping the JSON report valid.

### Verification and known limitations
- Windows beta preflight: formatting and Clippy passed; the full ordinary suite
  passed 498 tests with 16 explicit manual/platform skips. This does not replace
  the manual or live-session acceptance described below.
- AH-100 and AH-101 are accepted and recorded in DONE.md. Real-session context and
  idle delivery was verified on Windows 11 with Claude Code 2.1.311 and Codex CLI
  0.166.0-alpha.5.1 through isolated bridges using production parsing and IPC.
  These checks do not establish end-to-end acceptance of installed hook commands.
- Codex context observation is version-specific; `PostModelRequest` is absent from
  the general hooks guide. Untested versions, desktop/remote sessions, compaction
  dispatch and subagent delivery are not claimed. Context warnings require an
  already running resident; idle reminders do not establish provider cache expiry.
- Native preview verified that the numeric idle label fits. Real-session queue
  delivery does not establish card appearance or focus behavior. Installer
  clean-machine acceptance and other open platform checks remain in TODO.md.
- Recorded warning verification did not establish a green full suite: checks
  encountered perf_baseline, tray_about and native startup failures. See
  docs/ah-100-context-warnings.md and docs/ah-101-windows-acceptance.md for
  historical commands, evidence and coverage limits. Performance calibration and
  the fixed-scale artwork check are included in this beta.

## [0.5.0] - 2026-10-31

Native Windows installation and explicit harness setup, with the first Linux
release alongside Windows x64 and macOS Apple Silicon.

### Added
- Per-user Windows setup EXE alongside the portable ZIP. Select Codex and Claude
  Code integration, independently enable usage display, and launch after installation.
  Upgrades retain the installation directory, harness choices and current usage
  settings. Configuration comments, formatting and unrelated options are preserved.
- Native `--setup --harness codex|claude` and `--unsetup` commands with dry-run
  previews, backups, installation ownership receipts and rollback on write failure.
  The installer requests graceful shutdown of its previous resident and waits for
  process exit before replacing files; another installation is left running.
- Optional live Codex hook compatibility and process-ancestry checks, separate
  from the ordinary test suite, with isolated configuration and bounded cleanup.
- Linux x86_64 GNU archive built on Ubuntu 22.04: X11/XWayland notification overlay,
  StatusNotifierItem tray with usage graph and details, PulseAudio-compatible sound,
  supported agent-window activation, and a Claude Code plugin launcher.
- Resident-only session reminders: Claude input-context thresholds in tokens or
  decimal kilotokens, and independent idle reminders after agent responses. Default
  idle thresholds are Claude 55 minutes and Codex 25 minutes; zero disables them.
  Activity cancels a pending idle reminder and the next completion rearms it.

### Changed
- Native Windows setup binds hooks to a fixed executable path. Codex uses its
  existing PowerShell executor without an additional nested PowerShell launcher;
  Claude uses an executable and argument array. WSL, Bash and PowerShell 7 are not
  required by the installed Windows application or its native setup.
- Windows usage workers notify the GUI when results, errors or pushed updates are
  published. Tray usage no longer waits for the 15-second housekeeping timer;
  updates do not open a notification card. Provider startup and network latency remain.
- Hook delivery failures and artwork scaling fallback receive operational log entries.

### Breaking changes
- Resident IPC now uses version 2. Before replacing a portable version-1 installation,
  stop its resident through the old executable (`--quit`) or its tray menu. The Windows
  installer uses the previous installed executable for graceful shutdown.

### Known limitations
- Windows installer clean-machine acceptance without developer tools, Windows 10,
  and visual/live startup acceptance of the latest usage wake change remain open.
  The installer is unsigned. Native macOS and Linux installers are follow-up work;
  those platforms retain their portable distributions.
- Generated Codex Windows hooks were verified with Codex 0.166.0-alpha.5.1 and its
  default PowerShell executor. Custom non-PowerShell executors are outside that
  verified contract. Codex hook trust still requires the user's normal review.
- Local Windows `perf_baseline::hot_paths_stay_inside_their_baseline_budgets` fails:
  artwork preparation averaged 65 ms against the unchanged 30 ms budget. All other
  executed test targets passed; this performance issue remains unresolved.
- Linux requires composited X11 or XWayland and a StatusNotifierItem host (GNOME
  commonly needs an AppIndicator extension). Native Wayland is not implemented;
  plain Xorg acceptance remains open. Window activation depends on the compositor.
- Session warning live acceptance remains open. Codex context-size warnings and
  actual provider cache-expiry detection are unsupported; idle reminders measure
  elapsed time after responses, without a cache-expiry guarantee.
- macOS remains ad-hoc signed, unnotarized and Apple Silicon only. Claude Desktop
  does not run statusLine; its session-name and statusLine quota coverage is unchanged.

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
