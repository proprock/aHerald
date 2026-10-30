# aHerald implementation plan

aHerald is a portable, native Windows 10/11 x64 notification companion. Codex is
the first supported agent. Ship one executable, `aherald.exe`, built with stable
Rust and the MSVC toolchain. Start with Milestone 1 and keep each milestone usable.

## Agreed decisions

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
  edit Codex configuration or install startup entries.
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

- [ ] **AH-097 — Windows plugin launcher and acceptance (split from AH-091).**
  Add and verify the Windows PowerShell launcher and `AHERALD_PATH` discovery; check
  actual plugin installation and event delivery on Windows without disturbing existing hooks.

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

- [ ] **AH-100 — Per-session context-size warning.** Provide independently configurable
  context-size warnings for Claude and Codex, with separate enablement and thresholds.
  Warn when a session's context exceeds its configured threshold; accept both `N`
  tokens and `Nk` kilotokens as threshold notation. Track context and warning state
  by agent and session so one session cannot trigger or suppress another's warning.
  Verify supported context-size sources for each agent before implementation; document
  unavailable coverage and resolve the kilotoken multiplier, defaults and repeat/reset
  policy before coding. Acceptance: tests cover both notations, invalid values, the
  threshold boundary, independent agent settings, session isolation and unavailable
  context data; verify warnings in real supported agent sessions.
  Preliminary research (2026-10-30): Claude statusLine is a viable source, subject
  to an agreed AH-059 privacy allowlist extension; no permitted passive source
  for arbitrary Codex sessions established. See [feasibility report](docs/session-warnings-research.md).
  First slice on `feature/ah-100-101-session-warnings`: Claude context threshold
  only; Codex context and actual cache-expiry warnings remain deferred. The
  maintainer authorized the corresponding AH-059 input-context allowlist extension
  on 2026-10-30. Implementation/verification evidence: [session warnings](docs/session-warnings-research.md).

- [ ] **AH-101 — Per-session cache-TTL warning.** Provide independently configurable
  cache-TTL warnings for Claude and Codex, with separate enablement and thresholds
  in minutes. Warn when more than the configured number of minutes has passed since
  the last message in that session. Track the last-message time, timer and warning
  state by agent and session; activity in another session must not reset the timer.
  Verify supported message-activity sources for each agent before implementation;
  agree on which messages reset the timer, defaults, repeat/reset behavior and session
  end/restart handling before coding. Describe this as an elapsed-time warning, without
  claiming that the provider's cache has actually expired. Acceptance: controlled-time
  tests cover the threshold boundary, timer reset, independent agent settings, session
  isolation, session cleanup and unavailable activity data; verify warnings in real
  supported agent sessions.
  Preliminary research (2026-10-30): a per-session elapsed-time reminder is feasible
  through lifecycle hooks, with resident-owned timers. Exact last-message semantics
  and actual cache expiry are separate questions; Claude additionally documents a
  reported cache deadline. See [feasibility report](docs/session-warnings-research.md).
  Agreed 2026-10-30: start the timer at the end of the agent response, not at user
  submission. New user/tool activity cancels it; the next completion rearms it.
  Default idle thresholds: Claude 55 minutes, Codex 25 minutes (agreed 2026-10-30);
  `idle_minutes = 0` disables an agent's reminder. One warning per idle episode,
  resident-only, memory-only bounded state. This slice is an elapsed-time reminder,
  not a provider cache-expiry guarantee. Live integration acceptance remains open.

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
