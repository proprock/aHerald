# aHerald implementation plan

aHerald is a portable, native Windows 10/11 x64 notification companion. Codex is
the first supported agent. Ship one executable, `aherald.exe`, built with stable
Rust and the MSVC toolchain. Start with Milestone 1 and keep each milestone usable.

## Agreed decisions

- Default configuration: `%APPDATA%\aHerald\config.toml`; `--config` overrides it.
- Support resident and detached transient notifications. A hook must finish
  independently of notification lifetime, animation, monitoring, or usage queries.
- Use one overlay with a count and controls for switching between events.
- Supply simple original PNG animations and WAV sounds under MIT; assets remain
  external and replaceable without recompilation.
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

## Milestone 1 — Functional notification engine

- [x] **AH-001 — Bootstrap the native Rust project.** Dependencies: none.
  Create a testable library and one GUI executable targeting
  `x86_64-pc-windows-msvc`. Use focused modules for CLI/configuration, normalized
  events, process transport, notifications, tray, and Codex adapters as needed;
  avoid empty speculative subsystems. Prefer `windows`, `serde`, `serde_json`,
  `toml`, PNG-only `image`, and `tracing`; use standard-library concurrency and
  commit Cargo.lock. Acceptance: Cargo checks pass and a release executable builds
  without an external GUI framework or runtime dependency.

- [x] **AH-002 — Implement CLI and validated TOML configuration.** Depends on AH-001.
  Support `--hook`, `--resident`, `--transient`, `--event`, `--status`, `--quit`,
  and `--config`; report unimplemented later-milestone operations explicitly.
  Explicit mode arguments override the configured mode; reject conflicting
  explicit operations. Resolve relative assets against the configuration directory.
  Cover event enablement/duration, positioning/scaling/fades, sound volume,
  deduplication, resident options, Codex monitoring, and usage refresh.
  Acceptance: loading, defaults, invalid values, missing/malformed files, path
  resolution, and mode selection are tested; configuration failures cannot crash
  or change permission behavior in hook mode.

- [ ] **AH-003 — Define normalized events and bounded hook input.** Depends on AH-002.
  Model `turn_complete`, `permission_required`, `input_required`, `error`, and
  `interrupted`, with priority, source, and optional session/turn/request identity.
  Parse stdin using explicit size/time limits; tolerate unknown JSON fields and
  reject malformed or oversized inputs safely. Verify official hook contracts
  before adding Codex-specific parsing and output. Keep operational diagnostics
  off stdout and never emit approval, denial, continuation, or input rewrites.
  Acceptance: fixtures test normalization, optional identifiers, unknown events,
  malformed input, input bounds, and event-specific non-blocking responses.

- [ ] **AH-004 — Implement detached transient delivery.** Depends on AH-003.
  Launch a detached GUI child and hand off only bounded normalized event data
  through a local channel, not sensitive command-line arguments. Bound startup
  and transfer time. Keep `--event` available for demonstrations without Codex.
  Acceptance: an integration test proves the hook returns before an overlay's
  lifetime ends, including `duration_ms = 0`; startup failure exits safely.

- [ ] **AH-005 — Render a native layered overlay.** Depends on AH-004.
  Use `WS_POPUP`, `WS_EX_TOPMOST`, `WS_EX_LAYERED`, `WS_EX_TOOLWINDOW`, and
  `WS_EX_NOACTIVATE`, with `UpdateLayeredWindow` and correct alpha handling.
  Implement text fallback, click-to-dismiss, automatic dismissal, smooth fades,
  and message-loop timers. A zero duration persists until dismissal or a verified
  resolution event. Acceptance: no focus theft or Alt-Tab entry, responsive UI,
  transparent background, working timers and dismissal; basic DPI-correct sizing.

- [ ] **AH-006 — Load and animate external PNG assets.** Depends on AH-005.
  Support TOML-described sprite sheets in row-major order and deterministic PNG
  frame sequences. Support FPS and optional per-frame duration metadata, looping,
  and nearest-neighbor/bilinear scaling. Bound decoded dimensions, frame counts,
  and memory; handle invalid dimensions, missing files, and corrupt images with
  the text fallback. Acceptance: frame extraction, timing, sequence ordering,
  bounds, alpha conversion, and asset failures have focused tests.

- [ ] **AH-007 — Add native asynchronous sound and sample resources.** Depends on AH-006.
  Use a Windows audio API supporting per-playback volume without changing system
  volume. Serialize playback to prevent overlap; honor global/per-event settings
  and repeated-event suppression. Add original sample PNG/WAV resources, metadata,
  and example TOML for the event types. Acceptance: sound does not block the GUI,
  mute/volume work, missing WAV files are harmless, and asset provenance is recorded.

- [ ] **AH-008 — Accept the first vertical slice.** Depends on AH-001–AH-007.
  Run the standard Cargo checks and release build. Demonstrate transient visual
  and sound notifications using external resources and text fallback. Record a
  Windows smoke test and hook-return timing under finite and indefinite duration.
  Acceptance: the engine works without Codex installed and all focused tests pass.

## Milestone 2 — Resident architecture

- [ ] **AH-009 — Enforce a single resident instance.** Depends on AH-008.
  Scope a named mutex and named pipe to the current Windows user and logon session.
  Apply appropriate security descriptors and reject remote pipe clients. Require
  no elevation. Acceptance: concurrent starts yield one resident; tests exercise
  ownership, access restrictions, and recovery after process termination.

- [ ] **AH-010 — Implement bounded, versioned IPC and control operations.** Depends on AH-009.
  Add framing, validated lengths, acknowledgement, connection/read/write deadlines,
  and bounded retries. Forward hook events, start resident when absent, and implement
  `--status`/`--quit`. Acceptance: round-trip, oversized/malformed messages,
  incompatible versions, broken connections, startup races, and graceful lifecycle
  are tested; no TCP listener exists.

- [ ] **AH-011 — Queue, deduplicate, and switch notification events.** Depends on AH-010.
  Use a bounded queue with High/Medium/Low priority and FIFO within each priority.
  Show one overlay with count and navigation controls. Persistent waits must not
  prevent presentation of other events; dismissal affects only the selected event.
  Correlate duplicates by available session/turn/request identity and source,
  including cross-source duplicates; avoid merging unrelated sessions.
  Acceptance: simultaneous sessions, persistent events, overflow, repeated delivery,
  switching, priority, and individual dismissal are tested.

- [ ] **AH-012 — Add native tray and graceful shutdown.** Depends on AH-011.
  Use `Shell_NotifyIconW`; restore the icon after Explorer restarts. Add usage
  summary/details/refresh, sound and visual toggles, pause, configuration location,
  and exit. Pause suppresses presentation/audio while resolution processing
  continues. Acceptance: menu actions work, Explorer restart recovers, and exit
  stops workers and releases native resources; unavailable usage is explicit.

## Milestone 3 — Verified Codex integration

- [ ] **AH-013 — Record version and event-source compatibility.** Depends on AH-008.
  Inspect current official documentation and matching release/protocol schemas.
  Record tested Codex versions, payload fixtures, capabilities, and coverage limits.
  Planning observed `codex-cli 0.164.0-alpha.11.1`; this is not proof of stable
  release support. Acceptance: a technical note distinguishes verified behavior,
  unsupported behavior, and experimental/version-dependent behavior.

- [ ] **AH-014 — Deliver lifecycle hook integration and examples.** Depends on AH-010, AH-013.
  Verify `Stop`, `PermissionRequest`, and version-supported `Interrupt`; provide
  Windows hooks.json examples with correct path quoting and response contracts.
  Never decide permissions. Acceptance: mocked and real supported hook invocations
  forward notifications promptly; instructions explain manual installation,
  trust requirements, removal, and preservation of existing hooks.

- [ ] **AH-015 — Implement supported input-wait observation.** Depends on AH-013.
  Investigate documented stable sources for actual wait entry and resolution,
  including app-server request/resolution events where legitimately observable.
  Share app-server transport with AH-017 if needed. Do not assume a private server
  observes other CLI sessions, resume their threads, create a proxy client, or
  parse unstable transcripts. Acceptance: supported sources distinguish executing,
  waiting, and resolved states; unobservable sessions report unsupported explicitly.
  Lack of a stable source is documented rather than replaced with heuristic detection.

- [ ] **AH-016 — Correlate pending and terminal states.** Depends on AH-011, AH-014, AH-015.
  Resolve pending notifications only on verified matching request/turn lifecycle
  evidence. Deduplicate multiple sources and handle completion/interruption.
  A failed individual tool call is not automatically an agent execution error.
  Acceptance: fixtures cover multiple requests/sessions, late resolution,
  duplicate sources, and unsupported resolution; manual dismissal always works.

## Milestone 4 — Usage and quotas

- [ ] **AH-017 — Connect to the official app-server.** Depends on AH-013.
  Manage one long-lived stdio child with handshake, bounded JSON messages,
  request IDs, timeout, reconnect/backoff, and shutdown. Handle Codex absence,
  authentication unavailability, and unsupported methods without blocking UI.
  Acceptance: mock transport tests cover normal responses, timeouts, disconnects,
  malformed messages, and reconnect; credentials are never read or logged directly.

- [ ] **AH-018 — Normalize available rate-limit data.** Depends on AH-017.
  Implement `account/rateLimits/read` and supported updates, preferring multi-bucket
  data when available. Preserve primary/secondary windows, percentages, Unix reset
  timestamps, additional buckets, and officially exposed credits. Distinguish
  missing data from zero. Do not consume reset credits or add account mutations.
  Acceptance: fixtures cover legacy/multi-bucket, optional fields, unknown fields,
  unavailable values, and local-time reset conversion including DST boundaries.

- [ ] **AH-019 — Present usage independently of notifications.** Depends on AH-012, AH-018.
  Show compact tray usage and a native details window for all available buckets.
  Use local reset times, default 300-second refresh, manual refresh, and explicit
  stale/unavailable states. Acceptance: usage failure leaves notifications working,
  polling is bounded/backed off, and no session data goes to third parties.

## Milestone 5 — Polish and portable release

- [ ] **AH-020 — Finish placement and display-change handling.** Depends on AH-012.
  Support center, top center, bottom right, near cursor, and selected monitor.
  Keep overlays in the work area and handle monitor removal, negative desktop
  coordinates, DPI changes, and scaling. Acceptance: manual multi-monitor and
  mixed-DPI checks pass without focus theft or screen-wide flashing.

- [ ] **AH-021 — Harden resources and recovery.** Depends on AH-016, AH-019, AH-020.
  Add bounded resident asset caching and operational logging with no credentials,
  full conversation content, or sensitive tool arguments. Exercise queue floods,
  duplicate storms, malformed assets, resident crashes, broken IPC, and unavailable
  Codex. Acceptance: limits and recovery behavior are documented and tested;
  repeated notifications do not leak handles, GDI objects, threads, or memory.

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
