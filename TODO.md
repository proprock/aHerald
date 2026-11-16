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
- Wait cards and delivery (2026-11-16, after `audit-20261116.agents.md`): a Codex permission card is
  delayed by default and dropped when its turn moves on within the delay (AH-304; the default comes
  from Watchdog telemetry). A shown permission or input card of any harness leaves on evidence that
  its request was resolved, not only at the turn end (AH-305). The resident is the default mode:
  `[general] mode` defaults to `"resident"`, and `aherald --notify` uses the resident unless
  `--transient` is given, whatever the configuration says (AH-310). Transient stays supported as an
  explicit choice. The `[remote]` listener passes guest tool and model
  progress, identities only, so the same rules hold for agents in containers (AH-303).

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
  Status: items 1-7 are implemented (`package.yml` shared by `release.yml` and `nightly.yml`, the publish job,
  the idle-night check, `docs/nightly.md`, the cron moved to `37 4 * * *` UTC = 05:37 CET, 06:37 CEST). `actionlint` and
  `zizmor` are clean. Left: the runner acceptance below (a manual run on a branch, then one on `master` with
  the owner's go-ahead, a second run, a forced package failure, an idle scheduled night, and the stable
  `release.yml` on the next release candidate).
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
  Status (2026-11-06): the bound is now `support::START_BOUND` (30 s), used by the 11 calls that run a
  program under `run_bounded*`/`capture_bounded` (`hook_log`, `config_portable`, `hooks`, `claude_hooks`,
  `external_notify`, `statusline`, `hook_lifecycle`, `process_wait`); `wait_until` polls and the speed
  assertions keep 3 s. The failure was not reproduced before the change: on Linux with 4 to 24 CPU hogs
  pinned to 2 to 4 cores, 0 of 155 runs of six of these binaries and 0 failures in 3 full workspace runs
  (noisy tests skipped) failed. So the change is not proven to be the fix. Left: watch the CI and nightly runs;
  close when a week of runs shows no start-bound failure, or reopen with the exact message if one appears.
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

- [ ] **AH-315 — `canceled_audio_does_not_wait_for_the_serial_gate` misses its 300 ms bound under load (found in AH-221).**
  `src/windows/audio.rs`: the test spawns a thread whose `acquire_controlled` must give up at once on a
  set cancel flag, and asserts the whole spawn and join took under 300 ms. Seen once in 30 runs of the
  whole lib suite with a busy loop on every CPU (Windows 11, 4 CPUs, 2026-11-16): `canceled audio waited
  for another playback`. Depends on nothing. Do first: measure where the time goes under load (thread
  start against the wait itself). Then make the check independent of scheduling, for example by
  asserting that the call returns well before the 1000 ms timeout it was given, without loosening what
  it proves (a canceled request never waits for the gate). Acceptance: 40 loaded lib-suite runs on
  `windows-latest` without this failure, and the test still fails when cancellation is ignored.

### P2 — structural refactoring of the residents (in this order)

## Milestone 13 — Messenger notifications (2026-11-06)

- [ ] **AH-291 — Hand check of `include_host` on a real messenger, on each platform.**
  Depends on AH-285 (merged). With a real sink (Telegram or ntfy) set `include_host = true`, then `include_host`
  plus `include_folder`, on Windows, macOS and Linux: send a card with `aherald --event input_required` or a real hook
  and record, per platform and OS version, what the messenger showed (the header line, the blank line, the
  computer name); once with the name unavailable if that can be arranged, to see the IP address fallback. Acceptance:
  the three results are written in `DONE.md` as text; a difference between platforms becomes a bug ticket.

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

## Milestone 14 — One `proprock` plugin catalog (2026-11-09)

- [ ] **AH-299 — `src/linux/audio.rs` does not compile on aarch64 Linux (found while checking AH-297).**
  `pa_simple_new` is called with `c"...".as_ptr()` (`*const u8` on aarch64, where `c_char` is unsigned) but
  the binding expects `*const i8`, so `cargo build` fails with E0308 on an arm64 Linux container
  (x86_64 CI never sees it). Use `std::ffi::c_char` in the binding. Linux builds are x86_64 only today;
  decide whether arm64 Linux is in scope before fixing.

- [ ] **AH-298 — Status line and dependencies in the Windows installer (left from AH-297).**
  The Windows installer (`installer/windows.iss`) has no status line task and does not tick the hooks a
  warning needs; pass `--claude-statusline` and enable the dependent tasks on the task page. Do it on Windows
  together with AH-214: the wizard can be built and seen only there.
  Acceptance: the installer ticks the Claude hooks task for a Claude context or idle task and a status line
  task for the context task, and clears them with their parent; seen in the wizard on a clean machine.

- [ ] **AH-296 — Rerun a Nightly job that never got a runner (and only that).**
  Nightly and releases sometimes fail because a macOS job waits 15 minutes for a runner and is cancelled
  (v0.16.0 on 2026-11-06 and Nightly on 2026-11-09). In that case the job has `runner_id` 0, an empty
  `runner_name` and no steps; a real failure always has a runner and steps. Add a workflow `nightly-retry.yml`
  on `workflow_run` (Nightly, `failure`, `master`, `schedule` or `workflow_dispatch` only) that reads the jobs
  of the failed attempt and runs `gh run rerun --failed` only when every failed job except the final
  `CI gate` never got a runner, at most twice (`run_attempt` below 3). Selection logic in a script with a test
  on fixtures taken from real API answers: all runner-less, a real failure, a mix, and the attempt limit.
  Permissions `actions: write` and `contents: read`, no new secret. A `workflow_run` workflow starts only from
  the default branch, so the live check comes after the merge: report it as checked or not checked. Not in
  scope: `release.yml` (a rerun touches publication, a separate decision) and `ci.yml`.
  Acceptance: the test passes; a deliberate or natural runner shortage leads to one automatic rerun, a failing
  test does not; the evidence goes into `DONE.md`.

## Milestone 15 — Audit follow-ups (2026-11-16)

Source: `audit-20261116.agents.md` (local, untracked; it lists what was not read). Conventions:
`AGENTS.md` applies. Test first, with red and green commits. Report each platform as **ran locally**,
**compiled/tested in CI only**, **checked by hand on hardware** or **not checked**. AH-302 to AH-305 run
in this order; the others are independent. Suggested order (AH-310 and AH-306 done): AH-311, then AH-302 to
AH-305, AH-312, AH-308, and the rest when convenient.

- [ ] **AH-302 — Record the Codex hook order around an automatically reviewed approval.**
  Blocks AH-304 and AH-305. Source reading of `rust-v0.169.0` (`codex-rs/core/src/tools/approvals.rs`,
  `tools/registry.rs`) says: `PreToolUse`, then `PermissionRequest`, then the reviewer, then the tool, then
  `PostToolUse` (only when the tool succeeded), then the next `PostModelRequest`. Confirm it on a real session.
  Extend `tests/live/codex_approval_observation.ps1` (or add a `.sh` twin) so its recorder keeps, per hook
  call, only: event name, `turn_id` and `tool_use_id` as short hashes, `agent_id` presence, and a monotonic
  sequence number. No tool input, prompt or path. Run three cases with `approvals_reviewer = "auto_review"`:
  a request the reviewer approves, one it denies, and two tool calls in one model step where one needs
  approval. Record the Codex version.
  Acceptance: `docs/codex-approval-observation.md` gains a table with the observed order per case, states
  whether `PreToolUse` or `PostToolUse` of a sibling call can arrive between a `PermissionRequest` and its
  resolution, and `docs/harness-compatibility.md` names the tested version. No production code changes.

- [ ] **AH-303 — Carry `tool_use_id` and deliver Codex progress to the resident.**
  Depends on AH-302. Today `events::parse_hook` drops `tool_use_id` (`src/events.rs:380`), Codex activity is
  forwarded only with the idle warning on (`src/commands/deliver.rs:189`), and a `PostModelRequest` returns
  before any event is built (`src/commands/deliver.rs:137-150`).
  1. Add `tool_use_id: Option<String>` to `events::Event` (`#[serde(default, skip_serializing_if)]`, validated
     like the other identities in `Event::validate`). Fill it in `parse_hook` for `PreToolUse` and
     `PostToolUse`. An old resident ignores the unknown field already.
  2. Add `Request::CodexProgress { session_id, turn_id, agent_id }` to `src/ipc.rs` for a completed
     `PostModelRequest`, or extend `codex_context::Report` with optional `turn_id`; choose the one that keeps
     old residents accepting new hooks, and say which in the pull request. Subagent reports (`agent_id`)
     must reach the queue too; today `codex_context::parse` drops them.
  3. In `dispatch`, forward Codex and Claude tool activity (`PreToolUse`, `PostToolUse`,
     `UserPromptSubmit`) to a running resident always, not only with the idle warning on: AH-304 and AH-305
     need it for every session. Keep `forward_if_running` (never start a resident for activity) and its
     bound. Measure the added hook time per tool call on one OS (median of 50 runs, before and after) and
     put the numbers in the pull request; if it exceeds 5 ms, stop and ask.
  4. Guests (agreed 2026-11-16): the `[remote]` listener passes the same activity instead of dropping it
     (`src/remote.rs:197-200`), still without `window` or `tty`. `aherald-ping` sends for these events only
     the fields the host reads (event name, `session_id`, `turn_id`, `agent_id`, `tool_use_id`, and
     `status` for `PostModelRequest`); the two shell senders post the payload as they do today. Add
     `PreToolUse`, `PostToolUse` and, for Codex, `PostModelRequest` to `examples/sandbox/claude-settings.json`
     and `examples/sandbox/codex-hooks.json`. Update `docs/sandboxes.md`, `docs/sandbox-setup-for-agents.md`
     and `docs/privacy.md` (what a guest now sends and how often: once or twice per tool call). The listener
     test `tool_and_prompt_activity_is_not_delivered` changes on purpose: say so in the pull request.
     Keep the three senders consistent (`tests/sandbox_sender.rs`, `crates/aherald-ping/tests/`).
  Tests (pure, `src/events.rs` and `tests/api/`): the id is parsed, validated and survives the wire; a
  payload with a hostile id is rejected; `SECRET` in `tool_input` never appears in the serialized event.
  Update `crates/aherald-ping` and both shell senders only if the listener contract changes
  (`tests/` contract test `parser_contract.rs` must stay green).  Acceptance: the tests pass on Linux; Windows and macOS compile in CI.

- [ ] **AH-304 — Hold a Codex permission card until the request is still open (`[codex] permission_delay_sec`).**
  Depends on AH-303. Agreed 2026-11-16: the delay is **on by default**; `0` switches it off. Range 0..=120.
  Choose the default from real data before writing the constant. The owner's live Watchdog instance
  (sibling project `watchdog`; its location and commands are in that repository's `LIVE.md`, which is
  machine-local: never copy its paths, session ids or any row into this repository) records every Codex
  hook with `hook_event_name`, `session_id`, `turn_id`, `tool_use_id` and `received_at_us` in the SQLite
  table `event_facts` (`docs/storage.md` and `src/agent_watchdog/facts_query.py` there; open the database
  read-only). For each Codex `PermissionRequest`, compute the time to the first later event of the same
  session and turn that counts as evidence below (`PostToolUse`, the next model request, `Stop`,
  `Interrupt`). Report the count, p50, p75, p90, p95 and p99, and a histogram in 1 s buckets up to 60 s,
  separately for the Codex CLI and desktop profiles if both have rows (Watchdog captures `PermissionRequest`
  only where its hook profile enables it: state which profiles and how many days the sample covers).
  Expect two groups: automatic review (seconds) and a person (tens of seconds or more). Take the default as
  the smallest whole second that covers at least 90 % of the first group, and not above 15 without asking.
  If the sample has fewer than 50 requests or shows no clear first group, use 8 and say so. Write the
  figures (numbers only) to `docs/notes/ah-304-permission-delay.md` with the date, the Codex versions and
  the chosen value. Repeat for Claude `PermissionRequest` rows and record them in the same note: they decide
  whether Claude needs the delay too, which is a separate decision.
  Behaviour: a Codex `PermissionRequired` event with `permission_delay_sec > 0` is held in the queue for that
  time: no card, no sound, no messenger forward (`Controls::forward` at `src/resident_request.rs:593` runs only
  when the card is released). While held, the event is dropped (`Outcome::Resolved`, debug line
  `auto_resolved`) when evidence of the same `session_id`, `turn_id` and `agent_id` arrives:
  - `PostToolUse` whose `tool_use_id` was open (a `PreToolUse` seen, no `PostToolUse` yet) when the request
    arrived; with several open ids, only when all of them have completed;
  - a completed `PostModelRequest`;
  - `Stop`, `Interrupt`, `SessionEnd`, `UserPromptSubmit`.
  `PreToolUse` alone is not evidence unless AH-302 showed it is safe. Claude and external harnesses are
  unchanged. Transient mode has no later state: document that the delay needs the resident.
  Design: keep it pure in `src/queue.rs` (or a small `src/pending_approvals.rs` the queue owns): a held list
  with a release deadline on the caller's clock, a per-turn set of open tool ids bounded to 64 turns and 32
  ids each (drop the oldest), and `Queue::tick` releasing due entries through the normal admission path.
  The residents must wake for the deadline: expose it next to `warnings.has_deadline()` so the three
  loops (`src/windows/resident_overlay.rs:209`, `src/linux/resident.rs:1068`, the macOS wait) need one shared
  predicate, not three edits of logic.
  Tests (pure): approved in time, denied then `PostModelRequest`, human wait released after the delay with
  sound and forward exactly once, two open tools with one finishing, another session and another turn do not
  resolve it, a subagent's event does not resolve the parent's, delay 0 behaves as today, bounds hold; the
  built-in default equals the value in `config.example.toml` (the default-config parity test, AH-256).
  `CHANGELOG.md` gets a `Changed` entry: Codex permission cards now wait by default, and how to restore
  the old behaviour.
  Docs: `config.example.toml`, `docs/configuration.md`, `docs/codex-hooks.md`, `docs/codex-approval-observation.md`
  (the heuristic, its cost, and the long-running-command false positive), `CHANGELOG.md`. Then close AH-099
  or restate what it still needs.
  Acceptance: tests pass on Linux; by hand on one OS with a real Codex auto-review session, an approved
  request shows no card and a real human prompt shows one after the delay; record the Codex version.

- [ ] **AH-305 — A shown permission or input card of any harness disappears when its request is resolved.**
  Depends on AH-303; independent of the delay. Today the card stays until `Stop` or `Interrupt` of the turn
  (`Queue::observe_terminal`, `src/queue.rs:155`). Apply the AH-304 evidence rules to cards already in the
  queue, by extending `Queue::observe_activity` (`src/queue.rs:144`) for `Source::CodexHook`.
  Agreed 2026-11-16: every harness gets this, in one pull request per harness, Codex first.
  **Claude.** Its hooks carry no turn id and one session holds at most one wait card (`same`,
  `src/queue.rs:68`). Before coding, read the current Claude Code hook reference and record in
  `docs/claude-hooks.md`, with the version: which of `PreToolUse`, `PostToolUse` and `PostToolUseFailure`
  carry `tool_use_id`; which event follows a permission the user denies; whether `PermissionRequest` or the
  `permission_prompt` notification fires for a call that the `auto` permission mode approves by itself
  (if it does, note it for AH-304's Claude question); and what follows an answered `elicitation_dialog` and
  `agent_needs_input`. Never invent a field: an id that is not documented is not used. Evidence, always for
  the same `session_id` and `agent_id`:
  - a tool finish (`PostToolUse` or `PostToolUseFailure`) whose `tool_use_id` was open when the wait arrived;
    with several open ids, when all of them have finished. Without documented ids, any tool finish counts
    only if no other tool of that session was open;
  - `UserPromptSubmit`; `Stop`, `StopFailure` and `SessionEnd` already resolve it.
  A subagent's events (`agent_id` set) never resolve the main session's card, and the reverse. If Claude gets
  a new hook event (`PostToolUseFailure`), add it in the same change to `hook_setup::events(Agent::Claude)`,
  `plugins/aherald/hooks/hooks.json`, the `examples/claude-settings*.json` files and `docs/claude-hooks.md`,
  and say in `CHANGELOG.md` that setup must be run again.
  **OpenCode, Pi, Goose, Crush.** Any `--activity` of the session already resolves its waits
  (`Queue::observe_activity`); with AH-310 that reaches the resident by default. Add only the missing tests:
  one per bridge in `tests/bridges/bridges.test.mjs` that the answer to a prompt (`permission.replied`,
  `ui_prompt_end`, `PermissionResult`) sends `--activity`, and one queue test that it clears the card. Goose
  reports no waits: nothing to do.
  **Shape.** One evidence function in the queue for all sources, with the per-harness difference reduced to
  "waits by turn" (Codex) or "by session" (Claude, external); do not add a second copy of the open-tool
  bookkeeping of AH-304.
  Tests (pure, `tests/api/queue.rs` and `tests/api/codex_lifecycle.rs`): the card resolves on the matching
  `PostToolUse` and on `PostModelRequest`; a sibling tool, another turn and another session leave it; the
  selection moves to the next visible card. This changes an agreed contract on purpose: the test
  `requests_without_request_identity_are_not_merged_or_resolved_by_tool_results`
  (`tests/api/codex_lifecycle.rs:84`) keeps its "not merged" assertion and loses the "or resolved by tool
  results" part of its name; say so in the pull request.
  Claude tests (pure): the card leaves on the matching tool finish, on a failed tool, and on a new prompt;
  a parallel tool that was open, a subagent and another session leave it; a `PermissionRequest` followed by
  its `permission_prompt` notification is still one card.
  Acceptance: tests pass; `docs/codex-hooks.md`, `docs/claude-hooks.md` and `docs/harnesses.md` each say when
  a wait card leaves; by hand, one real Claude Code permission prompt answered in the terminal clears its
  card before the turn ends (record the version and OS).

- [ ] **AH-307 — Transient mode on macOS: implement it or say that it is absent.**
  `notification::show` returns `Err("Windows is required")` on macOS (`src/notification.rs:52-56`) while
  `[general] mode = "transient"` is an explicit choice since AH-310 (and the value in every existing
  `config.toml`), and the README says the behaviour is identical. By reading:
  `show` acknowledges the parent first (`process::acknowledge`) and then returns the error, so
  `aherald --event turn_complete` and `aherald --notify x --kind error` succeed and show nothing. Confirm
  with one run on the M1 before changing anything.
  Preferred: show the card through `mac::resident::run_notice`, as `startup_notice` already does, with sound.
  Otherwise: make macOS fall back to the resident, and add the limit to README "Limits",
  `docs/portable-macos.md` and `docs/configuration.md`. Either way replace the error text with one that names
  the cause and the fix.
  Acceptance: a test of the chosen behaviour in the macOS CI leg; one manual line with the macOS version.

- [ ] **AH-308 — One resident step for the three window systems.**
  The three `Ui` structs still repeat the same decisions (see the audit, A1). Extract, one pull request each,
  into platform-independent modules with pure tests, and delete the copies in the same change:
  1. `card::Heading` for an Apply + Test sample (verbatim in `src/windows/resident_overlay.rs:405-416` and
     `src/linux/resident.rs:1002-1014`; check `src/mac/resident.rs`).
  2. The artwork wait: `pending`, `art_wait`, `ART_WAIT_MS`, "prepared for another scale, ask again"
     (`src/linux/resident.rs:892-927` is the cleanest form) as one state machine over `ArtworkWorker`.
  3. "Compose again or only update the text" (`shown_card` / `CardKey`).
  4. Sound start and cancel, including how the tray Sound switch reaches `audio::start_controlled`:
     done by AH-311 (`presenter::Announcer`, `audio::start_announced`, `audio::silence`; all three
     residents mirror the switch into their `Config` on `SwitchesChanged`).
  5. The once-a-second and on-change tray usage refresh.
  Then split `Ui::draw_now` in `src/windows/resident_overlay.rs:245` along the Linux steps.
  Acceptance per step: the new module has tests that run on Linux; all three residents call it; no behaviour
  change (existing tests untouched); Windows and macOS reported as CI only plus one manual card check each.

- [ ] **AH-309 — A capability table per harness and OS, and per-harness event switches.**
  1. Docs: one table in `docs/harnesses.md` (linked from the README) with rows for Codex, Claude Code,
     OpenCode, Pi, Goose, Crush and columns for each signal, wait resolution, session name, click to
     activate, focus suppression, usage, context and idle warnings, remote delivery, each cell per OS where it
     differs, and "why not" for a gap (harness limit or not built). Sources: `src/agent.rs:52-64`,
     `src/focus.rs`, `src/activation.rs`, `docs/harness-compatibility.md`.
  2. Ask the owner, then implement if agreed: `[events.<kind>.<harness>] enabled` / `sound_enabled`
     overrides (`Events` is indexed by kind only, `src/config.rs:184`), so a kind can be off for one harness.
     Old files stay valid. Tests in `tests/config.rs` and `src/resident_request.rs`.
  Acceptance: the table matches the code (each "yes" names the test or manual evidence); item 2 only after
  the owner's answer.

- [ ] **AH-316 — Manual check of AH-310 with a real OpenCode or Pi session.**
  Split from AH-310 (closed without it: neither harness was installed). With the installed bridge and no
  mode flag, a permission prompt in a real OpenCode or Pi session shows a card through the resident (it
  starts one if none runs), and answering the prompt in the harness clears the card before the turn ends.
  Acceptance: one line in `DONE.md` with the harness version, aHerald version and OS.

- [ ] **AH-311 — The sound of a new event follows one rule on Windows, macOS and Linux.**
  Today (by reading): with the usage card open, Windows and macOS start no sound until it closes
  (`src/windows/resident_overlay.rs:315-322`, `src/mac/resident.rs:736-752`) while Linux plays at once
  (`src/linux/resident.rs:414`); and a card whose window is suppressed by focus but whose sound is not plays
  on Windows and Linux but not on macOS when no other card is visible, because macOS returns before
  `take_unannounced` (`src/mac/resident.rs:753-767`).
  Rule to implement (the Linux one, which matches "every new event sounds once"): each pass takes the
  unannounced entries and starts their sounds before any decision about what to draw, unless paused.
  Put "which sounds start this pass" into one platform-independent function (inputs: paused, the sound
  switch, playing count, the queue, the Apply + Test sample; output: entries to play and whether to cancel)
  with pure tests for: usage card selected, no visible card, paused, sound switched off, the preview sample
  sounding once per visit, and `MAX_SOUNDS`. All three residents call it; this is step 4 of AH-308, done first.
  Acceptance: the pure tests pass; Linux ran locally; Windows and macOS CI only, plus one manual check each
  (open the usage card, send `aherald --resident --event error` with a sound configured, hear it).
  Status: implemented on `feature/AH-311-sound-rule` (pure tests in `tests/api/presenter.rs`, module
  `announcer`); closes with its CI evidence. The manual checks moved to AH-317.

- [ ] **AH-317 — Manual check of AH-311 on Windows and macOS.**
  Depends on AH-311. With a sound configured for `error`, open the usage card from the tray, run
  `aherald --resident --event error` and hear the sound while the usage card stays open. Use a resident
  started for the check, not one the user already runs.
  Acceptance: one line per OS in `DONE.md` with the OS version and aHerald commit.

- [ ] **AH-318 — One rule for when a real card ends the Apply + Test preview.**
  Windows ends the preview as soon as the queue holds any item (`src/windows/resident_overlay.rs`, `end_test`
  call in `draw_now`); Linux and macOS end it only when the queue has a selected card, so an event whose
  window is suppressed by focus keeps the preview open there. Pick one rule, move it into a
  platform-independent function with pure tests, and call it from the three residents (fits AH-308).
  Acceptance: pure tests pass; Linux ran in CI; Windows and macOS CI only.

- [ ] **AH-312 — One resident contract suite that runs on every OS.**
  The end-to-end resident tests are Windows-only (`tests/resident.rs`, `resident_fixture.rs`,
  `resident_render_faults.rs`, `tray*.rs`, `hook_lifecycle.rs`); macOS has two tests
  (`tests/mac_resident.rs`) and Linux one file (`tests/linux_apply.rs`). Most scenarios in
  `tests/resident.rs` observe only `--status` and exit codes, so they need no window handle.
  1. Extract an OS-neutral fixture into `tests/support/resident.rs`: start a resident with a unique scope
     and private runtime directory, wait for readiness through `--status`, send events with the built
     executable, stop and reap it on drop. Reuse the Windows fixture, `Resident` in `tests/remote_listener.rs`
     and the one in `tests/mac_resident.rs`; do not write a fourth.
  2. Move these scenarios into `tests/resident_contract.rs` (no `#![cfg]`): sixteen concurrent hooks reach
     one resident exactly once; start-up races between status, forwarding and quit; a crashed resident is
     replaced and accepts events; a disabled kind still resolves waits; `SessionEnd` clears a wait and never
     starts a resident; a second resident exits quietly. Keep window-specific assertions in the Windows file.
  3. CI: run the suite under `xvfb-run` and a private D-Bus on Linux (as `linux_apply`), and plainly on the
     macOS and Windows runners. Add the file to `.github/platform-flags.sh` rules only if it is gated, and keep
     `tests/ci_platform_flags.sh` green. A missing display on a CI runner must fail, not skip.
  4. Add one transient test outside Windows (`--transient --event turn_complete` on Linux under Xvfb: the
     process shows a window and exits by its timer), so AH-307 has a place for its macOS case.
  Acceptance: the suite passes on the three CI legs; no existing Windows assertion was removed; the run
  time added per leg is stated in the pull request.

- [ ] **AH-313 — The `[remote]` listener blocks in `accept` instead of polling every 20 ms.**
  `run` in `src/remote.rs:404-453` sets the listener non-blocking and sleeps `POLL` (20 ms) between tries: 50
  wake-ups a second for as long as `[remote]` is on, against "an idle resident does not wake up
  periodically". Use a blocking `accept` and stop it from `Drop` by setting the flag and connecting to the
  bound address once (std only, same on all three OSes); keep the 30 s bind retry, sleeping on a condition
  variable or `park_timeout` that `Drop` can interrupt.
  Tests: the existing listener tests stay green (stop on drop, bind retry, connection cap, stalled client);
  add one Linux test in the style of `linux_idle_resident_has_no_periodic_wakeups...`
  (`tests/linux_apply.rs:517`) that counts voluntary context switches of an idle resident with `[remote]`
  enabled.
  Acceptance: tests pass on Linux; Windows and macOS CI only.

- [ ] **AH-314 — Small consistency fixes found by the audit.**
  1. `examples/hooks.windows.json` and `examples/hooks.macos.json` lack `PostModelRequest`, which
     `hook_setup::events(Agent::Codex)` installs, so a manual Codex setup gets no context warning. Add it,
     and add a test that parses every shipped example and the plugin `hooks.json` and compares their event
     names with `hook_setup::events` (the sandbox examples are an explicit subset: assert the subset).
  2. Replace the error text `"Windows is required"` in `src/notification.rs:55` (with AH-307 if that lands
     first).
  3. Stale comments: `tests/unsupported_platform.rs:1-2` and the version in `src/codex_context.rs:1`
     (re-verify the `PostModelRequest` fields against the current Codex release and record it in
     `docs/harness-compatibility.md`).
  4. `Source::Usage.label()` returns `"Codex"` (`src/events.rs:93`): find its callers; if the Claude usage
     card or a forwarded message can show it, fix the label, otherwise leave a comment why it is unused.
  5. `TODO.md`: remove the empty "P2 — structural refactoring of the residents" heading and the pointer to
     the missing `audit-20261105.md` (or restore the file).
  Acceptance: the new test fails before item 1 and passes after; the other items need no test.

- [ ] **AH-294 — Codex plugin for aHerald and its catalog entry.**
  Depends on AH-293 (done). `proprock/marketplace` also carries a Codex catalog (`.agents/plugins/marketplace.json`);
  aHerald has no Codex plugin package yet (only Codex hook setup through `--install-hooks codex`).
  Read the current Codex plugin and hook documentation first (`AGENTS.md`, harness integrations), build a
  package in the layout kaBoost uses (`adapters/codex/package/kaboost`), add the entry to the Codex
  catalog, extend `scripts/update-marketplace.sh` and the `update_marketplace` job to pin both catalogs,
  and add a disposable-profile smoke test like kaBoost's `scripts/smoke_codex_remote.py`. Acceptance:
  `codex plugin marketplace add proprock/marketplace` then `codex plugin add aherald@proprock` installs
  files equal to the pinned commit, a synthetic hook event reaches a resident, and the release job bumps
  both catalogs; Codex version recorded in `DONE.md`.
