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
  is not fully calibrated. See docs/notes/perf-baseline-investigation.md.

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
  docs/notes/ah-097-windows-acceptance.md.

- [ ] **AH-215 — Cause of the `native_assets` timeout in the full Cargo test run (split from AH-097).**
  `cargo test --all-targets` stops at `native_assets` with a notification startup timeout,
  both inside and outside the sandbox. Reproduce once, find the cause, and fix it test-first,
  or document why the timeout is environmental. Acceptance: the full run passes on Windows 11,
  with the commands and results recorded.

- [ ] **AH-221 — Flaky native memory-growth test on the Windows runner.**
  `notification::overlay::card_tests::repeated_card_rendering_and_dpi_resize_release_native_resources`
  (src/windows/overlay.rs) failed on a hosted `windows-latest` runner with "private commit kept growing"
  (30.4 MB at cycles 50-100, 40.4 MB at 150-200, baseline 29.0 MB) on a change that did not touch
  the code, and passed on the same diff and on master. Verdict (2026-11-04): a pull request skips
  it (`--skip` in the `cargo test` step of ci.yml) and the nightly full run keeps it, so a real
  leak still shows within a day. The release skips it too (AH-251); only the nightly run keeps it. Work left: reproduce on Windows 11,
  decide whether the growth is a real leak (GDI/heap) or allocator and runner noise, and make
  the test deterministic (for example, count GDI and USER handles instead of private commit, or
  widen the window with several samples). Acceptance: the test passes 20 runs in a row on a
  hosted runner, and the `--skip` is removed from ci.yml.

- [ ] **AH-272 — macOS DMG installation by hand (left from AH-102).**
  On the MacBook Pro M1 with a released DMG: install into /Applications and launch with a Finder
  double-click; Codex and Claude Code hook delivery and dismissal with the installed app; the whole
  path as a normal user without developer tools; an older running resident with a newer client.
  Decide whether the setup window needs Remove and Repair buttons (today removal clears both choices
  and repair applies again). Not signed or notarized (AH-218).
  Acceptance: results recorded with the macOS, harness and aHerald versions.

- [ ] **AH-273 — Ubuntu package acceptance on a clean desktop (left from AH-103).**
  On a clean supported Ubuntu desktop without developer tools: install the released `.deb` with apt
  and with the graphical package manager, run the console setup, see hook delivery and dismissal,
  then update and remove. Record the Ubuntu version, desktop and session type, harness versions and
  any system packages needed. Xorg and multi-monitor behaviour stay AH-093.
  Acceptance: results recorded; docs/installation.md updated if a step differs.

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

- [ ] **AH-274 — Homebrew Cask by hand on the M1 (left from AH-117).**
  `brew tap proprock/tap && brew install --cask aherald`, the setup window, hooks, delivery and
  dismissal; `brew upgrade` with a resident running (restart as documented); `brew uninstall` after
  cleaning the hooks in user context, and the repair path when that was skipped.
  Acceptance: results recorded with the macOS, Homebrew and aHerald versions.

### macOS parity with 0.9.0-beta

The warning engine (`session_warnings`), the setup backend and the tray text are shared
already, and the macOS resident keeps the warning state. What differs is the tray controls
and the acceptance evidence; reuse the Linux and Windows pieces rather than parallel code.

- [ ] **AH-275 — macOS tray warnings and Apply by hand (left from AH-118).**
  On the M1: the menu order and check marks; Context and Idle warnings switches change which live
  cards appear; Apply keeps usage and events running; Apply + Test shows the carousel with sounds,
  arrows, wheel and dismissal.
  Acceptance: results recorded with the macOS and aHerald versions.

- [ ] **AH-119 — macOS live warning acceptance and documentation.**
  The macOS counterpart of AH-115. With the AH-118 switches, verify real Codex and Claude
  Code context and idle warning cards on the M1: synthetic reports first, then installed hook
  dispatch and the installed statusLine wrapper (the setup window can enable them), including
  dismissal, configured sound and an idle warning suppressed while the switch is off.
  Distinguish adapter/bridge evidence from end-to-end delivery, use isolated configuration,
  bounded waits and owned-process cleanup, declare any account, network or billable request
  before each run, record macOS and harness versions and commands, and update docs and the
  verification matrix. Extends AH-104 (live checks on macOS); depends on AH-118.

- [ ] **AH-278 — Linux X11 focus suppression on Xorg (left from AH-222).**
  On an Xorg session: a card is suppressed while the originating xterm (or another one-session
  terminal) is in front; a tabbed terminal (GNOME Terminal), tmux and another window still deliver.
  Acceptance: results recorded with the distribution, desktop, terminal and aHerald versions.

- [ ] **AH-276 — macOS terminal-tab suppression by hand (left from AH-120).**
  With `[general] detect_terminal_tab = true` on the M1: the Automation prompt and its text
  (`NSAppleEventsUsageDescription` in the bundle); a card is suppressed while the originating tab of
  Terminal.app and of iTerm2 is in front; another tab, another terminal and an unknown origin still
  deliver.
  Acceptance: results recorded with the macOS, terminal and aHerald versions.

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
  `src/windows/resident_overlay.rs:1171-1175`, `src/mac/resident.rs:1192-1196` and
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
  Partial 2026-11-06: the shared mode selector and configuration switch are implemented, with
  focused tests for the default, disabled lookup, enabled lookup and both transient switches.
  Windows cross-target Clippy passed locally; Windows/macOS CI and the manual resident check remain.

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

- [ ] **AH-277 — Windows console companion checks (left from AH-211).**
  By hand: `aherald-cli` in Windows Terminal and on Windows Server 2019 (output, exit code, an unknown
  option); a live Claude Code and Codex hook (still `aherald.exe`) opens no console window. Automated:
  `tests/installer_windows.ps1` calls `aherald-cli.exe` instead of piping `aherald.exe`, so the
  installer lifecycle covers the companion.
  Acceptance: manual results recorded with the Windows build; the installer test passes in the
  Windows CI.

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

- [ ] **AH-232 — macOS: Claude-only usage refreshes while idle (the Mac check and the subscription are left).**
  Shipped on `chore/AH-246-single-sources`: `resident_request::idle_poll_needed(codex_usage, claude_usage,
  has_deadline)` with a test of all eight combinations; the Windows timer and the macOS wait use it, so a
  Claude-only setup now keeps polling on macOS. The macOS and Windows call sites compile only in their CI.
  Left: (1) a manual check on a Mac with only `[claude.usage] enabled = true`: the tray icon and tooltip
  update while idle (record the macOS version); (2) the better variant: subscribe the macOS resident to
  `usage.on_change` / `claude.on_change` with the existing `Waker` (as Windows does at
  `src/windows/resident_overlay.rs:1298-1300`) so the 1 s wake-up can go. Then move this task to DONE.md.

### P1 — consistency, CI and the release pipeline

- [ ] **AH-280 — Release-candidate checks left from AH-251 and AH-252.**
  The pull request runs of AH-251 and AH-252 are verified (see DONE.md); what only a release or the nightly
  run can show is left. On the next release candidate: the `Skipping: ...` line, with the four test names, in
  the Windows, macOS and Linux test steps of `release.yml` (the Windows step now uses `shell: bash`), and the
  `--version` build time of the same commit built twice (two release builds, or a rebuild of one tag) is
  equal. On the next nightly run: the four noisy tests run and pass or fail on their own, not skipped.
  Acceptance: the three logs, the two `--version` lines and the nightly log are recorded in DONE.md; a
  difference opens a ticket.
  Also from AH-253 (scripts moved to `scripts/`): `package-macos.sh`, `package-dmg.sh` and
  `check-release-macos.sh`/`check-release-dmg.sh` were checked only with `sh -n` and by reading, so the macOS
  packaging and DMG steps of that release candidate must pass, and the nightly run (AH-282) must pass with the
  moved Windows and Linux scripts.

- [ ] **AH-281 — Screenshots and an animation in the README.**
  Show what the program looks like: the notification cards (input required, done, error, the usage
  warning), the tray icon with its tooltip, and the tray menu (Sound, Pop-up, Messenger, Settings), on at
  least Windows and one of macOS or Linux. Capture on a clean desktop with no personal paths, folder names
  or account data (a demo session or `--demo` output is fine); store files under `docs/images/`, compress
  them (PNG or WebP, short GIF or WebM/MP4 loop for the card animation, each under ~1 MB), give each an
  `alt` text, and link them from `README.md` next to the feature list. Acceptance: README renders the images
  on GitHub in light and dark themes; no image holds private data; sizes are listed in the PR.
  Partial 2026-11-06: Linux demo cards (turn complete, input required, error, context warning)
  and an animated demo are in the README, captured on an isolated Xvfb display with demo data.
  Left: tray icon with tooltip and menu, Windows images, and GitHub light/dark rendering acceptance.

- [ ] **AH-282 — Rolling nightly prerelease.**
  Extend `nightly.yml` so that a green nightly run publishes the release `nightly` and replaces the
  previous one. Depends on nothing; touch `release.yml` only to share code, with its behaviour and the
  version checks unchanged.
  1. **Reuse.** Inspect `nightly.yml` (it calls `ci.yml` as `full_ci`) and `release.yml`
     (`package`, `package_macos`, `package_linux`, `package_ping`). Move the packaging into a reusable
     workflow (`workflow_call`) or composite actions that both call, if that removes real duplication; do not
     copy the jobs. Reuse the `package-*.sh`/`.ps1` scripts and the existing file names.
  2. **Assets.** The same as a stable release: Windows x64 portable ZIP and Setup EXE; macOS ARM64 ZIP and
     DMG; Linux x64 TAR.GZ and DEB; the Linux musl `aherald-ping` for x64 and ARM64; and `SHA256SUMS`.
     The version stays the one in `Cargo.toml`; no version tag is needed. Name the build by commit
     (for example the asset suffix `-nightly-<short sha>`) only where a script already takes a suffix.
  3. **Publish only after success.** The publish job `needs` the full CI and every package job, runs on
     `schedule` and `workflow_dispatch` of the default branch only, and first checks that every expected
     asset exists and is not empty. Upload artifacts with `retention-days: 1`; no build caches kept for it.
  4. **Replace, do not accumulate.** One prerelease: tag `nightly`, title `aHerald Nightly`,
     `prerelease: true`, not "latest", target the exact commit SHA built, notes with the UTC build date, the
     SHA and a link to the commit. Order: (a) prepare and verify all assets locally in the job; (b) move the
     `nightly` tag to the SHA; (c) create or edit the release and upload the assets (`gh release` with
     `--clobber`, or create the new release first and delete the old assets after the upload succeeded);
     (d) verify the asset list through `gh release view`. Never delete the existing nightly before (a) has
     passed. Check each `gh` step explicitly; a failure leaves the previous nightly and fails the run, with
     the recovery step (re-run the workflow) in the log. A short window with moved tag and old or partial
     assets is unavoidable, since a tag move and an upload are not one operation: document it.
  5. **Safety.** Top-level `permissions: contents: read`; `contents: write` only on the publish job.
     `concurrency` group `nightly-publish` with `cancel-in-progress: false`. The job touches only the tag
     and release named `nightly`, never a `v*` tag or a regular release (guard the tag name and fail if
     the release found is not a prerelease). GitHub release immutability: if the repository setting makes
     releases immutable, a published release cannot be edited and a tag cannot be moved; check the setting,
     and document that it must be off for this repository (or that the nightly then needs another design).
  6. **Docs.** `README.md` and `RELEASE.md` (or a short `docs/nightly.md`): the permanent URL
     `https://github.com/proprock/aHerald/releases/tag/nightly`; stable versus nightly (nightly is the latest
     green `master`, unsigned like the stable builds, may break, not for production); the schedule (the cron
     of `nightly.yml`) and how to run it by hand (`gh workflow run nightly.yml`); retention (one release, old
     assets replaced); what a failed night means (the previous nightly stays, the failed run is the signal).
     `CHANGELOG.md` `[Unreleased]` gets a line.
  7. **Idle nights.** Today `nightly.yml` has no such guard: it runs the full CI every night, also when
     `master` has not moved since the last green run. Add a first job `changes` (`schedule` only; a manual
     `workflow_dispatch` always runs everything) that reads the commit the current `nightly` release was
     built from (its `target_commitish`, or the SHA in its notes) and compares it with `github.sha`. When they
     are equal, `full_ci`, the package jobs and the publish job are skipped (`if:` on the job output) and the
     run summary says "no change since <sha>". Keep the `stable_canary` clippy job and the dependency audit
     running on an idle night: they exist to catch a new compiler or a new advisory with no commit at all. A
     missing release (the first night) or an unreadable one counts as changed, so a failure of the check
     never skips a build. Record in the notes how a skipped night looks in the run list (green, short) so it
     is not mistaken for a failure.
  Acceptance: `actionlint` and `zizmor` clean; a `workflow_dispatch` run on a branch other than `master`
  does not publish; a scheduled run on a `master` that has not moved since the nightly's commit skips
  the build and publish jobs but still runs the canary and the audit; one manual run on `master` (with the owner's go-ahead, since it creates the release)
  publishes all assets with a matching `SHA256SUMS`, and the tag points to the built commit; a second run
  leaves exactly one release; a run with a failing package job (forced on a scratch branch) leaves the first
  nightly intact; the stable `release.yml` run on the next release candidate is unchanged in behaviour.

- [ ] **AH-289 — Program starts in tests have a 3 s bound that a loaded full run can exceed.**
  Seen twice in a full workspace run on a busy 12-core Linux machine: `config_portable` once and
  `hook_log::a_hook_input_that_is_not_json_or_is_too_large_is_logged_by_code_only` once failed on
  `run_bounded_with_input(..., Duration::from_secs(3))` and passed alone 3 of 3. About 45 call sites in
  `tests/*.rs` pass `from_secs(3)` (`resident`, `hooks`, `tray`, `process_wait`, `native_overlay`, `window_wait`,
  `hook_log`, ...). Depends on nothing. Do first: reproduce under load (`scripts/manual/stress.sh` with
  `hook_log` and `config_portable` added, or `taskset -c 0,1` with two CPU hogs) and record the failure rate
  before changing anything. Then give the bound one name in `tests/support` (a constant for "start a program
  and wait for it", generous, e.g. 30 s, since the bound only matters when the child hangs) and use it at the
  call sites that start a program; keep short bounds where the test asserts on speed. Check that the failing
  starts go through `support::spawn`/`without_forks` where a fork window can matter (AH-279, AH-284).
  Acceptance: the stress run on the loaded runner shows no failure of these two tests in the number of
  repeats recorded before; a hung child still fails the test within the new bound (a self-test in
  `tests/support`); no assertion about speed was loosened.

### P2 — structural refactoring of the residents (in this order)

## Milestone 13 — Messenger notifications (2026-11-06)

- [ ] **AH-285 — Messenger messages: an optional computer name and a header line.**
  With the same chat fed by several computers, a message does not say which one sent it. Add an opt-in
  per-sink switch `include_host` (default `false`, like `include_folder`) that puts the computer's name into
  the message, and change the message layout. Depends on nothing. The new layout, a header line, a blank
  line and the existing message line (the example names are made up):
  ```
  <host>/<project_dir>

  <harness>: <message>
  ```
  for example `workstation-1/aHerald`, a blank line, then `Claude: turn complete`. `<project_dir>` is the
  folder name that `include_folder` sends today (`Event::folder`); `<harness>: <message>` is the line sent
  today without the folder (`Claude: turn complete`).
  1. **Header rules.** The header is built from what is enabled and available: host and folder joined
     by `/`; only the host (`workstation-1`); only the folder (`aHerald`); neither means no header and no
     blank line, so the default message is exactly the one sent today (`Claude: turn complete`). A folder
     that is not known (the event carries none) is omitted, not shown empty. The folder no longer appears in
     brackets after the harness name; this changes the text for users of `include_folder` and goes into
     `CHANGELOG.md` under `Changed`.
  2. **Host name.** The OS computer name, read per platform without a new heavy dependency (`libc`
     `gethostname` on macOS and Linux, the Win32 computer-name call or `COMPUTERNAME` on Windows; a new direct
     crate needs the justification that `AGENTS.md` asks for). Sanitise it like the folder name: control
     characters removed, at most 64 characters, no FQDN trimming, no hashing or other obfuscation. If it cannot be
     read or is empty, fall back to the computer's IP address: the address of the interface that the default
     route uses (the local address of a UDP socket "connected" to a public address, which sends no packet;
     no network request, no DNS), IPv4 first, shown as plain text (`192.0.2.10`). If there is no address
     either, the host is left out. Neither lookup may block or fail a card or a hook; the fallback is used only
     with `include_host = true` and is covered by the same privacy entry, which names the IP address too.
  3. **Placeholders.** `{text}` stays the whole message and now includes the header and the blank line when
     there is a header; add `{host}` (empty unless `include_host = true`) next to `{kind}`, `{harness}` and
     `{folder}`. A message now has two lines, so check each consumer: a JSON body escapes the newline, a form
     body percent-encodes it, plain text (ntfy) keeps it, and a command sink (`src/command_sink.rs`) gets it in
     `{text}` in `argv` or on stdin. Document that a body which wants its own layout can use the single
     placeholders instead of `{text}`.
  4. **Privacy.** The computer name goes to a third party, so it is opt-in and off by default, agreed here
     and recorded in `docs/privacy.md` (the "Messenger forwarding" row: what, when, where it goes, how to turn
     it off) and in `docs/configuration.md` (the `[[forward.sinks]]` options and the placeholders) in the same
     change. `README.md` only where it shows a forwarded message.
  5. **Parity.** Message building lives in `src/sink.rs` and is platform independent; only the host-name
     read differs per OS. Check that all three platforms get the same text for the same event.
  Acceptance: pure tests in `src/sink.rs` for the four header cases (none, host, folder, both), an event
  without a folder, a host that cannot be read (the IP fallback, and no address either), a 64-character cut and removal of control characters, and
  `{host}`/`{text}` in a JSON, a form and a plain-text body plus a command sink with the two-line text; the
  existing sink tests pass with the default message unchanged; `docs/privacy.md` and `docs/configuration.md`
  updated; the usual checks on Linux and CI on the three platforms; one manual check on a real messenger
  (record what it showed) with `include_host = true` on each platform.

- [ ] **AH-286 — Hand check of the shared tray menu on Windows and macOS.**
  Depends on AH-257. Open the tray menu on a Windows 10/11 machine and on a Mac and compare it with the Linux
  tray: the same entries in the same order (usage lines, Sound, Pop-up, Messenger only with a sink, Pause
  notifications, Context warnings, Idle warnings, version, Edit configuration, Apply + Test, Apply, Exit), the
  separators above Sound (only with usage lines), Context warnings, the version and Edit configuration, and the
  check marks following the switches. Acceptance: one line per OS with the OS version in `DONE.md`.

- [ ] **AH-288 — Hand check of Linux card artwork and the usage card after the `Ui` refactor.**
  Depends on AH-262. On a Linux desktop (X11 or XWayland) show a Codex or Claude card with artwork, one without,
  and the usage card (menu item, wheel paging, About): the card appears within a moment of its artwork (at most
  300 ms late), animates, fades out, and a second monitor or a scale change redraws it. Acceptance: one line with
  the distribution and session type in `DONE.md`.
