# aHerald implementation plan

aHerald is a portable, native notification companion for AI coding agents on Windows
10/11 x64, macOS (Apple Silicon) and Linux (X11/XWayland): Codex and Claude Code through
their hooks, OpenCode, Pi, Goose and Crush through `--notify` and bridges, and agents in
containers or VMs through `[remote]` and `aherald-ping`. One executable per platform, built
with stable Rust. Keep each milestone usable. Project rules: `AGENTS.md`.

## Agreed decisions

- Routine artwork performance check: `cargo test --test perf_artwork -- --nocapture`;
  frozen 64x64 fixtures, scale 1x, nearest interpolation, 3 warmups and 25 uncached
  preparations per kind; report distribution and retain a 30 ms per-kind mean budget.
  Keep real monitor DPI in resident `perf_baseline`; mean budgets are 61 ms for
  artwork and 28 ms for composition, rounded down to whole ms within 10% above
  observed maxima (55.805 and 25.83 ms). Other limits stay unchanged; higher DPI
  is not fully calibrated. See docs/perf-baseline-investigation.md.

- Default configuration: `config.toml` beside `aherald.exe`; no APPDATA lookup.
  Create a missing default with standard settings; `--config` selects an existing custom file.
- Support resident and detached transient notifications. A hook must finish
  independently of notification lifetime, animation, monitoring, or usage queries.
- Use one overlay with a count and controls for switching between events.
- Always show the event source and signal text in the overlay. Optional animation
  decorates the same card and identifies the agent; it never replaces that text.
- Select optional pet artwork by agent; keep permission-response buttons as future
  product work. Dismissing the current card never answers an agent request.
- Supply simple original PNG animations and WAV sounds under MIT; assets remain
  external and replaceable without recompilation.
- Load builtin animations automatically from `assets` beside the executable.
  With visual=auto, a configured pet replaces builtin artwork for all inherited
  events. Explicit event visual choices enable hybrids; animation paths require
  visual=custom. Failed selected artwork falls back to builtin, then text.
- Observe input waits only through verified, documented stable interfaces. Do not
  parse unstable transcripts or take control of unrelated sessions. Unsupported
  coverage is explicit and does not block release.
- Supply manual hook/autostart instructions and examples. Do not automatically
  edit Codex configuration or install startup entries. Explicit `--setup --apply`
  and the installer's user-selected harnesses authorize hook configuration only;
  no autostart changes are implied.
- Installer architecture (2026-10-31): each OS ships its own native installer.
  Installers delegate explicitly selected harness setup to the native executable;
  hooks contain fixed executable paths, with no runtime discovery launcher.
  Keep portable archives. No marketplace/plugin is required for hook-only setup.
  Claude uses exec-form hooks; Codex's shell-only contract remains an explicit
  limitation. Keep install paths stable across upgrades; rerun setup after moves.
  Uninstall removes only the installation's saved hook bindings before its binary.
  See docs/installation.md and AH-097/AH-102/AH-103.
- Keep code comments, documentation, and agent instructions in English.

## Task tracking

Each task lists dependencies and acceptance criteria. Check a task only after its
criteria have been verified. Record commands, results, tested versions, and any
remaining environmental or coverage limitations when completing a milestone.
An implementation alone is not evidence of Windows visual acceptance.
Completed tasks move to `DONE.md`; follow-ups and deferred items stay here under new IDs.

## Milestone 5a — macOS and Linux support (first priority)

Agreed scope (2026-10-27): native bindings per OS, no winit or other GUI framework
(`objc2`/AppKit on macOS; `x11rb` plus D-Bus on Linux). Linux targets X11/XWayland
first; Wayland is evaluated separately and may ship with reduced features. Live
verification runs on the user's MacBook Pro M1 (`aarch64-apple-darwin`) and an Ubuntu
machine (GNOME defaults to Wayland, so also test an Xorg login). CI (GitHub Actions)
covers automated checks only. New direct dependencies follow the rule in AGENTS.md
("Boundaries and principles"): justify each one, ask the owner first only for heavy ones.

- [ ] **AH-214 — Windows installer GUI and clean-machine acceptance (split from AH-097).**
  Depends on AH-097 (closed 2026-11-04). On a clean Windows 10/11 machine without WSL,
  Bash or development runtimes, verify the installer GUI and harness checkboxes, the
  launch-after-install option on interactive setup only, selection changes, upgrade and
  uninstall, and hook delivery and dismissal for installed Codex and Claude Code. Record
  the Windows build, harness versions and limitations. Evidence so far:
  docs/ah-097-windows-acceptance.md.

- [ ] **AH-215 — Cause of the `native_assets` timeout in the full Cargo test run (split from AH-097).**
  `cargo test --all-targets` stops at `native_assets` with a notification startup timeout,
  both inside and outside the sandbox. Reproduce once, find the cause, and fix it test-first,
  or document why the timeout is environmental. Acceptance: the full run passes on Windows 11,
  with the commands and results recorded.

- [ ] **AH-221 — Flaky native memory-growth test on the Windows runner.**
  `notification::overlay::card_tests::repeated_card_rendering_and_dpi_resize_release_native_resources`
  (src/overlay.rs) failed on a hosted `windows-latest` runner with "private commit kept growing"
  (30.4 MB at cycles 50-100, 40.4 MB at 150-200, baseline 29.0 MB) on a change that did not touch
  the code, and passed on the same diff and on master. Verdict (2026-11-04): a pull request skips
  it (`--skip` in the `cargo test` step of ci.yml) and the nightly full run keeps it, so a real
  leak still shows within a day. Release still runs it. Work left: reproduce on Windows 11,
  decide whether the growth is a real leak (GDI/heap) or allocator and runner noise, and make
  the test deterministic (for example, count GDI and USER handles instead of private commit, or
  widen the window with several samples). Acceptance: the test passes 20 runs in a row on a
  hosted runner, and the `--skip` is removed from ci.yml.

- [ ] **AH-102 — Native macOS installer and setup UI.**
  Uses the common native setup backend from AH-097. Deliver GUI app installation
  and Homebrew Cask packaging for Apple Silicon, preserving the portable archive.
  Provide native user-context selection of Codex, Claude Code, both or neither;
  start unchecked on first setup, retain the selection on upgrade, and delegate
  changes to `--setup` / `--unsetup` with explicit `--apply`.
  Do not block package updates/removal on resident shutdown. Check old-resident
  and new-client IPC compatibility; document when users must restart residents.
  Match Windows usage selection and launch-after-install behavior: read current
  values on upgrades, keep new usage off, explain Claude auto, preserve configured
  sources and unrelated formatting, and include changes in setup rollback.
  Use a stable installed app path and fixed hook executable paths, without
  runtime discovery or developer runtimes. Preserve the harness's normal hook
  trust flow and do not add autostart. Reuse dry run, all-file preflight, backups,
  rollback, idempotence and exact saved-binding removal. Keep the receipt and
  writable user data outside the signed app bundle; never modify its contents
  during setup. Preserve configuration, assets customized by the user, logs and
  backups on upgrade/removal, and preserve foreign hooks and other installations.
  Define a user-context removal flow for both GUI and Cask installations that
  cleans owned hooks before removing the executable, with visible failure and
  a repair path. Define behavior for an active resident and moved app bundles.
  Extend macOS release CI with installer/Cask artifacts and checksums alongside
  the portable archive; do not claim signing/notarization without implementing
  and verifying it. Add isolated native lifecycle checks for first install,
  repeated setup, selection changes, malformed configuration, failed writes,
  upgrade, cleanup failure and uninstall after environment overrides change.
  Acceptance: verify the GUI, both installation paths, native hooks, delivery and
  dismissal on the MacBook Pro M1, recording OS/harness versions and limitations;
  verify a normal user installation without Homebrew or developer tools for the
  GUI path. Document setup, repair and removal. See docs/installation.md.
  Scope change (2026-11-03): the Homebrew Cask is split out as AH-117 (separate tap
  repository); AH-102 delivers the DMG, the in-app setup window and the portable archive.
  Status 2026-11-03 (branch `feature/ah-102-macos-installer`): implemented
  `setup_selection::Selection` (shared with the Linux console setup), the AppKit setup window
  (`--setup-window`, also opened by a bare launch of an installed bundle), `package-dmg.sh`,
  `check-release-dmg.sh`, `tests/macos_app_lifecycle.sh` (also run against the mounted image),
  release/CI steps and docs. Automated: fmt, Clippy and `cargo test --all-targets` pass on macOS
  (748 passed); the lifecycle check passes and fails on a no-op executable; the DMG built and
  verified locally. Manual on the M1 (macOS Tahoe 26.6.2, run from a copy of the debug build
  with isolated CODEX_HOME/CLAUDE_CONFIG_DIR): window layout, scrolling details, Preview, Apply,
  retained selection, removal and a malformed settings file passed. Still open: installing from
  the real DMG into /Applications and the Finder double-click launch, Codex/Claude hook
  delivery and dismissal with the installed app, a normal user without developer tools,
  old-resident/new-client IPC compatibility check on macOS, and Remove/Repair buttons (removal
  works by clearing both choices; repair by applying again). Not signed or notarized.
  `[claude.statusline] format` checked on macOS (Darwin 25.6.0, debug build, commands only): the
  same text prints with the config beside the executable and with the config under
  Application Support in an app bundle, and through the wrapper that `--setup` installed for a
  non-empty format. The setup window explains it; the text itself stays in config.toml.

- [ ] **AH-103 — Ubuntu package and interactive console setup.**
  Uses the common native setup backend from AH-097. Deliver an x86_64 `.deb`
  installable through apt and a graphical package manager, preserving the
  portable archive. Provide an interactive console setup run by the user after
  installation, with selection of Codex, Claude Code, both or neither; start
  unchecked on first setup, retain the selection on upgrade, and delegate changes
  to `--setup` / `--unsetup` with explicit `--apply`. Do not require an
  interactive terminal or configure user hooks from package scripts.
  Match Windows graceful shutdown before updates/removal: verify the owned app
  path, wait for process exit, and abort if graceful shutdown fails.
  Match Windows usage selection and launch-after-install behavior: read current
  values on upgrades, keep new usage off, explain Claude auto, preserve configured
  sources and unrelated formatting, and include changes in setup rollback.
  Use stable installation paths and fixed hook executable paths, without runtime
  discovery or extra developer runtimes. Preserve normal hook trust and do not
  add autostart. Reuse dry run, preflight, backups, rollback, idempotence, saved
  file paths and exact binding ownership. Preserve foreign hooks, configuration,
  customized assets, logs and backups across updates/removal.
  The system package owns its executable and built-in resources. Use XDG config,
  data and state directories for each user's config, receipt, assets and logs;
  preserve user files even on purge and keep portable layout unchanged. Package
  scripts must not modify user homes or hooks. Document that each user should run
  --unsetup --apply before removal; removal remains noninteractive and may leave
  stale hooks, recoverable by reinstalling and running cleanup as that user.
  Extend Linux release CI with the package and checksum alongside the portable
  archive; add isolated native lifecycle checks for install, repeated setup,
  selection changes, malformed configuration, failed writes, upgrade, cleanup
  failure and uninstall after environment overrides change.
  Acceptance: verify apt and graphical installation, interactive console setup, hook
  delivery/dismissal, update and removal on a clean supported Ubuntu desktop
  without developer tools. Record Ubuntu version, desktop/session type and
  harness versions; document required system packages, repair and removal.
  See docs/installation.md; warning setup controls are tracked in AH-114 and
  overlay Xorg/multi-monitor acceptance remains AH-093.
  Partial 2026-11-02: package construction, XDG paths and console setup have
  isolated tests. A private dpkg root exercises install, same-version upgrade,
  user cleanup and purge with dependency checks forced; an Xvfb/D-Bus probe
  verified new-client IPC against an older resident. On Ubuntu 22.04.5, lintian
  reported no findings and apt-get -s resolved the local package on this host
  (with a warning about an unreadable third-party sources file). Clean Ubuntu apt/graphical
  installation, real hook delivery/dismissal and update/removal remain to be
  accepted; the isolated tests do not establish those results.

- [ ] **AH-104 — Optional live harness checks on macOS and Linux.**
  Extend the optional group in tests/live/README.md with native macOS/Linux
  equivalents of the Windows Codex hook-argument compatibility probe, plus
  installer-generated hook dispatch checks for both Codex and Claude Code.
  Use installed harnesses and verify observed process arguments/responses,
  prompt hook completion, notification delivery and dismissal where applicable.
  Keep compatibility probes distinct from end-to-end delivery acceptance.
  Record tested OS/harness versions and declare account, network and potentially
  billable request requirements per check. Use isolated configuration, bounded
  waits and owned-process cleanup; preserve normal user hooks, trust and accounts.
  Keep these checks explicit and opt-in, outside ordinary Cargo tests and mandatory
  CI; any live CI job must require deliberate invocation and its own credentials.
  Acceptance: run the applicable checks on the M1 and Ubuntu, record evidence and
  unsupported cases, and document repeatable commands. Uses AH-097; installer
  delivery checks also depend on AH-102/AH-103. Linux warning-specific live
  acceptance and documentation are tracked in AH-115.

- [ ] **AH-117 — Homebrew Cask for aHerald (split from AH-102).**
  Publish a Cask for the Apple Silicon app in a separate tap repository, installing the
  released DMG or ZIP by URL and SHA-256 from the release artifacts. Do not claim signing or
  notarization; keep the portable archive. Removal must clean owned hooks in user context
  before the executable is removed (`zap` is not enough): define the uninstall flow, its
  visible failure and the repair path, and document that Homebrew runs as the user while
  hooks belong to that user. Updates must not block on a running resident; document the
  restart. Acceptance: `brew install --cask`, setup window, hooks, delivery and dismissal,
  upgrade and uninstall on the MacBook Pro M1.
  Decisions 2026-11-03: tap `proprock/homebrew-tap` (`brew tap proprock/tap`), shared by
  future projects; no quarantine removal (README documents right-click, Open); stable releases
  only, updated by the `update_tap` release job through a pull request using the secret
  `TAP_TOKEN`. Status: `proprock/homebrew-tap` exists with the Cask (0.9.0, checksum verified
  against the released DMG), README, `update-cask.sh` and a style-audit CI (green). `TAP_TOKEN`
  is set. Not yet checked: `brew install --cask`, upgrade and uninstall on the M1, and the first
  `update_tap` run.

### macOS parity with 0.9.0-beta

The warning engine (`session_warnings`), the setup backend and the tray text are shared
already, and the macOS resident keeps the warning state. What differs is the tray controls
and the acceptance evidence; reuse the Linux and Windows pieces rather than parallel code.

- [ ] **AH-118 — Context/Idle warning switches and Apply / Apply + Test in the macOS tray.**
  The Windows tray (`src/tray.rs`) and the Linux tray (`src/linux/tray.rs`, handlers in
  `src/linux/resident.rs`) offer "Context warnings", "Idle warnings", "Apply" and
  "Apply + Test"; the macOS menu (`src/mac/menu.rs`) has only "Restart". Add the same entries
  with the same order, labels and check marks, wired to `Warnings::set_enabled` and to
  re-reading the configuration with the preview cards, as on the other platforms.
  Acceptance: pure menu tests for order, labels and check state; switches change what the
  resident admits, observed through the resident (not source text); Apply keeps usage and
  event handling running; verified by hand on the MacBook Pro M1 (cards, dismissal).
  Status 2026-11-03 (branch `feature/ah-118-mac-tray-warnings`, stacked on AH-102): the menu
  has the Windows order and separators (usage lines, Sound, Pop-up, Pause, Context warnings,
  Idle warnings, About, Edit configuration, Apply + Test, Apply, Exit; Apply was Restart). The
  switches call the shared `Warnings` gates (tested through the resident state). Apply + Test
  restarts the resident with `--test-after-apply` and shows the shared `demo::Carousel` as on
  Windows and Linux: arrows, wheel and body clicks, a "[Test]" heading with "Disabled in
  config", each sample's sound once, ended by dismissal or the first real card; the click
  decision is the pure `input::carousel_click`. Automated: fmt, Clippy and
  `cargo test --all-targets` (753 passed). Not yet checked by hand on the M1: the menu itself,
  the switches' effect on live cards, Apply, Apply + Test (cards, sounds, arrows, wheel, dismiss).

- [ ] **AH-119 — macOS live warning acceptance and documentation.**
  The macOS counterpart of AH-115. With the AH-118 switches, verify real Codex and Claude
  Code context and idle warning cards on the M1: synthetic reports first, then installed hook
  dispatch and the installed statusLine wrapper (the setup window can enable them), including
  dismissal, configured sound and an idle warning suppressed while the switch is off.
  Distinguish adapter/bridge evidence from end-to-end delivery, use isolated configuration,
  bounded waits and owned-process cleanup, declare any account, network or billable request
  before each run, record macOS and harness versions and commands, and update docs and the
  verification matrix. Extends AH-104 (live checks on macOS); depends on AH-118.

- [ ] **AH-222 — Suppress notifications while the user watches the originating session (Linux X11).**
  Windows (AH-098) and macOS (AH-120) suppress the card and the sound on positive evidence of both
  the window and the session; on Linux `focus::observe` reports "unknown", so everything is
  delivered. Evaluate X11 (`_NET_ACTIVE_WINDOW` and the window's PID against the terminal process
  aHerald records) and Wayland-under-XWayland limits; a terminal with tabs needs a way to prove the
  active tab (its tty). Same rule: suppress only on positive evidence of both, otherwise deliver; no
  new permission prompt. Agree on the design before implementation.
  Status (branch `feature/AH-222-linux-focus`): implemented for X11 sessions (`src/linux/focus.rs`).
  Window: the captured host window equals `_NET_ACTIVE_WINDOW`. Session: the owner's `comm` is a
  one-session terminal (xterm, urxvt, rxvt, st, mlterm, aterm, eterm, alacritty) and it owns no other
  window. Wayland sessions, tabbed terminals, tmux and errors deliver. Pure tests cover the rule; not
  run against a live X server here (the session is Wayland, no Xvfb): acceptance on Xorg with xterm
  and with a tabbed terminal is open. Konsole (D-Bus `currentSession`) and Kitty (remote control) could
  later give the selected tab.

- [ ] **AH-120 — Suppress notifications while the user watches the originating session (macOS).**
  Windows suppresses the card and the sound when the Claude session's classic console is
  focused (AH-098, `src/focus.rs`); on other systems `focus::observe` reports "unknown" and
  nothing is suppressed. Evaluate the macOS options: the frontmost application through
  `NSWorkspace` and the terminal process aHerald already records (`src/mac/terminal.rs`) tell
  the app, but a tabbed Terminal.app or iTerm window also needs a way to prove the active tab
  or session (for example its tty). The project rule stays: suppress only on positive evidence
  of both the window and the session; otherwise deliver. Agree on the design and any new
  permission (Automation/Accessibility prompts are not acceptable by default) before
  implementation; keep both `suppress_when_focused` options and their defaults.
  Acceptance: pure policy tests with the macOS evidence, and live checks on the M1 in Terminal
  and in a second terminal app, including unknown origins that must still be delivered.
  Design agreed 2026-11-03: opt-in Automation. `[general] detect_terminal_tab` (default false)
  adds `Event::tty` (the nearest ancestor's device, one `ps` per hook) and, for Terminal.app
  and iTerm2 only, reads the selected tab's device with `osascript` (1.5 s bound) when that
  terminal is in front (`NSRunningApplication.isActive`, no NSWorkspace feature added). Equal
  devices are `Session::Same`; everything else delivers. Claude main sessions only, as on
  Windows. Status 2026-11-03 (branch `feature/ah-120-mac-focus-suppression`): implemented with
  unit tests (761 pass); NOT checked live: the Automation prompt, the real tab comparison in
  Terminal.app and iTerm2, a second tab/terminal delivering, and the `NSAppleEventsUsageDescription`
  text in the bundle.

### Linux parity with 0.8.0-beta

The warning engine, adapter parsing, setup backend and measured warning text are
shared already. Verify their Linux integration and reuse them rather than creating
parallel implementations. The tasks below cover behavior and acceptance; existing
shared development-profile and artwork performance changes need no separate port.

- [ ] **AH-114 — Add warning options to Linux console setup.**
  Extend the AH-103 console setup with independent context and idle choices for each
  agent, using the common setup backend. Load existing settings and preserve
  custom thresholds and unrelated configuration. On first context enablement,
  supply Codex 250k / Claude 400k only if no threshold exists; retain the idle
  defaults of Codex 25 / Claude 55 minutes. Persist enablement separately from
  thresholds. When Claude and its context warning are selected, configure the
  statusLine wrapper transactionally, preserving the original command and
  options; avoid nested wrappers and restore only an unchanged owned binding on
  disable, deselection or uninstall. Include all changes in setup rollback.
  Acceptance: isolated Linux lifecycle checks cover first setup, existing custom
  settings, repeat setup, selection changes, upgrade, disable, uninstall and
  failed writes/rollback. Verify the choices and wrapper through the console setup
  as part of AH-103. Depends on AH-103 and the shared AH-097 setup backend.
  Partial 2026-11-02 (statusLine only): the wrapper's start-up cost was measured
  (release median 3.9 ms bare, 4.5 ms with `--then`; 3.5 ms of it is the loader mapping
  63 libraries) and guarded by `tests/perf_statusline.rs`; a separate wrapper executable was
  considered and deferred (docs/statusline-performance.md). Added `[claude.statusline] format`
  (src/statusline_format.rs): printed only when the user has no `--then` command, config read
  only on that path, never created. Setup installs the wrapper for a context threshold or a
  format. Console setup now asks the four context/idle questions for each selected agent
  (defaults off on a first setup on every OS: the shipped example has `idle_enabled = false`),
  `tests/console_setup_linux.py` covers first setup, wrap/restore, repeat and deselection,
  `tests/setup.rs` has a root-safe Linux rollback test, and `tests/linux_deb_lifecycle.sh`
  asserts thresholds, wrapper, upgrade and restoration. Live-session acceptance stays AH-115.
  2026-11-02: the status line shares the tray menu's text pieces (`usage_service::window_text`,
  `percent_text`, `countdown`) and the icon's colour thresholds (`tray_usage::level_rgb`);
  `{context_window}`, `{rate_limits.five_hour}` and `{rate_limits.seven_day}` render as
  `ctx 91%`, `5h 79% / 3:29`; menu and tooltip already share `View::compact` on every OS.
  Review 2026-11-03: every item of the brief is implemented and covered (console prompts and
  wrapper in `tests/console_setup_linux.py`, thresholds, disable and rollback in `tests/setup.rs`,
  deb lifecycle in `tests/linux_deb_lifecycle.sh`, all in Linux CI). What remains is the manual
  console run on a clean Ubuntu desktop, which is the AH-103 acceptance; no code change is open.

- [ ] **AH-115 — Linux live warning acceptance and documentation.**
  Extend AH-104's opt-in native live checks to cover real Codex and Claude Code
  context and idle warning sessions, installed hook dispatch and the Claude
  statusLine wrapper. Distinguish adapter/bridge evidence from end-to-end installed
  hook delivery. Use isolated configuration, bounded waits and owned-process
  cleanup; preserve existing user hooks, trust and accounts, and declare any
  account, network or potentially billable request requirements before each run.
  On Ubuntu, verify the warning cards, tray switches, Apply and Apply + Test,
  including dismissal and configured sound. Record OS, desktop/session and
  harness versions, commands, results and limitations; update the Linux guide,
  configuration instructions and verification matrix. Acceptance: applicable
  automated and explicitly authorized live checks pass with evidence recorded;
  unsupported versions or environments are stated rather than inferred. Xorg
  and multi-monitor acceptance remains AH-093. Depends on AH-111 through AH-114
  for their respective behavior and installer checks, and coordinates with AH-104.
  Partial 2026-11-02: isolated Xvfb/D-Bus tests exercise synthetic Codex and
  Claude context reports, tray gating and one-minute idle delivery/clearing.
  These synthetic tests do not establish installed hook or statusLine delivery.
  On 2026-11-02 the user confirmed on Ubuntu 22.04.5 GNOME Wayland/XWayland:
  Apply, Apply + Test, tray switches, synthetic Codex and Claude context cards,
  configured sounds and dismissal, and Claude idle delivery/activity clearing
  and suppression with the idle switch disabled.
  The private live-test bridge also builds on Linux; its offline smoke test sends
  synthetic reports from both adapters to an isolated resident. On 2026-11-02,
  Codex CLI 0.166.0 passed a real context request through this bridge on Ubuntu
  22.04.5 with Xvfb and a private D-Bus. This does not test installed hooks.
  Claude Code 2.1.316 also passed real prompt/Stop/end and idle queue/clear
  through the bridge in the same environment. Claude context and the installed
  statusLine wrapper and installed hooks remain open.
  Repeatable commands and remaining probe coverage are in `tests/live/README.md`.

- [ ] **AH-093 — Linux overlay Xorg and multi-monitor acceptance (split from AH-068).**
  Depends on AH-068. On Ubuntu, log in to Xorg and confirm `XDG_SESSION_TYPE=x11`;
  manually verify card transparency, always-on-top behavior and absence of focus theft.
  Verify monitor placement and DPI on multiple monitors when available, recording the
  session type, desktop environment, monitor setup and results. Xorg and multi-monitor
  behavior have not yet been verified.


## Milestone 5 — Polish and portable release

- [ ] **AH-267 — Codex session names off by default (`[codex] session_names`).**
  Today the resident always asks the user's own `codex app-server` for the thread name of every
  Codex card (`session_source::Service::codex()` or `::shared(usage.names())`, chosen in
  `src/resident_overlay.rs:1171-1175`, `src/mac/resident.rs:1192-1196` and
  `src/linux/resident.rs:273-281`). Only transient mode has a switch (`[codex] transient_names`,
  default false). Agreed 2026-11-05: add `[codex] session_names` (default **false**), the one switch
  for Codex name lookups.
  - `session_names = false`: no lookup in either mode. No `codex app-server` child or worker is
    started for names, and the card keeps the harness-only heading. Reuse the disabled path of
    `session_source::Service::transient` (`src/session_source.rs:78-90`), for example as
    `Service::disabled()`. Usage polling (`[codex.usage]`) is unaffected and still runs its own
    connection.
  - `session_names = true`: the resident looks names up as it does today (shared connection when
    `[codex.usage]` is on, its own otherwise). Transient mode additionally needs
    `transient_names = true`, which keeps its meaning and its cost note.
  - Existing configurations stay valid: `transient_names` remains an accepted key
    (`deny_unknown_fields` must not reject old files). Claude cards are unaffected.
  - Update `config.example.toml` (`[codex]`, with a comment on what the lookup does),
    `docs/configuration.md`, `docs/codex-hooks.md:78`, `docs/privacy.md` (the "Codex session names"
    row moves to opt-in) and `CHANGELOG.md` under `Changed` (names now need opting in).
  Test first: in `tests/transient_names.rs` (or a new `tests/session_names.rs`), the default is false
  and parses; the disabled service never calls its connect closure and labels a Codex event without
  a name; with `session_names = true` a fake transport's name reaches the label; and transient
  requires both switches. Wire all three residents through one shared helper,
  `session_source::Service::for_resident(&Config, Option<NameLookup>)`, so the choice is not copied
  three times.
  Acceptance: the tests above pass on Linux; the macOS and Windows CI legs pass (the residents only
  compile there; report them as "CI only"); by hand, a resident with the default configuration shows
  Codex cards without a name and starts no `codex app-server` while `[codex.usage]` is off (check
  the process list).

- [ ] **AH-106 — Optional context and five-hour quota indicators in notifications.**
  Discuss and agree on the design before implementation: optionally show context
  utilization as a horizontal bar and the quota for the current five-hour usage
  window in the notification window. Agree on layout, configuration, agent coverage
  and unavailable-data behavior; verify supported data sources before coding.
  Acceptance: the agreed indicators can be enabled or disabled, show the relevant
  agent/session data and handle unavailable values according to the agreed design;
  verify the notification layout visually.

- [ ] **AH-099 — Notify only when Codex approval needs the user.** A Codex
  `PermissionRequest` hook can fire for an automatically reviewed request, so the
  current permission card can appear even when no user action is needed. Investigate
  a supported way to distinguish automatic review from a pending human approval,
  including auto-review rejection or fallback to a human. Do not infer the outcome
  solely from the hook firing, suppress all permission events, or parse transcripts.
  Implement suppression only when the distinction is reliable, preserving hook
  responses and genuinely pending permission notifications. Acceptance: tests cover
  automatic approval, a real human request, rejection/fallback and unknown outcomes;
  verify in a real Codex session and document any protocol limitations.
  Investigation (2026-11-01): the installed `0.166.0-alpha.5.1` hook runs before
  reviewer routing and exposes no reviewer/verdict/human-wait signal. Selective
  suppression remains blocked on a supported observable source; preserve current
  notifications. See [evidence and remaining acceptance](docs/codex-approval-observation.md).

- [ ] **AH-054 — Capture a live Claude `StopFailure` payload (split from AH-036).**
  Depends on AH-036. The mapping (`StopFailure` to `error`, resolving same-session
  pending cards) is covered by automated tests, but no live payload exists. A bad model
  name and an invalid `ANTHROPIC_API_KEY` with `claude -p` fail before a turn starts and
  fire no hook (2026-10-26). Try an interactive session that hits a real API error
  (for example a rate limit) with the capture hook, then compare the fields (`error`)
  with `docs/claude-hooks.md`. Acceptance: a captured payload matches the documented
  fields, or the doc records the difference and the mapping is adjusted test-first.

- [ ] **AH-211 — Windows: a console companion for command-line output and the `statusLine`
  command.** One cause, two symptoms: `aherald.exe` is a GUI-subsystem program (so a hook never
  opens a console window) and has no console of its own.
  1. Command-line output. `--version`, `--help`, `--status`, `--setup`, `--perf-report`,
  `--raw-output` and their error messages print nothing in PowerShell or cmd unless piped
  (`| Out-String`); the packaging scripts and docs work around it.
  2. The `statusLine` command. On Windows the command in `~/.claude/settings.json` is always (with
  or without `--then`) a PowerShell script spelled as `[char[]]` codes, which nobody can read or
  audit. It is that way because Claude Code runs a `statusLine` through Git Bash if installed and
  PowerShell otherwise, and the command must survive both (they expand `$`, quotes and backslashes
  differently); PowerShell neither waits for a GUI program nor collects its stdout unless piped; and
  `Out-String` would re-encode UTF-8 and ANSI colours, so the script copies bytes by hand. A
  `statusLine` has no `args` or `shell` field (only `type`, `command`, `padding`,
  `refreshInterval`, `hideVimModeIndicator`), so there is no exec form as for hooks.
  Approach: ship `aherald-cli.exe` beside `aherald.exe`, a tiny console-subsystem program that starts
  `aherald.exe` with the same arguments, gives it its own stdin, stdout and stderr, waits and
  returns its exit code. It is not for hooks (a parent without a console could flash a window); it
  is fine for the `statusLine` (Claude Code runs it in a terminal; check the IDE extension) and for
  commands typed by hand. The `statusLine` command becomes `C:/…/aherald-cli.exe --statusline`.
  Decide on Windows: (a) a path with spaces cannot be quoted the same way in Git Bash and in
  PowerShell, so such an install needs the fallback below; (b) the user's wrapped command: keep
  `--then '…'` on the command line (the same quoting problem) or read the original `statusLine`
  from the receipt `aherald-setup.json`, which already holds it, so the line stays short.
  Fallback: an aHerald-owned `statusline.ps1` beside the executable (listed in the receipt, backed
  up and removed like the bridges of AH-204) with the command
  `powershell -NoProfile -ExecutionPolicy Bypass -File C:/…/statusline.ps1`, for a path with spaces
  or a missing companion; the script copies stdin and stdout byte for byte. Not chosen:
  `AttachConsole` at start (the shell still does not wait for a GUI program).
  Acceptance, by hand on Windows 11 and Server 2019 (not by reading code) and with a CI pipe test:
  `aherald-cli --version` and an unknown option print in PowerShell, cmd and Windows Terminal
  without `| Out-String`, with the exit code kept; with output piped nothing changes; the status
  line, a wrapped user command, its exit code and non-ASCII output are unchanged;
  `--install-statusline` and `--uninstall-statusline` restore the original exactly and replace the
  old encoded command; a hook from Claude Code and Codex still delivers and opens no console window;
  `package-release.ps1` no longer needs its pipe workaround.
  Status 2026-11-04 (branch `feature/ah-211-windows-console-companion`): `aherald-cli.exe`
  (std only) exists. Inheriting the console shows nothing for a GUI child (seen in a real
  PowerShell and cmd), so it relays stdin, stdout and stderr through pipes and returns the exit
  code; pipe tests compare it with `aherald.exe` (`tests/aherald_cli.rs`), and `--version` and an
  unknown option were checked by hand in PowerShell and cmd. Decisions: (a) a bare
  `C:/…/aherald-cli.exe --statusline` when no character in the path is special to either shell;
  else `powershell -File "…/statusline.ps1"` with an aHerald-owned script (receipt, backup, removal);
  the encoded command stays only for a path with `"`, `$` or a backtick. (b) The maintainer chose
  the receipt: `--statusline` without `--then` reads the original from `aherald-setup.json`; a
  missing receipt gives a plain line, a corrupt one a plain line and a note on stderr.
  `tests/statusline_windows.rs` runs the generated command through PowerShell and Git Bash:
  bytes, non-ASCII and ANSI output and exit code 7 relayed; `--unsetup` restores the original
  exactly and removes the script; setup replaces an earlier encoded command. `package-release.ps1`,
  `build-installer.ps1` and the installer ship the companion; the script no longer pipes. About 7 ms
  per status line refresh is added by the companion (25 ms to 32 ms, release build).
  Remaining: manual checks on Server 2019 and in Windows Terminal; a live Claude Code and Codex
  hook (still `aherald.exe`, unchanged) shows no console window; the installer lifecycle with the
  companion (`tests/installer_windows.ps1` still pipes).
  `--install-statusline` / `--uninstall-statusline` now work on Windows (they share the setup
  receipt; tests in `tests/statusline_windows.rs` cover dry run, wrap, idempotence, exact restore,
  the spaced-path script, an earlier encoded command, and hooks left alone).

## Milestone 8 — Extensible harness integrations and basic OpenCode/Pi support

Agreed scope (2026-11-02): prepare aHerald for additional harnesses and deliver
basic OpenCode and Pi coding agent integrations through verified hooks or their
documented equivalents. A harness is distinct from its API/model provider:
OpenCode or Pi using an OpenAI/Anthropic API remains an OpenCode or Pi session.
Do not reuse Codex/Claude session identity, credentials or account usage merely
because the underlying provider matches.

Basic support means notification delivery for verified observable events, with
correct session/lifecycle handling. Permission/input-wait coverage is conditional
on a documented passive observation interface; report unsupported cases explicitly.
Usage, context, session cost, session-name lookup and click-to-activate are optional
future work for new harnesses, not milestone requirements. Preserve existing
Codex/Claude functionality without generalizing all advanced infrastructure.

Small JS/TS bridges running inside the harness's own extension runtime are approved.
aHerald remains native Rust and requires no separate JavaScript runtime. Keep each
slice usable, follow test-first development, and start after the current milestone.

Scope reduced (2026-11-03): one universal input replaces per-harness adapters. A
harness hook or bridge states the harness name and one of the existing event kinds as
arguments (`aherald --notify <harness> --kind <kind> [--activity | --ended]`); only the
session id and working directory are read from stdin. No new event kinds, no adapter
extraction or refactor of Codex/Claude, no per-harness `enabled` switch (installing the
hook or bridge is the opt-in), no usage/context/idle support for new harnesses. There
are no users to migrate: IPC and configuration changes are plain replacements, and local
configuration files are edited by hand. Setup, installers and the live acceptance suite
are deferred (AH-204, AH-205).

- [ ] **AH-205 — Live acceptance and onboarding guide for new harnesses (deferred).**
  Former AH-209 and AH-210. Opt-in live checks with isolated harness configuration,
  and a short guide for adding the next harness (arguments, bridge, tests, docs).

## Milestone 9 — Manual checks left from 0.13.0 and 0.14.0

The features shipped (AH-206, AH-213, AH-219, AH-220 in `DONE.md`); these checks need a person at
the machine. Record the OS version, the aHerald version and what was seen.

- [ ] **AH-268 — macOS by hand: Messenger menu item and harness checkboxes.**
  On the MacBook Pro M1 with a released build: with a `[forward]` sink configured, the menu bar
  menu shows **Messenger** after Pop-up, the check mark follows the switch and a card is (not) sent
  accordingly; without a sink the item is absent (AH-219). The setup window shows the OpenCode, Pi
  and Goose checkboxes unchecked by default, installs and removes their bridge files, and keeps an
  installed bridge checked on a second run (AH-206).
  Acceptance: both results recorded here or in `DONE.md` with the macOS and aHerald versions.

- [ ] **AH-269 — Windows 11 by hand: a real messenger and the command sink.**
  With a real webhook (Telegram bot or ntfy topic) in `[forward]`, a resident started from the
  installer's Run key sends a turn-end card, the tray **Messenger** switch stops and resumes it, and
  a missing secret variable shows a notice naming the variable (AH-213). Run one `type = "command"`
  sink (for example a PowerShell script that appends to a file): the arguments and stdin arrive,
  and a failing command shows its exit status only. The automated command-sink tests use `sh` and are
  skipped on Windows (AH-220).
  Acceptance: results recorded with the Windows build and aHerald version.

- [ ] **AH-270 — Linux by hand: Messenger tray item and the command sink.**
  On a Linux desktop with a StatusNotifierItem host: the tray menu shows **Messenger** after Pop-up
  only with a sink configured and the switch works (AH-219). One `type = "command"` sink with a real
  program (`notify-send`, `mail` or `ssh`) delivers a turn-end message (AH-220).
  Acceptance: results recorded with the distribution, desktop and aHerald version.

## Milestone 10 — Release trust (deferred)

- [ ] **AH-218 — Sign the release artifacts.** Build provenance is already attested for every
  archive, installer, disk image and package (`gh attestation verify`), and it costs nothing. What
  is left needs a certificate or an account, so it waits for a decision on cost:
  - Windows Authenticode for `aherald.exe` and the setup EXE, which removes the SmartScreen warning
    (options: SignPath Foundation for open-source projects, Azure Artifact Signing, or a paid
    code-signing certificate).
  - macOS Developer ID signing and notarization (Apple Developer Program), which removes the
    Gatekeeper "Open Anyway" step in the README.
  - Acceptance: the release workflow signs with a secret held in the `release` environment, and a
    clean Windows 11 and macOS 15 machine starts the downloaded file without a warning.

## Milestone 11 — Notifications from sandboxes and virtual machines

Agreed design (2026-11-05, plan in the planning session; evidence labels there): an agent that
runs inside a container or VM cannot reach the resident's same-user IPC. Add one opt-in transport,
HTTP/1.1 POST over TCP to a listener inside the resident, and two guest-side senders that speak
the same wire contract: a shell script (instructions for a person or for the agent inside the
guest) and a static binary `aherald-ping`. The host parses the raw hook payload with the existing
`Agent::parse_hook`; the sender never parses agent formats and never supplies `window`/`tty`.
Out of scope: TLS, cloud sandboxes, relays, multi-user use, a daemon in the guest, Unix-socket
bind mounts, non-native harnesses (OpenCode, Pi, Goose, Crush) in the first release.

- [ ] **AH-229 — `aherald-ping --proxy <URL>` for guests that only reach the host through an HTTP proxy.**
  Priority: low. Today only the shell sender works under `srt` (it reaches the host through the
  proxy that `srt` sets for `curl`); the static `aherald-ping` cannot connect there. Add an explicit
  `--proxy URL` flag (and `AHERALD_PROXY`), never read from `http_proxy`/`no_proxy`: `srt` lists the
  Docker/LAN ranges in `no_proxy` as CIDR, which newer curl treats as "direct" and breaks, so guessing
  from the environment is unreliable. With a proxy set: connect to it, send the request in absolute
  form (`POST http://host:port/v1/hook?agent=...`), send `Proxy-Authorization: Basic` from the URL's
  `user:pass@` (hand-written base64, no dependency), keep the same one-second deadline and the
  silent exit 0. Estimate: 60 to 90 lines plus 60 to 80 lines of tests, a few KB of binary size.
  Acceptance: unit tests with a fake proxy (absolute-form request, credentials, refused proxy,
  hung proxy under 3 s, no proxy means unchanged behaviour); `tests/sandbox_runtimes.rs` srt test
  also passes the static sender; `docs/sandboxes.md` srt row says to use `--proxy`. Only `srt`
  0.0.81 can be checked here; Docker Sandboxes still needs its own policy rule for the port.

- [ ] **AH-230 — Non-native harnesses (OpenCode, Pi, Goose, Crush) in containers and VMs.**
  Priority: low. Start with Goose and Crush (their bridge is one command line); do OpenCode and Pi
  after. Their bridges (`examples/harnesses/`) run `aherald --notify <key> --kind K [--activity|--ended]`
  with an optional stdin `{session_id, cwd|working_dir}`; the harness states the kind, so the host
  parses nothing, and nothing private is in the payload. Work: host route
  `POST /v1/notify?harness=KEY&kind=K[&ended]` in `src/remote.rs` (validate with `is_harness_key`,
  build the event with `events::external_event`, same token, limits and `respond()`; activity events
  are not accepted); `aherald-ping --notify KEY --kind K [--ended]` with the same flags as
  `aherald --notify` (drops activity locally) and a `--notify` branch in `aherald-ping.sh`; bridge
  files pick the program from `AHERALD_PATH` or are edited by hand; a recipe per harness in
  `docs/sandboxes.md` and the guest instructions. Check first whether the host drops events from a
  harness that `--setup` has not enabled; if it does, add a small `[remote]` switch.
  Estimate: 250 to 350 lines with tests, about half a day, +1 to 3 KB of binary size.
  Acceptance: host and sender tests with fake stdin (kind, `ended`, activity refused, bad key,
  bad token, oversize); the existing `--notify` tests unchanged. Limits: none of the four harnesses
  is installed here, so that a harness really calls its bridge from a guest stays unverified;
  context and idle warnings do not work for guest sessions.

## Milestone 12 — Audit follow-ups (2026-11-05)

Source: `audit-20261105.md` (local, untracked), an architecture, CI and release audit of v0.15.0.
Conventions for these tasks: test first wherever a test is named; this repository's `AGENTS.md`
applies (dependencies, parity, per-platform status wording). `src/mac/` builds only in the macOS CI
and the Windows modules only in the Windows CI: report such parts as "CI only", never as verified.
Always run `cargo fmt --all -- --check`, `cargo clippy --locked --workspace --all-targets -- -D warnings`
and `cargo test --locked --workspace --all-targets`, and push a branch so CI covers the other platforms.
P2 tasks run in the listed order; AH-266 goes last.

### P0 — defects and wrong statements

- [ ] **AH-231 — Linux resident: no periodic wake-ups while idle.**
  `src/linux/resident.rs:836` sleeps 16 ms on every pass even with nothing shown. Make the loop block
  until there is work: an X11 event, an IPC/queue change, the next warning or usage deadline, or an animation
  frame. Suggested approach: wait with `poll(2)` (libc is already a dependency) on the X connection fd
  (`x11rb` `RustConnection` exposes it via `AsRawFd`) plus an `eventfd`/pipe that `Shared` writes to when
  `Controls` changes state (mirror the macOS `Waker`, `mac/resident.rs:105-171`). The timeout is the
  earliest warning deadline, 16 ms only while animating or displayed.
  **Caution:** today the Linux tray picks up new usage snapshots only because of the 16 ms poll; the usage
  services run on their own threads and the UI loop has no usage deadline. Wire `usage.on_change` and
  `claude.on_change` (`src/usage_service.rs:821`) to the same eventfd, as Windows does at
  `src/resident_overlay.rs:1298-1300`. Without this, blocking reproduces the macOS bug (AH-232) on Linux.
  Acceptance: a new ignored-under-xvfb test like Windows `idle_resident_has_no_periodic_wakeups…`
  (`src/resident.rs:538`) counts loop passes over 2 s idle (≤ 3); a test with a stub `UsageSource` shows a
  changed snapshot reaching the tray levels within one wake-up; cards, the tray menu and warnings still work
  (`xvfb-run … cargo test --test linux_apply -- --ignored --test-threads=1` and `--test remote_listener` pass).
  Verifiable on Linux.

- [ ] **AH-232 — macOS: Claude-only usage refreshes while idle.**
  `src/mac/resident.rs:1228` decides the idle wait from `codex.usage.enabled` only. Windows also checks
  `claude.is_some()` (`src/resident_overlay.rs:229`) and subscribes to `on_change` (`:1298-1300`).
  Extract `pub(crate) fn idle_poll_needed(codex_usage: bool, claude_usage: bool, has_deadline: bool) -> bool`
  into `src/resident_request.rs` with unit tests, and use it in the Windows timer condition and the macOS
  wait. Better still, also subscribe macOS to `usage.on_change`/`claude.on_change` with the existing
  `Waker`. Acceptance: unit tests for all 8 combinations pass on Linux; macOS CI compiles. Not run:
  live tray on a Mac (add a manual check line).

- [ ] **AH-233 — Same Apply diagnostics on every OS.**
  On an invalid config, Linux posts a different message and does not log `InvalidConfiguration`
  (`src/linux/resident.rs:354-360` vs `src/resident_overlay.rs:752-756`, `src/mac/resident.rs:557-563`). Add
  `pub(crate) fn apply_outcome(path: &Path) -> Result<Config, ApplyError>` to `src/resident_request.rs`,
  where `ApplyError` has `message()` and logs the record once. Call it from all three. Acceptance: a unit test
  for valid/invalid/missing files and the exact message; Linux behaviour checked under xvfb; the macOS and
  Windows CI compile and pass.

- [ ] **AH-234 — `--install-hooks` honours `CLAUDE_CONFIG_DIR` and `CODEX_HOME`; README recommends `--setup`.**
  `hook_setup::default_file` (`src/hook_setup.rs:70-75`) hard-codes the home paths. Replace its body with the
  lookup in `setup::file_for` (`src/setup.rs:511-521`); move that function to `paths.rs` and use it from both.
  The same applies to the statusLine target in `main.rs:208-216`. Change README.md:73 to
  `aherald --setup --harness claude --apply  # or codex`, and keep `--install-hooks` documented as
  the legacy path in docs/installation.md:138.
  Acceptance: a test in `tests/hooks.rs` with `CLAUDE_CONFIG_DIR` set writes there; the same for `CODEX_HOME`.

- [ ] **AH-236 — Claude plugin version follows the crate.**
  `plugins/aherald/.claude-plugin/plugin.json` says 0.1.0. Set it to the crate version and extend
  `tests/workspace.rs` to assert `plugin.json.version == CARGO_PKG_VERSION`.
  Acceptance: the test fails before the fix and passes after; the release checklist (AH-240) lists the file.

- [ ] **AH-237 — Bridge contract tests cannot skip silently in CI.**
  `tests/bridges.rs` returns success without Node ≥ 22. Add `AHERALD_REQUIRE_NODE=1`: when it is set and Node
  is missing, the test panics. In `ci.yml`, on the `ubuntu-22.04` leg of `quality`, add a SHA-pinned
  `actions/setup-node` with `node-version: 22` before `cargo test`, and set the env var for that step.
  Acceptance: the CI log shows the bridge test executing (`node --test` output on failure); zizmor and actionlint pass.

- [ ] **AH-238 — Help text lists every public flag.**
  Move `help()` from `src/lib.rs` to `src/cli.rs`, as a multi-line `concat!` grouped by purpose (hooks,
  resident control, setup, usage, diagnostics). Add the missing `--notify --kind --activity --ended`,
  `--agent`, `--setup-window`, `--setup-executable` and `--help`; leave the internal `--*-child` and
  `--test-after-apply` out. Add a unit test with a list of public flags that asserts each one appears in
  `help()` and is accepted by `Cli::parse` in a minimal valid combination. Update
  `tests/unsupported_platform.rs` and `tests/config_portable.rs:37`.
  Acceptance: the tests pass; `aherald --help` is readable at 100 columns.

- [ ] **AH-239 — The two shell guest senders stay identical.**
  Done on `docs/agents-release-privacy` (2026-11-05): README.md and `docs/privacy.md` state that `aherald-ping`
  strips the payload while the shell sender and the plugin launcher post it whole over plain HTTP. Left: add a
  test that `examples/sandbox/aherald-ping.sh` and the remote block of `plugins/aherald/hooks/aherald-hook.sh`
  stay identical: extract the lines between marker comments `# BEGIN remote` / `# END remote` and compare them
  in `tests/sandbox_sender.rs`. Acceptance: the test fails when one copy changes.

- [ ] **AH-271 — Split the remaining partly shipped tasks (follow-up of AH-241).**
  `CHANGELOG.md` cites nine IDs that are still open in TODO.md because their manual or live checks are
  outstanding: AH-093, AH-102, AH-103, AH-115, AH-117, AH-118, AH-120, AH-211, AH-222. For each, move the
  shipped part to DONE.md with its status lines, and keep or open an ID holding only the checks still
  marked "Not run" / "not yet checked" (as AH-268 to AH-270 did for AH-206/213/219/220). Do not mark a
  check done without its recorded evidence.
  Acceptance: every ID in CHANGELOG exists in DONE.md; no open task describes shipped code as pending.

### P1 — consistency, CI and the release pipeline

- [ ] **AH-242 — Shared quoting helpers.**
  Add `src/quote.rs` with `pub(crate) fn posix(s: &str) -> String` (single-quote, `'\''`) and
  `pub(crate) fn powershell(s: &str) -> String` (`''`). Replace the copies at `hook_setup.rs:59,64,223,228`,
  `setup.rs:474,932,962` and `bridge_setup.rs:87`. Acceptance: unit tests (spaces, quotes, `$`, newline, empty
  string); `tests/hooks.rs`, `tests/setup.rs` and `tests/setup_bridges.rs` pass unchanged.

- [ ] **AH-243 — One atomic file writer.**
  Add `src/atomic_file.rs`: `write(path, bytes, backup: bool)` = sibling temp file, `sync_all`, optional
  `.aherald-backup` copy, rename (Windows: `MoveFileExW` with `REPLACE_EXISTING|WRITE_THROUGH`). Use it in
  `hook_setup::write_inner` (`hook_setup.rs:399-436`) and `setup::apply_change` (`setup.rs:584-586`). Keep
  `default_config::create` as is (it is exclusive-create, a different contract) but reuse its temp-name code.
  Acceptance: unit tests (new file, replace, backup content, temp cleaned up on error); the setup tests pass.

- [ ] **AH-244 — One civil-date implementation.**
  Move `days_from_civil`/`civil_from_days` from `operational_log.rs:45-70` into `src/time.rs` and make
  `claude_usage::iso_to_unix` (`claude_usage.rs:191-232`) use it. Acceptance: table tests (epoch, leap day,
  2100-03-01, a negative offset), plus the existing log and usage tests.

- [ ] **AH-245 — Templating out of `webhook.rs`; honour Retry-After.**
  Move `expand_env`, `render` and `Escape` (`webhook.rs:21-133`) to `src/template.rs`; `webhook.rs` and
  `command_sink.rs` import from it. In `sink.rs`, let `send` return an error with an optional retry delay; when
  `Webhook` sees 429/503 with `retry_after`, wait `min(retry_after, 30 s)` instead of the fixed 2 s backoff.
  Acceptance: `tests/forward_webhook.rs` gets a fake-HTTP case with `Retry-After: 1` that is retried after ≥1 s;
  existing forward tests pass.

- [ ] **AH-246 — One source for harness labels and bridge lists.**
  `events.rs:124-129` (`HARNESS_LABELS`) must take labels from `Bridge::label` (plus Crush). `setup_selection.rs`
  and the Linux console prompts (`main.rs:472-476`) iterate `Bridge::ALL` instead of naming fields. Leave the
  Windows `.iss` and the macOS window as they are, but add a test that `installer/windows.iss` mentions every
  `Bridge::key()`. Acceptance: adding a variant to `Bridge` fails a test until the installer is updated.

- [ ] **AH-247 — Contract test between aherald-ping and the main hook parser.**
  `wanted(agent, &Value)` (`crates/aherald-ping/src/lib.rs:75`) is private; expose it as
  `#[doc(hidden)] pub`. In a new `crates/aherald-ping/tests/parser_contract.rs`, build one payload per hook
  event name (Claude: `Stop`, `StopFailure`, `Notification`, `PermissionRequest`, `UserPromptSubmit`,
  `SessionEnd`, …; Codex: the names in `tests/fixtures/codex-*.json`, `stop.json`, `permission-request.json`,
  `interrupt.json`) and assert that `wanted` is true exactly when
  `aherald::agent::Agent::{Claude,Codex}.parse_hook(bytes)` (`src/agent.rs:46`) returns `Ok(Some(event))` for a
  turn end or a wait. List the deliberately dropped events (activity, session end) in the test with a reason.
  Acceptance: renaming or adding an event on either side fails the test.

- [ ] **AH-248 — MSRV on every OS.**
  Turn the `msrv` job (`ci.yml:85-109`) into a matrix `[windows-latest, macos-15, ubuntu-22.04]` gated by
  `needs.changes.outputs.os`; on Linux install `libpulse0`. Keep `cargo check --locked --workspace`.
  Acceptance: the job is green on all three; `ci-gate` still needs it.

- [ ] **AH-249 — `[lints]` table.**
  Add a `[workspace.lints]` table, with `[lints] workspace = true` in both crates:
  `rust.unsafe_op_in_unsafe_fn = "warn"` and `clippy.undocumented_unsafe_blocks = "warn"`.
  Step 1: count the findings per OS (Linux locally; Windows and macOS from one CI run on a draft branch) and
  record the counts in the PR. If one OS has more than ~50, add `#![allow(clippy::undocumented_unsafe_blocks)]`
  to its platform modules (`src/mac/*`, the Windows root modules) with a `// TODO(AH-249)` and enforce the lint
  only on shared and new code. Step 2: fix the remaining findings with real `// SAFETY:` comments, never empty ones.
  Acceptance: clippy `-D warnings` green on all three; the per-OS counts are in the PR description.

- [ ] **AH-250 — cargo-deny for licenses and bans.**
  Add `deny.toml` (allow MIT, Apache-2.0, BSD-2/3, ISC, Unicode-3.0, Zlib; deny unknown registries and git
  sources; `multiple-versions = "warn"`). In `ci.yml`, replace `cargo audit` with a SHA-pinned
  `EmbarkStudios/cargo-deny-action` (`check advisories licenses bans sources`), run on `deps == 'true'` and in
  nightly. Acceptance: the job passes; a deliberately disallowed license in a scratch branch fails.

- [ ] **AH-251 — One skip list for noisy tests; release uses it.**
  Decided 2026-11-05: this replaces the AH-221 verdict ("Release still runs it"); the nightly run keeps
  the noisy checks. Move the four `--skip` names from `ci.yml:189-196` to `.github/noisy-tests.txt` (one per line), read by a
  small step that builds the arguments. Use the same list in the three test steps of `release.yml`. Update
  AH-221's note in TODO.md (the release no longer runs the memory-growth check; nightly does). Acceptance: PR and
  release logs show the same skips; the nightly run still runs them.

- [ ] **AH-252 — Reproducible build stamp.**
  In `release.yml` and `ci.yml`, export `SOURCE_DATE_EPOCH=$(git log -1 --format=%ct)` before building. In
  `build.rs`, add a comment explaining that without it the stamp is the last build-script run. Acceptance:
  two release builds of the same commit print the same `--version` line.

- [ ] **AH-253 — One version reader for scripts; scripts move to `scripts/`.**
  Move `package-*.sh`, `package-release.ps1`, `build-installer.ps1`, `check-release*.{sh,ps1}` into `scripts/`.
  Add `scripts/version.sh` (`crate_version`, `require_tag_matches`) and `scripts/Version.ps1`, both based on
  `cargo metadata --no-deps --format-version 1` (python3 or `ConvertFrom-Json`), so `tests/workspace.rs:26-35`'s
  sed constraint can go. Update `ci.yml`, `release.yml`, `.github/platform-flags.sh` (one `scripts/*` rule per
  OS suffix), `tests/ci_platform_flags.sh`, README and docs/portable.md. Keep `generate_*.rs` as Cargo examples
  but move them to `examples/tools/` and update `[[example]]` paths.
  Acceptance: the CI package steps and the nightly full run pass; `ls *.sh *.ps1` in the root is empty.

- [ ] **AH-254 — Docs index and engineering notes split.**
  Create `docs/README.md` with user guides grouped (install, configure, harnesses, sandboxes, troubleshooting).
  Move the engineering notes (`ah-0xx-*`, `milestone-*`, `*-investigation`, `wayland-spike`, `diagnostics-audit`,
  `session-warnings-research`, `plans/`) to `docs/notes/` with `git mv` and fix links (`grep -rn "docs/ah-"`).
  Fix README.md:174: nextest is not the test runner (the decision is in DONE.md); point to `tests/live/README.md`
  and `docs/notes/ah-109-nextest.md` as history. Acceptance: a link check (`grep -o '](docs/[^)]*)'` resolves)
  passes for README, docs and TODO.

- [ ] **AH-256 — Default config parity test.**
  In `tests/config.rs`, parse `config.example.toml` and compare the whole `Config` with `Config::default()`
  (derive or implement `PartialEq` where missing), with an explicit allowlist of intended differences and a
  comment for each. Acceptance: changing a default in only one place fails the test.

### P2 — structural refactoring of the residents (in this order)

- [ ] **AH-257 — Shared tray menu model.**
  Generalize the pure `src/mac/menu.rs` into `src/tray_menu.rs`: `enum Command` (Pause, Sound, Popup,
  Messenger, ContextWarnings, IdleWarnings, Usage, EditConfig, Apply, ApplyTest, About, Quit, …), `struct Item
  { command, label, checked, enabled, separator_before }`, and `fn items(state: &MenuState) -> Vec<Item>`.
  Windows (`tray.rs:9-22,207-258`) maps `Command` ↔ numeric IDs with a `match`; Linux (`linux/tray.rs:137-185`)
  maps it to dbusmenu IDs; macOS uses it directly. Labels exist only in `tray_menu.rs`. Acceptance: the pure
  tests (order, separators, Messenger only with a sink, checked states) run on Linux; all three CI legs pass;
  manual check lines for each OS's tray.

- [ ] **AH-258 — Shared command effects.**
  Depends on AH-257. Add `fn apply(command, &mut Shared, &Config) -> Vec<Effect>` to `src/resident_request.rs`,
  with `Effect` covering Redraw, CancelAudio, OpenConfig, Restart{test}, ShowNotice(..), Quit and the rest. The
  three dispatchers (`resident_overlay.rs:707-826`, `linux/resident.rs:~300-420`, `mac/resident.rs:517-628`)
  only translate a platform event to `Command` and execute the `Effect`s. AH-233's `apply_outcome` becomes part
  of it. Acceptance: unit tests for each command's effects; the existing tray tests pass on all OS.

- [ ] **AH-259 — Shared `Shared` state and constructor.**
  Move the near-identical `Shared` struct, its `Controls` impl, `toggle_warning` and the constructor
  (`resident.rs:23-48,133-140`, `linux/resident.rs:30-60`, `mac/resident.rs:49-81,284`) into
  `resident_request.rs`. Platform extras (the Windows `window` handle) go into a generic `ext: T` field.
  Acceptance: no behaviour change; all CI legs pass.

- [ ] **AH-260 — Shared resident runtime start and teardown.**
  Depends on AH-259. `resident_runtime::start(config, scope, wake: impl Fn()) -> Runtime` performs
  `notice::install`, `forward::start`, `remote::start_if_enabled` and the listener thread
  (`serve_until`→`respond`→`diagnostics.record`, including `perf::tick`, made available to every OS). `Drop` tears
  down in the existing order. Replace the three ~100-line copies. Acceptance: `tests/resident.rs`,
  `tests/mac_resident.rs`, `tests/remote_listener.rs` (xvfb) and `linux_apply` pass; Linux now logs perf ticks.

- [ ] **AH-261 — Shared usage view.**
  Extract `codex_shown`, the usage card text and pagination, and the `Levels::from_views` input
  (`resident_overlay.rs:110-140,267,559`, `mac/resident.rs:365-470,752`, `linux/resident.rs:300-315`) into pure
  functions in `tray_usage.rs`. Subscribe macOS and Linux to `Service::on_change` as Windows does. Acceptance: pure
  tests for Codex-only, Claude-only and both; drawing code untouched.

- [ ] **AH-262 — Linux resident as a `Ui` struct.**
  Depends on AH-231 and AH-258. Turn the ~25 locals of `present` (`linux/resident.rs:244-298`) into a
  `struct Ui` with `step`, `handle_*` and `draw` methods, mirroring `mac/resident.rs`. Move artwork preparation
  to a worker thread like Windows and macOS (`resident_overlay.rs:1122-1160`). Acceptance: the xvfb tests pass;
  no function in the file exceeds ~150 lines.

- [ ] **AH-263 — Compile-time contract for platform twins.**
  Align the signatures: `restart::launch(test: bool)` on both (delete the unused `unix/restart.rs:10 launch()`),
  and `shell::open -> Result<(), String>` on both. Add `src/platform_contract.rs` (`#[cfg(test)]`) that pins
  each twin with `const _: fn(bool) -> … = crate::restart::launch;`, and the same for `pipe::request`,
  `local::Scope`, `detached`, `https::request_at` and `shell::open`. Collapse the duplicated
  `get/post/get_at/post_at` wrappers into `http_common.rs` over the per-OS `request_at`. Acceptance: the
  contract module compiles in all three CI legs.

- [ ] **AH-264 — CLI operations as an enum.**
  Replace `Cli.operation: Option<String>` (`cli.rs:6`) with `enum Operation`, and the `if` chain in
  `main.rs:73-433` with a `match` calling one function per operation (moved into `src/commands/` or kept in
  `main.rs`). Parse the child modes (`main.rs:76-131`) in `Cli::parse`. A small parser crate (`lexopt`,
  `pico-args`) is allowed if it removes code (justify it in the PR per `AGENTS.md`). Acceptance: `tests/cli.rs`
  and `tests/aherald_cli.rs` pass; `main.rs` is under 250 lines.

- [ ] **AH-265 — `--install-hooks` delegates to the setup backend.**
  Depends on AH-234, AH-242 and AH-243. Make `--install-hooks X` / `--uninstall-hooks X` run `setup` for that
  single harness (receipt, exact matching, rollback, exec form), keeping `--hooks-file` as a target override.
  The same for `--install-statusline` on unix (Windows already uses `setup::run_statusline_only`). Then delete
  the planner code left unused in `hook_setup.rs`. Migration: entries written by the old path (a shell
  string containing "aherald" and `--hook`) are recognised and replaced once. Acceptance: `tests/hooks.rs`
  rewritten against the new behaviour, a migration test for an old-style entry, and `hook_setup.rs` under 300 lines.

- [ ] **AH-266 — Windows modules into `src/windows/`; fewer test binaries.**
  Depends on AH-257…AH-263 being merged (to avoid conflicts). `git mv` the Windows-only root modules into
  `src/windows/` (`#[path]` wiring in `lib.rs` as for `unix/`), make the 7 unused `pub mod`s `pub(crate)`, and
  group the `lib.rs` declarations. Merge the ~45 public-API-only integration tests into `tests/api/main.rs`
  (one module per former file). Simplify `.github/platform-flags.sh` to `src/windows/*`, `src/linux/*`,
  `src/mac/*`, `src/unix/*` and add a self-check to `tests/ci_platform_flags.sh`: every `tests/*.rs` with a file-level `#![cfg(windows)]`
  (or unix, linux, macos) classifies to exactly that platform, and vice versa.
  Acceptance: measure `cargo test --no-run` wall time on Linux before and after (record both); all CI legs and
  the nightly run pass.
