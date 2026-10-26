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

## Milestone 5 — Polish and portable release

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

- [ ] **AH-037 — Opt-in Claude 5h/7d quota.** Depends on AH-036.
  `[claude.usage] enabled` (default false, interval 300..86400 s) polls the undocumented
  `GET api.anthropic.com/api/oauth/usage` over WinHTTP with the token Claude Code stores
  in `.credentials.json`. Agreed exception to the token-reading rule (see AGENTS.md):
  token used only for that call, never logged, stored or refreshed. Parsing, credential,
  status and driver behavior are tested with fakes (no network). Live check on
  2026-10-25 (signed-in Claude Code, `anthropic-beta: oauth-2025-04-20`): HTTP 200 with
  `five_hour`/`seven_day` `{utilization, resets_at}` as assumed (a stale token gave 401).
  Tested with Claude Code 2.1.310. Manually confirmed 2026-10-25: separate Claude menu
  line (alphabetical before Codex), details card with plan, 5h/Week, Extra usage and Spend,
  forced refresh on opening details. Remaining: confirm the tray
  tooltip/menu/card with both agents enabled.

- [ ] **AH-056 — Verify Claude click in Windows Terminal (split from AH-055).**
  Depends on AH-055. Only the classic console host was checked live. With Windows Terminal
  installed, confirm that a Claude CLI card raises the Windows Terminal window (the hook
  records the owner of the pseudo-console window), that two terminal windows of different
  sessions each raise their own window, and that a minimized window restores. Known limit:
  tabs are not selected; decide whether UI Automation tab selection is worth its fragility.

- [ ] **AH-038 — Claude follow-ups (deferred).**
  Session names for Claude; `SubagentStop`/`SessionEnd`; a
  statusLine-based quota fallback (`rate_limits` in statusLine JSON, no token needed);
  honor `Retry-After` and a 15-minute backoff cap for Claude polling; move remaining
  Codex-specific sites (activation, session labels, terminal evidence) behind `Agent`.

- [ ] **AH-051 — Pure card presenter and reentrancy-safe Ui (split from AH-042).**
  Depends on AH-042. Move "which card, frame, alpha and sounds now" into a pure
  presenter driven by queue state and a controlled clock, leaving `Ui` a thin Win32
  adapter; stop the window procedure from borrowing `Ui` while `draw` holds it across
  reentrant `ShowWindow`/`SetWindowTextW`. Present a static card only when frame,
  alpha, countdown second or placement changes; cache usage-card layout and tray
  placement while the card is open. Acceptance: presenter unit tests; lifecycle, tray
  and leak tests; standard Cargo checks. Source: `audit-20261026.md` A6, E7, P2.

- [ ] **AH-052 — Finish the usage-source cleanup (split from AH-044).**
  Depends on AH-044. Remove the test-only `View::summary`/`details`/`brief` path and
  move its assertions to the shown card text; type `Bucket.details` (for example as
  structured extra-usage and spend fields) instead of pre-formatted lines; drop the
  Codex-shaped JSON that `claude_usage::snapshot_from_response` still builds before
  `Snapshot::parse`. Acceptance: tests assert the card text; no production code is
  kept only for tests; standard Cargo checks. Source: `audit-20261026.md` A3, A4.

- [ ] **AH-045 — Harden the Claude quota HTTPS request.** Depends on AH-037.
  Implemented and tested; left open only for the manual system-proxy check (steps in
  `docs/claude-hooks.md`, "Manual checks"), which was not run. Done: redirect policy
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

- [ ] **AH-048 — Reduce animation memory and sharpen scaled text.** Depends on AH-020, AH-041.
  Each animation frame keeps a full-card DIB and DC, so long animations exceed the
  32 MiB budget and silently drop to text, and session-name updates redraw every
  frame. Compose one base card and blend only the artwork region per frame. Cards
  render at 96 DPI and scale nearest-neighbor, which makes text blocky at 125–200%.
  `docs/milestone-5.md` records that choice, so confirm the change with the user,
  then render at the monitor's effective DPI, rebuild on `WM_DPICHANGED` and keep
  nearest-neighbor for pixel art only. Acceptance: an animation that exceeded the
  budget shows artwork within bounded memory; leak tests pass; visual check at
  100/150/200%; standard Cargo checks. Source: `audit-20261026.md` P3, P4.

- [ ] **AH-049 — Reuse app-server clients and decoded sounds.**
  The resident runs separate `codex app-server` processes for usage and session
  names, and transient mode starts one per Codex card. WAV files are read and parsed
  for every event, bilinear scaling premultiplies twice, `activation::find` allocates
  64 KB per enumerated window, and the IPC listener repeats the queue's terminal,
  duplicate and obsolete checks. Share one app-server client in the resident where
  the protocol allows, cache decoded sounds per file and volume, and let `push`
  return its outcome. Acceptance: process tests show the expected app-server child
  count; sound, scaling, activation and queue tests keep their results; standard
  Cargo checks. Source: `audit-20261026.md` P5, P6.

- [ ] **AH-050 — Tighten internal types and remove dead code.**
  Replace string-typed `general.mode`, `logging`, `notifications.position`, `monitor`
  and `interpolation` with Serde enums, and the `events` string map with typed
  per-kind access. Replace `Result<_, String>` matching such as
  `"local pipe deadline exceeded"` with typed IPC errors. Share the RAII handle (three
  copies), the Segoe UI font constructor (three calls) and alpha restoration after
  GDI text; narrow pixel-loop `unsafe` blocks to FFI calls. Remove unused
  `Agent::hook_source` and the unused `now` argument of `View::status`; keep
  test-only helpers only where tests need them. Acceptance: configuration format and
  behavior are unchanged; invalid values still fail validation; standard Cargo
  checks. Source: `audit-20261026.md` A2, A5, A7.

- [ ] **AH-053 — Test the transient placement mutex timeout race (split from AH-047).**
  Depends on AH-047. Transient cards serialize placement with a named mutex held until
  the card is shown, with a 500 ms wait. No test covers two hook processes where the
  second hits that timeout. Add one that holds the mutex past the timeout and asserts
  the waiting process still shows its card (or degrades as designed) without hanging
  or leaking the handle. Acceptance: the test fails if the timeout path blocks or
  drops the card; standard Cargo checks.

- [ ] **AH-022 — Package and document the portable application.** Depends on AH-021.
  Produce a release ZIP containing executable, example configuration, PNG/WAV
  assets, metadata, license, and instructions. Explain resident/transient modes,
  CLI, replacement assets, configuration, manual hooks/autostart, uninstall, build
  commands, and event-source limitations. Acceptance: clean extraction works
  without administrator privileges or installed GUI runtimes.

- [ ] **AH-023 — Complete release acceptance.** Depends on AH-022.
  Run automated checks, Windows IPC/lifecycle integration tests, and the smoke
  matrix on Windows 10 and 11. Cover topmost/transparency/focus, sound, switching,
  tray, Explorer restart, DPI/monitors, hook latency, and long-lived waits.
  Acceptance: publish the tested environment/results; label any untested platform
  explicitly instead of claiming verified support.

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
