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

- [x] **AH-091 — Hook setup commands and plugins.** Depends on AH-067.
  Agreed 2026-10-28: relax the "never edit hooks" rule to "only on an explicit user command, with a
  dry run by default, a backup and a diff of aHerald's own entries" (AGENTS.md). (1)
  `aherald --install-hooks|--uninstall-hooks claude|codex [--apply] [--hooks-file PATH]` merges only
  aHerald's entries (idempotent, key order and other content kept, symlink and mode kept, `*.aherald-backup`);
  Codex still needs the user to trust the hooks in `/hooks`. Needs serde_json `preserve_order` (approved).
  (2) Plugin `aherald` in a marketplace in this repository (`.claude-plugin/marketplace.json`, also read by
  Codex), hooks in `plugins/aherald/hooks/hooks.json` through a launcher script that finds `aHerald.app`
  and always exits 0. macOS first; the Windows (PowerShell, `AHERALD_PATH`) and Linux variants follow
  when verified on those machines. Acceptance: on the M1 the plugin installs from the repo with
  `claude plugin marketplace add` and delivers a card; `--install-hooks` round-trips on a real file.
  Accepted 2026-10-29 on the M1 (macOS): `--install-hooks claude` on a copy of the real
  `~/.claude/settings.json` (dry run unchanged; apply adds five entries and a backup; second apply no
  change; uninstall restores the content and key order, only a trailing newline is added); the plugin
  installed from this repository (`claude plugin marketplace add` and `install aherald@proprock`,
  isolated `CLAUDE_CONFIG_DIR`); the launcher script delivered a card through the installed app and
  exits 0 without it. Not run: Codex (`codex` not installed here). The Windows (PowerShell,
  `AHERALD_PATH`) and Linux plugin variants stay to be done when verified on those machines.

- [ ] **AH-093 — Linux overlay Xorg and multi-monitor acceptance (split from AH-068).**
  Depends on AH-068. On Ubuntu, log in to Xorg and confirm `XDG_SESSION_TYPE=x11`;
  manually verify card transparency, always-on-top behavior and absence of focus theft.
  Verify monitor placement and DPI on multiple monitors when available, recording the
  session type, desktop environment, monitor setup and results. Xorg and multi-monitor
  behavior have not yet been verified.

- [ ] **AH-069 — Linux tray.** Depends on AH-068.
  StatusNotifierItem with dbusmenu over D-Bus (dependency approval first); re-register
  when the watcher restarts. Document that GNOME needs the AppIndicator extension.
  Acceptance: manual menu check on Ubuntu; watcher-restart check.

- [ ] **AH-070 — Linux audio.** Depends on AH-061.
  Choose a backend with user approval (PulseAudio/PipeWire-pulse versus ALSA) and keep the
  serialized-playback gate. Acceptance: no overlapping playback; audible check on Ubuntu.

- [ ] **AH-071 — Linux click-to-activate and path discovery.** Depends on AH-068.
  `_NET_ACTIVE_WINDOW` activation and terminal-window capture; Codex executable discovery
  without `.exe`/`.cmd`; Claude and Codex paths through `HOME`/`XDG_*`. Acceptance:
  manual check on Ubuntu; discovery covered by tests with a fake `PATH`.

- [ ] **AH-072 — Linux packaging, documentation and verification matrix.**
  Depends on AH-068, AH-069, AH-070. `tar.gz` release (glibc versus musl decision), `.sh`
  hook examples, setup and removal steps. Acceptance: tested Ubuntu, session type
  (Xorg/Wayland) and Codex versions recorded; unsupported desktops listed explicitly.

- [ ] **AH-073 — Wayland spike.** Depends on AH-068.
  Evaluate `wlr-layer-shell` (KDE, wlroots) and the XWayland fallback on GNOME; list
  degraded features and their cost. Decide with the user whether to implement.
  Acceptance: written findings with measured behavior on the Ubuntu Wayland session.

## Milestone 5 — Polish and portable release

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
