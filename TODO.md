# aHerald implementation plan

aHerald is a portable, native Windows 10/11 x64 notification companion. Codex is
the first supported agent. Ship one executable, `aherald.exe`, built with stable
Rust and the MSVC toolchain. Start with Milestone 1 and keep each milestone usable.

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
covers automated checks only. New direct dependencies need user approval per AGENTS.md.
AH-060 rewrote the Windows-only target statements in AGENTS.md, INFO.md, README and Cargo.toml.

- [ ] **AH-097 — Native setup and Windows installer (replaces the launcher split from AH-091).**
  Provide `--setup --harness codex|claude` (repeatable) and `--unsetup`, dry-run by
  default and explicit --apply. Call the fixed executable inside Codex's existing
  Windows PowerShell executor without starting another PowerShell process;
  verify real hook arguments, response, process ancestry and diagnostic timings
  in the opt-in live group. Resolve the executable at setup time; preserve
  unrelated settings/hooks, back up changes, preflight all files, keep a receipt
  for exact uninstall, and make repeated setup idempotent. Supply a per-user Inno
  Setup installer with harness checkboxes and native setup delegation, stable
  installation paths, safe updates/removal, and portable ZIP alongside setup EXE
  in Windows CI. After preflight, gracefully stop the resident from the selected
  directory and wait for its exit before update/removal; abort on shutdown
  failure and preserve residents from other directories.
  Offer a checked launch-after-install option on successful
  interactive setup, with no launch during silent installation or failed setup.
  Add independent Codex/Claude usage checkboxes, off on new installations and
  loaded from existing configuration on upgrade. Explain Claude auto (local
  login, then statusLine); preserve its configured source, other settings,
  comments and formatting. Include configuration changes in setup rollback.
  Offer per-agent context/idle warning checkboxes, loaded from live configuration.
  First context enablement supplies Codex 250k / Claude 400k only when no threshold
  exists; idle uses Codex 25 / Claude 55 minutes. Preserve custom thresholds when
  disabling or re-enabling warnings, with separate persisted enabled flags.
  When Claude context is enabled for a selected Claude harness, configure its
  statusLine wrapper transactionally, preserve the original command and options,
  avoid nesting on repeat setup, and restore only an unchanged owned wrapper on
  disable, deselection or uninstall.
  Verify generated hooks, delivery and dismissal, installer GUI,
  selection changes, upgrade/uninstall, and a clean Windows machine without
  WSL/Bash or development runtimes. See docs/installation.md.
  Status 2026-11-01 (branch `feature/ah-097-windows-completion`): the setup backend,
  Windows installer and CI packaging were already implemented. Fixed setup taking
  over other installations' hooks; a regression failed before the fix and passes
  afterward. Setup checks: 26 passed, one optional Git Bash test ignored. The
  isolated installer lifecycle and real-Codex launch/response/ancestry/timing probe
  passed; formatting, Clippy, release build and packaging passed. Full Cargo tests
  stopped at `native_assets` with a notification startup timeout, both inside and
  outside the sandbox; cause remains unresolved. Full installer GUI/launch-option
  and installed-harness delivery/dismissal acceptance remain open. Clean-machine
  acceptance is explicitly deferred by the maintainer (no machine available).
  Evidence, commands and remaining checks: docs/ah-097-windows-acceptance.md.

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

- [x] **AH-116 — Profile the macOS cost of `--statusline --then` on real hardware.**
  On a GitHub macOS runner (2026-11-02) `aherald --statusline --then true` took 8.4 and
  9.6 ms more than starting `sh` alone, while the bare wrapper costs about 1.2 ms over
  `aherald --version` (as on Linux) and a test process starting `sh -c true` costs 0.08 ms.
  Spawning a child from the aHerald process seems slower on macOS; the cause is unknown.
  Reproduce on the user's MacBook Pro M1 with `cargo test --test perf_statusline --
  --nocapture`, then profile the spawn (Instruments or `sample`, `dtruss`): suspects are the
  size of the process image, `posix_spawn` versus fork, and the piped stdin/stdout setup in
  `statusline::run_then`. Acceptance: the cause is recorded in
  docs/statusline-performance.md and either fixed, or the 12 ms macOS allowance
  (`THEN_SPAWN_ALLOWANCE_MS` in tests/perf_statusline.rs) is justified by the measurement
  or removed. Low priority: a status line refreshes at most every 300 ms.
  Done 2026-11-03 on the M1: `--then true` costs 3.3-3.5 ms over the bare wrapper, equal to
  starting `sh` (3.28 ms); the spawn adds nothing. The CI figure came from subtracting the
  `--version` median, which clamped the shell's cost to zero. The macOS allowance is removed and
  the check uses the absolute `sh -c true` median (docs/statusline-performance.md).

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


- [ ] **AH-095 — Evaluate native layer-shell on KDE/wlroots (split from AH-073).**
  When a KDE or wlroots session is available, measure protocol support, placement,
  focus, scaling and agent activation; compare the native backend with XWayland and
  agree on scope and dependencies before implementation.

## Milestone 5 — Polish and portable release

- [x] **AH-105 — Create an application icon, at least for Windows.** Design and
  add an aHerald application icon, with Windows as the minimum supported platform.
  Acceptance: the built Windows executable displays the icon correctly in Explorer
  at common icon sizes and display scales. Implementation and verification:
  [Windows application icon](docs/app-icon.md). Completed 2026-11-01: embedded
  multi-size resource, native release-EXE loading tests and Explorer screenshots accepted.

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

- [ ] **AH-020 — Finish placement and display-change handling.** Depends on AH-012.
  Support center, top center, bottom right, near cursor, and selected monitor.
  Keep overlays in the work area and handle monitor removal, negative desktop
  coordinates, DPI changes, and scaling. Acceptance: manual multi-monitor and
  mixed-DPI checks pass without focus theft or screen-wide flashing.
  Implementation and automated checks committed; manual multi-monitor/mixed-DPI
  acceptance remains unverified (hardware unavailable, confirmed 2026-10-25).
  Also carries the case split from AH-076: a card that opens on a second monitor with a
  different DPI is drawn at that monitor's scale (cards are not draggable by design).

- [x] **AH-090 — No silent decisions: a log line for every dropped input, an error card for
  the ones the user can fix.**
  aHerald sometimes drops or degrades what was configured or sent without a trace: a sound
  longer than five seconds, larger than 2 MiB, not 16-bit PCM or missing is ignored
  (`audio::cached` and `Wave::parse` errors vanish), and other `let _ =` or `if let Ok` sites
  swallow a failure. First, audit every such site (sounds, artwork, configuration values that
  fall back to defaults, events refused by validation or the queue, clicks that find no agent,
  usage polls, Keychain and file reads) and list them here. Then, at least: one operational log
  line per case with a fixed code (never a payload, a prompt, a token or a full path), once per
  failing episode rather than per event. Second channel, for failures the user can act on (a
  refused sound or artwork file, an invalid configuration value): the standard notification card
  with the error icon, shown once per episode and never from the hook process, which must stay
  fast and silent. Acceptance: each audited case has a test that the record is written and,
  where it applies, the card is queued; hook behavior and its stdout contract are unchanged.
  Decide when the card is built: whether it is a normal `error` event from aHerald itself and how
  repeats are throttled.
  Status 2026-10-28 (branch `feature/ah-090-no-silent-decisions`): log channel started.
  Done, each with a test that the line appears once per failing file state and never carries a
  path: a sound that cannot be used (`audio.rejected unreadable|too_large|too_long|unsupported|
  malformed`, from the new `audio::SoundError`), a sound that cannot be played (`audio.failed`,
  once per run of failures), a configured custom animation or pet that falls back to the built-in
  picture (`artwork.rejected custom|pet`). Found by the audit and fixed: log timestamps on Unix
  carried a +00:00 offset whatever the zone (`log_local_offset` test).
  Status 2026-10-28 (branch `feature/ah-090-audit-rest`): audit items closed with tests: hook
  input that is unreadable, too large, late or not JSON (`hook.rejected invalid|too_large|timeout|
  unreadable`, Warn, fixed codes, no input; stdout and exit code unchanged); a valid event that
  shows nothing on purpose (`event.dropped disabled|duplicate|obsolete`, Debug, from
  `resident_request::Effects::dropped`); a `monitor` with no such display, now one portable
  `notice::MonitorWatch` used by macOS and by the Windows placement (card and log line once per
  set of connected displays). Decided not to log: a hook event name the agent does not report on
  (normal traffic, e.g. every Claude event configured), and the `kill`/`wait` cleanup `let _ =`
  in `resident_client` (failures there already return an error to the hook's stderr).
  Branch `feature/ah-090-last-silent-sites` (2026-10-29): the last two silent sites now log, with
  tests: `artwork.reduced` (Warn, once per scale) and `hook.delivery_failed` (Warn, hook cannot
  reach, start or queue to the resident; stdout and exit code unchanged). Noticed, not changed:
  with an unusable runtime directory the hook fails before it prints its reply. The Windows look of
  the notice card and the Windows `monitor` notice are not seen on a Windows machine.
  Decided 2026-10-28 with the maintainer: the second channel is an ordinary `error` card that
  belongs to neither agent (`Source::App`, headed "aHerald notice", then only the message in up to
  three lines and no kind row, the built-in
  error picture only, no pet, custom artwork, sound or per-event setting, always shown for 8 s),
  once per file like the log line, and also for configuration problems found at start. Built on
  branch `feature/ah-090-notice-card`: `events::Event::notice`, `notice` (sink installed by a
  resident, so hooks and transient cards stay silent), `resident_request::notice_sink`, a card for
  a refused sound, an unplayable sound, unusable artwork, and (macOS) a `monitor` that is not
  connected (`display.monitor_missing`). Seen live on the M1 as log lines and three queued cards;
  the cards were looked at on the M1 (red error icon, message wrapped in up to three lines). Remaining: the missing
  `monitor` notice on Windows, the Windows look of the notice card, and the rest of the audit above.
  Status 2026-11-01 (branch `feature/ah-090-completion`): audited decisions and executable
  evidence are listed in [the diagnostic audit](docs/diagnostics-audit.md). Added portable
  usage/name-lookup failure episodes; invalid-input, framing and full-queue episodes; bounded
  activation-mailbox diagnostics; per-agent pet, missing built-in and preparation-budget
  diagnostics. Interactive startup configuration errors show a separate eight-second notice
  and retain exit 2; hooks stay prompt and card-free. Hook replies now precede runtime-scope
  creation. The Unix runtime regression requires Unix execution. Windows monitor fallback
  passed its native log/card test. The real resident notice was inspected on Windows build
  22621.4317: built-in red error picture, two-line message, no kind row, 8 s timer.
  Verification completed on Windows: formatting, Clippy and full `cargo test --all-targets`
  passed, including the performance baseline and native asset checks. The asset test now
  separates Windows creation of a fresh copied executable from rendering checks; production
  delivery budgets are unchanged. See [the startup investigation](docs/native-assets-investigation.md)
  and the audit for observed timings and platform limits.

- [ ] **AH-054 — Capture a live Claude `StopFailure` payload (split from AH-036).**
  Depends on AH-036. The mapping (`StopFailure` to `error`, resolving same-session
  pending cards) is covered by automated tests, but no live payload exists. A bad model
  name and an invalid `ANTHROPIC_API_KEY` with `claude -p` fail before a turn starts and
  fire no hook (2026-10-26). Try an interactive session that hits a real API error
  (for example a rate limit) with the capture hook, then compare the fields (`error`)
  with `docs/claude-hooks.md`. Acceptance: a captured payload matches the documented
  fields, or the doc records the difference and the mapping is adjusted test-first.

- [ ] **AH-045 — Harden the Claude quota HTTPS request.** Depends on AH-037.
  Implemented and tested; left open only for the manual system-proxy check (steps in
  `docs/claude-hooks.md`, "Manual checks"), which was not run: on 2026-10-27 the dev
  machine had no proxy (`netsh winhttp` direct, `ProxyEnable=0`, no PAC), and direct
  access works. Needs a machine with a manual or PAC proxy. Done: redirect policy
  "never" plus 3xx mapped to "Claude usage request redirected (refused)", proven by a
  loopback test where a 302 gets exactly one request; `AUTOMATIC_PROXY`; `Wiped`
  buffers for the credentials file, token and headers (copies made by the parser,
  std or WinHTTP are not covered). Original brief: WinHTTP follows redirects by default, and `WINHTTP_ACCESS_TYPE_DEFAULT_PROXY`
  ignores system and PAC proxies. Disable redirects so the bearer token reaches only
  `api.anthropic.com` and report 3xx as a sanitized service error; switch to
  `WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY`. Clear header buffers holding the token after
  use without adding a dependency. Acceptance: a test against a local endpoint shows
  a 3xx response becomes an error without a follow-up request (if the client cannot
  target one, document a manual redirect check instead); a manual check behind a
  system proxy succeeds; standard Cargo checks. Source: `audit-20261026.md` E6.

- [ ] **AH-211 — Windows: show the output of command-line operations in the terminal.**
  `aherald.exe` is a GUI-subsystem program (so a hook never opens a console window), and it has no
  console of its own: `--version`, `--help`, `--status`, `--setup`, `--perf-report` and their error
  messages print nothing in PowerShell or cmd unless the output is piped (`| Out-String`), which
  the packaging scripts and docs already work around. Do it on Windows 11, not by reading code.
  Approach to try: at start, when standard output and error are not already redirected (hooks and
  pipes are, so they are untouched), call `AttachConsole(ATTACH_PARENT_PROCESS)` and point the
  standard handles at `CONOUT$`/`CONIN$`; with no parent console (Explorer, the tray, autostart)
  do nothing. The shell does not wait for a GUI program, so a prompt can print before the output:
  decide whether that is acceptable or whether `--status`-like operations should wait.
  Acceptance (checked by hand on Windows 11, Windows Server 2019 and in a CI pipe test):
  `aherald --version` and an unknown option print in PowerShell, cmd and Windows Terminal without
  `| Out-String`; with output piped or redirected nothing changes; a hook from Claude Code and
  Codex still delivers and opens no console window; `Start-Process` and the tray start leave no
  console behind; `package-release.ps1` no longer needs its pipe workaround.

- [ ] **AH-212 — Windows: a readable `statusLine` command.** The wrapper in
  `~/.claude/settings.json` is a PowerShell one-liner whose script is spelled as `[char[]]` codes
  (so neither Git Bash nor PowerShell expands it), which nobody can read or audit. Claude Code's
  `statusLine` has no `args` or `shell` field (only `type`, `command`, `padding`,
  `refreshInterval`, `hideVimModeIndicator`), so there is no exec form as for hooks. Its own
  Windows recipe is `powershell -NoProfile -File C:/path/script.ps1` (forward slashes work in both
  shells). Write the script as an aHerald-owned file beside the executable (listed in the receipt,
  backed up and removed like the bridges of AH-204), keep `command` to one short line, and decide
  where the user's `--then` command goes (the file, not the command line). Mind the execution
  policy (`-ExecutionPolicy Bypass`) and that the script must copy stdin and stdout byte for byte
  (UTF-8, ANSI colours). Acceptance, by hand on Windows 11 and Server 2019: the status line, the
  user's wrapped command, its exit code and non-ASCII output are unchanged; `--install-statusline`
  and `--uninstall-statusline` restore the original exactly; the old encoded command is
  recognised and replaced.

## Milestone 6 — Claude parity (deferred)

- [Codex hooks](https://learn.chatgpt.com/docs/hooks)
- [Codex app-server](https://learn.chatgpt.com/docs/app-server)

## Milestone 7 — Test reliability and infrastructure

Improve the existing Rust test suite incrementally; retain behavioral assertions
and ordinary Cargo compatibility. Complete these tasks in order, without a wholesale
framework rewrite. Keep live/account checks opt-in and preserve the agreed artwork
performance check and resident `perf_baseline` limits. Record baseline failures
separately from regressions introduced by this milestone.

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

- [x] **AH-201 — Verify the harness event contracts (OpenCode, Pi, Goose, Crush).**
  Inspect official documentation and the released source/types of the installed
  versions. Record, per harness, how an external command can learn: turn end, error,
  interruption, permission request and its resolution, input wait, session identity,
  main session versus subagents, extension/hook discovery and failure behavior (a
  failing hook must not stop the agent). Distinguish final idle from intermediate
  completion (retries, compaction, queued follow-ups). Target versions: OpenCode 2.2.0
  (Homebrew `opencode`; the npm `opencode-ai` line is 1.19.x), Pi 1.5.0 (needs Node
  >= 22.19; Node 23.7 fails in its bundled undici), Goose and Crush as documented.
  Aider and Zero are recorded as unsupported: Aider's `--notifications-command` gets no
  arguments, stdin or session id and fires on the next prompt of any kind; Zero's hooks
  (`beforeTool`, `afterTool`, `sessionStart`, `sessionEnd`) have no turn-end or
  permission event and its stream-JSON events exist only for headless `zero exec`.
  Acceptance: `docs/harness-compatibility.md` with a versioned matrix of supported
  notifications, unsupported cases, sources and sanitized fixtures; candidate events
  of OpenCode and Pi verified with isolated opt-in probes (documentation alone is not
  live acceptance). No transcript parsing, proxy sessions or account-token access.
  Progress 2026-11-03: Pi and OpenCode turn end, error, interruption, permission (OpenCode)
  and prompts (Pi) are live-verified, and so are sub-agents (OpenCode child sessions carry
  `parentID`; Pi children run as `--mode json` processes, `ctx.hasUI` false). Open: the TUI
  permission flow and live probes for Crush and Goose (documentation only so far).
  Closed 2026-11-03 with those limits recorded in docs/harness-compatibility.md: the
  Crush/Goose rows, the OpenCode TUI permission flow, Pi compaction and queued follow-ups
  are not live-verified and must be re-checked in AH-203 before their mappings are shipped.

- [x] **AH-202 — Universal external event input and per-harness pets.**
  Depends on AH-201. `--notify <harness> --kind <existing kind> [--activity|--ended]`
  with the session id and `cwd` (card folder) read from bounded stdin; nothing else is
  read or logged. The harness key matches `[a-z0-9_-]{1,32}`; labels come from a short
  table (OpenCode, Pi, Goose, Crush) and default to the key. `Source::External(key)`
  joins the queue identity, so the same session id in two harnesses stays independent;
  waits are per session and `--activity` / `--ended` resolve them as for Claude.
  Replace the IPC source wire format outright (no versioned migration). Replace
  `[notifications.pet]`, `[notifications.pet_animations]` and `[agents.*]` with
  `[pets]` (`default`, and one path per harness key, `demo` for demo cards) and
  `[pet_animations]` (shared) / `[pet_animations.<harness>]`; `default` and `animations`
  are not valid harness keys; update the example configuration and docs. Codex and
  Claude behavior, their hooks and warnings stay as they are.
  Acceptance: tests cover valid/invalid input and size limits, each kind, per-session
  waits, activity and end resolving waits, identical session ids in different
  harnesses, label fallback, the new pet configuration and its rejection of the old
  keys; the existing Codex/Claude suite passes unchanged except for configuration keys.

- [x] **AH-203 — OpenCode and Pi bridges, Goose and Crush examples.**
  Depends on AH-201 and AH-202. A minimal TS bridge for each of OpenCode and Pi runs
  inside the harness runtime and calls `aherald --notify` at a fixed path, passing only
  the permitted fields (never prompts, message bodies, tool arguments or credentials).
  Pi uses its final-idle boundary (`agent_settled`, `aborted` -> interrupted); OpenCode
  maps session idle/error and, if the probes confirm it, permission asked/replied.
  Goose and Crush need no code: ship example hook configuration (Crush `TurnEnd`,
  `PermissionRequest`/`PermissionResult`, `StopFailure`, `Interrupt`, `SessionEnd`;
  Goose `Stop`, `UserPromptSubmit`, `SessionEnd`). Bounded delivery; a missing or
  failing aHerald never affects the harness. Manual setup and removal documented in
  `docs/`, with a support matrix.
  Acceptance: the bridges run against controlled fixtures through the real handler and
  IPC path (normalization, ignored events, lifecycle clearing, bounded completion,
  safe failure with no resident or executable); real-version results are recorded
  separately from synthetic ones.
  Result (2026-11-03): bridge contract tests (`tests/bridges`, Node) use synthetic events and a
  recorder in place of aherald; the bridges also ran live on OpenCode 2.2.0 and Pi 1.5.0
  (`BRIDGE=1 sh tests/live/harness_probes/run.sh ...`). Not checked live: the OpenCode TUI
  permission flow, Pi compaction and queued follow-ups, and Crush permission and interrupt hooks.
  Crush 0.99.0 (complete, error) and Goose 1.57.0 (complete) ran live with the shipped examples.
  Calls are serialized, so a short headless OpenCode run can end
  before its last call.

- [x] **AH-204 — Bridge file setup for harnesses (command line).**
  Former AH-205 and AH-208. `aherald --setup --harness opencode|pi|goose` installs, updates and
  removes aHerald-owned bridge files through the setup receipt (exact text recorded; dry run,
  `*.aherald-backup`, rollback; foreign or edited files are never replaced). The bridges are
  embedded in the executable, so nothing is packaged. Crush stays manual (its `crushrc` is the
  user's script). Installer pages (Windows installer, macOS setup window, Linux console setup)
  are not extended: they leave installed bridges alone; see AH-206.
  Acceptance: process-level tests for dry run, install, repeat, foreign file, deselection,
  edited-file removal, rollback and argument validation.

- [ ] **AH-205 — Live acceptance and onboarding guide for new harnesses (deferred).**
  Former AH-209 and AH-210. Opt-in live checks with isolated harness configuration,
  and a short guide for adding the next harness (arguments, bridge, tests, docs).

- [ ] **AH-206 — Offer the new harnesses in the installers (deferred).**
  Depends on AH-204. Windows installer, macOS setup window and Linux console setup list OpenCode,
  Pi and Goose beside Codex and Claude (default unchecked, installed ones retained) and pass the
  selection as `UsageOptions::bridges`. The Windows part can be checked by CI only on this setup.
