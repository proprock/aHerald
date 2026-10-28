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

- [ ] **AH-062 — macOS overlay.** Depends on AH-061.
  Non-activating, transparent, floating `NSPanel` via `objc2`, reusing the portable BGRA
  rendering; text through CoreText. Handle multiple displays, Retina scale, Spaces and
  full-screen apps. Acceptance: manual check on the M1 MacBook: no focus theft, correct
  transparency, dismissal, event switching, display change.
  Agreed 2026-10-27: `objc2`, `objc2-app-kit`, `objc2-foundation` only under
  `cfg(target_os = "macos")`; text drawn with AppKit (`NSString`) into the bitmap, so no
  CoreText crate; scope is the overlay plus a resident without a tray (tray is AH-063).
  Step 1 done: `src/card_paint.rs` holds the card's shapes (straight BGRA) and a list of
  text runs; Windows `Surface::card` draws those runs with GDI (unchanged pixels).
  Status 2026-10-27 (branch `feature/ah-062-macos-overlay`): `src/mac/` has the AppKit
  text renderer (`text`), the in-memory `Surface`/`Composition` (`surface`), screen
  placement (`geometry`), the non-activating `NSPanel` (`panel`) and the resident loop
  (`resident`: IPC thread plus AppKit loop, fades, countdown, artwork, queue). Automated:
  unit tests for text, surface and geometry, `tests/mac_resident.rs` (status, queueing, quit,
  single instance). Observed with `--demo`: a 402x150 pt window at level 25, no change of the
  frontmost application. Mouse input (dismiss button, arrows, click-to-dismiss) works: `src/mac/input.rs` tests plus a
  live check with synthetic clicks on `--resident` (arrow 1/3 to 2/3, body click removes one
  card, one click one action). `ureq` is now behind the default `https-client` feature, so `cargo check --no-default-features
  --target aarch64-apple-darwin` needs no C toolchain (checked natively on the M1; not yet from
  Windows). Remaining: the manual checks on the M1 MacBook (transparency, multiple displays,
  Spaces, full-screen apps, display change), moving the duplicated IPC handler
  and `event_card_text` into shared code once Windows CI can verify the refactor.

- [ ] **AH-063 — macOS tray.** Depends on AH-062.
  Includes the usage card: it is opened and paged from the tray's usage rows and anchored
  under the status item (as on Windows), so it has no trigger before the tray exists.
  `NSStatusItem` and menu parity with Windows (sound, visual, pause, edit configuration,
  restart, exit, usage rows). Acceptance: manual menu check on macOS.
  Status 2026-10-28 (branch `feature/ah-063-macos-tray`): `src/mac/tray.rs` (`NSStatusItem`,
  quota-bar icon, tooltip), `menu.rs` (menu model with the Windows order, separators and check
  marks, unit-tested), the About and Codex usage cards under the item (`geometry::place_below`,
  scroll wheel pages), and the Restart-refused notice. Observed on the M1 with synthetic clicks:
  the menu opens, Pause toggles and shows its check mark, About and the Codex line open their
  card, a click closes it, Exit stops the resident. Usage with live quota data was checked against Codex 0.133 from the
  desktop app (95% / 95%, plus plan, matching the app); the CLI is found in
  `/Applications/Codex.app` or `~/Applications` when it is not on `PATH`, and reset times use the
  local zone. Checked by hand 2026-10-28: Edit configuration opens the file in the default
  editor; Restart replaces the process (the old one exits, a new pid answers `--status`, the
  icon disappears and returns). The menu bar icon was also read in dark and light system
  appearance (the bar keeps the wallpaper tint, the green bars stay readable in both; a light
  wallpaper was not tried). Not verified: the status item on a second display. Claude usage rows wait for AH-066.

- [ ] **AH-064 — macOS audio.** Depends on AH-061.
  CoreAudio/AudioQueue playback with the serialized-playback gate. Acceptance: no
  overlapping playback across processes (process test), audible check on macOS.
  Status 2026-10-28 (branch `feature/ah-064-macos-audio`): `src/unix/audio.rs`
  (`audio::serial`: `flock` gates for serial playback and repeat suppression, cancel-aware,
  reusable by Linux) and `src/mac/audio.rs` (AudioQueue through declared AudioToolbox calls, no
  new crate). Automated: gate and dedup unit tests with a fake output, `tests/unix_audio.rs`
  (a sound waits for another process's sound; a killed process frees the gate; the test fails
  when the gate is removed). The resident plays each new event once and mutes on Sound/Pause.
  Audible check: the manual smoke test plays the sample (`cargo test --lib
  mac::audio::tests::the_default_device -- --ignored`); the maintainer heard the three sounds of a resident
  in turn (turn_complete, input_required, error), with the cards, on 2026-10-28. Sounds are off
  unless an event sets `sound = '<wav>'`. Transient mode stays unsupported on macOS.

- [ ] **AH-065 — macOS click-to-activate.** Depends on AH-062.
  Replace the HWND capture with PID/TTY-based terminal identification and
  `NSRunningApplication` activation; Codex and Claude Desktop URL schemes; Claude Desktop
  session lookup under `~/Library/Application Support/Claude` (still `cliSessionId` only).
  Document Accessibility/Automation prompts. Acceptance: manual check with Terminal,
  iTerm2 and Claude Desktop; policy tests stay platform-neutral.
  Status 2026-10-28 (branch `feature/ah-065-macos-activate`): `src/mac/terminal.rs` (parent
  chain from `proc_pidinfo`, falling back to `ps` for root-owned `login`; first ancestor whose
  bundle id is Terminal, iTerm2, Ghostty, Warp, kitty, Alacritty or WezTerm; its pid travels in
  `Event::window`; raised through `open -b`, re-checked at click time) and `src/mac/activation.rs`
  (macOS agents and the worker; the policy is the shared `activation::click`). No Accessibility
  or Automation permission is needed because nothing is scripted; only the application comes
  forward, not a window or tab. Checked by hand on the M1: a hook run from Terminal.app, a click
  from Finder brings Terminal forward; a Claude event with a Desktop session id brings Claude
  Desktop forward (link `claude://claude.ai/epitaxy/local_*`); a Codex event brings Codex.app
  forward. With Desktop showing another session, a click on a card of this session's hook
  event switched it back to this session (title checked on a screenshot). With a second
  Desktop session of the same project, events for each session's id moved Desktop to that
  session and back (titles checked on screenshots).
  Not verified: iTerm2 (not installed), Warp and the other terminals, tab or window selection
  in a terminal (would need Automation).

- [ ] **AH-066 — macOS Claude usage via Keychain.** Depends on AH-061.
  Optional, default off: when `[claude.usage]` is enabled, read the Claude Code OAuth
  item from the Keychain and use it only for the usage endpoint. Needs an explicit
  AGENTS.md exception (extends AH-037) agreed first. Never log, copy or write back.
  Acceptance: parsing tests with fixtures; manual check; token never appears in logs.

- [ ] **AH-067 — macOS packaging and documentation.** Depends on AH-062, AH-063, AH-064.
  `.app` bundle (`LSUIElement`), config and assets location for a bundle, `.sh` hook
  examples, setup and removal steps, release check script. Decide arm64-only versus
  universal. Signing and notarization are deferred and documented as a limitation.
  Acceptance: bundle launches on the M1 MacBook; docs list tested macOS and Codex versions.

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
