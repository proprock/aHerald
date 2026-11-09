# Changelog

All notable changes to aHerald are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and versions follow
[Semantic Versioning](https://semver.org/). Ticket IDs refer to `DONE.md`.

Before tagging a release, rename `Unreleased` to the version and date, for example
`## [0.1.0] - 2026-11-01`, and add its compare link at the bottom; the release workflow
publishes that section as the notes. The full procedure is in `RELEASE.md`.

## [Unreleased]

### Added
- The setup window and the Linux console setup offer the Claude Code status line wrapper as its
  own choice (`--claude-statusline true|false`; `--setup-settings` reports `claude_statusline`).
  A choice now switches on what it needs (a warning its agent's hooks, the Claude context warning
  the status line, usage, warnings and the status line a resident), and clearing the hooks clears
  what depended on them. The macOS window gains a **Done** button that unlocks after Apply (AH-297).

## [0.17.0] - 2026-11-09

### Added
- Messenger sinks can name the computer: `include_host = true` (off by default, per sink) puts the
  computer's name, or its IP address if the name cannot be read, into the message, and `{host}` is
  available in a webhook body and a command. It is recorded in `docs/privacy.md` (AH-285).

### Changed
- The Claude Code plugin is installed from the shared `proprock` marketplace
  (`claude plugin marketplace add https://github.com/proprock/marketplace.git`, then
  `claude plugin install aherald@proprock`). This repository no longer carries its own marketplace
  named `proprock`: two sources cannot share that name, and adding one dropped the other's plugin.
  If you added `proprock/aHerald` as a marketplace, run `claude plugin marketplace remove proprock`
  and add the new one. A stable release now updates the `aherald` pin in that catalog by itself
  (AH-293).
- A forwarded message with `include_folder` or `include_host` now starts with a header line
  (`host/folder`, `host` or `folder`) and a blank line, then `Claude: input required`; the folder no
  longer stands in brackets after the harness name. Without either setting the message is unchanged
  (AH-285).

## [0.16.0] - 2026-11-06

### Added
- A rolling `nightly` prerelease: a green nightly run on `master` replaces it with fresh builds for every platform, and a night without changes builds nothing. See `docs/nightly.md`.
- A webhook that answers HTTP 429 or 503 with `Retry-After` is retried after that delay (at most 30
  seconds) instead of the fixed two seconds (AH-245).

### Changed
- The README now shows Linux demo cards and a short animation; remaining screenshots are tracked in AH-281 in `TODO.md`.
- Codex session name lookups are now off by default; set `[codex] session_names = true` to enable them.
  Transient cards also require `transient_names = true`. See AH-267 in `TODO.md` for remaining acceptance.
- `--install-hooks`, `--uninstall-hooks`, `--install-statusline` and `--uninstall-statusline` now go through the
  same setup backend as `--setup` on every system: they write a receipt, change only the named harness,
  write hooks in the exec form and roll back a failed write. Entries written by the older command are
  replaced once; `--uninstall-hooks` removes only entries of the running executable, no longer another
  installation's (AH-265).
- `aherald` parses its internal child modes in the command-line parser and runs each operation in its own
  function; no change for users (AH-264).
- The Linux resident no longer wakes 60 times a second while idle: the loop sleeps until an X11 event, a
  request, a tray command or new usage numbers arrive (AH-231).
- `aherald --help` is grouped by purpose and lists every public flag, including `--notify`, `--kind`,
  `--activity`, `--ended`, `--agent`, `--setup-window` and `--setup-executable` (AH-238).
- Releases skip the same noisy timing and memory tests as pull requests; the nightly run keeps them
  (AH-251).
- The minimum-compiler build runs on Windows, macOS and Linux, not only Windows (AH-248).
- CI and release builds fix the build time to the commit time, so two builds of one commit report the
  same time in `--version` and About (AH-252).
- The About card, the Debian package and the crate describe aHerald as a companion for AI coding
  agents on Windows, macOS and Linux, no longer for Codex and Claude Code on Windows first (AH-235).

### Fixed
- A refused Apply (a configuration that cannot be loaded) says the same thing and is logged the same way
  on Windows, macOS and Linux; Linux did not log it.
- On macOS the idle resident keeps refreshing the tray when only Claude usage is enabled, as on Windows
  (see TODO.md, AH-232 for the check on a Mac).
- `--install-hooks`, `--uninstall-hooks` and the statusLine commands now write to `CLAUDE_CONFIG_DIR` and
  `CODEX_HOME` when they are set, like `--setup`; they used the home directory only (AH-234).
- The Claude plugin manifest reported version 0.1.0; it now carries the version of the program, and a
  test keeps it there (AH-236).

## [0.15.0] - 2026-11-05

### Added
- `[remote]`: an opt-in listener (off by default) that takes hook events from agents in containers
  and virtual machines over HTTP with a token, and `aherald --remote-info` to print the endpoint and
  token (AH-224). Guests send with `examples/sandbox/aherald-ping.sh` (shell and `curl`; the Claude
  plugin launcher does too when `AHERALD_ENDPOINT` is set) or the new `aherald-ping` program, a
  separate crate in a new Cargo workspace that sends only what aHerald reads (AH-225, AH-226).
  Releases attach static `aherald-ping` binaries for Linux x86_64 and aarch64 (musl) with checksums
  and provenance; recipes in `docs/sandboxes.md` (AH-227, AH-228).
- Linux X11: a card and its sound are suppressed while you look at the session behind it, as on
  Windows and macOS, when the Claude session runs in a one-session terminal (xterm, urxvt, rxvt, st,
  mlterm, aterm, eterm, alacritty) whose window is the active one and owns no other window. Tabbed
  terminals such as GNOME Terminal, tmux and Wayland sessions are always delivered (AH-222).

## [0.14.0] - 2026-11-05

### Added
- The Windows installer, the macOS setup window and the Linux console setup now offer the OpenCode,
  Pi and Goose bridge files beside Codex and Claude Code. They start unchecked, show an installed
  bridge as checked and apply the whole selection (clearing a box removes that bridge, kept as
  `*.aherald-backup`). `--setup-settings` reports `opencode_bridge`, `pi_bridge` and `goose_bridge`
  (AH-206).
- A `command` forward sink runs a program for each message, for services without an HTTP endpoint:
  mail through `msmtp` or `mutt`, `signal-cli`, `notify-send` over `ssh`, `say`. `command` is an
  array (the program and its arguments), so no shell reads a folder name; `stdin` is optional text
  for its standard input; `${NAME}` secrets and `{text}`, `{kind}`, `{harness}`, `{folder}` work as
  in a webhook. A failing, missing or over-10-second command is one error card with its exit
  status, never its output (AH-220).
- The **Messenger** switch is now in the macOS and Linux tray menus too, beside Sound and Pop-up
  and only when a sink is configured (AH-219).

## [0.13.0] - 2026-11-05

### Added
- Opt-in messenger forwarding: `[forward]` and `[[forward.sinks]]` send each new card (kind,
  harness and, only if allowed, the folder name) with a webhook POST, which serves Telegram, ntfy,
  Slack and others. `${NAME}` in the `url` or `body` is replaced by an environment variable and
  scrubbed from errors, so no secret lives in `config.toml`; `{text}`, `{kind}`, `{harness}` and
  `{folder}` are filled at each send, escaped for JSON and form bodies. Failures show as one card
  per distinct error. A **Messenger** switch in the Windows tray menu turns it on and off. The
  macOS and Linux residents forward too, without the menu switch yet (AH-213).
- Windows: `aherald-cli.exe`, a console companion beside `aherald.exe`. It starts `aherald.exe`
  with the same arguments and standard input, shows its output in PowerShell, cmd and Windows
  Terminal without `| Out-String`, and returns its exit code. Hooks still call `aherald.exe`
  directly (AH-211).
- Windows: `--install-statusline` and `--uninstall-statusline` now work (a dry run unless
  `--apply`), wrapping and restoring your own status line command exactly (AH-211).

### Changed
- Windows: the Claude Code `statusLine` wrapper is now a readable command
  (`C:/…/aherald-cli.exe --statusline`, or a `statusline.ps1` beside the executable for a path with
  spaces) instead of an encoded PowerShell script, except for paths with `"`, `$` or a backtick.
  Your own status line command is kept in the setup receipt rather than on the command line;
  running setup again upgrades an earlier installation (AH-211).

## [0.12.0] - 2026-11-04

### Changed
- The configuration reference is now `config.example.toml` (was `aHerald.example.toml`), next to
  the `config.toml` it describes. The Windows installer removes the old copy on upgrade.

### Added
- `[claude] suppress_while_busy` (default `true`): no "turn complete" card or sound while Claude
  Code's `Stop` payload still lists background work (any task but a monitor, or a one-shot
  scheduled wakeup); the next `Stop` with nothing left running shows it.
- `aherald --raw-output usage --harness codex|claude` and `aherald --raw-output statusline`
  print the last unparsed answer of a usage source, or the last payload Claude Code sent to the
  statusLine wrapper, from the running resident's memory (only the latest one each; never
  written to disk or the log). The source, status and age go to stderr. The IPC frame limit
  grows from 4 KiB to 256 KiB to carry them; the wrapper now sends its payload (up to 32 KiB)
  with each report.
- Claude usage for accounts without 5-hour and 7-day windows (enterprise): the plan reads
  `enterprise`, the menu and tooltip line shows the spend (`Claude enterprise: spend 0.74 /
  100.00 USD (1%)`) and the card shows the spend and extra-usage lines with the provider's
  percentage, state and limit-reached flag. A whole-number `used_credits` sent as `74.0` is read
  too.

### Fixed
- Errors that only went to stderr, which a GUI program does not have, are now shown:
  an interactive start that fails for any reason shows it on a card (it did for a bad
  configuration only); Apply that cannot start its replacement and Edit configuration that
  cannot open a file say so on a card; a tray icon that cannot be updated says so once (it
  printed every second); with a configuration it cannot read, the status line prints
  `Invalid configuration: …` instead of nothing.
- Windows: Edit configuration opens Notepad whenever the shell cannot open `config.toml` (it did
  only for two error codes) and says why on a card if that fails too; it used to print to a
  console the program does not have.
- A Claude usage response aHerald cannot use no longer reads "Invalid Codex protocol message":
  it says "Claude usage response not recognised" or "…has no quota or spend data".

## [0.11.0] - 2026-11-04

### Changed
- One form for every time span, in the tray and usage card, the status line and the card age:
  `12 min`, `3:05`, `2d 3:05` (card age adds ` ago`). Was `3d 5h` for quota resets, `1h 12m` /
  `12m 30s` for status line durations and whole hours or days for card ages. Quota resets still
  round up, so the time left never promises more than there is; durations and ages round down.
- Status line: `{context_window}` now says what it counts (`ctx 9% used`, `ctx 91% remain`).
  `[claude.statusline] display_ctx` (default `used`) and `display_usage` (default `remain`)
  choose it for the context and for the quotas; they replace the borrowed `[codex.usage] display`.
  With the defaults the context now reads as used (it followed the Codex setting, remaining, before):
  set `display_ctx = "remain"` for the old meaning.
- The About card shows how long ago the binary was built (`Built 2d 3:05 ago`), and
  `--version` prints the build time: `aHerald 0.10.0 (built 2026-11-03 22:52 UTC)`. The time is
  stamped by `build.rs`; `SOURCE_DATE_EPOCH` overrides it for reproducible builds.

## [0.10.0] - 2026-11-03

### Changed
- Tray menu (Windows, macOS, Linux): a separator now follows the version (About) line.
- Breaking: `[pets]` and `[pet_animations]` replace `notifications.pet`,
  `notifications.pet_animations` and `[agents.*]`; the old keys are rejected (AH-202).

### Fixed
- macOS and Linux: an interactive start that fails on its configuration now shows the reason on an
  aHerald card, as Windows already did (it was only on stderr, invisible when started from Finder).
  Docs: on macOS 15+ the first start of the ad-hoc signed app needs Open Anyway in Privacy &
  Security; right-click, Open no longer works.

### Added
- `aherald --notify <harness> --kind <kind> [--activity|--ended]`: one input for events of
  other coding harnesses, with per-harness queues and pets (AH-202).
- Bridges and hook examples for OpenCode, Pi, Crush and Goose in `examples/harnesses`; see
  `docs/harnesses.md` (AH-203).
- `aherald --setup --harness opencode|pi|goose` installs those bridges as aHerald-owned files
  (dry run by default, receipt, backups, rollback); `--unsetup` removes them (AH-204).

## [0.9.1] - 2026-11-03

### Added
- Homebrew cask for Apple Silicon Macs: `brew tap proprock/tap` and `brew install --cask aherald`
  (tap `proprock/homebrew-tap`). It installs the released disk image's app, does not remove the
  Gatekeeper quarantine (right-click, Open) and, on uninstall, removes aHerald's own hook entries
  as your user before deleting the app. The release workflow now opens a pull request in the tap
  for each stable release (AH-117).

macOS: unchanged from 0.9.0: ad-hoc signed only, not notarized, Apple Silicon only. The cask
install, upgrade and uninstall have not been checked on a Mac yet; see TODO.md (AH-117).

## [0.9.0] - 2026-11-03

macOS: the app and the disk image are ad-hoc signed only, not signed with a Developer ID and
not notarized, so Gatekeeper blocks the first start of a downloaded copy (right-click, Open).
Apple Silicon only. The disk image, setup window and tray controls had automated checks and
partial manual checks on one MacBook Pro (M1); see TODO.md (AH-102, AH-118, AH-120).

### Added
- macOS: opt-in `[general] detect_terminal_tab` lets `suppress_when_focused` hide a card for
  the selected tab of Terminal.app or iTerm2 (asks macOS for Automation access; other
  terminals and Codex keep delivery) (AH-120).
- macOS disk image with an in-app setup window: choose Codex and Claude Code, usage and
  warnings, preview and apply through the common setup backend; opening the app from Finder
  or `aherald --setup-window` starts it (AH-102).
- Independent notification and sound suppression for a focused originating Claude
  session in a classic Windows console. Both options default to enabled; unknown
  origins, tabbed terminals and desktop sessions retain delivery. Live acceptance
  and coverage limits are recorded in docs/active-session.md (AH-098).
- Embedded multi-size Windows application icon (AH-105).
- Diagnostics for rejected inputs, activation and session lookup failures, and
  missing or unusable artwork. Interactive Windows startup shows a bounded error
  card for invalid configuration; hook responses remain independent (AH-090).

### Fixed
- Native setup preserves another aHerald installation's hooks during initial and
  repeated setup. Legacy commands are upgraded only for the current executable;
  a moved installation replaces only bindings identified by its saved receipt
  (AH-097).

## [0.9.0-beta] - 2026-11-02

### Added
- Ubuntu 22.04+ x86-64 Debian package (`aherald-vX.Y.Z-linux-x86_64-gnu.deb` with a
  SHA-256 file) next to the portable archive, and interactive `aherald --console-setup`
  for Codex and Claude Code hooks, usage monitoring and, per selected agent, the
  context and idle warnings. Package scripts never touch user homes or hooks; user
  settings, receipt, assets and logs use XDG directories and survive upgrade, removal
  and purge. See docs/installation.md.
- Linux parity with 0.8.0-beta: context and idle warnings, tray Context/Idle warnings
  switches, Apply and Apply + Test, and Linux native setup with a dry-run preview.
- Claude status line text: `[claude.statusline] format` prints a template over Claude
  Code's statusLine JSON when you have no status line command of your own (your own
  command is still wrapped and wins). It supports `{a.b.c}` fields, `{[ ... ]}`
  optional groups, ready-made `{context_window}`, `{rate_limits.five_hour}` and
  `{rate_limits.seven_day}` segments in the form of the tray menu line (`5h 79% / 3:29`)
  with colours matching the tray icon, and short forms of token counts, dollars,
  durations and reset times. See docs/configuration.md and `aHerald.example.toml`,
  which lists every field.
- CI verifies the Debian package, its isolated dpkg lifecycle and console setup on
  Ubuntu 22.04 for every code change, in parallel with the existing checks.

### Changed
- **Breaking:** notification lifetimes are configured in seconds. Rename
  `events.<kind>.duration_ms` to `duration_sec` (for example `3000` becomes `3`);
  a configuration that still has `duration_ms` is rejected.
- **Behaviour:** idle warnings are off unless `idle_enabled = true` (the default
  changed from on to off, also in the shipped example). Configurations that already set
  `idle_enabled` keep their value; if you relied on the old default, enable idle with
  setup, the installer or the key. Context warnings still need a threshold.
- The status line, tray menu and tooltip share one text form for usage windows.
- `--setup-settings`, console setup and the Windows installer all start with context
  and idle warnings off on a new configuration.

### Fixed
- The Claude statusLine wrapper and the Codex context hook no longer wait up to 150 ms
  for a resident to start when none is running; they now return at once (about 4 ms
  instead of 160 ms per status line refresh without a resident), on every platform.
- Interrupted Linux setup restores hooks, configuration and a user's own status line
  together (covered by a root-safe rollback test).

### Verification and known limitations
- Linux was developed and checked on Ubuntu 22.04.5 (GNOME, Wayland with XWayland):
  tray, cards, Apply and Apply + Test, configured sounds and synthetic context and
  idle reports were confirmed by hand. Real Codex 0.166.0 and Claude Code 2.1.316
  sessions passed through a private test bridge. Automated checks cover the package
  lifecycle in an isolated dpkg root, console setup in a pseudo-terminal, setup
  rollback and the status line wrapper.
- Not yet accepted: installed-hook delivery and the installed statusLine wrapper in a
  live Claude Code or Codex session on Linux, a clean-machine apt and graphical
  installation, Xorg and multi-monitor placement, and Wayland-native layer shell
  (KDE/wlroots). See TODO.md (AH-093, AH-103, AH-115).
- Windows and macOS behaviour is unchanged apart from the items above; their manual
  acceptance was not repeated for this beta.

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

[Unreleased]: https://github.com/proprock/aHerald/compare/v0.17.0...HEAD
[0.17.0]: https://github.com/proprock/aHerald/compare/v0.16.0...v0.17.0
[0.16.0]: https://github.com/proprock/aHerald/compare/v0.15.0...v0.16.0
[0.15.0]: https://github.com/proprock/aHerald/compare/v0.14.0...v0.15.0
[0.14.0]: https://github.com/proprock/aHerald/compare/v0.13.0...v0.14.0
[0.13.0]: https://github.com/proprock/aHerald/compare/v0.12.0...v0.13.0
[0.12.0]: https://github.com/proprock/aHerald/compare/v0.11.0...v0.12.0
[0.11.0]: https://github.com/proprock/aHerald/compare/v0.10.0...v0.11.0
[0.10.0]: https://github.com/proprock/aHerald/compare/v0.9.1...v0.10.0
[0.9.1]: https://github.com/proprock/aHerald/compare/v0.9.0...v0.9.1
[0.9.0]: https://github.com/proprock/aHerald/compare/v0.9.0-beta...v0.9.0
[0.9.0-beta]: https://github.com/proprock/aHerald/compare/v0.8.0-beta...v0.9.0-beta
[0.8.0-beta]: https://github.com/proprock/aHerald/compare/v0.5.0...v0.8.0-beta
[0.5.0]: https://github.com/proprock/aHerald/compare/v0.4.0...v0.5.0
[0.4.0]: https://github.com/proprock/aHerald/compare/v0.1.0...v0.4.0
[0.1.0]: https://github.com/proprock/aHerald/releases/tag/v0.1.0
