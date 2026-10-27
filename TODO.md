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

- [ ] **AH-060 — Platform seam, non-Windows build and CI.**
  Make the library and `main.rs` compile on macOS and Linux with stub platform modules.
  Define per-OS boundaries for overlay, tray, IPC, audio, activation, process spawn,
  HTTPS and open-link/open-file. Gate Windows-only tests. Replace hard-coded `\\`,
  `USERPROFILE` and `LOCALAPPDATA` assumptions with a path helper. Add `.gitattributes`
  and a GitHub Actions matrix (windows, macos, ubuntu) running fmt, clippy and test.
  Update the target statement in AGENTS.md, INFO.md, README and this file.
  Acceptance: the standard checks pass on all three OSes in CI; behavior on Windows
  is unchanged (existing Windows tests still pass).
  Status 2026-10-27: done and checked locally on Windows (fmt, clippy, tests, release
  build) and with `clippy --all-targets -D warnings` for `x86_64-unknown-linux-gnu` and
  `aarch64-apple-darwin`. Stubs live in `src/unsupported.rs`; `src/paths.rs` holds the
  path helper; `.github/workflows/ci.yml` (PR/master: fmt, clippy, test on three OSes) and
  `.github/workflows/release.yml` (tag `v*`: Windows x64 zip with SHA-256, draft, then
  publish behind the `release` environment; a first cut that AH-022 still has to
  complete) and `.gitattributes` are added. Remaining: a
  green CI run on all three OSes (the branch was not pushed; no Linux/macOS test has
  executed yet, only cross-compiled checks) and the Windows-only `\\` and
  `LOCALAPPDATA` literals inside `src/activation/native.rs` and `src/shell.rs`, which
  stay because those files are Windows-only.

- [ ] **AH-061 — Unix common layer (IPC, spawn, HTTPS, open).** Depends on AH-060.
  Unix-domain-socket IPC in a per-user directory (mode 0600, peer-credential check,
  no TCP, `flock` single instance); detached child spawn with fd closing; restart;
  `open`/`xdg-open`; an HTTPS client for usage polling (dependency comparison and user
  approval first). Acceptance: IPC framing/lifecycle, single-instance race and
  rejected-peer tests pass on macOS and Ubuntu; hook returns independently of the resident.
  Status 2026-10-27: implemented in `src/unix/` (`local`, `pipe`, `detached`, `restart`,
  `shell`, `https`; `libc` and `ureq` with rustls approved by the user, none of them in
  the Windows build) and `src/resident_client.rs` (`ensure`/`forward`, now shared).
  Verified on Windows (fmt, clippy, tests) and on Debian under WSL (clippy
  `-D warnings`, `cargo test --all-targets`: `tests/unix_ipc.rs`, `tests/unix_hook.rs`,
  `tests/https.rs` and the unit tests in `src/unix/`). Remaining before this closes: the
  same run on macOS (the MacBook or CI; ring needs Apple's C toolchain, so it cannot be
  cross-checked here) and a green CI run on both Unix runners. Not covered yet: the
  Unix resident loop itself needs an overlay (AH-062, AH-068); proxy behaviour of
  `ureq` (environment variables only) is unchecked, like the Windows system-proxy item
  in AH-045.

- [ ] **AH-062 — macOS overlay.** Depends on AH-061.
  Non-activating, transparent, floating `NSPanel` via `objc2`, reusing the portable BGRA
  rendering; text through CoreText. Handle multiple displays, Retina scale, Spaces and
  full-screen apps. Acceptance: manual check on the M1 MacBook: no focus theft, correct
  transparency, dismissal, event switching, display change.
  Agreed 2026-10-27: `objc2`, `objc2-app-kit`, `objc2-foundation` only under
  `cfg(target_os = "macos")`; text drawn with AppKit (`NSString`) into the bitmap, so no
  CoreText crate; scope is the overlay plus a resident without a tray (tray is AH-063).
  Step 1 done: `src/card_paint.rs` holds the card's shapes (straight BGRA) and a list of
  text runs; Windows `Surface::card` now draws those runs with GDI, with unchanged pixels
  (the existing card tests pass) and new portable tests. Remaining: the AppKit text
  renderer, the `NSPanel` presenter, the macOS event loop and `resident::run`, and a
  `cargo check --target aarch64-apple-darwin` seam (ureq/ring does not cross-build here,
  so make `ureq` an optional feature). None of it can run on this Windows machine, so it
  should be developed on the MacBook.

- [ ] **AH-063 — macOS tray.** Depends on AH-062.
  `NSStatusItem` and menu parity with Windows (sound, visual, pause, edit configuration,
  restart, exit, usage rows). Acceptance: manual menu check on macOS.

- [ ] **AH-064 — macOS audio.** Depends on AH-061.
  CoreAudio/AudioQueue playback with the serialized-playback gate. Acceptance: no
  overlapping playback across processes (process test), audible check on macOS.

- [ ] **AH-065 — macOS click-to-activate.** Depends on AH-062.
  Replace the HWND capture with PID/TTY-based terminal identification and
  `NSRunningApplication` activation; Codex and Claude Desktop URL schemes; Claude Desktop
  session lookup under `~/Library/Application Support/Claude` (still `cliSessionId` only).
  Document Accessibility/Automation prompts. Acceptance: manual check with Terminal,
  iTerm2 and Claude Desktop; policy tests stay platform-neutral.

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

- [ ] **AH-085 — Redesign the notification card layout.**
  Layout (left to right): a wider accent stripe whose colour marks the event kind; the
  artwork unchanged; the countdown top right unchanged. Text rows: (1) `<harness>, <folder>`
  (the folder is the last component of the hook's `cwd`; only `<harness>` when there is
  none, never a session ID); (2) the readable session name, or an empty row (the row is
  always reserved so the card never changes height); (3) the event kind, larger and set
  slightly apart, in the normal text colour. Bottom row: left `[Dismiss] <age>` (the button
  only when `click_to_dismiss = false`), right `[<] n/m [>]` only when several cards are
  queued; the arrows are bordered buttons in the Dismiss style and navigation wraps around.
  The usage card moves to the same layout. Folder is display-only: it is not logged.
  Acceptance: automated tests for folder extraction (separators, trailing separators, drive
  roots, control and bidi characters, length), the heading model, button geometry, wrap-around
  navigation and age text; Codex and Claude hooks carry the folder through the resident IPC;
  rendering tests keep the card height equal for one and several cards; docs and example
  config updated; manual visual check of both agents at 100% and 150% recorded.
  Scope added 2026-10-27: `border_color` (global, per event, usage) outlines the card and
  frames its buttons. Implemented with automated tests; remaining: the manual visual check
  (not done; no interactive desktop run yet).

- [ ] **AH-086 — `--version` and a tray About card.**
  `aherald --version` prints `aHerald <version>` (no configuration needed); the tray
  item "About aHerald" shows the version on a usage-style card and a second choice
  closes it. Implemented with automated tests (CLI output and conflicts, the card text,
  menu entry, and a native test that the card opens and closes). Remaining: look at the
  card on a real desktop at 100% and 150%.

- [ ] **AH-020 — Finish placement and display-change handling.** Depends on AH-012.
  Support center, top center, bottom right, near cursor, and selected monitor.
  Keep overlays in the work area and handle monitor removal, negative desktop
  coordinates, DPI changes, and scaling. Acceptance: manual multi-monitor and
  mixed-DPI checks pass without focus theft or screen-wide flashing.
  Implementation and automated checks committed; manual multi-monitor/mixed-DPI
  acceptance remains unverified (hardware unavailable, confirmed 2026-10-25).

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

- [ ] **AH-076 — Visual check of native-DPI cards at 100/150/200% (split from AH-048).**
  Depends on AH-048. Cards are now drawn at the target monitor's DPI. Check on a real
  display at 100%, 150% and 200%: sharp text, pixel art still crisp (nearest-neighbor),
  a long animation (more than 64 frames at 530×244) shows its artwork, and the usage
  card and a transient card redraw when dragged between monitors of different DPI
  (`WM_DPICHANGED`). `preview_placement` and `--demo` are the usual ways to see cards.
  Acceptance: observed results per scale recorded in `docs/milestone-5.md`.

- [ ] **AH-023 — Complete release acceptance.** Depends on AH-022.
  Run automated checks, Windows IPC/lifecycle integration tests, and the smoke
  matrix on Windows 10 and 11. Cover topmost/transparency/focus, sound, switching,
  tray, Explorer restart, DPI/monitors, hook latency, and long-lived waits.
  Acceptance: publish the tested environment/results; label any untested platform
  explicitly instead of claiming verified support.
  Status 2026-10-27: the automated part and hook latency on Windows 11 are recorded in
  `docs/release-acceptance.md`. Remaining: Windows 10 smoke, the visual/manual checks,
  long-lived waits, a GitHub release workflow run, and the unexplained `800700e9`
  pipe error noted there.

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
