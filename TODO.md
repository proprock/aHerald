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

- [ ] **AH-067 — macOS packaging and documentation.** Depends on AH-062, AH-063, AH-064.
  `.app` bundle (`LSUIElement`), config and assets location for a bundle, `.sh` hook
  examples, setup and removal steps, release check script. Decide arm64-only versus
  universal. Signing and notarization are deferred and documented as a limitation.
  Acceptance: bundle launches on the M1 MacBook; docs list tested macOS and Codex versions.

- [ ] **AH-088 — Share the code the macOS port duplicated from Windows.** Depends on AH-062,
  AH-065.
  The resident IPC request handler, `event_card_text`, the activation worker and the card
  surface/composition exist twice (`src/resident.rs`, `overlay.rs`, `activation/native.rs` and
  `src/mac/`) because the Windows code could not be compiled while the macOS port was written.
  Extract them into portable modules used by both. Acceptance: no behavior change, Windows CI
  and the native Windows tests green.
  Status 2026-10-28 (branch `refactor/ah-088-share-code`): the IPC request handler is
  `resident_request::respond` (unit-tested; both residents call it), `event_card_text` lives in
  `card_paint`, and `activation::Worker` is portable with a per-platform runner (unit-tested).
  The Windows side was type-checked and linted from the Mac with
  `cargo clippy --target x86_64-pc-windows-msvc --all-targets`; Windows CI is the run check.
  Remaining: the card surface and composition (`overlay.rs` `Surface`/`Composition` against
  `mac/surface.rs`), which needs the GDI surface moved onto a shared pixel buffer.

- [ ] **AH-068 — Linux X11 overlay.** Depends on AH-061.
  `x11rb`: ARGB visual, notification window type, no focus, RandR monitors, DPI.
  Works under XWayland. Acceptance: manual check on Ubuntu (Xorg login and XWayland):
  transparency, always on top, no focus theft, multi-monitor if available.

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
  Audit, still silent: a configured `monitor` index with no such display (falls back to the main
  one); hook input that is malformed or unknown (silent by contract on stdout, but no log line);
  events of a disabled kind and deduplicated or obsolete events (no debug line); artwork
  prepared at a smaller scale because of the memory budget (counter only); `let _ =` on child
  start-up in the hook path (`resident_client`).
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

- [ ] **AH-059 — Claude session names and statusLine quota fallback (split from AH-038).**
  Priority: low. Deferred to the next milestone (decided 2026-10-27); not started.
  Both items need the Claude `statusLine` command, the only place `session_name` and
  `rate_limits` (`five_hour`/`seven_day` `used_percentage`, `resets_at` in Unix seconds)
  are delivered; hooks carry neither, and transcripts are off limits (AGENTS.md). Claude
  Code allows one `statusLine` command, so aHerald must not replace the user's.
  Candidate design, to confirm with the user first: `aherald.exe --statusline
  [--then <command>]` forwards only `session_id`, `session_name` and `rate_limits` to the
  resident, then pipes stdin to the user's previous command and prints its output; manual
  setup only, with an example and removal steps. Alternative to check for Desktop only:
  whether a readable title exists in `local_*.json` (needs a new privacy exception).
  Acceptance: tests for the forwarded fields and the pass-through; the name shown on the
  Claude card; a quota card without reading the OAuth token; docs updated.

## Standard verification

```text
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
cargo build --release --target x86_64-pc-windows-msvc
```

Use mocked Codex streams and app-server responses for deterministic tests. Native
integration tests must clean up their own processes and IPC objects. Manual visual
checks supplement, rather than replace, logic and lifecycle tests.

## Official integration references

Planning references checked on 2026-10-24; verify again against the version being
implemented. Release behavior and installed capabilities take precedence over
unreleased schemas.

- [Codex hooks](https://learn.chatgpt.com/docs/hooks)
- [Codex app-server](https://learn.chatgpt.com/docs/app-server)
