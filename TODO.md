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

- [ ] **AH-102 — Native macOS installer and setup UI.**
  Uses the common native setup backend from AH-097. Deliver GUI app installation
  and Homebrew Cask packaging for Apple Silicon, preserving the portable archive.
  Provide native user-context selection of Codex, Claude Code, both or neither;
  start unchecked on first setup, retain the selection on upgrade, and delegate
  changes to `--setup` / `--unsetup` with explicit `--apply`.
  Match Windows graceful shutdown before updates/removal: verify the owned app
  path, wait for process exit, and abort if graceful shutdown fails.
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

- [ ] **AH-103 — Ubuntu package and native setup UI.**
  Uses the common native setup backend from AH-097. Deliver an x86_64 `.deb`
  installable through apt and a graphical package manager, preserving the
  portable archive. Provide native user-context selection of Codex, Claude Code,
  both or neither; start unchecked on first setup, retain the selection on
  upgrade, and delegate changes to `--setup` / `--unsetup` with explicit `--apply`.
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
  Agree on system package versus per-user writable data/receipt ownership before
  implementation. Package scripts must never write root's or another user's
  hooks. Define user-context hook cleanup before binary removal for GUI/package
  workflows, including multiple desktop users, apt removal, cleanup failures,
  repair, and an active resident; do not silently leave broken hook bindings or
  execute user hook commands as root.
  Extend Linux release CI with the package and checksum alongside the portable
  archive; add isolated native lifecycle checks for install, repeated setup,
  selection changes, malformed configuration, failed writes, upgrade, cleanup
  failure and uninstall after environment overrides change.
  Acceptance: verify apt and graphical installation, native setup UI, hook
  delivery/dismissal, update and removal on a clean supported Ubuntu desktop
  without developer tools. Record Ubuntu version, desktop/session type and
  harness versions; document required system packages, repair and removal.
  See docs/installation.md; overlay Xorg/multi-monitor acceptance remains AH-093.

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
  delivery checks also depend on AH-102/AH-103.

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

- [ ] **AH-105 — Create an application icon, at least for Windows.** Design and
  add an aHerald application icon, with Windows as the minimum supported platform.
  Acceptance: the built Windows executable displays the icon correctly in Explorer
  at common icon sizes and display scales.

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

- [ ] **AH-098 — Suppress notifications for the active originating session.** Add two
  independent configuration options, both enabled by default: suppress notification
  windows when the originating window is focused, and suppress sounds in the same case.
  Each option controls only its own output; disabling it preserves that output even when
  the originating window is focused. For a desktop agent with multiple sessions, suppress
  only when the event belongs to the active session in the focused window: an event from
  another session must still notify. Establish reliable window and session identification
  for each supported platform and agent; if either cannot be determined, deliver the
  notification rather than suppress it. Preserve hook responses and event lifecycle
  handling. Acceptance: automated tests cover both options, their defaults and independent
  combinations, focused/unfocused windows, same/different/unknown desktop sessions and
  unknown focus; verify the behavior in a real terminal and a desktop session.

- [ ] **AH-020 — Finish placement and display-change handling.** Depends on AH-012.
  Support center, top center, bottom right, near cursor, and selected monitor.
  Keep overlays in the work area and handle monitor removal, negative desktop
  coordinates, DPI changes, and scaling. Acceptance: manual multi-monitor and
  mixed-DPI checks pass without focus theft or screen-wide flashing.
  Implementation and automated checks committed; manual multi-monitor/mixed-DPI
  acceptance remains unverified (hardware unavailable, confirmed 2026-10-25).
  Also carries the case split from AH-076: a card that opens on a second monitor with a
  different DPI is drawn at that monitor's scale (cards are not draggable by design).

- [ ] **AH-090 — No silent decisions: a log line for every dropped input, an error card for
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

- [ ] **AH-092 — Make the Windows resident pause test wait for the window to hide.**
  `resident_uses_one_window_and_switches_and_dismisses_only_selected_event` (`tests/resident.rs`)
  failed once on `windows-latest` (PR #63, 2026-10-29) at `assert!(!IsWindowVisible(window))` right
  after the status reported `paused: true`, and passed on a rerun of the same commit and on master.
  The pause reaches the status before the overlay window is hidden, so the check races. Fix test-first
  on Windows: wait for the window to be hidden with a bounded deadline (as the wait for `paused` does)
  and fail with a clear message on timeout; do not remove the assertion. If the window is still visible
  well after `paused`, that is a product bug and gets its own regression test. Acceptance: the test
  passes repeatedly on Windows (for example 20 consecutive runs) and still fails when a paused
  resident leaves the window visible.

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

## Milestone 6 — Claude parity (deferred)

- [Codex hooks](https://learn.chatgpt.com/docs/hooks)
- [Codex app-server](https://learn.chatgpt.com/docs/app-server)

## Milestone 7 — Test reliability and infrastructure

Improve the existing Rust test suite incrementally; retain behavioral assertions
and ordinary Cargo compatibility. Complete these tasks in order, without a wholesale
framework rewrite. Keep live/account checks opt-in and preserve the agreed artwork
performance check and resident `perf_baseline` limits. Record baseline failures
separately from regressions introduced by this milestone.

- [ ] **AH-107 — Make asynchronous checks and process cleanup deterministic.**
  Inventory process launches, unbounded waits, fixed sleeps used as synchronization,
  shared desktop resources and cleanup paths in ordinary tests. Finish AH-092 rather
  than duplicating its implementation: wait for actual window visibility changes,
  not only resident status. Replace synchronization sleeps with condition-based
  waits and bounded deadlines; retain deliberate delays that are themselves part
  of the scenario. Ensure timeout diagnostics identify the expected and last observed
  state without exposing sensitive payloads. Bound hook/process completion, drain
  captured output without pipe deadlocks, and terminate and reap only owned processes
  on timeout or assertion failure. For detached residents, retain ownership evidence,
  request graceful shutdown, observe exit and use bounded fallback cleanup when needed.
  Acceptance: regression tests demonstrate delayed-but-successful state changes,
  permanently missing effects, hung children and cleanup after failure. Verify that
  the guards themselves do not leave owned processes or IPC resources behind. Run
  the AH-092 Windows test 20 consecutive times and show that a resident which remains
  visible still fails. Do not hide failures with retries or relaxed assertions.

- [ ] **AH-108 — Extract small reusable test fixtures and bounded wait helpers.**
  Depends on AH-107. Introduce a focused `tests/support` module for repeated setup
  in resident/render/logging tests: temporary directory ownership, unique per-test
  IPC scope, configuration creation, resident/process guard and bounded state waits.
  Start with `resident_render_faults`, `resident_perf_log` and `perf_baseline`, then
  migrate other genuine duplicates. Keep platform-specific ownership and Win32
  handle operations behind appropriately gated helpers. Use RAII to clean up on
  success and panic, wait for owned processes to exit before removing their files,
  and keep fixture lifetime explicit in each test. Avoid a generic fixture registry,
  speculative traits or changes to production APIs solely to share test boilerplate.
  Preserve tests whose separate process is intentional, including Unix `TZ` checks.
  Acceptance: migrated tests retain their observable contracts; fixture tests cover
  unique scopes, concurrent independent residents, startup failure, timeout and panic
  cleanup. No fixture may stop an unrelated running aHerald instance. Record focused
  and relevant full-check results; obtain agreement before any new direct dependency.

- [ ] **AH-109 — Evaluate and adopt cargo-nextest with explicit resource scheduling.**
  Depends on AH-107 and AH-108. Measure current Windows test build and execution time
  separately, then compare the same test selection under a pinned, Windows-compatible
  nextest release. Include repeated runs, pass/fail/ignored counts and any pre-existing
  failures; do not claim a speedup from unmatched selections or cache conditions.
  Inventory tests that share the desktop, placement mutex, tray, audio or other OS-wide
  resources. Before parallel nextest execution, place conflicting tests in a common
  group with concurrency one: nextest runs each test in a separate process, so the
  existing process-local `static Mutex` is insufficient. Keep per-test unique IPC
  scopes even within serialized groups. Configure bounded per-test execution,
  useful failure output and CI reports, with automatic retries disabled by default.
  Define documented selections for fast deterministic checks, native integration,
  performance and explicit manual/live checks. Performance checks must run without
  competing GUI/performance workloads; retain their fixtures, budgets and required
  acceptance role unless a separate scope decision changes them. Do not blindly
  enable ignored tests, installed harnesses or account access in CI. Keep ordinary
  `cargo test` supported and account explicitly for any doctests or other checks
  outside nextest's coverage. Adopt nextest as the CI runner only after Windows
  compatibility and resource isolation pass; record a justified keep-Cargo decision
  if it provides no practical benefit. Acceptance: both runners execute equivalent
  intended coverage; GUI contention, hung-test reporting and owned-process cleanup
  are verified, existing platform CI checks remain covered, and measured results,
  commands, runner version and limitations are documented. Compare compilation costs
  before proposing integration-binary consolidation; preserve intentional isolation.

- [ ] **AH-110 — Apply rstest selectively where parameterization improves diagnostics.**
  Depends on AH-108 and the AH-109 runner decision. Compare `rstest` with ordinary
  table-driven Rust tests on representative configuration and hook-input matrices.
  Seek dependency agreement before adding it: document license/maintenance, pinned
  version, Rust 1.88 and Windows compatibility, required features, compile-time cost
  and the absence of a runtime dependency in the distributed executable. If approved,
  use it as a dev-dependency only and convert a small representative set first.
  Give each case a meaningful name so one failing payload/value can be selected and
  diagnosed independently. Reuse explicit RAII fixtures from AH-108; parameterization
  must not weaken process ownership, timeouts, privacy or hook response assertions.
  Avoid Cartesian-product case growth and do not rewrite simple tests merely for
  stylistic consistency. Acceptance: the pilot preserves all existing input/expected
  output pairs, reports and filters individual failing cases under the selected runner,
  and passes focused and relevant full checks. Record the readability and build-time
  comparison, then retain only conversions with a clear benefit. If the dependency
  is declined or adds no value, document the result and keep ordinary Rust tests.
