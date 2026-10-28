# aHerald completed tasks

Tasks moved here from TODO.md after every deliverable and acceptance criterion
passed. Open follow-ups and deferred items stay in TODO.md under new IDs.

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

- [x] **AH-003 — Define normalized events and bounded hook input.** Depends on AH-002.
  Model `turn_complete`, `permission_required`, `input_required`, `error`, and
  `interrupted`, with priority, source, and optional session/turn/request identity.
  Parse stdin using explicit size/time limits; tolerate unknown JSON fields and
  reject malformed or oversized inputs safely. Verify official hook contracts
  before adding Codex-specific parsing and output. Keep operational diagnostics
  off stdout and never emit approval, denial, continuation, or input rewrites.
  Acceptance: fixtures test normalization, optional identifiers, unknown events,
  malformed input, input bounds, and event-specific non-blocking responses.

- [x] **AH-004 — Implement detached transient delivery.** Depends on AH-003.
  Launch a detached GUI child and hand off only bounded normalized event data
  through a local channel, not sensitive command-line arguments. Bound startup
  and transfer time. Keep `--event` available for demonstrations without Codex.
  Acceptance: an integration test proves the hook returns before an overlay's
  lifetime ends, including `duration_ms = 0`; startup failure exits safely.

- [x] **AH-005 — Render a native layered overlay.** Depends on AH-004.
  Use `WS_POPUP`, `WS_EX_TOPMOST`, `WS_EX_LAYERED`, `WS_EX_TOOLWINDOW`, and
  `WS_EX_NOACTIVATE`, with `UpdateLayeredWindow` and correct alpha handling.
  Implement text fallback, click-to-dismiss, automatic dismissal, smooth fades,
  and message-loop timers. A zero duration persists until dismissal or a verified
  resolution event. Acceptance: no focus theft or Alt-Tab entry, responsive UI,
  transparent background, working timers and dismissal; basic DPI-correct sizing.

- [x] **AH-006 — Load and animate external PNG assets.** Depends on AH-005.
  Support TOML-described sprite sheets in row-major order and deterministic PNG
  frame sequences. Support FPS and optional per-frame duration metadata, looping,
  and nearest-neighbor/bilinear scaling. Bound decoded dimensions, frame counts,
  and memory; handle invalid dimensions, missing files, and corrupt images with
  the text fallback. Acceptance: frame extraction, timing, sequence ordering,
  bounds, alpha conversion, and asset failures have focused tests.

- [x] **AH-007 — Add native asynchronous sound and sample resources.** Depends on AH-006.
  Use a Windows audio API supporting per-playback volume without changing system
  volume. Serialize playback to prevent overlap; honor global/per-event settings
  and repeated-event suppression. Add original sample PNG/WAV resources, metadata,
  and example TOML for the event types. Acceptance: sound does not block the GUI,
  mute/volume work, missing WAV files are harmless, and asset provenance is recorded.

- [x] **AH-008 — Accept the first vertical slice.** Depends on AH-001–AH-007.
  Run the standard Cargo checks and release build. Demonstrate transient visual
  and sound notifications using external resources and text fallback. Record a
  Windows smoke test and hook-return timing under finite and indefinite duration.
  Acceptance: the engine works without Codex installed and all focused tests pass.

## Additional asset support

- [x] **AH-024 — Load local Codex pet packages.** Depends on AH-008.
  Accept `notifications.pet` pointing to local `pet.json`, resolve the sprite path
  inside the package, and decode PNG/WebP standard v1/v2 atlases. Map aHerald events
  to the documented occupied state frames and timing. Keep explicit event animations
  as overrides; resource errors use text fallback. Do not run installer commands.
  Acceptance: focused tests verify paths, frames, alpha, timing, version/size bounds,
  invalid resources and native window recovery; visually demonstrate a real pet.
  User approved extending the existing image decoder with WebP on 2026-10-24.

- [x] **AH-025 — Keep source and signal text beside optional agent artwork.** Depends on AH-024.
  Always render the source and normalized signal in one native card. Add animation
  as an optional decoration, selected by agent with a shared fallback; preserve
  explicit event animation overrides. Invalid artwork keeps the source/text card.
  Acceptance: test source isolation, asset precedence, visible text pixels, alpha
  composition, native card recovery/dismissal; visually inspect the Codex/Becky card.
  This slice does not implement Claude event ingestion, a resident queue, or replies.

## Milestone 2 — Resident architecture

- [x] **AH-009 — Enforce a single resident instance.** Depends on AH-008.
  Scope a named mutex and named pipe to the current Windows user and logon session.
  Apply appropriate security descriptors and reject remote pipe clients. Require
  no elevation. Acceptance: concurrent starts yield one resident; tests exercise
  ownership, access restrictions, and recovery after process termination.

- [x] **AH-010 — Implement bounded, versioned IPC and control operations.** Depends on AH-009.
  Add framing, validated lengths, acknowledgement, connection/read/write deadlines,
  and bounded retries. Forward hook events, start resident when absent, and implement
  `--status`/`--quit`. Acceptance: round-trip, oversized/malformed messages,
  incompatible versions, broken connections, startup races, and graceful lifecycle
  are tested; no TCP listener exists.

- [x] **AH-011 — Queue, deduplicate, and switch notification events.** Depends on AH-010.
  Use a bounded queue with High/Medium/Low priority and FIFO within each priority.
  Show one overlay with count and navigation controls. Persistent waits must not
  prevent presentation of other events; dismissal affects only the selected event.
  Correlate duplicates by available session/turn/request identity and source,
  including cross-source duplicates; avoid merging unrelated sessions.
  Acceptance: simultaneous sessions, persistent events, overflow, repeated delivery,
  switching, priority, and individual dismissal are tested.

- [x] **AH-012 — Add native tray and graceful shutdown.** Depends on AH-011.
  Use `Shell_NotifyIconW`; restore the icon after Explorer restarts. Add usage
  summary/details/refresh, sound and visual toggles, pause, configuration location,
  and exit. Pause suppresses presentation/audio while resolution processing
  continues. Acceptance: menu actions work, Explorer restart recovers, and exit
  stops workers and releases native resources; unavailable usage is explicit.

## Milestone 3 — Verified Codex integration

- [x] **AH-013 — Record version and event-source compatibility.** Depends on AH-008.
  Inspect current official documentation and matching release/protocol schemas.
  Record tested Codex versions, payload fixtures, capabilities, and coverage limits.
  Planning observed `codex-cli 0.164.0-alpha.11.1`; this is not proof of stable
  release support. Acceptance: a technical note distinguishes verified behavior,
  unsupported behavior, and experimental/version-dependent behavior.

- [x] **AH-014 — Deliver lifecycle hook integration and examples.** Depends on AH-010, AH-013.
  Verify `Stop`, `PermissionRequest`, and version-supported `Interrupt`; provide
  Windows hooks.json examples with correct path quoting and response contracts.
  Never decide permissions. Acceptance: mocked and real supported hook invocations
  forward notifications promptly; instructions explain manual installation,
  trust requirements, removal, and preservation of existing hooks.

- [x] **AH-015 — Implement supported input-wait observation.** Depends on AH-013.
  Investigate documented stable sources for actual wait entry and resolution,
  including app-server request/resolution events where legitimately observable.
  Share app-server transport with AH-017 if needed. Do not assume a private server
  observes other CLI sessions, resume their threads, create a proxy client, or
  parse unstable transcripts. Acceptance: supported sources distinguish executing,
  waiting, and resolved states; unobservable sessions report unsupported explicitly.
  Lack of a stable source is documented rather than replaced with heuristic detection.

- [x] **AH-016 — Correlate pending and terminal states.** Depends on AH-011, AH-014, AH-015.
  Resolve pending notifications only on verified matching request/turn lifecycle
  evidence. Deduplicate multiple sources and handle completion/interruption.
  A failed individual tool call is not automatically an agent execution error.
  Acceptance: fixtures cover multiple requests/sessions, late resolution,
  duplicate sources, and unsupported resolution; manual dismissal always works.

## Milestone 4 — Usage and quotas

- [x] **AH-017 — Connect to the official app-server.** Depends on AH-013.
  Manage one long-lived stdio child with handshake, bounded JSON messages,
  request IDs, timeout, reconnect/backoff, and shutdown. Handle Codex absence,
  authentication unavailability, and unsupported methods without blocking UI.
  Acceptance: mock transport tests cover normal responses, timeouts, disconnects,
  malformed messages, and reconnect; credentials are never read or logged directly.
  Codex discovery: prefer codex.exe in absolute PATH directories, then fall back
  to the existing npm installation's native x64 binary; never copy/install Codex
  or execute npm shims. Verify precedence, spaced prefixes, missing/incomplete
  installations, and a live usage read with the ordinary Windows PATH.

- [x] **AH-018 — Normalize available rate-limit data.** Depends on AH-017.
  Implement `account/rateLimits/read` and supported updates, preferring multi-bucket
  data when available. Preserve primary/secondary windows, percentages, Unix reset
  timestamps, additional buckets, and officially exposed credits. Distinguish
  missing data from zero. Do not consume reset credits or add account mutations.
  Acceptance: fixtures cover legacy/multi-bucket, optional fields, unknown fields,
  unavailable values, and local-time reset conversion including DST boundaries.

- [x] **AH-019 — Present usage independently of notifications.** Depends on AH-012, AH-018.
  Show compact tray usage and a native details window for all available buckets.
  Use local reset times, default 300-second refresh, manual refresh, and explicit
  cached values and unavailable states. Acceptance: usage failure leaves notifications working,
  polling is bounded/backed off, and no session data goes to third parties.

## Milestone 5 — Polish and portable release

- [x] **AH-026 — Preview the current configuration in a looping demo.**
  Support `--demo [SECONDS]`, with a three-second default pause between scenarios.
  Cycle enabled event types and a multi-event navigation scenario through the shared
  native engine; retain configured resources, sounds, position, scale and finite
  durations. Advance persistent waits automatically. Keep demo events isolated from
  the resident and allow tray pause/exit. Acceptance: CLI validation, deterministic
  timing/repetition/disabled-event tests, native lifecycle/isolation tests, visual
  inspection and standard Cargo checks. No configuration is written.
  Verified on Windows 11 build 22621 on 2026-10-25; see `docs/demo.md` for
  commands, manual observations, cold-cache behavior and test limitations.

- [x] **AH-027 — Simplify cards and expose visual event settings.**
  Hide count/navigation for one event; show bottom-right `<`/`>` for multiple.
  Dismiss informational cards by body click; retain an explicit Dismiss button
  only when body dismissal is disabled. Show remaining seconds for finite events.
  Use configurable per-event accent colors and shared/agent pet state overrides
  covering all nine standard tracks. Acceptance: behavioral layout/input/timer,
  configuration and atlas extraction tests, native pixel/lifecycle checks, visual
  inspection and standard Cargo checks. See `docs/card-controls.md`.

- [x] **AH-028 — Make artwork sources explicit and defaults usable.**
  Support global auto/builtin/pet and per-event inherit/builtin/pet/custom.
  Enable full builtin, full pet and both hybrid directions without clearing old
  animation paths. Stage external builtin resources beside Cargo executables.
  Recover failed pet/custom assets to builtin, then text. Acceptance: executable
  source-selection/configuration/fallback tests, native default-resource checks,
  visual inspection and standard Cargo checks. See `docs/visuals.md`.

- [x] **AH-029 — Expire grouped demo cards individually.**
  Give each selected grouped card its own preview timeout; retain the remaining
  cards until they expire or are dismissed. Hide the window only after the last
  card, then begin the configured quiet interval. Use the queued lifetime for
  countdowns and fades, including demo waits. Acceptance: a controlled-time
  three-card regression, repeat/navigation checks, one visual pass and Cargo checks.
  See `docs/demo.md`.

- [x] **AH-030 — Identify the originating Codex session.**
  Show `Codex: <stored thread name>` in resident and transient cards, falling back
  to a short session ID when unavailable. Resolve only supported read-only metadata
  in the background, without delaying hooks or restarting card lifetimes. Bound
  cache/queues and isolate concurrent sessions. Acceptance: protocol, cache and
  native rendering checks, one visual inspection, and standard Cargo verification.
  See `docs/session-source.md`; automatic sidebar captions may have no stored name.

- [x] **AH-021 — Harden resources and recovery.** Depends on AH-016, AH-019, AH-020.
  Add bounded resident asset caching and operational logging with no credentials,
  full conversation content, or sensitive tool arguments. Exercise queue floods,
  duplicate storms, malformed assets, resident crashes, broken IPC, and unavailable
  Codex. Acceptance: limits and recovery behavior are documented and tested;
  repeated notifications do not leak handles, GDI objects, threads, or memory.

- [x] **AH-031 — Simplify the tray menu and compact usage summary.**
  Show `Codex: 5h 25% / 2:35, 7d 10% / 6d 11h`; configure `[usage] display`
  as `remain` (default) or `used`. Use days and whole hours, H:MM below one day, and minutes
  below one hour, with explicit missing values and retained last received values. Remove Refresh usage and
  the separate Usage details action. The summary adds one persistent usage card
  to the shared notification queue beside the tray: plan, both windows, local
  minute-precision reset dates, workspace credits and all reset credits. Queue
  arrows switch cards; mouse wheel pages long details; body click dismisses only
  the usage card. Open configuration through its Windows association, falling
  back to Notepad. Restart gracefully replaces the process and reloads settings.
  Verified with domain/native lifecycle tests, synthetic visual inspection of
  details and queue navigation, and standard Cargo checks. See `docs/tray-usage.md`.

- [x] **AH-032 — Switch to the agent application on notification click.**
  Configure `[notifications] activate_agent_on_click = true` by default.
  Body click opens `codex://threads/<thread-id>` with validated, encoded identity;
  missing identity or shell failure tries an existing Codex window. Dismissal
  remains controlled independently by `click_to_dismiss`. Arrows and Dismiss
  never activate; demo and usage stay isolated. Resident and transient share the
  activation adapter. Failures retain card behavior and record a fixed diagnostic.
  Verified with focused/native tests, standard Cargo checks, and user-confirmed
  transition from a transient test card to the correct chat. See
  `docs/notification-click.md` for versions and manual-check limitations.

- [x] **AH-033 — Default to config.toml beside the executable.**
  Without `--config`, use `config.toml` beside `aherald.exe`, independently of
  the working directory. Create an absent standard file with current defaults;
  preserve existing files. No APPDATA search or automatic migration. Explicit
  missing custom files remain errors. Resolve resources from the selected file.
  Report creation errors with target path and reason. Simplify standard hook
  examples and document manual migration. Verified with process tests under
  Unicode/spaced paths, concurrent creation, preservation and failures, and
  standard Cargo checks. See `docs/configuration.md`.

- [x] **AH-034 — Show usage levels in a dynamic tray icon.**
  Two columns show primary/5h and secondary/week. Fill follows usage.display;
  remaining quota sets green (>60%), yellow (>20%) or red, with unavailable gray.
  Last received values remain current throughout the icon, tooltip, menu and card;
  age and refresh errors remain in details. Native sizes 16/20/24/32 were visually
  checked. Tests verify updates, pixel/handle reuse, TaskbarCreated recovery and
  restoring the current icon after shell deletion. Standard Cargo checks and the
  Windows x64 release build passed. See `docs/tray-usage.md` for evidence and limits.

- [x] **AH-035 — Configure card background colors.**
  Add global `[notifications] background_color` with the existing #1C232D default.
  Optional `[events.NAME] background_color` and `[usage] background_color` override
  it by card type; omitted overrides inherit the global setting. Accept #RRGGBB
  as for accent color, preserving card opacity, rounded transparency and artwork.
  Verified: parsing/inheritance and native rendered pixels pass functional tests;
  inherited event, overridden event and usage backgrounds were visually checked.
  Standard Cargo checks and Windows x64 release build passed. See `docs/card-controls.md`.

- [x] **AH-036 — Claude Code hook notifications.**
  `Agent` enum (`src/agent.rs`), `--hook --agent claude`, `[claude] hooks_enabled`,
  mapping of `Notification`/`Stop`/`StopFailure`/`PermissionRequest` per
  `docs/claude-hooks.md`; the hook never answers Claude Code. Automated tests pass.
  Live checks (Claude Code, 2026-10-25 and 2026-10-26, interactive session with project
  hooks and `--setting-sources project`): `PermissionRequest`, `Notification`
  (`permission_prompt`, `idle_prompt`) and `Stop` payloads matched the documented
  fields; permission required and turn complete cards appeared, the permission card was
  cleared by `Stop`, and the hook exited 0 with empty stdout. `PermissionRequest` and
  `permission_prompt` arrived 6 s apart and showed two cards; fixed so an unresolved
  Claude card absorbs repeats (`claude_notification_arriving_seconds_after_permission_request_joins_the_pending_card`
  in tests/queue.rs, red before the fix). After the fix one card was observed.
  Not observed live: `StopFailure` (follow-up AH-054).

- [x] **AH-039 — Select tray graph agents.**
  Add `[tray] graph = "auto" | "both" | "claude" | "codex"`, default auto.
  Auto shows the agent with the lowest remaining percentage across 5h/week;
  ties prefer Codex, disabled/no-data agents are excluded, missing windows stay gray.
  Both splits each window's bar by width: Claude left, Codex right, independent
  full-height fills and fixed positions. Exclusive modes never fall back.
  Preserve display modes, colors, cached values, DPI sizing and icon reuse/restore.
  Acceptance: config/selection/raster/native tests, four-mode visual preview at
  16/20/24/32 px, isolated Explorer restoration and standard Cargo checks pass.
  Graph selection does not enable polling or change menu/card behavior.
  Verified on Windows 11 build 22621 with synthetic data; see `docs/tray-usage.md`.

- [x] **AH-040 — Resolve stale Claude pending cards.** Depends on AH-036.
  Done: Claude `Stop`/`StopFailure` resolve same-session pending cards; Claude pending
  events dedupe per session; every new event sounds once (agreed with the user).
  The live double-fire of `PermissionRequest` + `Notification` is still unobserved;
  dedup is safe either way. Original brief:
  Claude permission/input cards are persistent and never receive terminal evidence
  (`turn_id` is absent; `is_terminal_evidence` accepts only Codex hooks). They stay
  until dismissed, hold the High-rank selection and silence every later sound; with
  visuals off they cannot be dismissed at all. A Claude `Stop`/`StopFailure`
  resolves pending cards of the same `session_id` only. Deduplicate a
  `PermissionRequest` and `Notification` `permission_prompt` pair for one prompt if
  the AH-036 payload captures show both fire. Decide with the user how queued,
  unselected events announce themselves. Overlaps the terminal-evidence part of
  AH-038. Acceptance: queue tests for same-session clearing, cross-session and
  cross-agent isolation, the duplicate pair, and a later sound after a hidden
  persistent card; standard Cargo checks. Source: `audit-20261026.md` E1.

- [x] **AH-041 — Keep the resident alive on card render failures.** Depends on AH-021.
  Done: shared `Composition::compose` (largest frame, byte budget, text fallback) used by
  both modes; a resident draw failure hides the window, logs `render_failed` once,
  keeps the queue and retries next tick. Placement failure shows no card until placement
  succeeds (no fabricated position). Native fault tests use a debug-build-only
  `aherald.fault` file beside the configuration. Transient already cached countdown
  frames, set the title and scaled mouse input. Original brief:
  Card, artwork, placement and countdown errors propagate through `Ui::draw` and
  stop the resident, while transient mode falls back to a text card. Extract one
  card composition shared by both modes: size by the largest frame, enforce the
  animation byte budget, fall back to text, cache countdown frames, update the
  window title and scale mouse coordinates. Stop the resident only when its window
  or message loop is lost, and record a fixed diagnostic. Acceptance: an injected
  composition or placement failure leaves the resident accepting events and showing
  a text card; native rendering and leak tests and standard Cargo checks pass.
  Source: `audit-20261026.md` E2, A1.

- [x] **AH-042 — Wake the resident only for pending work.** Depends on AH-041.
  Done (part 1, agreed with the user; the rest is AH-051): `serve_until` waits on
  overlapped accept plus a stop event; IPC posts `WAKE` to the window; the 16 ms timer
  runs only while a card is visible or a timed card counts down (a 15 s housekeeping
  timer remains only while usage polling is enabled, so the tray tooltip/icon may lag
  by up to 15 s when idle); `Driver::wait` lets the usage worker sleep to its deadline
  (a live app-server connection is still polled every 500 ms for pushed updates);
  tooltip text is applied only on change. A test counts `WM_TIMER` deliveries on the
  idle GUI thread. Original brief:
  An idle resident wakes about 100 times per second: the 16 ms GUI timer, the 50 ms
  pipe accept poll with a new event per attempt, and the 50 ms usage worker loop per
  enabled agent. Post new IPC events to the window, run the timer only while a card
  is visible or animating, wait on overlapped accept plus a stop event, and sleep
  the usage worker until its next deadline. Present a static card only when frame,
  alpha, countdown second or placement changes; update the tooltip only when its
  text changes; cache usage-card layout and tray placement while the card is open.
  Move "which card, frame, alpha and sounds now" into a pure presenter driven by
  queue state and a controlled clock, leaving `Ui` a thin Win32 adapter, and stop
  the window procedure from borrowing `Ui` while `draw` holds it across reentrant
  `ShowWindow`/`SetWindowTextW` calls. Acceptance: presenter unit tests; a native
  test observes no periodic wakeups while idle and prompt display of a forwarded
  event; lifecycle, tray and leak tests and standard Cargo checks pass.
  Source: `audit-20261026.md` P1, P2, A6, E7.

- [x] **AH-043 — Report configuration errors and guard Restart.**
  Done: `invalid configuration: <table.key> <reason>` messages without values (see
  `docs/configuration.md`); tray Restart validates first and shows the error on the
  details card; fixed log codes for invalid configuration, render fallback/failure,
  resident startup failure and usage failure. Original brief:
  Parsing reports only "invalid configuration fields" and validation one generic
  message; the resident and Restart run without a console, so errors vanish and an
  invalid file makes Restart exit silently. Name the failing field and reason
  without echoing values. Validate the file before the tray Restart replaces the
  process; on failure keep the running resident and show the error on a card. Add
  fixed operational-log codes for invalid configuration, render fallback, resident
  startup failure and usage failure, without payloads. Acceptance: config tests
  assert field-level messages; a process test makes the file invalid, requests
  Restart and observes the original resident still serving; standard Cargo checks.
  Source: `audit-20261026.md` E5, A8.

- [x] **AH-044 — Separate usage sources and slow Claude retries.** Depends on AH-037.
  Done (agreed with the user; the tail is AH-052): `usage_service::UsageSource`
  (`read`, `listen`, `updates`, `is_live`) is implemented by `CodexSource` (the
  unchanged `Connection` and rolling updates) and `claude_usage::ClaudeSource` (one
  typed HTTPS read, no JSON-RPC emulation, no second backoff layer);
  `View` carries an `Agent`, and card text is chosen by it, not by label. HTTP 429 maps
  to `Error::RateLimited`, and the driver then waits at least `refresh_interval_sec`
  (manual refresh included); other failures keep the 10 s..300 s backoff. Fake-clock
  driver tests cover both; Codex tests and texts are unchanged. Original brief:
  `ClaudeTransport` emulates the Codex JSON-RPC app-server and reshapes HTTP data
  into Codex JSON, which adds pre-formatted `Bucket.details`, a second backoff layer
  and Codex-worded errors for Claude. Introduce a `UsageSource` boundary implemented
  by the Codex client and the Claude HTTPS client; select card text by `Agent`, not
  by label; remove the test-only `summary`/`details`/`brief` path and move its
  assertions to the shown card text. After a Claude HTTP 429 the next scheduled
  request waits at least `refresh_interval_sec`; manual refresh stays rate-limited.
  `Retry-After` and the 15-minute cap remain in AH-038. Acceptance: driver tests with
  a fake clock show no early Claude request after 429; Codex usage behavior and
  error text are unchanged; standard Cargo checks. Source: `audit-20261026.md` A3,
  A4, E4.

- [x] **AH-046 — Accept concurrent hook bursts.** Depends on AH-010.
  Done: `sixteen_concurrent_hooks_reach_one_resident_exactly_once` (tests/resident.rs)
  starts 16 `resident::forward` calls together against a cold resident; all succeed
  and one resident queues all 16. No event was lost in 28 runs at 16 hooks or 8 runs at
  48, so the server (single pipe instance, `ensure` before send) is unchanged; the
  test guards it, and `WaitNamedPipeW` or several instances stay the fallback if a
  slower machine loses events. Original brief: The resident pipe has one instance; busy clients retry with 10 ms sleeps, the
  status probe allows 30 ms and the event 150 ms, and each forward makes two round
  trips. Parallel subagent hooks may start extra residents or drop notifications
  while the hook still reports success. First add a test that forwards 16 hook
  events concurrently. If any is lost, serve several pipe instances or wait with
  `WaitNamedPipeW`, and send the event before probing status. Acceptance: one
  resident queues all 16 events exactly once and every hook returns within its
  existing timeout; IPC security and race tests and standard Cargo checks pass.
  Source: `audit-20261026.md` E3.

- [x] **AH-047 — Fix small resident and transient edge cases.**
  Done (decisions agreed with the user): the sound slot loss was already fixed by
  `Queue::take_unannounced(room)` (covered in tests/queue.rs), so no change;
  `[resident] tray_enabled` was removed from the config, the example and INFO.md
  (a file that still sets it is now rejected as an unknown key; no compatibility is
  kept, there are no external users); `overlay::register_class` accepts an existing
  class; simultaneous transient cards stack (`placement::stack`: downward, upward for
  `bottom_right`) by looking at visible `aHerald.Overlay` windows, with placement
  serialized across hook processes by a named mutex held until the card is shown.
  Tests: `stacking::*` in tests/placement.rs, `simultaneous_transient_cards_do_not_overlap`
  in tests/native_overlay.rs (3 real cards), `class_tests` in src/overlay.rs (no red
  phase observed: the helper is new). Not covered: the two-process race at the mutex
  timeout (500 ms). Each transient card still starts its own `codex app-server` for
  the session name. Original brief: The resident marks an event as sounded before checking for a free audio slot, so a
  fifth simultaneous sound is lost. `[resident] tray_enabled` is parsed and
  documented but has no effect; decide with the user whether to implement or
  deprecate it, keeping existing configuration files loadable. Transient window
  registration does not tolerate an existing class, and simultaneous transient cards
  overlap at one position; decide whether they stack. Acceptance: a sound waiting
  for a slot plays once a slot frees; `tray_enabled` behaves as documented; tests
  cover the chosen class and stacking behavior; standard Cargo checks.
  Source: `audit-20261026.md` E8.

- [x] **AH-055 — Claude click-to-activate (split from AH-038).** Depends on AH-036.
  Clicking a Claude card opens its own session. Desktop: the hook `session_id` is matched
  to `cliSessionId` in Claude Desktop's session files (only that field is read; user-approved
  2026-10-26) and `claude://claude.ai/epitaxy/local_<id>` is opened, with the Desktop window
  as fallback. Terminal: the hook records the visible console window of its own or an ancestor
  process (`console_window`), accepted at click time only for `conhost.exe`, `OpenConsole.exe`
  and `WindowsTerminal.exe`. Automated: policy with a fake agent, Desktop lookup in temp dirs,
  ancestor walk, event window field (`tests/activation.rs`, `tests/claude_desktop.rs`,
  `tests/click_target.rs`). Manual 2026-10-26/27 (classic console host, Claude Desktop):
  a card raised its terminal window; two Desktop sessions answered with a delay produced two
  cards, and each click switched to its own session. Not verified: Windows Terminal and two
  terminal windows (tracked in AH-056).

- [x] **AH-048 — Reduce animation memory and sharpen scaled text.** Depends on AH-020, AH-041.
  A card keeps one base surface, one working surface and the shared artwork frames; only
  the changed part is redrawn (frame, countdown second, session name). An 80-frame
  animation on a 530×244 card, which as full-card frames needed ~41 MiB and fell back to
  text, now shows its artwork (`composition_tests`); native-resource cycle test covers
  frame, countdown and heading redraws. Cards are drawn at the target monitor's effective
  DPI (user agreed, option A) and redrawn on DPI, display or monitor change; artwork too
  large for the DPI scale is prepared at the configured scale; nearest-neighbor stays
  only for fitting an oversized card and for pixel art via the interpolation setting
  (`docs/milestone-5.md`). Automated on Windows 11 build 22621: 277 passed, 0 failed,
  14 ignored (three consecutive runs); `fmt`, `clippy -D warnings` and the release
  build clean. `placement_failure_keeps_the_resident…` raced the failing draw once
  composition got fast; it now sends a wake after clearing the fault, as the resident
  retries on the next wake. Visual check at 100/150/200% moved to AH-076.

- [x] **AH-077 — Structured log lines and debug performance metrics.**
  Log lines are `<local ISO-8601 time, ms, UTC offset> <LEVEL> <component.event> [key=number]`
  through one shared logger with an atomic level gate; rotation cap is 512 KiB at `debug`,
  64 KiB otherwise. `perf` spans (IPC handling, process scan, Claude Desktop session
  lookup, artwork preparation, composition build, frame render and present, placement,
  usage reads, WAV load, configuration load) are aggregated per thread and window into
  `perf <span> n= avg= sd= min= p95= max=` lines, with cache/composition counters and
  gauges; a single over-threshold sample is a throttled `WARN slow` line. Tests cover
  format, gating, rotation, the aggregation math with injected time, and a debug resident
  that logs only fixed names and numbers. Not done: a queue-wait span (needs queue
  timestamps; the event-trace option was excluded), so IPC handling is the only
  receive-side span. Adds `Win32_System_ProcessStatus` to the existing `windows` crate.
  Automated on Windows 11 build 22621: 301 passed, 0 failed, 14 ignored (two consecutive
  runs); `fmt`, `clippy -D warnings` and the release build clean. A live-log read and the
  overhead comparison moved to AH-082.

- [x] **AH-082 — Read a real debug log and set a performance baseline (split from AH-077).**
  A debug resident under an isolated scope showed 24 animated cards; the log and
  `--perf-report` are readable and the top spans are real work (table and assessment in
  `docs/milestone-5.md`): a frame costs ~1 ms, nothing is slow, no frame is skipped, the
  artwork cache hits 20 of 24. `tests/perf_baseline.rs` now fails when a hot-path mean
  exceeds about three times the baseline, on cache misses, rebuilds or skipped frames.
  Finding: cards are composed ~3 times each (AH-083). The `warn` versus `debug` overhead
  comparison was not done (AH-084). The red phase of the budget
  assertions was not observed (budgets were set after measuring).

- [x] **AH-058 — Transient mode: avoid a `codex app-server` child per Codex card
  (split from AH-049).** Measured first (release, 8 runs): a transient name lookup costs
  130-150 ms and 20-560 ms to end the child; the resident's shared lookup costs 2-20 ms; Claude
  has no lookup. Decision with the user: no global switch, only transient mode is affected.
  New `[codex] transient_names` (default false): off, a transient card shows
  `Codex: session <id suffix>` and starts no child or worker thread; on, it keeps the old
  lookup. Resident unchanged. Test `tests/transient_names.rs` counts connection factory
  calls with a fake transport (0 when off, 1 when on) and checks the default; the stub
  `codex` on `PATH` process test from the ticket was replaced by this seam because the
  discovery only accepts a native `codex.exe`. Red phase: the test did not compile before
  the change. Automated: 307 passed, 0 failed, 14 ignored; `fmt` and `clippy -D warnings`
  clean; release build not run. Docs: `docs/codex-hooks.md`, `aHerald.example.toml`.

- [x] **AH-084 — Measure the cost of debug logging itself (split from AH-082).**
  Seven alternating runs of the 24-card burst at `warn` and `debug`, comparing the
  resident's CPU time: medians 172 ms (warn) and 141 ms (debug), spread about 100 ms, so
  the cost is below the method's resolution and far under 2% of a 16 ms frame; no code
  change. Figures in `docs/milestone-5.md`. The measuring test was temporary and is not
  kept. Wall time from first event to idle was not used: it is set by the 120 ms event
  spacing and card duration, not by frame cost.

- [x] **AH-074 — Claude "input required" card in an idle terminal session (found in AH-056).**
  Cause: the `idle_prompt` `Notification`, which Claude Code sends about a minute after
  `Stop`, was mapped to `input_required`, so a finished, idle session got a second card.
  `idle_prompt` is now ignored (the `Stop` card already announced the turn); `elicitation_dialog`
  and `agent_needs_input` still map to `input_required`. `docs/claude-hooks.md` updated. Red
  phase observed in `claude_notification_types_map_to_event_kinds`. The payload was not
  re-captured live: the cause rests on the earlier 2026-10-26 capture (`idle_prompt` after
  `Stop`) and the two cards seen on 2026-10-27. Automated: full suite 0 failed; `fmt` and
  `clippy -D warnings` clean.

- [x] **AH-083 — Compose a resident card once instead of about three times (found in AH-082).**
  A queue-count or index change redraws only the heading unless the navigation buttons
  appear or vanish, and a newly selected card waits up to 300 ms for its artwork before
  falling back to the text card (agreed with the user after timing ten installed pets:
  artwork preparation took 14-130 ms in the release build, mean about 60 ms, on a cache
  miss only). Rebuilds for 24 cards fell from 69 to 25; `tests/perf_baseline.rs` limit
  is 30. `tests/resident_render_faults.rs` covers the first card appearing with its
  artwork and a stalled worker (debug-only `slowart` fault) showing the text card after
  the wait limit. The red phase was observed for the rebuild limit and the stalled-worker
  test; the first-card test already passed before the change.

- [x] **AH-078 — GUI-thread stall detector (from AH-077).** GUI messages are timed per
  class (`gui.timer`, `gui.wake`, `gui.input`, `gui.other`); a message over 50 ms is a
  throttled `WARN slow gui.<class>` line. Implemented as per-class spans instead of a
  gap between loop iterations, because an idle `GetMessage` wait is not a stall.

- [x] **AH-079 — Animation frame-budget report (from AH-077).** `FramePacer` counts the
  animation frames a card jumped over because a draw came late (`frame.skipped`) next to
  `frame.presented`; tested with loops, repaints of one frame and new cards. Late-versus-
  target frame rate was not added: frames are presented at their own cadence, so skipped
  frames are the faithful measure.

- [x] **AH-080 — Periodic resource snapshot at debug (from AH-077).** Once a minute the
  resident writes `resources working_set_kb= gdi= user= threads=`; the composition size is
  the `composition.bytes` gauge. The flat GDI count across repeated redraws is covered by
  the existing native-resource cycle test.

- [x] **AH-081 — `aherald --perf-report` (from AH-077).** Ranks spans from
  `aherald.previous.log` and `aherald.log` by total time (samples, total, mean, worst-window
  p95, max, slow count) and sums counters; unparseable lines are skipped. Tested with a
  fixture log and through the binary.

- [x] **AH-056 — Verify Claude click in Windows Terminal (split from AH-055).**
  Depends on AH-055. Manual 2026-10-27 (Windows Terminal 1.24.12741.0, Claude CLI): a card
  raised the Windows Terminal window; with two sessions in two windows, each click raised
  its own window, and both restored from minimized. The first click was 6.7 s late because
  the Desktop session scan (703 files, cold cache) ran before the recorded window; the
  window is now tried first (`tests/activation.rs`), and clicks log
  `debug agent_activated ms=N` (`tests/operational_log.rs`); clicks after the fix took
  1 to 226 ms. Known limit, accepted: tabs are not selected, UI Automation tab selection
  is not built (fragile across Windows Terminal versions). Automated: 270 passed, 0 failed,
  14 ignored; `fmt` and `clippy -D warnings` clean. Follow-ups: AH-074, AH-075.

- [x] **AH-037 — Opt-in Claude 5h/7d quota.** Depends on AH-036.
  `[claude.usage] enabled` (default false, interval 300..86400 s) polls the undocumented
  `GET api.anthropic.com/api/oauth/usage` over WinHTTP with the token Claude Code stores
  in `.credentials.json`. Agreed exception to the token-reading rule (see AGENTS.md):
  token used only for that call, never logged, stored or refreshed. Parsing, credential,
  status and driver behavior are tested with fakes (no network). Live check 2026-10-25
  (signed-in Claude Code, `anthropic-beta: oauth-2025-04-20`): HTTP 200 with
  `five_hour`/`seven_day` `{utilization, resets_at}`; a stale token gave 401. Tested with
  Claude Code 2.1.310. Manual 2026-10-25/27: separate Claude menu line, details card with
  plan, 5h/Week, Extra usage and Spend, forced refresh on opening details, and tray
  tooltip/menu/card with both agents enabled.

- [x] **AH-053 — Test the transient placement mutex timeout race (split from AH-047).**
  Depends on AH-047. `tests/native_overlay.rs` holds the session-wide placement mutex from
  the test process, delivers a transient card and asserts that the card still appears
  (after the 500 ms wait) while the lock is held, then dismisses it and observes a clean
  exit. Checked by mutation: a placement wait that never times out makes the test fail
  with "card missing". Card-showing tests in that file now run one at a time because they
  share that mutex and screen area.

- [x] **AH-052 — Finish the usage-source cleanup (split from AH-044).** Depends on AH-044.
  Removed the test-only `View::summary`/`details`, `Service::summary`/`details` and
  `brief`; their assertions in `tests/usage_service.rs` and `tests/tray_usage.rs` now check
  the shown `card_text`. `Bucket.details` (pre-formatted lines) became typed
  `Bucket.extra_usage`/`spend` (`usage::Allowance`: enabled, used, limit, currency);
  the card renders the same text from them. `claude_usage::parse_response` builds the
  `Snapshot` directly (no Codex-shaped JSON, `snapshot_from_response`/`with_plan` gone).
  Codex card text is unchanged. Note: the dropped `details` view also showed reset-credit
  `granted` times and full-length reset timestamps, which the card never did.

- [x] **AH-051 — Pure card presenter and P2 redraw limits (split from AH-042).** Depends on AH-042.
  Done (part 1, agreed with the user; the reentrancy half is AH-057): `presenter::Presenter`
  turns the selected queue entry, animation clock and fade settings into `Shown`
  (frame, alpha, countdown second) and reports a card only when that differs from what is
  on screen; `Ui` presents and calls `ShowWindow` only then. The usage card measures its
  height and asks Explorer for the tray rectangle once per layout, not every 16 ms, and
  the cached placement is dropped on display, DPI, setting and taskbar changes.
  `tests/presenter.rs` covers once-only static cards, countdown seconds, animation frames,
  fade alpha and invalidation. Not covered by an automated test: the present-call count
  of the live window. Original brief: see TODO.md history; source `audit-20261026.md`
  A6, E7, P2.

- [x] **AH-057 — Reentrancy-safe resident Ui (split from AH-051).** Depends on AH-051.
  The resident window now owns `Ui` behind a `reentry::Gate`: the procedure borrows it
  only when no handler is active, so messages that `ShowWindow`/`SetWindowTextW` send
  during `draw` are kept (identical ones merged, backlog capped at 64) and handled after
  it, and the tray popup menu's nested loop runs with the gate open. The tray
  `TaskbarCreated` id lives outside `Ui`. `presenter::sound_slots`/`MAX_SOUNDS` replace the
  inline 4-sound cap. Tests: `tests/reentry.rs` (gate), `sound_slots_cap_concurrent_playback`,
  and a debug-build native test that sends a message mid-draw (`aherald.fault` = `reenter`).
  Limit: aliasing undefined behavior is not observable at run time, so the native test
  proves the deferral path stays alive, not the absence of aliasing. Source:
  `audit-20261026.md` E7, A6.

- [x] **AH-049 — Reuse app-server clients and decoded sounds.**
  Done (agreed with the user; transient mode is AH-058): with Codex usage polling on, the
  resident reads session names over the usage worker's connection
  (`usage_service::NameLookup`, `Connection::thread_name`), so one `codex app-server`
  child runs instead of two; with polling off the name worker keeps its own child.
  `audio::SoundCache` (8 entries, keyed by path, volume, size and modification time)
  replaces a disk read and parse per event. `Queue::push` returns `queue::Outcome`
  (Queued, Resolved, Duplicate, Obsolete, Full, Invalid) and the IPC listener no longer
  repeats the terminal, duplicate and obsolete checks (a repeated open Claude wait now
  answers Accepted instead of Rejected). Bilinear `Animation::prepare` premultiplies once
  and `activation::find` reuses one path buffer per enumeration. Tests:
  `tests/usage_shared.rs`, `tests/queue_outcome.rs`, `audio::cache` and a bilinear
  `prepare` test. Limit: the child count is asserted through a fake transport's connection
  factory (one factory call is one child), not by counting real processes; the
  allocation reductions have no automated test. Source: `audit-20261026.md` P5, P6.

- [x] **AH-050 — Tighten internal types and remove dead code.**
  Done: `general.mode`, `general.logging`, `notifications.position`, `monitor` and
  `interpolation` are the enums `Mode`, `LogLevel`, `Position`, `Monitor` and
  `Interpolation`; `Config::events` is `config::Events`, indexed by `events::Kind`
  (`Kind::ALL`, `Kind::from_name`; `EVENT_NAMES` is gone), so a missing key cannot panic.
  The TOML format is unchanged and bad values still fail without echoing them. In
  `pipe.rs` a private `Failure` enum replaces matching on `"local pipe deadline
  exceeded"`; public pipe functions still return the same `String` errors.
  `audio_windows` uses `local::Handle`; `overlay.rs` has `Font::segoe`, `draw_text`,
  `restore_alpha` and safe `Surface::pixels`/`pixels_mut`, so pixel loops run outside
  `unsafe`. Removed `Agent::hook_source` and the unused `now` argument of
  `View::status` and `View::compact`. Tests: `enumerated_settings_*` in
  `tests/config.rs`, `a_connected_client_that_stalls_*` in `tests/ipc.rs`; existing
  pixel and config tests guard the refactors. Limits: the `https.rs` handle closes with
  `WinHttpCloseHandle`, so it cannot share `local::Handle`; `[resident] tray_enabled`
  was already gone; test-only helpers (`pet::for_notification`, `Animation::index_at`,
  `AssetCache::retained_bytes`) stay because tests use them. Source:
  `audit-20261026.md` A2, A5, A7.

- [x] **AH-038 — Claude follow-ups.**
  Done (2026-10-27, `feature/AH-038`): Claude quota polling honors `Retry-After` (seconds
  form) capped at 15 minutes and never earlier than `refresh_interval_sec` (driver tests with
  a fake clock, loopback header test); `SessionEnd` clears that session's pending cards
  without a card or sound and reaches only a running resident (`Event.ended`; queue,
  hook-parse and resident IPC tests); `SubagentStop` stays ignored by decision; terminal
  evidence, per-session waits, session names, console window capture and activation
  selection go through `Agent`. Also fixed: an empty header slice reached
  `WinHttpSendRequest` as a dangling pointer. Not done, split to AH-059: Claude session
  names and the statusLine quota fallback. Not verified live: `SessionEnd` and a real 429
  `Retry-After` from Claude Code.

- [x] **AH-075 — Bound the Claude Desktop session scan by recency (found in AH-056).**
  Done (2026-10-27, `feature/AH-075`): on the dev machine (708 session files) a file's mtime
  tracked `lastActivityAt`, so `local_session_id` now skips files not modified in the last
  30 days (`MAX_AGE`); `MAX_FILES` still applies. Test: `tests/claude_desktop.rs`
  `sessions_untouched_for_months_are_not_scanned` (mtime set with `File::set_modified`).

- [x] **AH-022 — Package and document the portable application.**
  Done (2026-10-27, `feature/AH-075` branch): `package-release.ps1`
  builds `aherald-vX.Y.Z-windows-x64.zip` + `.sha256` (exe, example config, `assets/`,
  `examples/`, `docs/`, LICENSE, README, CHANGELOG, `START-HERE.md`, `MANIFEST.txt` with
  per-file SHA-256), then extracts it into a fresh temp folder as a non-administrator and
  runs `aherald.exe --version` (observed: `aHerald 0.1.0`, 1.29 MB zip). `release.yml` calls
  the script. `docs/portable.md` covers modes, CLI, configuration, replacing assets, manual
  hooks and autostart, uninstall, build commands and limits. `check-release.ps1` passes (no
  separate C runtime). Not verified: the workflow itself has not run on GitHub (not pushed);
  Windows 10 extraction belongs to AH-023.

- [x] **AH-023 — Complete release acceptance.**
  Done (2026-10-27): automated checks, hook latency, card/About/DPI visual checks and
  sound on Windows 11 build 22621, and the release workflow on tag `v0.1.0` are recorded
  in `docs/release-acceptance.md`. Long-lived waits are covered by the live resident's
  multi-hour runs, not a timed pending card. Windows 10 is out of scope and labelled
  untested. Not verified and labelled so: a full Explorer process restart and
  multi-monitor/mixed DPI (open as AH-020).

- [x] **AH-060 — Platform seam, non-Windows build and CI.**
  Done (2026-10-27): the library and `main.rs` compile on macOS and Linux behind stub
  platform modules (`src/unsupported.rs`); `src/paths.rs` is the path helper;
  `.github/workflows/ci.yml` runs fmt, clippy and tests on windows, macos and ubuntu
  (green on all three, PR #38); `.gitattributes` added; target statements updated.
  The Windows-only `\\` and `LOCALAPPDATA` literals in `src/activation/native.rs` and
  `src/shell.rs` stay because those files are Windows-only.

- [x] **AH-087 — State "not tested on Windows 10" in the release notes.**
  Done (2026-10-27): the 0.1.0 section of `CHANGELOG.md` says the release was not tested
  on Windows 10 (`README.md` and `docs/portable.md` limits already did). The
  "Windows 10/11" platform wording was left unchanged by decision.

- [x] **AH-085 — Redesign the notification card layout.**
  Layout: wider accent stripe by event kind; artwork and countdown unchanged; rows
  `<harness>, <folder>`, session name (row always reserved), larger event kind; bottom row
  `[Dismiss] <age>` and `[<] n/m [>]` with wrap-around; usage card on the same layout;
  `border_color` (global, per event, usage). Automated tests cover folder extraction, the
  heading model, button geometry, navigation, age text, IPC carrying the folder and equal
  card height. Done 2026-10-27: manual visual check of both agents at 100% and 150%
  observed by the user (see `docs/milestone-5.md`).

- [x] **AH-086 — `--version` and a tray About card.**
  `aherald --version` prints `aHerald <version>`; the tray item "About aHerald" shows the
  version on a usage-style card and a second choice closes it. Automated tests cover the
  CLI, the card text, the menu entry and a native open/close test. Done 2026-10-27: the
  card was observed at 100% and 150%, and redrew correctly when the Windows scale was
  changed while it was open.

- [x] **AH-076 — Visual check of native-DPI cards at 100/150/200% (split from AH-048).**
  Done 2026-10-27 (criterion narrowed with the user): text sharp at 100%, 150% and 200%;
  pixel art crisp and the largest local pet (WALL-E, 2.3 MB spritesheet) shows its artwork
  instead of the text fallback; the card redraws when the Windows scale is changed while
  it is open, on all three scales. The two-monitor mixed-DPI case moved to AH-020 (cards
  are not draggable). Not verified: the frame count of the WALL-E sheet. Results in
  `docs/milestone-5.md`.

## Milestone 5a — macOS and Linux support

- [x] **AH-061 — Unix common layer (IPC, spawn, HTTPS, open).** Depends on AH-060.
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
  `tests/https.rs` and the unit tests in `src/unix/`). Closed 2026-10-27: the same run passed on macOS 26 (aarch64-apple-darwin, M1: fmt,
  clippy `-D warnings`, `cargo test --all-targets`) and CI run 40589344740 on master was
  green on macos-latest, ubuntu-latest and windows-latest. Not covered yet: the
  Unix resident loop itself needs an overlay (AH-062, AH-068); proxy behaviour of
  `ureq` (environment variables only) is unchecked, like the Windows system-proxy item
  in AH-045.

- [x] **AH-063 — macOS tray.** Depends on AH-062.
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
  Closed 2026-10-28 with the maintainer: the acceptance (manual menu check) and the extra checks
  above passed. The status item on a second display moved to AH-062 and the light-wallpaper
  icon to AH-089.

- [x] **AH-064 — macOS audio.** Depends on AH-061.
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
  Closed 2026-10-28: both acceptance checks passed. Playback while the output device changes
  (for example Bluetooth) moved to AH-089.

- [x] **AH-065 — macOS click-to-activate.** Depends on AH-062.
  Replace the HWND capture with PID/TTY-based terminal identification and
  `NSRunningApplication` activation; Codex and Claude Desktop URL schemes; Claude Desktop
  session lookup under `~/Library/Application Support/Claude` (still `cliSessionId` only).
  Document Accessibility/Automation prompts. Acceptance: manual check with Terminal,
  iTerm2 and Claude Desktop; policy tests stay platform-neutral.
  Acceptance narrowed 2026-10-28 with the maintainer: Warp replaces iTerm2, which is not
  installed on the test machine.
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
  Closed 2026-10-28: Terminal.app and Warp (a hook run inside each, click from Finder brings it
  forward), Claude Desktop (two sessions of the project switched by their ids) and Codex.app
  passed. iTerm2, the other listed terminals and tab or window selection are not verified; the
  latter would need Automation.
