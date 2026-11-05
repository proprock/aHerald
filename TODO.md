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

- [ ] **AH-219 — Flaky native memory-growth test on the Windows runner.**
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

## Milestone 9 — Notifications to external messengers (deferred)

- [ ] **AH-213 — Sinks: send cards to a messenger, Telegram first.**
  Design agreed 2026-11-04. A *sink* lives in the resident, takes only events the queue accepted
  as new cards (`Outcome::Queued`, so deduplication, per-event `enabled`, session ends and
  obsolete waits are already settled) and never feeds anything back.
  - Contract: `struct Message { kind, harness, folder: Option<String>, text }` and
    `trait Sink: Send { fn name(&self) -> &str; fn send(&mut self, &Message) -> Result<(), String>; }`.
    No enum of kinds, no plugin loading; one method, tested with a stub.
  - Privacy by construction: the message carries the event kind, the harness and the folder name
    only, never prompts, `last_assistant_message`, paths or session names. The folder goes to a
    third party, so it is off unless `include_folder = true`.
  - Delivery: a worker thread with a bounded queue (the oldest is dropped when full), a timeout and
    capped retries, so a hung network never delays cards, IPC or the tray. Failures are shown, not
    hidden: the first and each different error posts a card through `notice::report_changed`; no
    secret ever appears in an error, a log line or `--raw-output`.
  - Per-sink filter: event kinds (a messenger usually wants `turn_complete`, `permission_required`,
    `input_required`, `error`), independent of `suppress_when_focused`, which is about the screen
    in front of the user. "Only when away from the computer" needs an idle detector that does not
    exist; leave it out of the pilot.
  - Secrets from the environment: the configuration names a variable, never holds the value
    (`token_env = "AHERALD_TELEGRAM_TOKEN"`, `chat_id_env = …`); `token_file = "…"` (owner-only
    file) as the second form. A literal `token = "…"` is rejected, so a pasted
    `config.toml` cannot leak it. A missing or empty variable is a startup notice naming the
    variable, not the value. On Windows a resident started from the Run key or the installer sees
    the user's environment only; say so in the docs and in the notice.
  - Telegram: `POST https://api.telegram.org/bot<token>/sendMessage` with `chat_id` and `text`
    (JSON body, no parse mode, so nothing needs escaping). `https.rs` (WinHTTP) and the `ureq`
    path only have GET: both need a bounded POST with a body, reusing the existing proxy/TLS
    handling and the `Wiped` secret type for the token. This is the largest part of the work.
  - Order of work: (1) POST in both HTTP layers, tested against the existing local test server;
    (2) the trait, the worker and the filter with a stub sink; (3) a generic *command or webhook*
    sink (a template with the kind and folder, run as an argument vector without a shell, or a
    POST to a URL), which already serves ntfy, Slack and Telegram; (4) Telegram as a preset on
    top of it, only if a preset is still wanted; (5) the settings window and `--setup` stay
    unchanged until the keys settle.
  Acceptance: unit tests for the filter, retry and drop-oldest, error de-duplication and secret
  redaction; a process test with a local fake Telegram server that sees exactly the allowed
  fields; a hung server leaves card delivery within its budget; by hand, one real bot on Windows 11
  and Linux.
  Status (branch `feature/AH-213`): done and tested — POST in both HTTP layers, the sink contract,
  worker, filter, retry, drop-oldest, error de-duplication, secret redaction, `[forward]`
  configuration, the Telegram sink against a fake Bot API, and the Windows tray **Messenger**
  switch (an agreed addition), and the generic **webhook** sink (`${NAME}` environment secrets,
  `{text}` body template) with Telegram as its documented example. The Telegram sink was built
  first and kept; whether to drop it in favour of the webhook is open. Left open: the by-hand check
  with a real bot on Windows 11 and Linux, and a compile and test run on macOS and Linux (AH-219
  for their menus).
  Not run here: `cargo check` for the macOS and Linux targets (no C cross toolchain).

- [ ] **AH-219 — Messenger switch in the macOS and Linux tray menus (split from AH-213).**
  Windows has a **Messenger** item beside Sound and Pop-up (`crate::forward::enabled`/`toggle`).
  Add the same item to `src/mac/menu.rs` (`Command`, `items`, the AppKit handler) and to the
  StatusNotifierItem menu in `src/linux/tray.rs`; both residents already start and stop the sinks.
  Acceptance: the pure menu tests list the item only when a sink runs, in the Windows order;
  checked by hand on macOS and a Linux desktop.

- [ ] **AH-220 — Command sink (split from AH-213).**
  The webhook sink is done under AH-213. What is left is a sink that runs a command as an argument
  vector without a shell, with the same `${NAME}` and `{text}` placeholders, for services that have
  a CLI rather than an HTTP endpoint. Reuses `forward::build` and the `Sink` contract.
  Acceptance: a test with a recording command; no shell is involved; the message never reaches a
  shell parser.

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
